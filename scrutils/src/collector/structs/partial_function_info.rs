use itertools::Itertools;
use log::trace;
use rustc_hir::def_id::DefId;
use rustc_middle::ty::{self, TyCtxt};
use std::iter::once;

use crate::common::storage::ClosureInfoStorageRef;
use crate::common::{ArgTys, ClosureInfo, TrackedTy};

fn extract_arg_tys<'tcx>(instance: ty::Instance<'tcx>, tcx: TyCtxt<'tcx>) -> ArgTys<'tcx> {
    let ty = instance.ty(tcx, ty::ParamEnv::reveal_all());
    match ty.kind() {
        ty::FnDef(_, _) => {
            let sig = tcx
                .fn_sig(instance.def_id())
                .subst(tcx, instance.substs)
                .skip_binder();
            let arg_tys = sig
                .inputs()
                .iter()
                .map(|ty| TrackedTy::from_ty(ty.to_owned()))
                .collect();
            ArgTys::new(arg_tys)
        }
        ty::Closure(_, substs) => {
            let closure_substs = substs.as_closure();
            let sig = closure_substs.sig().skip_binder();
            assert!(sig.inputs().len() == 1);
            let sig_tys = sig
                .inputs()
                .iter()
                .map(|ty| TrackedTy::from_ty(ty.to_owned()).spread_tuple())
                .flatten();
            let arg_tys = once(TrackedTy::from_ty(
                tcx.mk_imm_ref(tcx.mk_region_from_kind(ty::ReErased), ty),
            ))
            .chain(sig_tys)
            .collect();
            ArgTys::new(arg_tys)
        }
        _ => panic!("argument extraction from {:?} is unsupported", instance),
    }
}

#[derive(Clone, Debug)]
pub enum PartialFunctionInfo<'tcx> {
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

impl<'tcx> PartialFunctionInfo<'tcx> {
    pub fn new_function(instance: ty::Instance<'tcx>, tracked_args: ArgTys<'tcx>) -> Self {
        Self::Function {
            instance,
            tracked_args,
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

    pub fn is_closure(&self) -> bool {
        match self {
            PartialFunctionInfo::Closure { .. } => true,
            _ => false,
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

    pub fn expect_closure(&self) -> &ClosureInfo<'tcx> {
        match self {
            PartialFunctionInfo::Closure { closure_info, .. } => closure_info,
            _ => panic!("expect_closure_info called on {:?}", self),
        }
    }

    fn assemble(
        instance: ty::Instance<'tcx>,
        arg_tys: &ArgTys<'tcx>,
        closure_info_storage: ClosureInfoStorageRef<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Result<PartialFunctionInfo<'tcx>, String> {
        let inferred_args = if tcx.is_closure(instance.def_id()) {
            arg_tys.as_closure()
        } else {
            arg_tys.to_owned()
        };
        let provided_args = extract_arg_tys(instance, tcx);
        let merged_arg_tys = ArgTys::merge(inferred_args, provided_args);
        if tcx.is_closure(instance.def_id()) {
            match closure_info_storage.borrow().get(&instance.def_id()) {
                Some(closure_info) => Ok(PartialFunctionInfo::new_closure(
                    instance,
                    merged_arg_tys,
                    closure_info.to_owned(),
                )),
                None => {
                    return Err(format!(
                        "did not find a closure {:?} inside closure storage",
                        instance.def_id()
                    ))
                }
            }
        } else {
            Ok(PartialFunctionInfo::new_function(instance, merged_arg_tys))
        }
    }

    fn find_plausible_instances(
        def_id: DefId,
        arg_tys: &ArgTys<'tcx>,
        substs: ty::SubstsRef<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Vec<ty::Instance<'tcx>> {
        let generic_tys = tcx
            .fn_sig(def_id)
            .subst_identity()
            .inputs()
            .skip_binder()
            .to_vec();
        // Generate all plausible substitutions for each generic.
        (0..substs.len() as u32)
            .map(|subst_index| {
                let plausible_substs = arg_tys.extract_substs(&generic_tys, subst_index);
                plausible_substs
            })
            // Explore all possible combinations.
            .multi_cartesian_product()
            // Filter valid substitutions.
            .filter_map(|substs| Self::try_substitute_generics(def_id, substs, tcx))
            .collect()
    }

    fn try_substitute_generics(
        def_id: DefId,
        substs: Vec<ty::GenericArg<'tcx>>,
        tcx: TyCtxt<'tcx>,
    ) -> Option<ty::Instance<'tcx>> {
        // Check if every substitution is a type.
        let new_substs: ty::SubstsRef = tcx.mk_substs(substs.as_slice());
        let new_instance =
            ty::Instance::resolve(tcx, ty::ParamEnv::reveal_all(), def_id, new_substs).unwrap();
        new_instance
    }

    // Try resolving partial function data to full function data.
    pub fn resolve(
        &self,
        def_id: DefId,
        substs: ty::SubstsRef<'tcx>,
        arg_tys: &ArgTys<'tcx>,
        closure_info_storage: ClosureInfoStorageRef<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Result<Vec<PartialFunctionInfo<'tcx>>, String> {
        // Resolve function instances that need to be analyzed.
        let maybe_instance =
            ty::Instance::resolve(tcx, ty::ParamEnv::reveal_all(), def_id, substs).unwrap();

        let def_id = match maybe_instance {
            Some(instance) => instance.def_id(),
            None => def_id,
        };

        let fns = if tcx.is_mir_available(def_id) {
            vec![PartialFunctionInfo::assemble(
                maybe_instance.unwrap(),
                &arg_tys,
                closure_info_storage.clone(),
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
                .map(|instance| {
                    PartialFunctionInfo::assemble(
                        instance,
                        &arg_tys,
                        closure_info_storage.clone(),
                        tcx,
                    )
                })
                .collect();
            assembled_callees
        };

        fns.into_iter()
            .map(|fn_data| {
                let fn_data = fn_data?;
                if !tcx.is_closure(fn_data.instance().def_id()) {
                    let resolved_instance = self.substitute(fn_data.instance().to_owned(), tcx);
                    Ok(PartialFunctionInfo::new_function(
                        resolved_instance,
                        fn_data.tracked_args().to_owned(),
                    ))
                } else {
                    match closure_info_storage
                        .borrow()
                        .get(&fn_data.instance().def_id())
                    {
                        Some(upvars) => {
                            let resolved_instance = upvars.extract_instance(tcx);
                            Ok(PartialFunctionInfo::new_closure(
                                resolved_instance,
                                fn_data.tracked_args().to_owned(),
                                fn_data.expect_closure().to_owned(),
                            ))
                        }
                        None => Err(format!(
                            "closure {:?} not inside the storage",
                            fn_data.instance().def_id()
                        )),
                    }
                }
            })
            .collect()
    }

    pub fn substitute<T: ty::TypeFoldable<TyCtxt<'tcx>>>(&self, t: T, tcx: TyCtxt<'tcx>) -> T {
        self.instance()
            .subst_mir_and_normalize_erasing_regions(tcx, ty::ParamEnv::reveal_all(), t)
    }
}
