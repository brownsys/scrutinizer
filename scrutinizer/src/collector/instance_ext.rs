use rustc_middle::ty::{self, TyCtxt};
use std::iter::once;

use super::tracked_ty::TrackedTy;

pub trait InstanceExt<'tcx> {
    fn arg_tys(&self, tcx: TyCtxt<'tcx>) -> Vec<TrackedTy<'tcx>>;
}

impl<'tcx> InstanceExt<'tcx> for ty::Instance<'tcx> {
    fn arg_tys(&self, tcx: TyCtxt<'tcx>) -> Vec<TrackedTy<'tcx>> {
        let ty = self.ty(tcx, ty::ParamEnv::reveal_all());
        match ty.kind() {
            ty::FnDef(_, _) => {
                let sig = tcx
                    .fn_sig(self.def_id())
                    .subst(tcx, self.substs)
                    .skip_binder();
                sig.inputs()
                    .iter()
                    .map(|ty| TrackedTy::from_ty(ty.to_owned()))
                    .collect()
            }
            ty::Closure(_, substs) => {
                let closure_substs = substs.as_closure();
                let sig = closure_substs.sig().skip_binder();
                assert!(sig.inputs().len() == 1);
                let sig_tys = sig
                    .inputs()
                    .iter()
                    .map(|ty| TrackedTy::from_ty(ty.to_owned()).spread_tuple())
                    .flatten();
                once(TrackedTy::from_ty(
                    tcx.mk_imm_ref(tcx.mk_region_from_kind(ty::ReErased), ty),
                ))
                .chain(sig_tys)
                .collect()
            }
            _ => panic!("argument extraction unsupported"),
        }
    }
}
