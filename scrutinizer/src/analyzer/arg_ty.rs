use serde::ser::{Serialize, SerializeStructVariant};

use rustc_middle::mir::{Location, Operand};
use rustc_middle::ty::{Ty, TyCtxt};

use super::fn_data::FnData;
use super::ty_ext::TyExt;

#[derive(Debug, Clone)]
pub enum ArgTy<'tcx> {
    Simple(Ty<'tcx>),
    Erased(Ty<'tcx>, Vec<Ty<'tcx>>),
}

impl<'tcx> ArgTy<'tcx> {
    pub fn from_known_or_erased(
        arg: &Operand<'tcx>,
        location: &Location,
        current_fn: &FnData<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Self {
        let outer_body = tcx.optimized_mir(current_fn.get_instance().def_id());
        let arg_ty = arg.ty(outer_body, tcx);
        // Check whether argument type was erased.
        if arg_ty.contains_trait() {
            let backward_deps = current_fn.deps_for(arg, location, tcx);
            ArgTy::Erased(arg_ty, backward_deps)
        } else {
            ArgTy::Simple(arg_ty)
        }
    }
    pub fn from_known(arg_ty: Ty<'tcx>) -> Self {
        ArgTy::Simple(arg_ty)
    }
    pub fn is_poisoned(&self) -> bool {
        // If one of the influences in the erased type is erased itself,
        // we consider it poisoned, as it can never be resolved with certainty.
        // TODO: Can it ever happen if we reject functions with generic arguments?
        match self {
            ArgTy::Simple(_) => false,
            ArgTy::Erased(_, influences) => influences.iter().any(|ty| ty.contains_trait()),
        }
    }
    pub fn into_vec(self) -> Vec<Ty<'tcx>> {
        match self {
            ArgTy::Simple(ty) => vec![ty],
            ArgTy::Erased(ty, subst_tys) => subst_tys.into_iter().chain([ty]).collect(),
        }
    }
}

impl<'tcx> Serialize for ArgTy<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            ArgTy::Simple(ref ty) => {
                let mut tv = serializer.serialize_struct_variant("ArgTy", 0, "Simple", 1)?;
                tv.serialize_field("ty", format!("{:?}", ty).as_str())?;
                tv.end()
            }
            ArgTy::Erased(ref ty, ref vec_ty) => {
                let mut tv = serializer.serialize_struct_variant("ArgTy", 1, "Erased", 2)?;
                tv.serialize_field("ty", format!("{:?}", ty).as_str())?;
                tv.serialize_field(
                    "inlfluences",
                    &vec_ty
                        .iter()
                        .map(|ty| format!("{:?}", ty))
                        .collect::<Vec<_>>(),
                )?;
                tv.end()
            }
        }
    }
}
