use itertools::Itertools;
use regex::Regex;
use rustc_middle::ty::TyCtxt;

use crate::analyzer::{
    heuristics::{HasRawPtrDeref, HasTransmute},
    result::{FunctionWithMetadata, PurityAnalysisResult},
};
use crate::common::storage::{ClosureInfoStorage, FunctionInfoStorage};
use crate::common::FunctionInfo;
use crate::important::ImportantLocals;

fn analyze_item<'tcx>(
    item: &FunctionInfo<'tcx>,
    important_locals: ImportantLocals,
    passing_calls_ref: &mut Vec<FunctionWithMetadata<'tcx>>,
    failing_calls_ref: &mut Vec<FunctionWithMetadata<'tcx>>,
    storage: &FunctionInfoStorage<'tcx>,
    allowlist: &Vec<Regex>,
    tcx: TyCtxt<'tcx>,
) -> bool {
    let is_allowlisted = {
        let def_path_str = format!("{:?}", item.def_id());
        allowlist.iter().any(|lib| lib.is_match(&def_path_str))
    };

    let has_no_important_locals = important_locals.is_empty();

    if has_no_important_locals || is_allowlisted {
        let info_with_metadata = FunctionWithMetadata::new(
            item.to_owned(),
            important_locals.clone(),
            false,
            false,
            is_allowlisted,
        );
        passing_calls_ref.push(info_with_metadata);
        true
    } else {
        let has_unhandled_calls = match item {
            FunctionInfo::WithBody { unhandled, .. } => !unhandled.is_empty(),
            _ => false,
        };

        let has_raw_pointer_deref = match item {
            FunctionInfo::WithBody { body, .. } => body.has_raw_ptr_deref(tcx),
            _ => false,
        };

        let has_transmute = match item {
            FunctionInfo::WithBody { body, .. } => body.has_transmute(tcx),
            _ => false,
        };

        let has_no_leaking_calls = item
            .calls()
            .and_then(|calls| {
                let children_results = calls
                    .iter()
                    .map(|call| {
                        let new_important_locals =
                            important_locals.transition(call.args(), call.def_id().to_owned(), tcx);
                        let call_fn_info = storage.get_by_call(call);
                        analyze_item(
                            call_fn_info,
                            new_important_locals,
                            passing_calls_ref,
                            failing_calls_ref,
                            storage,
                            allowlist,
                            tcx,
                        )
                    })
                    .collect_vec();
                Some(children_results.into_iter().all(|r| r))
            })
            .unwrap_or(false);

        if !has_unhandled_calls && !has_raw_pointer_deref && !has_transmute && item.has_body() {
            let info_with_metadata = FunctionWithMetadata::new(
                item.to_owned(),
                important_locals.clone(),
                has_raw_pointer_deref,
                has_transmute,
                is_allowlisted,
            );
            passing_calls_ref.push(info_with_metadata);
        } else {
            let info_with_metadata = FunctionWithMetadata::new(
                item.to_owned(),
                important_locals.clone(),
                has_raw_pointer_deref,
                has_transmute,
                is_allowlisted,
            );
            failing_calls_ref.push(info_with_metadata);
        };

        return !has_unhandled_calls
            && !has_raw_pointer_deref
            && !has_transmute
            && has_no_leaking_calls;
    }
}

pub fn run<'tcx>(
    functions: FunctionInfoStorage<'tcx>,
    closures: ClosureInfoStorage<'tcx>,
    important_locals: ImportantLocals,
    annotated_pure: bool,
    allowlist: &Vec<Regex>,
    tcx: TyCtxt<'tcx>,
) -> PurityAnalysisResult<'tcx> {
    let origin = functions.get_with_body(functions.origin()).unwrap();

    let mut passing_calls = vec![];
    let mut failing_calls = vec![];

    let pure = analyze_item(
        origin,
        important_locals,
        &mut passing_calls,
        &mut failing_calls,
        &functions,
        allowlist,
        tcx,
    );

    if pure {
        PurityAnalysisResult::new(
            functions.origin().def_id(),
            annotated_pure,
            true,
            String::new(),
            passing_calls,
            failing_calls,
            closures,
        )
    } else {
        PurityAnalysisResult::new(
            functions.origin().def_id(),
            annotated_pure,
            false,
            String::from("unable to ascertain purity of inner function call"),
            passing_calls,
            failing_calls,
            closures,
        )
    }
}
