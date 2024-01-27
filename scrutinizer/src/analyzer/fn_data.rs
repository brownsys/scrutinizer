use itertools::Itertools;
use log::debug;
use rustc_hir::def_id::DefId;
use rustc_middle::ty::{self, Ty, TyCtxt};

use super::instance_ext::InstanceExt;
use super::tracked_ty::TrackedTy;
use super::upvar_tracker::{TrackedUpvars, UpvarTrackerRef};

#[derive(Clone, Debug)]
pub struct FnData<'tcx> {
    instance: ty::Instance<'tcx>,
    tracked_args: Vec<TrackedTy<'tcx>>,
    tracked_upvars: Option<TrackedUpvars<'tcx>>,
}

impl<'tcx> FnData<'tcx> {
    pub fn new(
        instance: ty::Instance<'tcx>,
        tracked_args: Vec<TrackedTy<'tcx>>,
        tracked_upvars: Option<TrackedUpvars<'tcx>>,
    ) -> Self {
        Self {
            instance,
            tracked_args,
            tracked_upvars,
        }
    }
    pub fn instance(&self) -> &ty::Instance<'tcx> {
        &self.instance
    }
    pub fn tracked_args(&self) -> &Vec<TrackedTy<'tcx>> {
        &self.tracked_args
    }
    pub fn upvars(&self) -> &Option<TrackedUpvars<'tcx>> {
        &self.tracked_upvars
    }
    fn merge_args(
        inferred_args: Vec<TrackedTy<'tcx>>,
        provided_args: Vec<TrackedTy<'tcx>>,
    ) -> Vec<TrackedTy<'tcx>> {
        inferred_args
            .into_iter()
            .zip(provided_args.into_iter())
            .map(|(inferred, provided)| match provided {
                TrackedTy::Present(..) => provided,
                TrackedTy::Erased(..) => inferred,
            })
            .collect_vec()
    }

    pub fn generate_new_fn_data(
        instance: ty::Instance<'tcx>,
        substs: ty::SubstsRef<'tcx>,
        arg_tys: &Vec<TrackedTy<'tcx>>,
        upvar_tracker: UpvarTrackerRef<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> FnData<'tcx> {
        let inferred_args = if tcx.is_closure(instance.def_id()) {
            arg_tys[1].spread_tuple()
        } else {
            arg_tys.clone()
        };
        let provided_args = instance.arg_tys(tcx);
        let merged_arg_tys = Self::merge_args(inferred_args, provided_args);
        let upvars = if tcx.is_closure(instance.def_id()) {
            let closure_ty = substs[0].expect_ty();
            let upvar_tracker = upvar_tracker.borrow();
            let upvars = upvar_tracker.get(&closure_ty);
            upvars.and_then(|upvars| Some(upvars.to_owned()))
        } else {
            None
        };
        FnData::new(instance, merged_arg_tys, upvars)
    }

    // Try resolving partial function data to full function data.
    pub fn resolve(
        def_id: DefId,
        substs: ty::SubstsRef<'tcx>,
        arg_tys: &Vec<TrackedTy<'tcx>>,
        upvar_tracker: UpvarTrackerRef<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Vec<FnData<'tcx>> {
        if arg_tys.iter().any(|arg_ty| arg_ty.poisoned()) {
            debug!(
                "encountered a poisoned argument at {:?} {:?}",
                def_id, arg_tys
            );
            return vec![];
        }

        // Resolve function instances that need to be analyzed.
        let maybe_instance =
            ty::Instance::resolve(tcx, ty::ParamEnv::reveal_all(), def_id, substs).unwrap();

        let def_id = match maybe_instance {
            Some(instance) => instance.def_id(),
            None => def_id,
        };

        let fns = if tcx.is_mir_available(def_id) {
            vec![FnData::generate_new_fn_data(
                maybe_instance.unwrap(),
                substs,
                &arg_tys,
                upvar_tracker.clone(),
                tcx,
            )]
        } else {
            // Extract all plausible instances if body is unavailable.
            let plausible_instances = Self::find_plausible_instances(def_id, &arg_tys, substs, tcx);
            plausible_instances
                .into_iter()
                .map(|(instance, substs)| {
                    FnData::generate_new_fn_data(
                        instance,
                        substs,
                        &arg_tys,
                        upvar_tracker.clone(),
                        tcx,
                    )
                })
                .collect()
        };
        fns
    }

    fn find_plausible_instances(
        def_id: DefId,
        arg_tys: &Vec<TrackedTy<'tcx>>,
        substs: ty::SubstsRef<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Vec<(ty::Instance<'tcx>, ty::SubstsRef<'tcx>)> {
        // Short-circut on poisoned arguments.
        let generic_tys = tcx
            .fn_sig(def_id)
            .subst_identity()
            .inputs()
            .skip_binder()
            .to_vec();
        // Generate all plausible substitutions for each generic.
        (0..substs.len())
            .map(|subst_index| {
                Self::find_plausible_substs_for(arg_tys, &generic_tys, subst_index as u32)
            })
            // Explore all possible combinations.
            .multi_cartesian_product()
            // Filter valid substitutions.
            .filter_map(|substs| Self::try_substitute_generics(def_id, substs, tcx))
            .collect()
    }

    fn find_plausible_substs_for(
        concrete_tys: &Vec<TrackedTy<'tcx>>,
        generic_tys: &Vec<Ty<'tcx>>,
        subst_index: u32,
    ) -> Vec<ty::GenericArg<'tcx>> {
        generic_tys
            .iter()
            // Iterate over generic and real type simultaneously.
            .zip(concrete_tys.iter())
            .map(|(generic_ty, concrete_ty)| {
                // Retrieve all possible substitutions.
                let subst_tys = concrete_ty.into_vec();
                let valid_substs = subst_tys
                    .into_iter()
                    .filter_map(|subst_ty| {
                        // Peel both types simultaneously until type parameter appears.
                        generic_ty
                            .walk()
                            .zip(subst_ty.walk())
                            .find(|(generic_ty, _)| match generic_ty.unpack() {
                                ty::GenericArgKind::Type(ty) => ty.is_param(subst_index),
                                _ => false,
                            })
                            .and_then(|(_, subst_to)| Some(subst_to))
                    })
                    .collect_vec();
                valid_substs
            })
            .flatten()
            .collect()
    }

    fn try_substitute_generics(
        def_id: DefId,
        substs: Vec<ty::GenericArg<'tcx>>,
        tcx: TyCtxt<'tcx>,
    ) -> Option<(ty::Instance<'tcx>, ty::SubstsRef<'tcx>)> {
        // Check if every substitution is a type.
        let new_substs: ty::SubstsRef = tcx.mk_substs(substs.as_slice());
        // TODO: Sometimes this panics if generics are not properly bound.
        let new_instance =
            ty::Instance::resolve(tcx, ty::ParamEnv::reveal_all(), def_id, new_substs).unwrap();
        new_instance.and_then(|instance| {
            if tcx.is_mir_available(instance.def_id()) {
                Some((instance, new_substs))
            } else {
                None
            }
        })
    }
}
