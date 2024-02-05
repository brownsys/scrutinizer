use itertools::Itertools;
use log::debug;
use rustc_middle::ty::Ty;
use serde::ser::{Serialize, SerializeStructVariant};
use std::collections::HashSet;
use std::fmt::Debug;

use super::ty_ext::TyExt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackedTy<'tcx> {
    Present(Ty<'tcx>),
    Erased(Ty<'tcx>, HashSet<Ty<'tcx>>),
}

impl<'tcx> TrackedTy<'tcx> {
    pub fn from_ty(ty: Ty<'tcx>) -> Self {
        if ty.contains_erased() {
            TrackedTy::Erased(ty, HashSet::new())
        } else {
            TrackedTy::Present(ty)
        }
    }
    pub fn into_vec(&self) -> Vec<Ty<'tcx>> {
        match self {
            TrackedTy::Present(ty) => vec![ty.to_owned()],
            TrackedTy::Erased(ty, deps) => {
                if deps.is_empty() {
                    vec![ty.to_owned()]
                } else {
                    deps.iter().cloned().collect_vec()
                }
            }
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
            TrackedTy::Erased(ty, deps) => {
                let new_ty = lambda(ty.to_owned());
                if new_ty.contains_erased() {
                    TrackedTy::Erased(
                        new_ty,
                        deps.iter().map(|ty| lambda(ty.to_owned())).collect(),
                    )
                } else {
                    TrackedTy::Present(new_ty)
                }
            }
        }
    }
    pub fn poisoned(&self) -> bool {
        // If one of the influences in the erased type is erased itself,
        // we consider it poisoned, as it can never be resolved with certainty.
        match self {
            TrackedTy::Present(_) => false,
            TrackedTy::Erased(_, deps) => deps.iter().any(|ty| ty.contains_erased()),
        }
    }
    pub fn spread_tuple(&self) -> Vec<TrackedTy<'tcx>> {
        let spread = match self {
            TrackedTy::Present(ty) => ty
                .tuple_fields()
                .iter()
                .map(|ty| TrackedTy::from_ty(ty))
                .collect(),
            TrackedTy::Erased(ty, deps) => {
                let mut base_tys = ty
                    .tuple_fields()
                    .iter()
                    .map(|ty| TrackedTy::from_ty(ty))
                    .collect_vec();
                deps.iter().for_each(|dep_ty| {
                    let dep_tys_instance = dep_ty
                        .tuple_fields()
                        .iter()
                        .map(|ty| TrackedTy::from_ty(ty))
                        .collect_vec();
                    dep_tys_instance.iter().zip(base_tys.iter_mut()).for_each(
                        |(dep_ty, base_ty)| {
                            base_ty.join(dep_ty);
                        },
                    );
                });
                base_tys
            }
        };
        debug!("spread tuple: from {:?} to {:?}", self, spread);
        spread
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
            TrackedTy::Erased(ref ty, ref vec_ty) => {
                let mut tv = serializer.serialize_struct_variant("TrackedTy", 1, "Erased", 2)?;
                tv.serialize_field("ty", format!("{:?}", ty).as_str())?;
                tv.serialize_field(
                    "deps",
                    &vec_ty.iter().map(|ty| format!("{:?}", ty)).collect_vec(),
                )?;
                tv.end()
            }
        }
    }
}
