use serde::ser::{Serialize, SerializeStructVariant};

use rustc_middle::mir::{Location, Operand};
use rustc_middle::ty::{self, Ty, TyCtxt};

use super::fn_ty::FnData;
use super::util::extract_deps;

#[derive(Debug, Clone)]
pub enum ArgTy<'tcx> {
    Simple(Ty<'tcx>),
    Erased(Ty<'tcx>, Vec<Ty<'tcx>>),
}

impl<'tcx> ArgTy<'tcx> {
    pub fn from_operand(
        arg: &Operand<'tcx>,
        location: &Location,
        current_fn: &FnData<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Self {
        let outer_body = tcx.optimized_mir(current_fn.instance.def_id());
        let arg_ty = arg.ty(outer_body, tcx);
        if arg_ty.walk().any(|ty| match ty.unpack() {
            ty::GenericArgKind::Type(ty) => ty.is_trait(),
            _ => false,
        }) {
            let backward_deps = extract_deps(
                arg,
                location,
                &current_fn.arg_tys,
                current_fn.instance.def_id(),
                outer_body,
                tcx,
            );
            ArgTy::Erased(arg_ty, backward_deps)
        } else {
            ArgTy::Simple(arg_ty)
        }
    }
    pub fn from_ty(arg_ty: Ty<'tcx>) -> Self {
        ArgTy::Simple(arg_ty)
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
