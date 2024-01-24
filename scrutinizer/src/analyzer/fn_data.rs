use rustc_middle::ty;

use super::important_locals::ImportantLocals;
use super::tracked_ty::TrackedTy;

#[derive(Debug)]
pub struct FnData<'tcx> {
    instance: ty::Instance<'tcx>,
    tracked_args: Vec<TrackedTy<'tcx>>,
    important_locals: ImportantLocals,
}

impl<'tcx> FnData<'tcx> {
    pub fn new(
        instance: ty::Instance<'tcx>,
        tracked_args: Vec<TrackedTy<'tcx>>,
        important_locals: ImportantLocals,
    ) -> Self {
        Self {
            instance,
            tracked_args,
            important_locals,
        }
    }
    pub fn important_locals(&self) -> &ImportantLocals {
        &self.important_locals
    }
    pub fn instance(&self) -> &ty::Instance<'tcx> {
        &self.instance
    }
    pub fn tracked_args(&self) -> &Vec<TrackedTy<'tcx>> {
        &self.tracked_args
    }
}
