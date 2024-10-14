use itertools::Itertools;
use log::warn;
use rustc_middle::ty::{self, Ty};

use crate::common::TrackedTy;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgTys<'tcx> {
    arg_tys: Vec<TrackedTy<'tcx>>,
}

impl<'tcx> ArgTys<'tcx> {
    pub fn new(arg_tys: Vec<TrackedTy<'tcx>>) -> Self {
        ArgTys { arg_tys }
    }

    pub fn as_closure(&self) -> Self {
        let mut closure_arg_tys = vec![self.arg_tys[0].clone()];
        closure_arg_tys.extend(self.arg_tys[1].spread_tuple().into_iter());
        ArgTys {
            arg_tys: closure_arg_tys,
        }
    }

    pub fn as_vec(&self) -> &Vec<TrackedTy<'tcx>> {
        &self.arg_tys
    }

    pub fn merge(inferred_args: ArgTys<'tcx>, provided_args: ArgTys<'tcx>) -> ArgTys<'tcx> {
        if inferred_args.arg_tys.len() != provided_args.arg_tys.len() {
            warn!("inferred argument length != provided argument length; inferred_args={:?}; provided_args={:?}", inferred_args, provided_args);
            return ArgTys {
                arg_tys: inferred_args.arg_tys,
            };
        }
        let merged_arg_tys = inferred_args
            .arg_tys
            .into_iter()
            .zip(provided_args.arg_tys.into_iter())
            .map(|(inferred, provided)| match provided {
                TrackedTy::Present(..) => provided,
                TrackedTy::Erased(..) => inferred,
            })
            .collect_vec();
        ArgTys {
            arg_tys: merged_arg_tys,
        }
    }

    pub fn extract_substs(
        &self,
        generic_tys: &Vec<Ty<'tcx>>,
        subst_index: u32,
    ) -> Vec<ty::GenericArg<'tcx>> {
        generic_tys
            .iter()
            // Iterate over generic and real type simultaneously.
            .zip(self.as_vec().iter())
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
}
