use itertools::Itertools;
use log::trace;
use rustc_hir::def_id::DefId;
use rustc_middle::ty::{self, Ty, TyCtxt};

use super::arg_tys::ArgTys;
use super::closure_info::{ClosureInfo, ClosureInfoStorageRef};
use super::instance_ext::InstanceExt;

#[derive(Clone, Debug)]
pub enum Callee<'tcx> {
    Function {
        instance: ty::Instance<'tcx>,
        tracked_args: ArgTys<'tcx>,
    },
    Closure {
        instance: ty::Instance<'tcx>,
        tracked_args: ArgTys<'tcx>,
        closure_info: ClosureInfo<'tcx>,
    },
}

impl<'tcx> Callee<'tcx> {
    pub fn new_function(instance: ty::Instance<'tcx>, tracked_args: ArgTys<'tcx>) -> Self {
        Self::Function {
            instance,
            tracked_args,
        }
    }

    pub fn is_function(&self) -> bool {
        match self {
            Callee::Function { .. } => true,
            _ => false,
        }
    }

    pub fn is_closure(&self) -> bool {
        match self {
            Callee::Closure { .. } => true,
            _ => false,
        }
    }

    pub fn new_closure(
        instance: ty::Instance<'tcx>,
        tracked_args: ArgTys<'tcx>,
        closure_info: ClosureInfo<'tcx>,
    ) -> Self {
        Self::Closure {
            instance,
            tracked_args,
            closure_info,
        }
    }

    pub fn instance(&self) -> &ty::Instance<'tcx> {
        match self {
            Self::Function { instance, .. } | Self::Closure { instance, .. } => instance,
        }
    }

    pub fn tracked_args(&self) -> &ArgTys<'tcx> {
        match self {
            Self::Function { tracked_args, .. } | Self::Closure { tracked_args, .. } => {
                tracked_args
            }
        }
    }

    pub fn expect_closure_info(&self) -> &ClosureInfo<'tcx> {
        match self {
            Callee::Closure { closure_info, .. } => closure_info,
            _ => panic!("no upvars associated with function {:?}", self),
        }
    }

    fn assemble_callee(
        instance: ty::Instance<'tcx>,
        substs: ty::SubstsRef<'tcx>,
        arg_tys: &ArgTys<'tcx>,
        closure_info_storage: ClosureInfoStorageRef<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Callee<'tcx> {
        let inferred_args = if tcx.is_closure(instance.def_id()) {
            arg_tys.as_closure()
        } else {
            arg_tys.to_owned()
        };
        let provided_args = instance.arg_tys(tcx);
        let merged_arg_tys = ArgTys::merge(inferred_args, provided_args);

        if tcx.is_closure(instance.def_id()) {
            let closure_info_storage = closure_info_storage.borrow();
            match closure_info_storage.get(&instance.def_id()) {
                Some(upvars) => Callee::new_closure(instance, merged_arg_tys, upvars.to_owned()),
                None => panic!(
                    "did not find a closure {:?} inside closure storage",
                    instance.def_id()
                ),
            }
        } else {
            Callee::new_function(instance, merged_arg_tys)
        }
    }

    // Try resolving partial function data to full function data.
    pub fn resolve(
        def_id: DefId,
        substs: ty::SubstsRef<'tcx>,
        arg_tys: &ArgTys<'tcx>,
        upvar_tracker: ClosureInfoStorageRef<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Vec<Callee<'tcx>> {
        // Resolve function instances that need to be analyzed.
        let maybe_instance =
            ty::Instance::resolve(tcx, ty::ParamEnv::reveal_all(), def_id, substs).unwrap();

        let def_id = match maybe_instance {
            Some(instance) => instance.def_id(),
            None => def_id,
        };

        let fns = if tcx.is_mir_available(def_id) {
            vec![Callee::assemble_callee(
                maybe_instance.unwrap(),
                substs,
                &arg_tys,
                upvar_tracker.clone(),
                tcx,
            )]
        } else {
            // Extract all plausible instances if body is unavailable.
            let plausible_instances = Self::find_plausible_instances(def_id, &arg_tys, substs, tcx);
            trace!(
                "finding plausible instances for def_id={:?}, arg_tys={:?}, substs={:?}, instances={:?}",
                def_id, arg_tys, substs, plausible_instances
            );
            let assembled_callees = plausible_instances
                .into_iter()
                .map(|(instance, substs)| {
                    Callee::assemble_callee(instance, substs, &arg_tys, upvar_tracker.clone(), tcx)
                })
                .collect();
            assembled_callees
        };
        fns
    }

    fn find_plausible_instances(
        def_id: DefId,
        arg_tys: &ArgTys<'tcx>,
        substs: ty::SubstsRef<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Vec<(ty::Instance<'tcx>, ty::SubstsRef<'tcx>)> {
        let generic_tys = tcx
            .fn_sig(def_id)
            .subst_identity()
            .inputs()
            .skip_binder()
            .to_vec();
        // Generate all plausible substitutions for each generic.
        (0..substs.len())
            .map(|subst_index| {
                let plausible_substs =
                    Self::find_plausible_substs_for(arg_tys, &generic_tys, subst_index as u32);
                plausible_substs
            })
            // Explore all possible combinations.
            .multi_cartesian_product()
            // Filter valid substitutions.
            .filter_map(|substs| Self::try_substitute_generics(def_id, substs, tcx))
            .collect()
    }

    fn find_plausible_substs_for(
        concrete_tys: &ArgTys<'tcx>,
        generic_tys: &Vec<Ty<'tcx>>,
        subst_index: u32,
    ) -> Vec<ty::GenericArg<'tcx>> {
        generic_tys
            .iter()
            // Iterate over generic and real type simultaneously.
            .zip(concrete_tys.as_vec().iter())
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
        let new_instance =
            ty::Instance::resolve(tcx, ty::ParamEnv::reveal_all(), def_id, new_substs).unwrap();
        new_instance.and_then(|instance| Some((instance, new_substs)))
    }
}
