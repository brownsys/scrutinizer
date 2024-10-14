use itertools::Itertools;
use regex::Regex;
use rustc_middle::mir::{Mutability, VarDebugInfoContents};
use rustc_middle::ty::TyCtxt;
use rustc_span::symbol::Symbol;
use std::collections::HashSet;

use crate::analyzer::{
    deps::compute_deps_for_body,
    heuristics::{HasRawPtrDeref, HasTransmute},
    result::{FunctionWithMetadata, PurityAnalysisResult},
};
use crate::body_cache::substituted_mir;
use crate::common::storage::{ClosureInfoStorage, FunctionInfoStorage};
use crate::common::FunctionInfo;
use crate::important::ImportantLocals;

fn analyze_item<'tcx>(
    item: &FunctionInfo<'tcx>,
    important_locals: ImportantLocals,
    passing_calls_ref: &mut Vec<FunctionWithMetadata<'tcx>>,
    failing_calls_ref: &mut Vec<FunctionWithMetadata<'tcx>>,
    deps: &mut HashSet<String>,
    storage: &FunctionInfoStorage<'tcx>,
    allowlist: &Vec<Regex>,
    trusted_stdlib: &Vec<Regex>,
    tcx: TyCtxt<'tcx>,
) -> bool {
    if let Some(instance) = item.instance() {
        let body = substituted_mir(&instance, tcx);
        deps.extend(compute_deps_for_body(body, tcx).into_iter());
    }

    let is_trusted = {
        let def_path_str = format!("{:?}", item.def_id());
        let trusted_stdlib_member = trusted_stdlib.iter().any(|lib| lib.is_match(&def_path_str));
        let self_ty = item.instance().and_then(|instance| {
            let body = substituted_mir(&instance, tcx);
            body.var_debug_info
                .iter()
                .find(|dbg_info| dbg_info.name == Symbol::intern("self"))
                .and_then(|self_dbg_info| match self_dbg_info.value {
                    VarDebugInfoContents::Place(place) => Some(place),
                    _ => None,
                })
                .and_then(|self_place| Some(self_place.ty(&body, tcx).ty))
        });
        let has_immut_self_ref = self_ty
            .and_then(|self_ty| {
                Some(
                    self_ty
                        .ref_mutability()
                        .and_then(|mutability| Some(mutability == Mutability::Not))
                        .unwrap_or(false),
                )
            })
            .unwrap_or(false);
        trusted_stdlib_member && !has_immut_self_ref
    };

    let is_allowlisted = {
        let def_path_str = format!("{:?}", item.def_id());
        allowlist.iter().any(|lib| lib.is_match(&def_path_str))
    };

    let has_no_important_locals = important_locals.is_empty();

    if has_no_important_locals || is_allowlisted || is_trusted {
        let info_with_metadata = FunctionWithMetadata::new(
            item.to_owned(),
            important_locals.clone(),
            false,
            is_allowlisted,
            false,
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
                            deps,
                            storage,
                            allowlist,
                            trusted_stdlib,
                            tcx,
                        )
                    })
                    .collect_vec();
                Some(children_results.into_iter().all(|r| r))
            })
            .unwrap_or(false);

        if !has_unhandled_calls && !has_raw_pointer_deref && !has_transmute && has_no_leaking_calls
        {
            let info_with_metadata = FunctionWithMetadata::new(
                item.to_owned(),
                important_locals.clone(),
                has_raw_pointer_deref,
                is_allowlisted,
                has_transmute,
            );
            passing_calls_ref.push(info_with_metadata);
            true
        } else {
            let info_with_metadata = FunctionWithMetadata::new(
                item.to_owned(),
                important_locals.clone(),
                has_raw_pointer_deref,
                is_allowlisted,
                has_transmute,
            );
            failing_calls_ref.push(info_with_metadata);
            false
        }
    }
}

pub fn run<'tcx>(
    functions: FunctionInfoStorage<'tcx>,
    closures: ClosureInfoStorage<'tcx>,
    important_locals: ImportantLocals,
    annotated_pure: bool,
    allowlist: &Vec<Regex>,
    trusted_stdlib: &Vec<Regex>,
    tcx: TyCtxt<'tcx>,
) -> PurityAnalysisResult<'tcx> {
    let origin = functions.get_with_body(functions.origin()).unwrap();

    let mut passing_calls = vec![];
    let mut failing_calls = vec![];
    let mut deps = HashSet::new();

    let pure = analyze_item(
        origin,
        important_locals,
        &mut passing_calls,
        &mut failing_calls,
        &mut deps,
        &functions,
        allowlist,
        trusted_stdlib,
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
            deps,
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
            deps,
        )
    }
}
