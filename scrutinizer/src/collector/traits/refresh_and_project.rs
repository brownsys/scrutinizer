use itertools::Itertools;
use rustc_abi::VariantIdx;
use rustc_middle::mir::{tcx::PlaceTy, PlaceElem, ProjectionElem};
use rustc_middle::ty::{self, TyCtxt};

pub trait RefreshAndProject<'tcx> {
    fn refresh_and_project(&self, projection_elem: &PlaceElem<'tcx>, tcx: TyCtxt<'tcx>) -> Self;
}

impl<'tcx> RefreshAndProject<'tcx> for PlaceTy<'tcx> {
    fn refresh_and_project(
        &self,
        projection_elem: &PlaceElem<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> PlaceTy<'tcx> {
        let new_projection = match projection_elem {
            ProjectionElem::Field(field_idx, ..) => {
                let fixed_projection = match self.ty.kind() {
                    ty::TyKind::Adt(adt_def, adt_substs) => {
                        let variant_idx = self.variant_index.unwrap_or(VariantIdx::from_usize(0));
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
                    _ => panic!(
                        "field projection of {:?} is not supported: kind={:?}",
                        self,
                        self.ty.kind()
                    ),
                };
                self.projection_ty(tcx, fixed_projection)
            }
            _ => self.projection_ty(tcx, projection_elem.to_owned()),
        };
        new_projection
    }
}
