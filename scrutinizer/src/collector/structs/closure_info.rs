use rustc_middle::ty::{self, Ty, TyCtxt};

use super::TrackedTy;

#[derive(Clone, Debug)]
pub struct ClosureInfo<'tcx> {
    pub with_substs: Ty<'tcx>,
    pub upvars: Vec<TrackedTy<'tcx>>,
}

impl<'tcx> ClosureInfo<'tcx> {
    pub fn extract_instance(&self, tcx: TyCtxt<'tcx>) -> ty::Instance<'tcx> {
        match self.with_substs.kind() {
            ty::TyKind::Closure(def_id, substs) => {
                ty::Instance::resolve(tcx, ty::ParamEnv::reveal_all(), *def_id, substs)
                    .unwrap()
                    .unwrap()
            }
            _ => unreachable!(""),
        }
    }
}
