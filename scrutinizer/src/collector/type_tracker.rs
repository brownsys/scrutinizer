use itertools::Itertools;
use rustc_middle::mir::{tcx::PlaceTy, Body, Local, Place, PlaceElem};
use rustc_middle::ty::TyCtxt;
use rustc_mir_dataflow::{fmt::DebugWithContext, JoinSemiLattice};
use rustc_span::def_id::DefId;
use rustc_utils::PlaceExt;
use std::collections::HashMap;

use super::normalized_place::NormalizedPlace;
use super::propagate::apply_fresh_projection;
use super::tracked_ty::TrackedTy;
use super::type_collector::TypeCollector;
use super::upvar_tracker::TrackedUpvars;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeTracker<'tcx> {
    pub places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>,
}

impl<'tcx> TypeTracker<'tcx> {
    pub fn new(places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>) -> Self {
        TypeTracker { places }
    }
    pub fn propagate(
        &mut self,
        place: &NormalizedPlace<'tcx>,
        ty: &TrackedTy<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) {
        place
            .interior_paths(tcx, body, def_id)
            .into_iter()
            .map(|interior_path| {
                let interior_path = NormalizedPlace::from_place(&interior_path, tcx, def_id);
                let projection_tail = interior_path
                    .projection
                    .iter()
                    .skip(place.projection.len())
                    .collect_vec();
                let transformed_ty = ty.map(|ty| {
                    let initial_place = PlaceTy::from_ty(ty);
                    projection_tail
                        .iter()
                        .fold(initial_place, |place_ty, projection_elem| {
                            apply_fresh_projection(&place_ty, projection_elem, tcx)
                        })
                        .ty
                });
                (interior_path, transformed_ty)
            })
            .for_each(|(interior_path, transformed_ty)| {
                let mut_place = self.places.get_mut(&interior_path).unwrap();
                mut_place.join(&transformed_ty);
            });
    }

    pub fn augment_closure_with_upvars(
        &mut self,
        upvars: &TrackedUpvars<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) {
        let closure_place = Place::make(Local::from_usize(1), &[PlaceElem::Deref], tcx);
        let closure_interior_places = closure_place.interior_places(tcx, body, def_id);
        upvars
            .upvars
            .iter()
            .zip(closure_interior_places.iter().skip(1))
            .for_each(|(upvar, interior_place)| {
                let normalized_interior_place =
                    NormalizedPlace::from_place(interior_place, tcx, def_id);
                self.places
                    .get_mut(&normalized_interior_place)
                    .unwrap()
                    .join(upvar);
                self.propagate(&normalized_interior_place, upvar, body, def_id, tcx);
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
        let place = NormalizedPlace::from_place(&Place::from_local(arg_local, tcx), tcx, def_id);
        self.places.get_mut(&place).unwrap().join(tracked_ty);
        self.propagate(&place, tracked_ty, body, def_id, tcx);
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
