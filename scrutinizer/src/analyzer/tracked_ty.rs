use itertools::Itertools;
use rustc_middle::mir::{AggregateKind, BinOp, Body, NullOp, Operand, Place, Rvalue, UnOp};
use rustc_middle::ty::{self, Ty, TyCtxt};
use serde::ser::{Serialize, SerializeStructVariant};
use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;

use super::ty_ext::TyExt;
use super::type_tracker::TrackedTypeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tracked<T: Debug + Eq + Hash + Clone> {
    Simple(T),
    Erased(T, HashSet<T>),
}

impl<T: Debug + Eq + Hash + Clone> Tracked<T> {
    pub fn join(&mut self, other: &Self) -> bool {
        let changed = match self {
            Tracked::Simple(..) => false,
            Tracked::Erased(.., deps_self) => match other {
                Tracked::Simple(v_other) => deps_self.insert(v_other.to_owned()),
                Tracked::Erased(.., deps_other) => deps_other
                    .iter()
                    .fold(false, |acc, elt| deps_self.insert(elt.to_owned()) || acc),
            },
        };
        changed
    }
    pub fn into_vec(self) -> Vec<T> {
        match self {
            Tracked::Simple(v) => vec![v],
            Tracked::Erased(.., deps) => deps.into_iter().collect_vec(),
        }
    }
}

pub type TrackedTy<'tcx> = Tracked<Ty<'tcx>>;

impl<'tcx> TrackedTy<'tcx> {
    pub fn apply(&self, lambda: impl Fn(Ty<'tcx>) -> Ty<'tcx>) -> TrackedTy<'tcx> {
        match self {
            TrackedTy::Simple(v) => TrackedTy::determine(lambda(v.to_owned())),
            TrackedTy::Erased(v, deps) => {
                let new_ty = lambda(v.to_owned());
                if new_ty.contains_erased() {
                    TrackedTy::Erased(
                        new_ty,
                        deps.iter().map(|ty| lambda(ty.to_owned())).collect(),
                    )
                } else {
                    TrackedTy::Simple(new_ty)
                }
            }
        }
    }
    pub fn determine(ty: Ty<'tcx>) -> Self {
        if ty.contains_erased() {
            TrackedTy::Erased(ty, HashSet::new())
        } else {
            TrackedTy::Simple(ty)
        }
    }
    pub fn poisoned(&self) -> bool {
        // If one of the influences in the erased type is erased itself,
        // we consider it poisoned, as it can never be resolved with certainty.
        match self {
            TrackedTy::Simple(_) => false,
            TrackedTy::Erased(_, influences) => influences.iter().any(|ty| ty.contains_erased()),
        }
    }
    pub fn spread(&self) -> Vec<TrackedTy<'tcx>> {
        match self {
            TrackedTy::Simple(ty) => ty
                .tuple_fields()
                .iter()
                .map(|ty| TrackedTy::determine(ty))
                .collect(),
            TrackedTy::Erased(ty, deps) => {
                let mut base_tys = ty
                    .tuple_fields()
                    .iter()
                    .map(|ty| TrackedTy::determine(ty))
                    .collect_vec();
                deps.iter().for_each(|dep_ty| {
                    let dep_tys_instance = dep_ty
                        .tuple_fields()
                        .iter()
                        .map(|ty| TrackedTy::determine(ty))
                        .collect_vec();
                    dep_tys_instance.iter().zip(base_tys.iter_mut()).for_each(
                        |(dep_ty, base_ty)| {
                            base_ty.join(dep_ty);
                        },
                    );
                });
                base_tys
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
            TrackedTy::Simple(ref ty) => {
                let mut tv = serializer.serialize_struct_variant("TrackedTy", 0, "Simple", 1)?;
                tv.serialize_field("ty", format!("{:?}", ty).as_str())?;
                tv.end()
            }
            TrackedTy::Erased(ref ty, ref vec_ty) => {
                let mut tv = serializer.serialize_struct_variant("TrackedTy", 1, "Erased", 2)?;
                tv.serialize_field("ty", format!("{:?}", ty).as_str())?;
                tv.serialize_field(
                    "inlfluences",
                    &vec_ty.iter().map(|ty| format!("{:?}", ty)).collect_vec(),
                )?;
                tv.end()
            }
        }
    }
}

pub trait HasTrackedTy<'tcx> {
    fn tracked_ty(
        &self,
        tracked_tys: &TrackedTypeMap<'tcx>,
        body: &Body<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx>;
}

impl<'tcx> HasTrackedTy<'tcx> for Place<'tcx> {
    fn tracked_ty(
        &self,
        tracked_tys: &TrackedTypeMap<'tcx>,
        body: &Body<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx> {
        tracked_tys
            .map
            .get(&self)
            .and_then(|ty| Some(ty.to_owned()))
            .unwrap_or(TrackedTy::determine(self.ty(body, tcx).ty).to_owned())
    }
}

impl<'tcx> HasTrackedTy<'tcx> for Operand<'tcx> {
    fn tracked_ty(
        &self,
        tracked_tys: &TrackedTypeMap<'tcx>,
        body: &Body<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx> {
        match self {
            &Operand::Copy(ref l) | &Operand::Move(ref l) => l.tracked_ty(tracked_tys, body, tcx),
            Operand::Constant(c) => TrackedTy::determine(c.literal.ty()),
        }
    }
}

struct BinOpWithTys<'tcx> {
    op: BinOp,
    lhs_ty: TrackedTy<'tcx>,
    rhs_ty: TrackedTy<'tcx>,
}

impl<'tcx> BinOpWithTys<'tcx> {
    fn new(op: BinOp, lhs_ty: TrackedTy<'tcx>, rhs_ty: TrackedTy<'tcx>) -> Self {
        BinOpWithTys { op, lhs_ty, rhs_ty }
    }
}

impl<'tcx> HasTrackedTy<'tcx> for BinOpWithTys<'tcx> {
    fn tracked_ty(
        &self,
        _tracked_tys: &TrackedTypeMap<'tcx>,
        _body: &Body<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx> {
        match self.op {
            BinOp::Add
            | BinOp::Sub
            | BinOp::Mul
            | BinOp::Div
            | BinOp::Rem
            | BinOp::BitXor
            | BinOp::BitAnd
            | BinOp::BitOr => {
                // TODO: should we perform this check?
                assert_eq!(self.lhs_ty, self.rhs_ty);
                self.lhs_ty.to_owned()
            }
            BinOp::Shl | BinOp::Shr | BinOp::Offset => {
                self.lhs_ty.to_owned() // lhs_ty can be != rhs_ty
            }
            BinOp::Eq | BinOp::Lt | BinOp::Le | BinOp::Ne | BinOp::Ge | BinOp::Gt => {
                TrackedTy::Simple(tcx.types.bool)
            }
        }
    }
}

impl<'tcx> HasTrackedTy<'tcx> for Rvalue<'tcx> {
    fn tracked_ty(
        &self,
        tracked_tys: &TrackedTypeMap<'tcx>,
        body: &Body<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx> {
        match *self {
            Rvalue::Use(ref operand) => operand.tracked_ty(tracked_tys, body, tcx),
            Rvalue::Repeat(ref operand, count) => {
                let tracked_ty = operand.tracked_ty(tracked_tys, body, tcx);
                tracked_ty.apply(|ty| tcx.mk_array_with_const_len(ty, count))
            }
            Rvalue::ThreadLocalRef(did) => TrackedTy::determine(tcx.thread_local_ptr_ty(did)),
            Rvalue::Ref(reg, bk, ref place) => {
                let place_tracked_ty = place.tracked_ty(tracked_tys, body, tcx);
                place_tracked_ty.apply(|place_ty| {
                    tcx.mk_ref(
                        reg,
                        ty::TypeAndMut {
                            ty: place_ty,
                            mutbl: bk.to_mutbl_lossy(),
                        },
                    )
                })
            }
            Rvalue::AddressOf(mutability, ref place) => {
                let place_tracked_ty = place.tracked_ty(tracked_tys, body, tcx);
                place_tracked_ty.apply(|place_ty| {
                    tcx.mk_ptr(ty::TypeAndMut {
                        ty: place_ty,
                        mutbl: mutability,
                    })
                })
            }
            Rvalue::Len(..) => TrackedTy::determine(tcx.types.usize),
            Rvalue::Cast(.., ref operand, _) => operand.tracked_ty(tracked_tys, body, tcx),
            Rvalue::BinaryOp(op, box (ref lhs, ref rhs)) => {
                let lhs_tracked_ty = lhs.tracked_ty(tracked_tys, body, tcx);
                let rhs_tracked_ty = rhs.tracked_ty(tracked_tys, body, tcx);
                BinOpWithTys::new(op, lhs_tracked_ty, rhs_tracked_ty).tracked_ty(
                    tracked_tys,
                    body,
                    tcx,
                )
            }
            Rvalue::CheckedBinaryOp(op, box (ref lhs, ref rhs)) => {
                let lhs_tracked_ty = lhs.tracked_ty(tracked_tys, body, tcx);
                let rhs_tracked_ty = rhs.tracked_ty(tracked_tys, body, tcx);
                let tracked_ty = BinOpWithTys::new(op, lhs_tracked_ty, rhs_tracked_ty).tracked_ty(
                    tracked_tys,
                    body,
                    tcx,
                );
                tracked_ty.apply(|ty| tcx.mk_tup(&[ty, tcx.types.bool]))
            }
            Rvalue::UnaryOp(UnOp::Not | UnOp::Neg, ref operand) => {
                operand.tracked_ty(tracked_tys, body, tcx)
            }
            Rvalue::Discriminant(ref place) => {
                let place_tracked_ty = place.tracked_ty(tracked_tys, body, tcx);
                place_tracked_ty.apply(|ty| ty.discriminant_ty(tcx))
            }
            Rvalue::NullaryOp(NullOp::SizeOf | NullOp::AlignOf, _) => {
                TrackedTy::determine(tcx.types.usize)
            }
            Rvalue::Aggregate(ref ak, ref ops) => TrackedTy::determine(match **ak {
                AggregateKind::Array(ty) => tcx.mk_array(ty, ops.len() as u64),
                AggregateKind::Tuple => tcx.mk_tup_from_iter(ops.iter().map(|op| {
                    match op.tracked_ty(tracked_tys, body, tcx) {
                        TrackedTy::Simple(ty) => ty,
                        TrackedTy::Erased(ty, _) => ty,
                    }
                })),
                AggregateKind::Adt(did, _, substs, _, _) => tcx.type_of(did).subst(tcx, substs),
                AggregateKind::Closure(did, substs) => tcx.mk_closure(did, substs),
                AggregateKind::Generator(did, substs, movability) => {
                    tcx.mk_generator(did, substs, movability)
                }
            }),
            Rvalue::ShallowInitBox(_, ty) => TrackedTy::determine(tcx.mk_box(ty)),
            Rvalue::CopyForDeref(ref place) => place.tracked_ty(tracked_tys, body, tcx),
        }
    }
}
