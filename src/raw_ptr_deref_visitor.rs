use rustc_hir as hir;
use rustc_middle::mir as mir;
use rustc_middle::ty as ty;

pub struct RawPtrDerefVisitor<'tcx> {
    tcx: ty::TyCtxt<'tcx>,
    local_decls: &'tcx mir::LocalDecls<'tcx>,
    def_id: hir::def_id::DefId,
    has_raw_ptr_deref: bool,
}

impl<'tcx> mir::visit::Visitor<'tcx> for RawPtrDerefVisitor<'tcx> {
    fn visit_place(
        &mut self,
        place: &mir::Place<'tcx>,
        context: mir::visit::PlaceContext,
        location: mir::Location,
    ) {
        // If Place contains a Deref projection.
        if place.is_indirect() {
            // Retrieve type of the local after each projection, check if it is an unsafe pointer.
            // TODO(artem): check whether it works with more involved projections.
            for (place_ref, _) in place.iter_projections() {
                if place_ref.ty(self.local_decls, self.tcx).ty.is_unsafe_ptr() {
                    self.has_raw_ptr_deref = true;
                }
            }
        }
        self.super_place(place, context, location);
    }
}

impl<'tcx> RawPtrDerefVisitor<'tcx> {
    pub fn new(tcx: ty::TyCtxt<'tcx>, local_decls: &'tcx mir::LocalDecls<'tcx>, def_id: hir::def_id::DefId) -> Self {
        Self { tcx, local_decls, def_id, has_raw_ptr_deref: false }
    }

    pub fn check(&self) -> bool {
        self.has_raw_ptr_deref
    }
}
