use itertools::Itertools;
use rustc_abi::VariantIdx;
use rustc_middle::mir::{tcx::PlaceTy, Body, PlaceElem, ProjectionElem};
use rustc_middle::ty::{self, TyCtxt};
use rustc_span::def_id::DefId;
use rustc_utils::PlaceExt;

use super::normalized_place::NormalizedPlace;
use super::tracked_ty::TrackedTy;

use super::type_tracker::TypeTracker;

pub fn apply_fresh_projection<'tcx>(
    place_ty: &PlaceTy<'tcx>,
    projection_elem: &PlaceElem<'tcx>,
    tcx: TyCtxt<'tcx>,
) -> PlaceTy<'tcx> {
    // dbg!(
    //     "applying projection {:?} to {:?}",
    //     projection_elem,
    //     place_ty
    // );
    let new_projection = match projection_elem {
        ProjectionElem::Field(field_idx, ..) => {
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
                _ => panic!("field projection of {:?} is not supported: kind={:?}", place_ty, place_ty.ty.kind()),
            };
            place_ty.projection_ty(tcx, fixed_projection)
        }
        _ => place_ty.projection_ty(tcx, projection_elem.to_owned()),
    };
    new_projection
}

pub fn propagate<'tcx>(
    type_tracker: &mut TypeTracker<'tcx>,
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

            // dbg!(
            //     ty,
            //     place,
            //     place.ty(body, tcx),
            //     &interior_path.projection,
            //     &projection_tail
            // );

            let transformed_ty = ty.map(|ty| {
                let initial_place = PlaceTy { ty, variant_index };
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
            let mut_place = type_tracker.places.get_mut(&interior_path).unwrap();
            mut_place.join(&transformed_ty);
        });
}
