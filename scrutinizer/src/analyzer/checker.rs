use log::trace;
use regex::Regex;
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::DefId;
use std::collections::HashMap;

use super::{
    important_locals::ImportantLocals,
    raw_ptr::HasRawPtrDeref,
    result::{PurityAnalysisResult, WithImportantLocals},
};
use crate::collector::{ClosureInfoStorageRef, FnInfo, FnInfoStorageRef};

fn check_fn_call_purity<'tcx>(fn_call: &FnInfo<'tcx>, tcx: TyCtxt<'tcx>) -> bool {
    let allowed_libs = vec![
        Regex::new(r"core\[\w*\]::intrinsics").unwrap(),
        Regex::new(r"core\[\w*\]::panicking").unwrap(),
        Regex::new(r"alloc\[\w*\]::alloc").unwrap(),
    ];
    match fn_call {
        FnInfo::Regular { instance, body, .. } => {
            let def_path_str = format!("{:?}", instance.def_id());
            let raw_pointer_encountered = body.has_raw_ptr_deref(tcx);
            !raw_pointer_encountered || allowed_libs.iter().any(|lib| lib.is_match(&def_path_str))
        }
        FnInfo::Ambiguous { def_id, parent, .. } | FnInfo::Extern { def_id, parent, .. } => {
            let def_path_str = format!("{:?}", def_id);
            allowed_libs.iter().any(|lib| lib.is_match(&def_path_str))
                || tcx.is_const_fn_raw(def_id.to_owned())
                || tcx.is_const_fn_raw(parent.def_id().to_owned())
        }
    }
}

fn check_purity<'tcx>(
    storage: FnInfoStorageRef<'tcx>,
    important_locals_storage: &HashMap<DefId, ImportantLocals>,
    tcx: TyCtxt<'tcx>,
) -> bool {
    let borrowed_storage = storage.borrow();
    borrowed_storage.fns().iter().all(|fn_call| {
        trace!(
            "call={:?} has important_locals={:?}",
            fn_call.def_id(),
            important_locals_storage.get(&fn_call.def_id()).unwrap(),
        );
        let has_no_important_locals = important_locals_storage
            .get(&fn_call.def_id())
            .unwrap()
            .is_empty();
        check_fn_call_purity(fn_call, tcx) || has_no_important_locals
    }) && borrowed_storage.unhandled().is_empty()
}

fn propagate_locals<'tcx>(
    storage: FnInfoStorageRef<'tcx>,
    important_locals_storage: &mut HashMap<DefId, ImportantLocals>,
    def_id: DefId,
    important_locals: ImportantLocals,
    tcx: TyCtxt<'tcx>,
) {
    storage
        .borrow()
        .fns()
        .iter()
        .filter(|func| func.def_id() == def_id)
        .filter_map(|func| func.calls())
        .flatten()
        .map(|call| {
            let new_important_locals =
                important_locals.transition(call.args(), call.def_id().to_owned(), tcx);
            (call.def_id().to_owned(), new_important_locals)
        })
        .for_each(|(def_id, important_locals)| {
            if important_locals_storage
                .get_mut(&def_id)
                .unwrap()
                .join(&important_locals)
            {
                propagate_locals(
                    storage.clone(),
                    important_locals_storage,
                    def_id,
                    important_locals,
                    tcx,
                )
            }
        });
}

pub fn produce_result<'tcx>(
    storage: FnInfoStorageRef<'tcx>,
    closures: ClosureInfoStorageRef<'tcx>,
    important_locals: ImportantLocals,
    annotated_pure: bool,
    tcx: TyCtxt<'tcx>,
) -> PurityAnalysisResult<'tcx> {
    let borrowed_storage = storage.borrow();

    let mut important_locals_storage = HashMap::from_iter(
        borrowed_storage
            .fns()
            .iter()
            .map(|func| (func.def_id(), ImportantLocals::empty())),
    );

    important_locals_storage
        .get_mut(&storage.borrow().origin().def_id())
        .unwrap()
        .join(&important_locals);

    propagate_locals(
        storage.clone(),
        &mut important_locals_storage,
        storage.borrow().origin().def_id(),
        important_locals,
        tcx,
    );

    let (passing_calls, failing_calls) = borrowed_storage
        .fns()
        .clone()
        .into_iter()
        .map(|call| {
            let important_locals = important_locals_storage
                .get(&call.def_id())
                .unwrap()
                .to_owned();
            WithImportantLocals {
                fn_info: call,
                important_locals,
            }
        })
        .partition(|fn_call| check_fn_call_purity(&fn_call.fn_info, tcx));

    if !check_purity(storage.clone(), &important_locals_storage, tcx) {
        let reason = if !borrowed_storage.unhandled().is_empty() {
            String::from("unhandled terminator")
        } else if !borrowed_storage
            .fns()
            .iter()
            .all(|fn_call| check_fn_call_purity(fn_call, tcx))
        {
            String::from("unable to ascertain purity of inner function call")
        } else {
            unreachable!()
        };
        PurityAnalysisResult::new(
            borrowed_storage.origin().def_id(),
            annotated_pure,
            false,
            reason,
            passing_calls,
            failing_calls,
            closures.borrow().to_owned(),
            borrowed_storage.unhandled().clone(),
        )
    } else {
        PurityAnalysisResult::new(
            borrowed_storage.origin().def_id(),
            annotated_pure,
            true,
            String::new(),
            passing_calls,
            failing_calls,
            closures.borrow().to_owned(),
            borrowed_storage.unhandled().clone(),
        )
    }
}
