use rustc_middle::ty::{self, Ty};

pub trait TyExt<'tcx> {
    fn contains_trait(&self) -> bool;
}

impl<'tcx> TyExt<'tcx> for Ty<'tcx> {
    fn contains_trait(&self) -> bool {
        self.walk().any(|ty| match ty.unpack() {
            ty::GenericArgKind::Type(ty) => ty.is_trait(),
            _ => false,
        })
    }
}
