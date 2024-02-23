use rustc_middle::mir::visit::{TyContext, Visitor};
use rustc_middle::mir::Body;
use rustc_middle::ty::{Ty, TyCtxt};

use super::callee::Callee;
use super::closure_info::ClosureInfoStorageRef;

struct ClosureCollector<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    closure_storage_ref: ClosureInfoStorageRef<'tcx>,
    current_fn: &'a Callee<'tcx>,
}

pub trait CollectClosures<'tcx> {
    fn collect_closures(
        &self,
        tcx: TyCtxt<'tcx>,
        closure_info_storage_ref: ClosureInfoStorageRef<'tcx>,
        current_fn: &Callee<'tcx>,
    );
}

impl<'tcx> CollectClosures<'tcx> for Body<'tcx> {
    fn collect_closures(
        &self,
        tcx: TyCtxt<'tcx>,
        closure_storage_ref: ClosureInfoStorageRef<'tcx>,
        current_fn: &Callee<'tcx>,
    ) {
        let mut closure_collector = ClosureCollector {
            tcx,
            closure_storage_ref,
            current_fn,
        };
        closure_collector.visit_body(self);
    }
}

impl<'a, 'tcx> Visitor<'tcx> for ClosureCollector<'a, 'tcx> {
    fn visit_ty(&mut self, ty: Ty<'tcx>, _: TyContext) {
        self.closure_storage_ref
            .borrow_mut()
            .try_resolve_and_insert(ty, self.current_fn.instance(), vec![], self.tcx);
    }
}
