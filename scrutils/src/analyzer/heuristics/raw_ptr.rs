use rustc_middle::mir::{
    visit::Visitor, Body, Location, Mutability, Place, ProjectionElem, Rvalue, Statement,
    StatementKind,
};
use rustc_middle::ty::TyCtxt;

struct RawPtrDerefVisitor<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    body: &'a Body<'tcx>,
    has_raw_ptr_deref: bool,
}

struct PlaceWithBody<'a, 'tcx> {
    place: &'a Place<'tcx>,
    body: &'a Body<'tcx>,
}

pub trait HasRawPtrDeref<'tcx> {
    fn has_raw_ptr_deref(&self, tcx: TyCtxt<'tcx>) -> bool;
}

impl<'tcx> HasRawPtrDeref<'tcx> for Body<'tcx> {
    fn has_raw_ptr_deref(&self, tcx: TyCtxt<'tcx>) -> bool {
        let mut ptr_deref_visitor = RawPtrDerefVisitor {
            tcx,
            body: self,
            has_raw_ptr_deref: false,
        };
        ptr_deref_visitor.visit_body(self);
        ptr_deref_visitor.has_raw_ptr_deref
    }
}

impl<'a, 'tcx> HasRawPtrDeref<'tcx> for PlaceWithBody<'a, 'tcx> {
    fn has_raw_ptr_deref(&self, tcx: TyCtxt<'tcx>) -> bool {
        self.place
            .iter_projections()
            .any(|(place_ref, projection)| {
                if let ProjectionElem::Deref = projection {
                    let ty = place_ref.ty(self.body, tcx).ty;
                    ty.is_unsafe_ptr() && ty.is_mutable_ptr()
                } else {
                    false
                }
            })
    }
}

impl<'a, 'tcx> Visitor<'tcx> for RawPtrDerefVisitor<'a, 'tcx> {
    fn visit_statement(&mut self, statement: &Statement<'tcx>, location: Location) {
        if let StatementKind::Assign(assignment) = &statement.kind {
            let place = &assignment.0;
            let rvalue = &assignment.1;

            let place_ext = PlaceWithBody {
                place,
                body: self.body,
            };

            if place_ext.has_raw_ptr_deref(self.tcx) {
                self.has_raw_ptr_deref = true;
            } else {
                if let Rvalue::Ref(_, borrow_kind, borrow_place) = rvalue {
                    let borrow_place_ext = PlaceWithBody {
                        place: borrow_place,
                        body: self.body,
                    };
                    if let Mutability::Mut = borrow_kind.mutability() {
                        if borrow_place_ext.has_raw_ptr_deref(self.tcx) {
                            self.has_raw_ptr_deref = true;
                        }
                    }
                }
            }
        };
        self.super_statement(statement, location);
    }
}
