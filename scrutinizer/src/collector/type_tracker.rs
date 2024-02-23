use itertools::Itertools;
use rustc_abi::FieldIdx;
use rustc_middle::mir::{tcx::PlaceTy, Body, Local, Operand, Place, PlaceElem};
use rustc_middle::ty::{Ty, TyCtxt};
use rustc_mir_dataflow::{fmt::DebugWithContext, JoinSemiLattice};
use rustc_span::def_id::DefId;
use rustc_utils::PlaceExt;
use serde::ser::SerializeStruct;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use super::arg_tys::ArgTys;
use super::closure_info::ClosureInfo;
use super::normalized_place::NormalizedPlace;
use super::project_fresh::project_fresh;
use super::tracked_ty::TrackedTy;
use super::type_collector::TypeCollector;

#[derive(Clone, Debug, Hash)]
pub struct Call<'tcx> {
    def_id: DefId,
    args: Vec<Operand<'tcx>>,
}

impl<'tcx> PartialEq for Call<'tcx> {
    fn eq(&self, other: &Self) -> bool {
        self.def_id == other.def_id && self.args == other.args
    }
}

impl<'tcx> Eq for Call<'tcx> {}

impl<'tcx> Call<'tcx> {
    pub fn new(def_id: DefId, args: Vec<Operand<'tcx>>) -> Self {
        Self { def_id, args }
    }
    pub fn args(&self) -> &Vec<Operand<'tcx>> {
        &self.args
    }
    pub fn def_id(&self) -> &DefId {
        &self.def_id
    }
}

impl<'tcx> Serialize for Call<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("Call", 2)?;
        state.serialize_field("def_id", format!("{:?}", self.def_id).as_str())?;
        state.serialize_field(
            "args",
            &self
                .args
                .iter()
                .map(|arg| format!("{:?}", arg))
                .collect_vec(),
        )?;
        state.end()
    }
}

#[derive(Debug, Clone)]
pub struct TypeTracker<'tcx> {
    places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>,
    calls: HashSet<Call<'tcx>>,
    unhandled: Vec<Ty<'tcx>>,
}

impl<'tcx> PartialEq for TypeTracker<'tcx> {
    fn eq(&self, other: &Self) -> bool {
        self.places == other.places
    }
}

impl<'tcx> Eq for TypeTracker<'tcx> {}

impl<'tcx> TypeTracker<'tcx> {
    pub fn new(places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>) -> Self {
        TypeTracker {
            places,
            calls: HashSet::new(),
            unhandled: vec![],
        }
    }

    pub fn get(&self, place: &NormalizedPlace<'tcx>) -> Option<&TrackedTy<'tcx>> {
        self.places.get(place)
    }

    pub fn places(&self) -> &HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>> {
        &self.places
    }

    pub fn augment_closure_with_upvars(
        &mut self,
        upvars: &ClosureInfo<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) {
        upvars.upvars.iter().enumerate().for_each(|(i, upvar)| {
            let type_placeholder = tcx.types.unit;
            let projection = if body.local_decls[Local::from_usize(1)].ty.is_ref() {
                vec![
                    PlaceElem::Deref,
                    PlaceElem::Field(FieldIdx::from_usize(i), type_placeholder),
                ]
            } else {
                vec![PlaceElem::Field(FieldIdx::from_usize(i), type_placeholder)]
            };
            let upvar_place = Place::make(Local::from_usize(1), projection.as_slice(), tcx);
            let normalized_upvar_place = NormalizedPlace::from_place(&upvar_place, tcx, def_id);
            self.places
                .entry(normalized_upvar_place.clone())
                .and_modify(|place| {
                    place.join(upvar);
                })
                .or_insert(upvar.to_owned());
            self.propagate(&normalized_upvar_place, upvar, body, def_id, tcx);
        });
    }

    pub fn augment_with_args(
        &mut self,
        arg_tys: &ArgTys<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) {
        arg_tys
            .as_vec()
            .iter()
            .enumerate()
            .skip(if tcx.is_closure(def_id) { 1 } else { 0 })
            .for_each(|(i, tracked_ty)| {
                let arg_local = Local::from_usize(i + 1);
                let place =
                    NormalizedPlace::from_place(&Place::from_local(arg_local, tcx), tcx, def_id);
                self.places
                    .entry(place.clone())
                    .and_modify(|place| {
                        place.join(tracked_ty);
                    })
                    .or_insert(tracked_ty.to_owned());
                self.propagate(&place, tracked_ty, body, def_id, tcx);
            })
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

                let variant_index = place.ty(body, tcx).variant_index;
                let transformed_ty = ty.map(|ty| {
                    let initial_place = PlaceTy { ty, variant_index };
                    projection_tail
                        .iter()
                        .fold(initial_place, |place_ty, projection_elem| {
                            project_fresh(&place_ty, projection_elem, tcx)
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

    pub fn update_with(
        &mut self,
        place: NormalizedPlace<'tcx>,
        tracked_ty: TrackedTy<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) {
        let place_ty_ref = self.places.get_mut(&place).unwrap();
        place_ty_ref.join(&tracked_ty);

        let place_ty = place_ty_ref.to_owned();
        self.propagate(&place, &place_ty, body, def_id, tcx);
    }

    pub fn return_type(
        &self,
        def_id: DefId,
        body: &Body<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx> {
        let normalized_return_place =
            NormalizedPlace::from_place(&Place::return_place(), tcx, def_id);

        let inferred_return_ty = self
            .places
            .get(&normalized_return_place)
            .unwrap()
            .to_owned();

        let provided_return_ty = TrackedTy::from_ty(normalized_return_place.ty(body, tcx).ty);

        match provided_return_ty {
            TrackedTy::Present(..) => provided_return_ty,
            TrackedTy::Erased(..) => inferred_return_ty,
        }
    }

    pub fn add_call(&mut self, call: Call<'tcx>) {
        self.calls.insert(call);
    }

    pub fn add_unhandled(&mut self, unhandled: Ty<'tcx>) {
        self.unhandled.push(unhandled);
    }

    pub fn calls(&self) -> &HashSet<Call<'tcx>> {
        &self.calls
    }

    pub fn unhandled(&self) -> &Vec<Ty<'tcx>> {
        &self.unhandled
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
        let updated_calls = other.calls.iter().fold(false, |updated, call_other| {
            let inserted = self.calls.insert(call_other.to_owned());
            inserted || updated
        });
        updated_places || updated_calls
    }
}
