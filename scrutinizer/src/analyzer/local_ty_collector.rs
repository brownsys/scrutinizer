use rustc_middle::mir::visit::Visitor;
use rustc_middle::mir::{self, Body, Local, Location, Place, Rvalue};
use rustc_middle::ty::{Ty, TyCtxt};

struct AssignmentVisitor<'tcx> {
    local_needle: Local,
    tcx: TyCtxt<'tcx>,
    body: &'tcx Body<'tcx>,
    types: Vec<Ty<'tcx>>,
}

pub trait GetLocalTys<'tcx> {
    fn get_local_tys_for(&'tcx self, tcx: TyCtxt<'tcx>, local: Local) -> Vec<Ty<'tcx>>;
}

impl<'tcx> GetLocalTys<'tcx> for Body<'tcx> {
    fn get_local_tys_for(&'tcx self, tcx: TyCtxt<'tcx>, local: Local) -> Vec<Ty<'tcx>> {
        let mut assignment_visitor = AssignmentVisitor {
            types: vec![],
            body: self,
            tcx,
            local_needle: local,
        };
        assignment_visitor.visit_body(self);
        assignment_visitor.types
    }
}

impl<'tcx> mir::visit::Visitor<'tcx> for AssignmentVisitor<'tcx> {
    fn visit_assign(&mut self, place: &Place<'tcx>, rvalue: &Rvalue<'tcx>, location: Location) {
        if place.local == self.local_needle {
            self.types.push(rvalue.ty(self.body, self.tcx));
        }
        self.super_assign(place, rvalue, location);
    }
}
