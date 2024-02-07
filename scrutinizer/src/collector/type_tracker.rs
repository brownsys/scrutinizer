use log::trace;
use rustc_abi::FieldIdx;
use rustc_middle::mir::{Body, Local, Place, PlaceElem};
use rustc_middle::ty::TyCtxt;
use rustc_mir_dataflow::{fmt::DebugWithContext, JoinSemiLattice};
use rustc_span::def_id::DefId;
use rustc_utils::PlaceExt;
use std::collections::HashMap;

use super::normalized_place::NormalizedPlace;
use super::propagate::propagate;
use super::tracked_ty::TrackedTy;
use super::type_collector::TypeCollector;
use super::closure_info::ClosureInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeTracker<'tcx> {
    pub places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>,
}

impl<'tcx> TypeTracker<'tcx> {
    pub fn new(places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>) -> Self {
        TypeTracker { places }
    }

    pub fn augment_closure_with_upvars(
        &mut self,
        upvars: &ClosureInfo<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) {
        trace!("augmenting closure with upvars");
        upvars.upvars.iter().enumerate().for_each(|(i, upvar)| {
            let type_placeholder = tcx.types.usize;
            let upvar_place = Place::make(
                Local::from_usize(1),
                &[
                    PlaceElem::Deref,
                    PlaceElem::Field(FieldIdx::from_usize(i), type_placeholder),
                ],
                tcx,
            );
            let normalized_upvar_place = NormalizedPlace::from_place(&upvar_place, tcx, def_id);
            self.places
                .entry(normalized_upvar_place.clone())
                .and_modify(|place| {
                    place.join(upvar);
                })
                .or_insert(upvar.to_owned());
            propagate(self, &normalized_upvar_place, upvar, body, def_id, tcx);
        });
    }

    pub fn augment_with_args(
        &mut self,
        arg_local: Local,
        tracked_ty: &TrackedTy<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) {
        trace!("augmenting call with args");
        let place = NormalizedPlace::from_place(&Place::from_local(arg_local, tcx), tcx, def_id);
        self.places
            .entry(place.clone())
            .and_modify(|place| {
                place.join(tracked_ty);
            })
            .or_insert(tracked_ty.to_owned());
        propagate(self, &place, tracked_ty, body, def_id, tcx);
    }
}

impl<'tcx> DebugWithContext<TypeCollector<'tcx>> for TypeTracker<'tcx> {}

impl<'tcx> JoinSemiLattice for TypeTracker<'tcx> {
    fn join(&mut self, other: &Self) -> bool {
        let updated_places = other.places.iter().fold(false, |acc, (key, other_value)| {
            let self_value = self.places.get_mut(key).unwrap();
            let updated = self_value.join(other_value);
            acc || updated
        });
        updated_places
    }
}
