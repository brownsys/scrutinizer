use itertools::Itertools;
use rustc_abi::{FieldIdx, VariantIdx};
use rustc_middle::mir::{tcx::PlaceTy, Body, Local, Operand, Place, PlaceElem, ProjectionElem};
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_mir_dataflow::{fmt::DebugWithContext, JoinSemiLattice};
use rustc_span::def_id::DefId;
use rustc_utils::PlaceExt;
use std::collections::{HashMap, HashSet};

use crate::collector::collector::Collector;
use crate::common::{ArgTys, ClosureInfo, FunctionCall, FunctionInfo, NormalizedPlace, TrackedTy};

/// Refreshes the type and applies projections to the place.
fn refresh_and_project<'tcx>(
    place_ty: PlaceTy<'tcx>,
    projection_elem: &PlaceElem<'tcx>,
    tcx: TyCtxt<'tcx>,
) -> PlaceTy<'tcx> {
    let new_projection = match projection_elem {
        // Field projections have cached types stored inside,
        // but they can be stale, hence we need to refresh them.
        ProjectionElem::Field(field_idx, ..) => {
            let fixed_projection = match place_ty.ty.kind() {
                // Extract the alias type if encountered and project.
                ty::TyKind::Alias(.., alias_ty) => index_ty_field(
                    PlaceTy {
                        ty: alias_ty.self_ty(),
                        variant_index: place_ty.variant_index,
                    },
                    field_idx,
                    tcx,
                ),
                // For all other types, simply project.
                _ => index_ty_field(place_ty, field_idx, tcx),
            };
            place_ty.projection_ty(tcx, fixed_projection)
        }
        // Non field projections don't need to be refreshed.
        _ => place_ty.projection_ty(tcx, projection_elem.to_owned()),
    };
    new_projection
}

/// Gets an indexed type of a place.
fn index_ty_field<'tcx>(
    place_ty: PlaceTy<'tcx>,
    field_idx: &FieldIdx,
    tcx: TyCtxt<'tcx>,
) -> PlaceElem<'tcx> {
    let fixed_projection = match place_ty.ty.kind() {
        // For adt's, get the type of the field, taking into account different variants.
        ty::TyKind::Adt(adt_def, adt_substs) => {
            let variant_idx = place_ty.variant_index.unwrap_or(VariantIdx::from_usize(0));
            let variant_def = adt_def.variant(variant_idx);
            let field = variant_def.fields.get(field_idx.to_owned()).unwrap();
            let fixed_ty = field.ty(tcx, adt_substs);
            ProjectionElem::Field(field_idx.to_owned(), fixed_ty)
        }
        // For closures, need to extract upvars and index them.
        ty::TyKind::Closure(.., closure_substs) => {
            let closure_substs = closure_substs.as_closure();
            let upvars = closure_substs.upvar_tys().collect_vec();
            let fixed_ty = upvars.get(field_idx.index()).unwrap();
            ProjectionElem::Field(field_idx.to_owned(), fixed_ty.to_owned())
        }
        // For tuples, get the type of the field.
        ty::TyKind::Tuple(inner_tys) => {
            let fixed_ty = inner_tys.get(field_idx.index()).unwrap();
            ProjectionElem::Field(field_idx.to_owned(), fixed_ty.to_owned())
        }
        // TODO: this is a catch-all statement, are there any other types that get projected?
        _ => {
            panic!(
                "field projection is not supported: ty={:?}, field_idx={:?}",
                place_ty, field_idx,
            );
        }
    };
    fixed_projection
}

/// Results of the type collection, possibly partial.
#[derive(Debug, Clone)]
pub struct CollectorDomain<'tcx> {
    places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>,
    calls: HashSet<FunctionCall<'tcx>>,
    unhandled: HashSet<Ty<'tcx>>,
}

impl<'tcx> PartialEq for CollectorDomain<'tcx> {
    fn eq(&self, other: &Self) -> bool {
        self.places == other.places
    }
}

impl<'tcx> Eq for CollectorDomain<'tcx> {}

impl<'tcx> CollectorDomain<'tcx> {
    /// Creates a fresh CollectorDomain with given places.
    pub fn new(places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>) -> Self {
        CollectorDomain {
            places,
            calls: HashSet::new(),
            unhandled: HashSet::new(),
        }
    }

    // Constructs a CollectorDomain from function info, panicking if function does not have a body.
    pub fn from_regular_fn_info(fn_info: &FunctionInfo<'tcx>) -> Self {
        if let FunctionInfo::WithBody {
            places,
            calls,
            unhandled,
            ..
        } = fn_info
        {
            CollectorDomain {
                places: places.clone(),
                calls: calls.clone(),
                unhandled: unhandled.clone(),
            }
        } else {
            panic!("attempted to construct CollectionDomain from FnInfo::WithoutBody");
        }
    }

    /// Returns an approximated set of types for a place from a result set.
    pub fn get(&self, place: &NormalizedPlace<'tcx>) -> Option<&TrackedTy<'tcx>> {
        self.places.get(place)
    }

    /// Returns all places from a result set.
    pub fn places(&self) -> &HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>> {
        &self.places
    }

    /// Add upvar info into the CollectorDomain of some closure.
    pub fn augment_closure_with_upvars(
        &mut self,
        upvars: &ClosureInfo<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) {
        upvars.upvars.iter().enumerate().for_each(|(i, upvar)| {
            // Create a place to insert the upvar type info.
            let type_placeholder = tcx.types.unit; // Since we redefine normalized place equality, this does not matter.
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

            // Find a place entry, update the type.
            self.places
                .entry(normalized_upvar_place.clone())
                .and_modify(|place| {
                    place.join(upvar);
                })
                .or_insert(upvar.to_owned());

            // Propagate the change to all subtypes.
            self.propagate(&normalized_upvar_place, upvar, body, def_id, tcx);
        });
    }

    /// Add argument info into the CollectorDomain of some closure.
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
            // For closure calls, the first argument is the closure itself so we need to skip it.
            .skip(if tcx.is_closure(def_id) { 1 } else { 0 })
            .for_each(|(i, tracked_ty)| {
                // Make a place to update the CollectionDomain.
                let arg_local = Local::from_usize(i + 1);
                let place =
                    NormalizedPlace::from_place(&Place::from_local(arg_local, tcx), tcx, def_id);
                // Perform an update.
                self.places
                    .entry(place.clone())
                    .and_modify(|place| {
                        place.join(tracked_ty);
                    })
                    .or_insert(tracked_ty.to_owned());
                // Propagate the underlying subtypes.
                self.propagate(&place, tracked_ty, body, def_id, tcx);
            })
    }

    /// Propagate the updated type information inside a CollectorDomain.
    fn propagate(
        &mut self,
        place: &NormalizedPlace<'tcx>,
        ty: &TrackedTy<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) {
        // Obtain all paths from a place.
        place
            .interior_paths(tcx, body, def_id)
            .into_iter()
            // Refresh the projection tail in case some of the types are stale.
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
                            refresh_and_project(place_ty, projection_elem, tcx)
                        })
                        .ty
                });
                (interior_path, transformed_ty)
            })
            .for_each(|(interior_path, transformed_ty)| {
                // Update the type.
                let mut_place = self.places.get_mut(&interior_path).unwrap();
                mut_place.join(&transformed_ty);
            });
    }

    /// Update a place with new type information.
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

    /// Approximate a return type for a CollectionDomain.
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

        // TODO: do we need this check? isn't the collected type always more specific?
        // Here, we return the collected return type if it is more specific than the inferred one.
        match provided_return_ty {
            TrackedTy::Present(..) => provided_return_ty,
            TrackedTy::Erased(..) => inferred_return_ty,
        }
    }

    /// Retrieves argument types for passed arguments.
    pub fn construct_args(
        &self,
        args: &Vec<Operand<'tcx>>,
        def_id: DefId,
        body: &Body<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> ArgTys<'tcx> {
        ArgTys::new(
            args.iter()
                .map(|arg| {
                    arg.place()
                        .and_then(|place| Some(NormalizedPlace::from_place(&place, tcx, def_id)))
                        .and_then(|place| self.get(&place))
                        .and_then(|ty| Some(ty.to_owned()))
                        .unwrap_or(TrackedTy::from_ty(arg.ty(body, tcx)))
                })
                .collect_vec(),
        )
    }

    /// Adds a call into a CollectorDomain.
    pub fn add_call(&mut self, call: FunctionCall<'tcx>) {
        self.calls.insert(call);
    }

    /// Adds an unhandled terminator into a CollectorDomain.
    pub fn add_unhandled(&mut self, unhandled: Ty<'tcx>) {
        self.unhandled.insert(unhandled);
    }

    /// Returns all calls from a CollectorDomain.
    pub fn calls(&self) -> &HashSet<FunctionCall<'tcx>> {
        &self.calls
    }

    /// Returns all unhandled terminators from a CollectorDomain.
    pub fn unhandled(&self) -> &HashSet<Ty<'tcx>> {
        &self.unhandled
    }
}

impl<'tcx> DebugWithContext<Collector<'tcx>> for CollectorDomain<'tcx> {}

impl<'tcx> JoinSemiLattice for CollectorDomain<'tcx> {
    /// Merging logic for two collector domains. Returns true if an update has been performed.
    fn join(&mut self, other: &Self) -> bool {
        // Merge three fields, return true if at least one of them has changes.
        let updated_places = other.places.iter().fold(false, |acc, (key, other_value)| {
            let self_value = self.places.get_mut(key).unwrap();
            let updated = self_value.join(other_value);
            acc || updated
        });
        let updated_calls = other.calls.iter().fold(false, |updated, call_other| {
            let inserted = self.calls.insert(call_other.to_owned());
            inserted || updated
        });
        let updated_unhandled = other
            .unhandled
            .iter()
            .fold(false, |updated, unhandled_other| {
                let inserted = self.unhandled.insert(unhandled_other.to_owned());
                inserted || updated
            });
        updated_places || updated_calls || updated_unhandled
    }
}
