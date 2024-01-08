use rustc_middle::ty::{self, Ty};

pub trait TyExt<'tcx> {
    fn contains_trait(&self) -> bool;
    fn contains_impl_trait(&self) -> bool;
    fn contains_param(&self) -> bool;
    fn contains_erased(&self) -> bool;
}

impl<'tcx> TyExt<'tcx> for Ty<'tcx> {
    fn contains_trait(&self) -> bool {
        self.walk().any(|ty| match ty.unpack() {
            ty::GenericArgKind::Type(ty) => ty.is_trait(),
            _ => false,
        })
    }
    fn contains_impl_trait(&self) -> bool {
        self.walk().any(|ty| match ty.unpack() {
            ty::GenericArgKind::Type(ty) => ty.is_impl_trait(),
            _ => false,
        })
    }
    fn contains_param(&self) -> bool {
        self.walk().any(|ty| match ty.unpack() {
            ty::GenericArgKind::Type(ty) => match ty.kind() {
                ty::Param(_) => true,
                _ => false,
            },
            _ => false,
        })
    }
    // TODO: revisit this, I am not sure this is exhaustive.
    fn contains_erased(&self) -> bool {
        !self.contains_closure() && (self.contains_param() || self.contains_trait() || self.contains_impl_trait())
    }
}
