use rustc_middle::ty::{self, Ty};

pub trait TyExt<'tcx> {
    fn contains_erased(&self) -> bool;
}

impl<'tcx> TyExt<'tcx> for Ty<'tcx> {
    // TODO: is there anyting else that we need to track?
    fn contains_erased(&self) -> bool {
        let contains_erased_type = self.walk().any(|ty| match ty.unpack() {
            ty::GenericArgKind::Type(ty) => match ty.kind() {
                ty::Param(..)
                | ty::FnPtr(..)
                | ty::RawPtr(..)
                | ty::Dynamic(..)
                | ty::Foreign(..) => true,
                _ => false,
            },
            _ => false,
        });
        !self.contains_closure() && contains_erased_type
    }
}
