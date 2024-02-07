use itertools::Itertools;
use rustc_middle::ty::Ty;
use serde::ser::{Serialize, SerializeStructVariant};
use std::collections::HashSet;
use std::fmt::Debug;

use super::ty_ext::TyExt;
use crate::util::transpose;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackedTy<'tcx> {
    Present(Ty<'tcx>),
    Erased(HashSet<Ty<'tcx>>),
}

impl<'tcx> TrackedTy<'tcx> {
    pub fn from_ty(ty: Ty<'tcx>) -> Self {
        if ty.contains_erased() {
            TrackedTy::Erased(HashSet::new())
        } else {
            TrackedTy::Present(ty)
        }
    }
    pub fn into_vec(&self) -> Vec<Ty<'tcx>> {
        match self {
            TrackedTy::Present(ty) => vec![ty.to_owned()],
            TrackedTy::Erased(deps) => deps.iter().cloned().collect_vec(),
        }
    }
    pub fn join(&mut self, other: &Self) -> bool {
        match self {
            TrackedTy::Present(..) => false,
            TrackedTy::Erased(.., deps_self) => match other {
                TrackedTy::Present(ty_other) => deps_self.insert(ty_other.to_owned()),
                TrackedTy::Erased(.., deps_other) => {
                    deps_other.iter().fold(false, |updated, dep_other| {
                        deps_self.insert(dep_other.to_owned()) || updated
                    })
                }
            },
        }
    }
    pub fn map(&self, lambda: impl Fn(Ty<'tcx>) -> Ty<'tcx>) -> TrackedTy<'tcx> {
        match self {
            TrackedTy::Present(ty) => TrackedTy::from_ty(lambda(ty.to_owned())),
            TrackedTy::Erased(deps) => {
                TrackedTy::Erased(deps.iter().map(|ty| lambda(ty.to_owned())).collect())
            }
        }
    }
    pub fn poisoned(&self) -> bool {
        // If one of the influences in the erased type is erased itself,
        // we consider it poisoned, as it can never be resolved with certainty.
        match self {
            TrackedTy::Present(_) => false,
            TrackedTy::Erased(deps) => deps.iter().any(|ty| ty.contains_erased()),
        }
    }
    pub fn spread_tuple(&self) -> Vec<TrackedTy<'tcx>> {
        match self {
            TrackedTy::Present(ty) => ty
                .tuple_fields()
                .iter()
                .map(|ty| TrackedTy::from_ty(ty))
                .collect(),
            TrackedTy::Erased(deps) => {
                if !deps.is_empty() {
                    let spread = deps
                        .iter()
                        .map(|dep_ty| dep_ty.tuple_fields().into_iter().collect_vec())
                        .collect_vec();
                    transpose(spread)
                        .into_iter()
                        .map(|v| TrackedTy::Erased(HashSet::from_iter(v.into_iter())))
                        .collect_vec()
                } else {
                    vec![]
                }
            }
        }
    }
}

impl<'tcx> Serialize for TrackedTy<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            TrackedTy::Present(ref ty) => {
                let mut tv = serializer.serialize_struct_variant("TrackedTy", 0, "Present", 1)?;
                tv.serialize_field("ty", format!("{:?}", ty).as_str())?;
                tv.end()
            }
            TrackedTy::Erased(ref deps) => {
                let mut tv = serializer.serialize_struct_variant("TrackedTy", 1, "Erased", 1)?;
                tv.serialize_field(
                    "deps",
                    &deps.iter().map(|ty| format!("{:?}", ty)).collect_vec(),
                )?;
                tv.end()
            }
        }
    }
}