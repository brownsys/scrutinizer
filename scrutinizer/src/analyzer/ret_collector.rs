use rustc_middle::mir::visit::Visitor;
use rustc_middle::mir::{self, Body, Location, Terminator, TerminatorKind};

struct RetVisitor {
    returns: Vec<Location>,
}

pub trait GetReturnSites<'tcx> {
    fn get_return_sites(&self) -> Vec<Location>;
}

impl<'tcx> GetReturnSites<'tcx> for Body<'tcx> {
    fn get_return_sites(&self) -> Vec<Location> {
        let mut ret_visitor = RetVisitor { returns: vec![] };
        ret_visitor.visit_body(self);
        ret_visitor.returns
    }
}

impl<'tcx> mir::visit::Visitor<'tcx> for RetVisitor {
    fn visit_terminator(&mut self, terminator: &Terminator<'tcx>, location: Location) {
        if let TerminatorKind::Return = &terminator.kind {
            self.returns.push(location);
        }
        self.super_terminator(terminator, location);
    }
}
