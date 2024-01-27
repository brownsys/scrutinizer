use std::{cell::RefCell, collections::HashMap, rc::Rc};

use rustc_middle::ty::Ty;

use super::TrackedTy;

pub type UpvarTrackerRef<'tcx> = Rc<RefCell<UpvarTracker<'tcx>>>;

pub type UpvarTracker<'tcx> = HashMap<Ty<'tcx>, TrackedUpvars<'tcx>>;

pub type TrackedUpvars<'tcx> = Vec<TrackedUpvar<'tcx>>;

pub type TrackedUpvar<'tcx> = TrackedTy<'tcx>;
