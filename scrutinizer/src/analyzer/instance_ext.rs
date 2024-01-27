use rustc_middle::ty::{self, TyCtxt};

use super::tracked_ty::TrackedTy;

pub trait InstanceExt<'tcx> {
    fn arg_tys(&self, tcx: TyCtxt<'tcx>) -> Vec<TrackedTy<'tcx>>;
}

impl<'tcx> InstanceExt<'tcx> for ty::Instance<'tcx> {
    fn arg_tys(&self, tcx: TyCtxt<'tcx>) -> Vec<TrackedTy<'tcx>> {
        let ty = tcx.type_of(self.def_id()).subst(tcx, self.substs);
        let sig = match ty.kind() {
            ty::FnDef(_, _) => tcx
                .fn_sig(self.def_id())
                .subst(tcx, self.substs)
                .skip_binder(),
            ty::Closure(_, substs) => substs.as_closure().sig().skip_binder(),
            _ => unreachable!("should not be here"),
        };
        sig.inputs()
            .iter()
            .map(|ty| TrackedTy::from_ty(ty.to_owned()))
            .collect()
    }
}
