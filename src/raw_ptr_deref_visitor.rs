use rustc_hir as hir;
use rustc_middle::mir as mir;
use rustc_middle::ty as ty;

pub struct RawPtrDerefVisitor<'tcx> {
    tcx: ty::TyCtxt<'tcx>,
    local_decls: &'tcx mir::LocalDecls<'tcx>,
    def_id: hir::def_id::DefId,
    has_raw_ptr_deref: bool,
}

fn has_raw_ptr_deref<'tcx>(place: &mir::Place<'tcx>, tcx: ty::TyCtxt<'tcx>,
                           local_decls: &'tcx mir::LocalDecls<'tcx>) -> bool {
    place.iter_projections().any(|(place_ref, _)| {
        let ty = place_ref.ty(local_decls, tcx).ty;
        ty.is_unsafe_ptr() && ty.is_mutable_ptr()
    })
}

impl<'tcx> mir::visit::Visitor<'tcx> for RawPtrDerefVisitor<'tcx> {
    fn visit_statement(&mut self, statement: &mir::Statement<'tcx>, location: mir::Location) {
        if let mir::StatementKind::Assign(assignment) = &statement.kind {
            let place = &assignment.0;
            let rvalue = &assignment.1;

            if has_raw_ptr_deref(place, self.tcx, self.local_decls) {
                self.has_raw_ptr_deref = true;
            } else {
                if let mir::Rvalue::Ref(_, borrow_kind, borrow_place) = rvalue {
                    if let mir::Mutability::Mut = borrow_kind.mutability() {
                        if has_raw_ptr_deref(borrow_place, self.tcx, self.local_decls) {
                            self.has_raw_ptr_deref = true;
                        }
                    }
                }
            }
        };
        self.super_statement(statement, location);
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
