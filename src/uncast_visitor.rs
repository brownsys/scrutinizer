use rustc_hir as hir;
use rustc_middle::mir as mir;
use rustc_middle::ty as ty;
use rustc_middle::mir::visit::Visitor;

struct UncastVisitor<'tcx> {
    tcx: ty::TyCtxt<'tcx>,
    place: mir::Place<'tcx>,
    original_place: Option<mir::Place<'tcx>>,
}

impl<'tcx> mir::visit::Visitor<'tcx> for UncastVisitor<'tcx> {
    fn visit_statement(&mut self, statement: &mir::Statement<'tcx>, location: mir::Location) {
        if let mir::StatementKind::Assign(assignment) = &statement.kind {
            let place = &assignment.0;
            let rvalue = &assignment.1;

            if self.place == *place {
                if let mir::Rvalue::Cast(_, op, _) = rvalue {
                    if let Some(original_place) = op.place() {
                        self.original_place = Some(original_place);
                        return;
                    }
                }
            }
        };
        self.super_statement(statement, location);
    }
}

pub fn uncast<'tcx>(tcx: ty::TyCtxt<'tcx>, place: mir::Place<'tcx>, body: &'tcx mir::Body<'tcx>) -> Option<ty::Ty<'tcx>> {
    let mut uncast_visitor = UncastVisitor { tcx, place, original_place: None };
    uncast_visitor.visit_body(body);
    match uncast_visitor.original_place {
        Some(original_place) => Some(original_place.ty(body, tcx).ty),
        None => None
    }
}