use super::types::ArgTy;
use crate::vartrack::compute_dependent_locals;

use regex::Regex;

use rustc_hir::def_id::DefId;
use rustc_middle::mir::{Body, Local, Location, Operand};
use rustc_middle::ty::{subst::GenericArgKind, Instance, ParamEnv, Ty, TyCtxt, TyKind};

use flowistry::indexed::impls::LocationOrArg;
use flowistry::infoflow::Direction;

pub(super) fn is_type_erased_closure_call(def_id: DefId, tcx: TyCtxt) -> bool {
    // All possible closure shims that we need to analyze.
    let closure_shims = vec![
        Regex::new(r"core\[\w*\]::ops::function::FnMut::call_mut").unwrap(),
        Regex::new(r"core\[\w*\]::ops::function::FnOnce::call_once").unwrap(),
        Regex::new(r"core\[\w*\]::ops::function::Fn::call").unwrap(),
    ];
    let def_path_str = format!("{:?}", def_id);
    closure_shims.iter().any(|lib| lib.is_match(&def_path_str)) && !tcx.is_mir_available(def_id)
}

pub(super) fn extract_callable_influences<'tcx>(
    arg_ty: &ArgTy<'tcx>,
    tcx: TyCtxt<'tcx>,
) -> Result<Vec<Instance<'tcx>>, ()> {
    match arg_ty {
        ArgTy::Simple(_) => Ok(vec![]),
        ArgTy::WithCallableInfluences(_, influences) => {
            influences
                .iter()
                .fold(Ok(vec![]), |result, ty| match ty.kind() {
                    TyKind::Closure(def_id, substs) | TyKind::FnDef(def_id, substs) => {
                        match Instance::resolve(
                            tcx,
                            ParamEnv::reveal_all(),
                            def_id.to_owned(),
                            substs,
                        )
                        .unwrap()
                        {
                            Some(instance) => result.and_then(|mut v| {
                                v.push(instance);
                                Ok(v)
                            }),
                            None => Err(()),
                        }
                    }
                    _ => Err(()),
                })
        }
    }
}

pub(super) fn extract_callable_deps<'tcx>(
    arg: &Operand<'tcx>,
    location: &Location,
    outer_arg_tys: &Vec<ArgTy<'tcx>>,
    outer_def_id: DefId,
    outer_body: &Body<'tcx>,
    tcx: TyCtxt<'tcx>,
) -> Vec<Ty<'tcx>> {
    let backward_deps = arg
        .place()
        .and_then(|place| {
            let targets = vec![vec![(place, LocationOrArg::Location(location.to_owned()))]];
            Some(compute_dependent_locals(
                tcx,
                outer_def_id,
                targets,
                Direction::Backward,
            ))
        })
        .unwrap_or(vec![]);
    // Retrieve backwards dependencies' types.
    backward_deps
        .into_iter()
        .map(|local| extract_callable_subtypes(local, outer_body, outer_arg_tys))
        .flatten()
        .filter_map(extract_callable_subtype)
        .collect()
}

fn extract_callable_subtype(ty: Ty) -> Option<Ty> {
    ty.walk()
        .find(|ty| match ty.unpack() {
            GenericArgKind::Type(ty) => match ty.kind() {
                TyKind::FnDef(..)
                | TyKind::Closure(..)
                | TyKind::FnPtr(..)
                | TyKind::Dynamic(..) => true,
                _ => false,
            },
            _ => false,
        })
        .and_then(|generic_arg| Some(generic_arg.expect_ty()))
}

fn extract_determinate_callable_subtype(ty: Ty) -> Option<Ty> {
    ty.walk()
        .find(|ty| match ty.unpack() {
            GenericArgKind::Type(ty) => match ty.kind() {
                TyKind::FnDef(..) | TyKind::Closure(..) => true,
                _ => false,
            },
            _ => false,
        })
        .and_then(|generic_arg| Some(generic_arg.expect_ty()))
}

fn extract_callable_subtypes<'tcx>(
    local: Local,
    body: &Body<'tcx>,
    arg_tys: &Vec<ArgTy<'tcx>>,
) -> Vec<Ty<'tcx>> {
    let mut arg_influences = if local.index() != 0 && local.index() <= arg_tys.len() {
        match arg_tys[local.index() - 1] {
            ArgTy::Simple(ty) => extract_callable_subtype(ty)
                .and_then(|ty| Some(vec![ty]))
                .unwrap_or(vec![]),
            ArgTy::WithCallableInfluences(ty, ref influences) => {
                extract_determinate_callable_subtype(ty)
                    .and_then(|ty| Some(vec![ty]))
                    .unwrap_or(influences.to_owned())
            }
        }
    } else {
        vec![]
    };
    if let Some(arg) = extract_determinate_callable_subtype(body.local_decls[local].ty) {
        arg_influences.push(arg)
    }
    arg_influences
}
