use regex::Regex;
use rustc_middle::ty::TyCtxt;

use super::{raw_ptr::HasRawPtrDeref, result::PurityAnalysisResult};
use crate::collector::{ClosureInfoStorageRef, FnInfo, FnInfoStorageRef};

fn check_fn_call_purity<'tcx>(fn_call: &FnInfo<'tcx>, tcx: TyCtxt<'tcx>) -> bool {
    let allowed_libs = vec![
        Regex::new(r"core\[\w*\]::intrinsics").unwrap(),
        Regex::new(r"core\[\w*\]::panicking").unwrap(),
        Regex::new(r"alloc\[\w*\]::alloc").unwrap(),
    ];
    match fn_call {
        FnInfo::Regular { instance, body, .. } => {
            // TODO: raw pointer dereference.
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

fn check_purity<'tcx>(storage: FnInfoStorageRef<'tcx>, tcx: TyCtxt<'tcx>) -> bool {
    let borrowed_storage = storage.borrow();
    borrowed_storage
        .fns()
        .iter()
        .all(|fn_call| check_fn_call_purity(fn_call, tcx))
        && borrowed_storage.unhandled().is_empty()
}

pub fn produce_result<'tcx>(
    storage: FnInfoStorageRef<'tcx>,
    closures: ClosureInfoStorageRef<'tcx>,
    annotated_pure: bool,
    tcx: TyCtxt<'tcx>,
) -> PurityAnalysisResult<'tcx> {
    let borrowed_storage = storage.borrow();
    let (passing_calls, failing_calls) = borrowed_storage
        .fns()
        .clone()
        .into_iter()
        .partition(|fn_call| check_fn_call_purity(fn_call, tcx));
    if !check_purity(storage.clone(), tcx) {
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
            check_purity(storage.clone(), tcx),
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
            check_purity(storage.clone(), tcx),
            String::new(),
            passing_calls,
            failing_calls,
            closures.borrow().to_owned(),
            borrowed_storage.unhandled().clone(),
        )
    }
}
