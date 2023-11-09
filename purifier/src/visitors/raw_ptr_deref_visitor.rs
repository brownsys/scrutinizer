use rustc_middle::mir;
use rustc_middle::mir::visit::Visitor;
use rustc_middle::ty;

struct RawPtrDerefVisitor<'tcx> {
    tcx: ty::TyCtxt<'tcx>,
    body: &'tcx mir::Body<'tcx>,
    has_raw_ptr_deref: bool,
}

fn place_has_raw_ptr_deref<'tcx>(
    place: &mir::Place<'tcx>,
    tcx: ty::TyCtxt<'tcx>,
    body: &'tcx mir::Body<'tcx>,
) -> bool {
    place.iter_projections().any(|(place_ref, _)| {
        let ty = place_ref.ty(body, tcx).ty;
        ty.is_unsafe_ptr() && ty.is_mutable_ptr()
    })
}

impl<'tcx> mir::visit::Visitor<'tcx> for RawPtrDerefVisitor<'tcx> {
    fn visit_statement(&mut self, statement: &mir::Statement<'tcx>, location: mir::Location) {
        if let mir::StatementKind::Assign(assignment) = &statement.kind {
            let place = &assignment.0;
            let rvalue = &assignment.1;

            if place_has_raw_ptr_deref(place, self.tcx, self.body) {
                self.has_raw_ptr_deref = true;
            } else {
                if let mir::Rvalue::Ref(_, borrow_kind, borrow_place) = rvalue {
                    if let mir::Mutability::Mut = borrow_kind.mutability() {
                        if place_has_raw_ptr_deref(borrow_place, self.tcx, self.body) {
                            self.has_raw_ptr_deref = true;
                        }
                    }
                }
            }
        };
        self.super_statement(statement, location);
    }
}

pub fn has_raw_ptr_deref<'tcx>(tcx: ty::TyCtxt<'tcx>, body: &'tcx mir::Body<'tcx>) -> bool {
    let mut ptr_deref_visitor = RawPtrDerefVisitor {
        tcx,
        body,
        has_raw_ptr_deref: false,
    };
    ptr_deref_visitor.visit_body(body);
    ptr_deref_visitor.has_raw_ptr_deref
}
