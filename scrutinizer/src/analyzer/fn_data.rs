use rustc_middle::mir::{Local, Location, Place};
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_utils::PlaceExt;

use flowistry::indexed::impls::LocationOrArg;
use flowistry::infoflow::Direction;

use crate::vartrack::compute_dependent_locals;

use super::arg_ty::ArgTy;
use super::important_locals::ImportantLocals;
use super::local_ty_collector::GetLocalTys;
use super::ret_collector::GetReturnSites;
use super::ty_ext::TyExt;

use itertools::Itertools;

#[derive(Debug)]
pub struct FnData<'tcx> {
    arg_tys: Vec<ArgTy<'tcx>>,
    instance: ty::Instance<'tcx>,
    important_locals: ImportantLocals,
    return_ty: Option<ArgTy<'tcx>>,
}

impl<'tcx> FnData<'tcx> {
    pub fn new(
        arg_tys: Vec<ArgTy<'tcx>>,
        instance: ty::Instance<'tcx>,
        important_locals: ImportantLocals,
        tcx: TyCtxt<'tcx>,
    ) -> Self {
        let mut fn_data = Self {
            arg_tys,
            instance,
            important_locals,
            return_ty: None,
        };
        let body = tcx.optimized_mir(fn_data.get_instance().def_id());
        let return_ty = body.return_ty();
        // Check whether argument type was erased.
        let return_ty = if return_ty.contains_erased() {
            let return_sites = body.get_return_sites();
            let ret_local = Local::from_usize(0);
            let ret_place = Place::make(ret_local, &[], tcx);
            let backward_types = return_sites
                .into_iter()
                .map(|location| fn_data.backward_deps_for(ret_place, &location, tcx))
                .flatten()
                .collect();
            ArgTy::Erased(return_ty, backward_types)
        } else {
            ArgTy::Simple(return_ty)
        };
        fn_data.return_ty = Some(return_ty);
        fn_data
    }
    pub fn important_locals(&self) -> &ImportantLocals {
        &self.important_locals
    }
    pub fn get_arg_tys(&self) -> &Vec<ArgTy<'tcx>> {
        &self.arg_tys
    }
    pub fn get_return_ty(&self) -> &ArgTy<'tcx> {
        self.return_ty.as_ref().unwrap()
    }
    pub fn get_instance(&self) -> &ty::Instance<'tcx> {
        &self.instance
    }
    // Call to Flowistry to calculate dependencies for argument `arg` found at location `location`.
    pub fn backward_deps_for(
        &self,
        place: Place<'tcx>,
        location: &Location,
        tcx: TyCtxt<'tcx>,
    ) -> Vec<Ty<'tcx>> {
        let def_id = self.instance.def_id();
        let backward_deps = {
            let targets = vec![vec![(place, LocationOrArg::Location(location.to_owned()))]];
            compute_dependent_locals(tcx, def_id, targets, Direction::Backward)
        };
        // Retrieve backwards dependencies' types.
        let deps_subtypes = backward_deps
            .into_iter()
            .map(|local| self.subtypes_for(local, tcx))
            .flatten()
            .unique()
            .collect();
        deps_subtypes
    }

    // Merge subtypes for a local if it is an argument, skip intermediate erased types.
    fn subtypes_for(&self, local: Local, tcx: TyCtxt<'tcx>) -> Vec<Ty<'tcx>> {
        let arg_influences = if local.index() != 0 && local.index() <= self.arg_tys.len() {
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
        let tys_for_local: Vec<Ty> = body
            .get_local_tys_for(tcx, local)
            .into_iter()
            .chain(vec![body.local_decls[local].ty])
            .collect();

        let non_erased_local_tys: Vec<Ty> = tys_for_local
            .into_iter()
            .filter(|ty| !ty.contains_erased())
            .collect();

        arg_influences
            .into_iter()
            .chain(non_erased_local_tys)
            .collect()
    }
}
