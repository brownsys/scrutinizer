use itertools::Itertools;
use log::debug;
use rustc_abi::{FieldIdx, VariantIdx};
use rustc_middle::mir::{tcx::PlaceTy, Body, Local, Operand, Place, PlaceElem, ProjectionElem};
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_mir_dataflow::{fmt::DebugWithContext, JoinSemiLattice};
use rustc_span::def_id::DefId;
use rustc_utils::PlaceExt;
use std::collections::{HashMap, HashSet};

use crate::collector::collector::Collector;
use crate::common::{ArgTys, ClosureInfo, FunctionCall, FunctionInfo, NormalizedPlace, TrackedTy};

fn refresh_and_project<'tcx>(
    place_ty: PlaceTy<'tcx>,
    projection_elem: &PlaceElem<'tcx>,
    tcx: TyCtxt<'tcx>,
) -> Result<PlaceTy<'tcx>, String> {
    let new_projection = match projection_elem {
        ProjectionElem::Field(field_idx, ..) => {
            let fixed_projection = match place_ty.ty.kind() {
                ty::TyKind::Alias(.., alias_ty) => index_ty_field(
                    PlaceTy {
                        ty: alias_ty.self_ty(),
                        variant_index: place_ty.variant_index,
                    },
                    field_idx,
                    tcx,
                ),
                _ => index_ty_field(place_ty, field_idx, tcx),
            }?;
            debug!("projecting ty={:?}, proj={:?}", place_ty, fixed_projection);
            place_ty.projection_ty(tcx, fixed_projection)
        }
        ProjectionElem::Deref => {
            if place_ty.ty.builtin_deref(true).is_none() {
                return Err(format!(
                    "projecting ty={:?}, but proj={:?} is invalid",
                    place_ty, projection_elem
                ));
            } else {
                debug!("projecting ty={:?}, proj={:?}", place_ty, projection_elem);
                place_ty.projection_ty(tcx, projection_elem.to_owned())
            }
        }
        ProjectionElem::Index(_) => {
            if place_ty.ty.builtin_index().is_none() {
                return Err(format!(
                    "projecting ty={:?}, but proj={:?} is invalid",
                    place_ty, projection_elem
                ));
            } else {
                debug!("projecting ty={:?}, proj={:?}", place_ty, projection_elem);
                place_ty.projection_ty(tcx, projection_elem.to_owned())
            }
        }
        _ => {
            debug!("projecting ty={:?}, proj={:?}", place_ty, projection_elem);
            place_ty.projection_ty(tcx, projection_elem.to_owned())
        }
    };
    Ok(new_projection)
}

fn index_ty_field<'tcx>(
    place_ty: PlaceTy<'tcx>,
    field_idx: &FieldIdx,
    tcx: TyCtxt<'tcx>,
) -> Result<PlaceElem<'tcx>, String> {
    let fixed_projection = match place_ty.ty.kind() {
        ty::TyKind::Adt(adt_def, adt_substs) => {
            let variant_idx = place_ty.variant_index.unwrap_or(VariantIdx::from_usize(0));
            let variant_def = adt_def.variant(variant_idx);
            let field = variant_def.fields.get(field_idx.to_owned()).unwrap();
            let fixed_ty = field.ty(tcx, adt_substs);
            ProjectionElem::Field(field_idx.to_owned(), fixed_ty)
        }
        ty::TyKind::Closure(.., closure_substs) => {
            let closure_substs = closure_substs.as_closure();
            let upvars = closure_substs.upvar_tys().collect_vec();
            let fixed_ty = upvars.get(field_idx.index()).unwrap();
            ProjectionElem::Field(field_idx.to_owned(), fixed_ty.to_owned())
        }
        ty::TyKind::Tuple(inner_tys) => {
            let fixed_ty = inner_tys.get(field_idx.index()).unwrap();
            ProjectionElem::Field(field_idx.to_owned(), fixed_ty.to_owned())
        }
        _ => {
            return Err(format!(
                "field projection is not supported: ty={:?}, field_idx={:?}",
                place_ty, field_idx
            ));
        }
    };
    Ok(fixed_projection)
}

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
    pub fn new(places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>) -> Self {
        CollectorDomain {
            places,
            calls: HashSet::new(),
            unhandled: HashSet::new(),
        }
    }

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
            panic!("non-regular fn_info");
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
    ) -> Result<(), String> {
        let paths = place
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
                let transformed_ty = ty.try_map(|ty| {
                    let initial_place = PlaceTy { ty, variant_index };
                    Ok(projection_tail
                        .iter()
                        .fold(Ok(initial_place), |place_ty, projection_elem| {
                            place_ty.and_then(|place_ty| {
                                refresh_and_project(place_ty, projection_elem, tcx)
                            })
                        })?
                        .ty)
                })?;
                Ok((interior_path, transformed_ty))
            })
            .collect::<Result<Vec<_>, String>>()?;
        for (interior_path, transformed_ty) in paths {
            let mut_place = self.places.get_mut(&interior_path).unwrap();
            mut_place.join(&transformed_ty);
        }
        Ok(())
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

    pub fn add_call(&mut self, call: FunctionCall<'tcx>) {
        self.calls.insert(call);
    }

    pub fn add_unhandled(&mut self, unhandled: Ty<'tcx>) {
        self.unhandled.insert(unhandled);
    }

    pub fn calls(&self) -> &HashSet<FunctionCall<'tcx>> {
        &self.calls
    }

    pub fn unhandled(&self) -> &HashSet<Ty<'tcx>> {
        &self.unhandled
    }
}

impl<'tcx> DebugWithContext<Collector<'tcx>> for CollectorDomain<'tcx> {}

impl<'tcx> JoinSemiLattice for CollectorDomain<'tcx> {
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
