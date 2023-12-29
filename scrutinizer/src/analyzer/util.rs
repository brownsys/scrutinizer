use super::arg_ty::ArgTy;
use crate::vartrack::compute_dependent_locals;

use rustc_hir::def_id::DefId;
use rustc_middle::mir::{Body, Local, Location, Operand, Place};
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_utils::PlaceExt;

use flowistry::indexed::impls::LocationOrArg;
use flowistry::infoflow::Direction;

use itertools::Itertools;

pub(super) fn extract_deps<'tcx>(
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
        .map(|local| extract_subtypes(local, outer_body, outer_arg_tys))
        .flatten()
        .collect()
}

fn contains_trait(ty: Ty) -> bool {
    ty.walk().any(|ty| match ty.unpack() {
        ty::GenericArgKind::Type(ty) => ty.is_trait(),
        _ => false,
    })
}

fn extract_subtypes<'tcx>(
    local: Local,
    body: &Body<'tcx>,
    arg_tys: &Vec<ArgTy<'tcx>>,
) -> Vec<Ty<'tcx>> {
    let mut arg_influences = if local.index() != 0 && local.index() <= arg_tys.len() {
        match arg_tys[local.index() - 1] {
            ArgTy::Simple(ty) => vec![ty],
            ArgTy::Erased(ty, ref influences) => {
                if influences.is_empty() {
                    vec![ty]
                } else {
                    influences.to_owned()
                }
            }
        }
    } else {
        vec![]
    };

    let local_ty = body.local_decls[local].ty;
    if !contains_trait(local_ty) {
        arg_influences.push(local_ty)
    }
    arg_influences
}

pub(super) fn find_plausible_substs<'tcx>(
    def_id: DefId,
    concrete_tys: &Vec<ArgTy<'tcx>>,
    substs: ty::subst::SubstsRef<'tcx>,
    tcx: TyCtxt<'tcx>,
) -> Vec<ty::Instance<'tcx>> {
    let generic_tys = tcx
        .fn_sig(def_id)
        .subst_identity()
        .inputs()
        .skip_binder()
        .to_vec();
    (0..substs.len())
        .map(|subst_index| {
            find_plausible_substs_for(&concrete_tys, &generic_tys, subst_index as u32)
        })
        .multi_cartesian_product()
        .filter_map(|substs| substitute_genetics(def_id, substs, tcx))
        .collect()
}

fn find_plausible_substs_for<'tcx>(
    concrete_tys: &Vec<ArgTy<'tcx>>,
    generic_tys: &Vec<Ty<'tcx>>,
    subst_index: u32,
) -> Vec<ty::GenericArg<'tcx>> {
    generic_tys
        .into_iter()
        .zip(concrete_tys.into_iter())
        .map(|(generic_ty, concrete_ty)| {
            let subst_tys = match concrete_ty {
                ArgTy::Simple(ty) => vec![ty],
                ArgTy::Erased(ty, subst_tys) => subst_tys.into_iter().chain([ty]).collect(),
            };
            let valid_substs: Vec<ty::GenericArg<'tcx>> = subst_tys
                .into_iter()
                .filter_map(|subst_ty| {
                    generic_ty
                        .walk()
                        .zip(subst_ty.walk())
                        .find(|(generic_ty, _)| match generic_ty.unpack() {
                            ty::GenericArgKind::Type(ty) => ty.is_param(subst_index),
                            _ => false,
                        })
                        .and_then(|(_, subst_to)| Some(subst_to))
                })
                .collect();
            valid_substs
        })
        .flatten()
        .collect()
}

fn substitute_genetics<'tcx>(
    def_id: DefId,
    substs: Vec<ty::GenericArg<'tcx>>,
    tcx: TyCtxt<'tcx>,
) -> Option<ty::Instance<'tcx>> {
    let is_ty = substs.iter().all(|subst| match subst.unpack() {
        ty::GenericArgKind::Type(_) => true,
        _ => false,
    });
    let contains_params = substs.iter().all(|subst| {
        subst.walk().any(|ty| match ty.unpack() {
            ty::GenericArgKind::Type(ty) => match ty.kind() {
                ty::Param(_) => true,
                _ => false,
            },
            _ => false,
        })
    });
    if is_ty && !contains_params {
        let new_substs = tcx.mk_substs(substs.as_slice());
        let new_instance =
            tcx.resolve_instance(ty::ParamEnv::reveal_all().and((def_id, new_substs)));
        new_instance.unwrap().and_then(|instance| {
            if tcx.is_mir_available(instance.def_id()) {
                Some(instance)
            } else {
                None
            }
        })
    } else {
        None
    }
}

pub fn calculate_important_locals(
    new_args: &Vec<Operand>,
    old_important_locals: &Vec<Local>,
    def_id: DefId,
    tcx: TyCtxt,
) -> Vec<Local> {
    if tcx.is_constructor(def_id) {
        return vec![];
    }
    let important_args: Vec<usize> = new_args
        .iter()
        .enumerate()
        .filter_map(|(i, arg)| {
            arg.place()
                .and_then(|place| place.as_local())
                .and_then(|local| {
                    if old_important_locals.contains(&local) {
                        // Need to add 1 because arguments' locals start with 1.
                        Some(i + 1)
                    } else {
                        None
                    }
                })
        })
        .collect();

    // Construct targets of the arguments.
    let important_arg_targets = vec![important_args
        .iter()
        .map(|arg| {
            let arg_local = Local::from_usize(*arg);
            let arg_place = Place::make(arg_local, &[], tcx);
            (arg_place, LocationOrArg::Arg(arg_local))
        })
        .collect()];

    // Compute new dependencies for all important args.
    compute_dependent_locals(tcx, def_id, important_arg_targets, Direction::Forward)
}
