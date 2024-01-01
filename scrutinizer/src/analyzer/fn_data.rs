use rustc_middle::mir::{Local, Location, Operand};
use rustc_middle::ty::{self, Ty, TyCtxt};

use flowistry::indexed::impls::LocationOrArg;
use flowistry::infoflow::Direction;

use crate::vartrack::compute_dependent_locals;

use super::arg_ty::ArgTy;
use super::important_locals::ImportantLocals;
use super::ty_ext::TyExt;

use itertools::Itertools;

pub struct FnData<'tcx> {
    arg_tys: Vec<ArgTy<'tcx>>,
    instance: ty::Instance<'tcx>,
    important_locals: ImportantLocals,
}

impl<'tcx> FnData<'tcx> {
    pub fn new(
        arg_tys: Vec<ArgTy<'tcx>>,
        instance: ty::Instance<'tcx>,
        important_locals: ImportantLocals,
    ) -> Self {
        Self {
            arg_tys,
            instance,
            important_locals,
        }
    }
    pub fn important_locals(&self) -> &ImportantLocals {
        &self.important_locals
    }
    pub fn get_arg_tys(&self) -> &Vec<ArgTy<'tcx>> {
        &self.arg_tys
    }
    pub fn get_instance(&self) -> &ty::Instance<'tcx> {
        &self.instance
    }
    // Call to Flowistry to calculate dependencies for argument `arg` found at location `location`.
    pub fn deps_for(
        &self,
        arg: &Operand<'tcx>,
        location: &Location,
        tcx: TyCtxt<'tcx>,
    ) -> Vec<Ty<'tcx>> {
        let def_id = self.instance.def_id();
        let backward_deps = arg
            .place()
            .and_then(|place| {
                let targets = vec![vec![(place, LocationOrArg::Location(location.to_owned()))]];
                Some(compute_dependent_locals(
                    tcx,
                    def_id,
                    targets,
                    Direction::Backward,
                ))
            })
            .unwrap_or(vec![]);
        // Retrieve backwards dependencies' types.
        backward_deps
            .into_iter()
            .map(|local| self.subtypes_for(local, tcx))
            .flatten()
            .unique()
            .collect()
    }

    // Merge subtypes for a local if it is an argument, skip intermediate erased types.
    fn subtypes_for(&self, local: Local, tcx: TyCtxt<'tcx>) -> Vec<Ty<'tcx>> {
        let mut arg_influences = if local.index() != 0 && local.index() <= self.arg_tys.len() {
            match self.arg_tys[local.index() - 1] {
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
        let def_id = self.instance.def_id();
        let body = tcx.optimized_mir(def_id);
        let local_ty = body.local_decls[local].ty;
        if !local_ty.contains_trait() {
            arg_influences.push(local_ty)
        }
        arg_influences
    }
}
