use rustc_hir::def_id::DefId;

use super::ArgTys;

#[derive(Clone, Debug)]
pub struct VirtualStack<'tcx> {
    stack: Vec<VirtualStackItem<'tcx>>,
}

impl<'tcx> VirtualStack<'tcx> {
    pub fn new() -> Self {
        VirtualStack { stack: vec![] }
    }

    pub fn push(&mut self, item: VirtualStackItem<'tcx>) {
        self.stack.push(item);
    }

    pub fn contains(&self, item: &VirtualStackItem<'tcx>) -> bool {
        self.stack.contains(&item)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualStackItem<'tcx> {
    def_id: DefId,
    tracked_args: ArgTys<'tcx>,
}

impl<'tcx> VirtualStackItem<'tcx> {
    pub fn new(def_id: DefId, tracked_args: ArgTys<'tcx>) -> Self {
        VirtualStackItem {
            def_id,
            tracked_args,
        }
    }
}
