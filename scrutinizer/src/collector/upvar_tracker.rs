use std::{cell::RefCell, collections::HashMap, rc::Rc};

use rustc_middle::ty::Ty;

use super::tracked_ty::TrackedTy;

pub type UpvarTrackerRef<'tcx> = Rc<RefCell<UpvarTracker<'tcx>>>;

pub type UpvarTracker<'tcx> = HashMap<Ty<'tcx>, TrackedUpvars<'tcx>>;

#[derive(Clone, Debug)]
pub struct TrackedUpvars<'tcx> {
    pub resolved_ty: Ty<'tcx>,
    pub upvars: Vec<TrackedUpvar<'tcx>>,
}

pub type TrackedUpvar<'tcx> = TrackedTy<'tcx>;
