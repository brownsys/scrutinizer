use serde::ser::{Serialize, SerializeStructVariant};

use rustc_middle::mir::{Location, Operand};
use rustc_middle::ty::{Ty, TyCtxt};

use super::fn_data::FnData;
use super::ty_ext::TyExt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefinedTy<'tcx> {
    Simple(Ty<'tcx>),
    Erased(Ty<'tcx>, Vec<Ty<'tcx>>),
}

impl<'tcx> RefinedTy<'tcx> {
    pub fn from_known_or_erased(
        operand: &Operand<'tcx>,
        location: &Location,
        current_fn: &FnData<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Self {
        let outer_body = tcx.optimized_mir(current_fn.get_instance().def_id());
        let operand_ty = operand.ty(outer_body, tcx);
        // Check whether argument type was erased.
        if operand_ty.contains_erased() {
            let backward_deps = operand
                .place()
                .and_then(|place| Some(current_fn.backward_deps_for(place, location, tcx)))
                .unwrap_or(vec![]);
            RefinedTy::Erased(operand_ty, backward_deps)
        } else {
            RefinedTy::Simple(operand_ty)
        }
    }
    pub fn from_known(operand_ty: Ty<'tcx>) -> Self {
        RefinedTy::Simple(operand_ty)
    }
    pub fn is_poisoned(&self) -> bool {
        // If one of the influences in the erased type is erased itself,
        // we consider it poisoned, as it can never be resolved with certainty.
        // TODO: Can it ever happen if we reject functions with generic arguments?
        match self {
            RefinedTy::Simple(_) => false,
            RefinedTy::Erased(_, influences) => influences.iter().any(|ty| ty.contains_erased()),
        }
    }
    pub fn into_vec(self) -> Vec<Ty<'tcx>> {
        match self {
            RefinedTy::Simple(ty) => vec![ty],
            RefinedTy::Erased(ty, subst_tys) => subst_tys.into_iter().chain([ty]).collect(),
        }
    }
}

impl<'tcx> Serialize for RefinedTy<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            RefinedTy::Simple(ref ty) => {
                let mut tv = serializer.serialize_struct_variant("RefinedTy", 0, "Simple", 1)?;
                tv.serialize_field("ty", format!("{:?}", ty).as_str())?;
                tv.end()
            }
            RefinedTy::Erased(ref ty, ref vec_ty) => {
                let mut tv = serializer.serialize_struct_variant("RefinedTy", 1, "Erased", 2)?;
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
