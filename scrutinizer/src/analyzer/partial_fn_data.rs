use rustc_hir::def_id::DefId;
use rustc_middle::mir::{Location, Operand};
use rustc_middle::ty::{self, Ty, TyCtxt};

use super::arg_ty::RefinedTy;
use super::fn_data::FnData;
use super::important_locals::ImportantLocals;
use super::instance_ext::InstanceExt;
use super::substs_ext::SubstsExt;

use itertools::Itertools;

#[derive(Debug)]
pub struct PartialFnData<'tcx> {
    def_id: DefId,
    substs: ty::SubstsRef<'tcx>,
    args: Vec<Operand<'tcx>>,
    arg_tys: Vec<RefinedTy<'tcx>>,
}

impl<'tcx> PartialFnData<'tcx> {
    pub fn new(
        def_id: DefId,
        substs: ty::SubstsRef<'tcx>,
        args: Vec<Operand<'tcx>>,
        location: &Location,
        outer_fn: &FnData<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Self {
        let arg_tys = args
            .iter()
            .map(|arg| RefinedTy::from_known_or_erased(arg, location, outer_fn, tcx))
            .collect();
        Self {
            def_id,
            substs,
            arg_tys,
            args,
        }
    }
    // Try resolving partial function data to full function data.
    pub fn try_resolve(
        &self,
        old_important_locals: &ImportantLocals,
        tcx: TyCtxt<'tcx>,
    ) -> Vec<FnData<'tcx>> {
        // Resolve function instances that need to be analyzed.
        let maybe_instance =
            ty::Instance::resolve(tcx, ty::ParamEnv::reveal_all(), self.def_id, self.substs)
                .unwrap();
        let def_id = match maybe_instance {
            Some(instance) => instance.def_id(),
            None => self.def_id,
        };
        let fns = if tcx.is_mir_available(def_id) {
            // Successfuly resolve full function data if MIR is available.
            let important_locals = old_important_locals.transition(&self.args, def_id, tcx);
            vec![FnData::new(
                self.arg_tys.clone(),
                maybe_instance.unwrap(),
                important_locals,
                tcx,
            )]
        } else {
            // Extract all plausible instances if body is unavailable.
            let plausible_instances = self.find_plausible_instances(def_id, tcx);
            if !plausible_instances.is_empty() {
                plausible_instances
                    .into_iter()
                    .map(|instance| {
                        let arg_tys = instance.arg_tys(tcx);
                        let important_locals =
                            old_important_locals.transition(&self.args, instance.def_id(), tcx);
                        FnData::new(arg_tys, instance, important_locals, tcx)
                    })
                    .collect()
            } else {
                // We are unable to verify the purity due to external reference or dynamic dispatch.
                return vec![];
            }
        };
        fns
    }

    fn find_plausible_instances(
        &self,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) -> Vec<ty::Instance<'tcx>> {
        // Short-circut on poisoned arguments.
        if self.arg_tys.iter().any(|arg_ty| arg_ty.is_poisoned()) {
            return vec![];
        }
        let generic_tys = tcx
            .fn_sig(def_id)
            .subst_identity()
            .inputs()
            .skip_binder()
            .to_vec();
        // Generate all plausible substitutions for each generic.
        (0..self.substs.len())
            .map(|subst_index| self.find_plausible_substs_for(&generic_tys, subst_index as u32))
            // Explore all possible combinations.
            .multi_cartesian_product()
            // Filter valid substitutions.
            .filter_map(|substs| self.try_substitute_generics(def_id, substs, tcx))
            .collect()
    }

    fn find_plausible_substs_for(
        &self,
        generic_tys: &Vec<Ty<'tcx>>,
        subst_index: u32,
    ) -> Vec<ty::GenericArg<'tcx>> {
        generic_tys
            .into_iter()
            // Iterate over generic and real type simultaneously.
            .zip(self.get_arg_tys().into_iter())
            .map(|(generic_ty, concrete_ty)| {
                // Retrieve all possible substitutions.
                let subst_tys = concrete_ty.into_vec();
                let valid_substs: Vec<ty::GenericArg<'tcx>> = subst_tys
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
                    .collect();
                valid_substs
            })
            .flatten()
            .collect()
    }

    fn try_substitute_generics(
        &self,
        def_id: DefId,
        substs: Vec<ty::GenericArg<'tcx>>,
        tcx: TyCtxt<'tcx>,
    ) -> Option<ty::Instance<'tcx>> {
        // Check if every substitution is a type.
        let new_substs: ty::SubstsRef = tcx.mk_substs(substs.as_slice());
        let param_env = ty::ParamEnv::reveal_all();

        if new_substs.maybe_invalid_for(def_id, param_env, tcx) {
            None
        } else {
            // Try substituting.
            let new_instance =
                tcx.resolve_instance(ty::ParamEnv::reveal_all().and((def_id, new_substs)));
            new_instance.unwrap().and_then(|instance| {
                if tcx.is_mir_available(instance.def_id()) {
                    Some(instance)
                } else {
                    None
                }
            })
        }
    }

    pub fn get_arg_tys(&self) -> Vec<RefinedTy<'tcx>> {
        self.arg_tys.clone()
    }
}
