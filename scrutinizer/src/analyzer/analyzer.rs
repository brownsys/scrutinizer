use regex::Regex;
use rustc_middle::ty::TyCtxt;

use super::{
    raw_ptr::HasRawPtrDeref,
    result::{FunctionWithMetadata, PurityAnalysisResult},
    ClosureInfoStorageRef, FunctionInfo, FunctionInfoStorageRef, ImportantLocals,
};

fn analyze_item<'tcx>(
    item: &FunctionInfo<'tcx>,
    important_locals: ImportantLocals,
    passing_calls_ref: &mut Vec<FunctionWithMetadata<'tcx>>,
    failing_calls_ref: &mut Vec<FunctionWithMetadata<'tcx>>,
    storage: FunctionInfoStorageRef<'tcx>,
    allowlist: &Vec<Regex>,
    tcx: TyCtxt<'tcx>,
) -> bool {
    let borrowed_storage = storage.borrow();

    let is_whitelisted = {
        let def_path_str = format!("{:?}", item.def_id());
        allowlist.iter().any(|lib| lib.is_match(&def_path_str))
    };

    let is_const_fn = tcx.is_const_fn_raw(item.def_id().to_owned());
    let has_no_important_locals = important_locals.is_empty();

    if has_no_important_locals || is_const_fn || is_whitelisted {
        let info_with_metadata = FunctionWithMetadata {
            function: item.to_owned(),
            important_locals: important_locals.clone(),
            raw_pointer_deref: false,
            const_fn: is_const_fn,
            whitelisted: is_whitelisted,
        };
        passing_calls_ref.push(info_with_metadata);
        true
    } else {
        let has_unhandled_calls = match item {
            FunctionInfo::WithBody { unhandled, .. } => !unhandled.is_empty(),
            _ => false,
        };

        let raw_pointer_deref = match item {
            FunctionInfo::WithBody { body, .. } => body.has_raw_ptr_deref(tcx),
            _ => false,
        };

        let all_children_calls_pure = item
            .calls()
            .and_then(|calls| {
                Some(calls.iter().all(|call| {
                    let new_important_locals =
                        important_locals.transition(call.args(), call.def_id().to_owned(), tcx);
                    let call_fn_info = borrowed_storage.get_by_call(call);
                    analyze_item(
                        call_fn_info,
                        new_important_locals,
                        passing_calls_ref,
                        failing_calls_ref,
                        storage.clone(),
                        allowlist,
                        tcx,
                    )
                }))
            })
            .unwrap_or(false);

        if !has_unhandled_calls && !raw_pointer_deref && all_children_calls_pure {
            let info_with_metadata = FunctionWithMetadata {
                function: item.to_owned(),
                important_locals: important_locals.clone(),
                raw_pointer_deref,
                const_fn: is_const_fn,
                whitelisted: is_whitelisted,
            };
            passing_calls_ref.push(info_with_metadata);
            true
        } else {
            let info_with_metadata = FunctionWithMetadata {
                function: item.to_owned(),
                important_locals: important_locals.clone(),
                raw_pointer_deref,
                const_fn: is_const_fn,
                whitelisted: is_whitelisted,
            };
            failing_calls_ref.push(info_with_metadata);
            false
        }
    }
}

pub fn run<'tcx>(
    origin: &FunctionInfo<'tcx>,
    functions: FunctionInfoStorageRef<'tcx>,
    closures: ClosureInfoStorageRef<'tcx>,
    important_locals: ImportantLocals,
    annotated_pure: bool,
    allowlist: &Vec<Regex>,
    tcx: TyCtxt<'tcx>,
) -> PurityAnalysisResult<'tcx> {
    let borrowed_functions = functions.borrow();

    let mut passing_calls = vec![];
    let mut failing_calls = vec![];

    let pure = analyze_item(
        origin,
        important_locals,
        &mut passing_calls,
        &mut failing_calls,
        functions.clone(),
        allowlist,
        tcx,
    );

    if pure {
        PurityAnalysisResult::new(
            borrowed_functions.origin().def_id(),
            annotated_pure,
            true,
            String::new(),
            passing_calls,
            failing_calls,
            closures.borrow().to_owned(),
        )
    } else {
        PurityAnalysisResult::new(
            borrowed_functions.origin().def_id(),
            annotated_pure,
            false,
            String::from("unable to ascertain purity of inner function call"),
            passing_calls,
            failing_calls,
            closures.borrow().to_owned(),
        )
    }
}
