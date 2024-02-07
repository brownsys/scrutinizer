use std::{cell::RefCell, collections::HashMap, rc::Rc};
use rustc_middle::ty::Ty;
use rustc_span::def_id::DefId;

use super::tracked_ty::TrackedTy;

pub type ClosureInfoStorageRef<'tcx> = Rc<RefCell<ClosureInfoStorage<'tcx>>>;

pub type ClosureInfoStorage<'tcx> = HashMap<DefId, ClosureInfo<'tcx>>;

#[derive(Clone, Debug)]
pub struct ClosureInfo<'tcx> {
    pub with_substs: Ty<'tcx>,
    pub upvars: Vec<TrackedTy<'tcx>>,
}
