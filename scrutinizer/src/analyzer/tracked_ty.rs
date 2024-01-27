use itertools::Itertools;
use log::debug;
use rustc_middle::mir::{AggregateKind, BinOp, Body, NullOp, Operand, Place, Rvalue, UnOp};
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::DefId;
use serde::ser::{Serialize, SerializeStructVariant};
use std::collections::HashSet;
use std::fmt::Debug;

use super::normalized_place::NormalizedPlace;
use super::ty_ext::TyExt;
use super::type_tracker::TypeTracker;
use super::upvar_tracker::UpvarTrackerRef;

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
            TrackedTy::Erased(.., deps) => deps.iter().cloned().collect_vec(),
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
        match self {
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
                let mut tv = serializer.serialize_struct_variant("TrackedTy", 0, "Simple", 1)?;
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

pub trait HasTrackedTy<'tcx> {
    fn tracked_ty(
        &self,
        type_tracker: &mut TypeTracker<'tcx>,
        upvar_tracker: UpvarTrackerRef<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx>;
}

impl<'tcx> HasTrackedTy<'tcx> for Place<'tcx> {
    fn tracked_ty(
        &self,
        type_tracker: &mut TypeTracker<'tcx>,
        _upvar_tracker: UpvarTrackerRef<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx> {
        type_tracker
            .places
            .get(&NormalizedPlace::from_place(self, tcx, def_id))
            .and_then(|ty| Some(ty.to_owned()))
            .unwrap_or(TrackedTy::from_ty(self.ty(body, tcx).ty).to_owned())
    }
}

impl<'tcx> HasTrackedTy<'tcx> for Operand<'tcx> {
    fn tracked_ty(
        &self,
        type_tracker: &mut TypeTracker<'tcx>,
        upvar_tracker: UpvarTrackerRef<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx> {
        match self {
            &Operand::Copy(ref l) | &Operand::Move(ref l) => {
                l.tracked_ty(type_tracker, upvar_tracker, body, def_id, tcx)
            }
            Operand::Constant(c) => TrackedTy::from_ty(c.literal.ty()),
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
        _type_tracker: &mut TypeTracker<'tcx>,
        _upvar_tracker: UpvarTrackerRef<'tcx>,
        _body: &Body<'tcx>,
        _def_id: DefId,
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
                assert_eq!(self.lhs_ty, self.rhs_ty);
                self.lhs_ty.to_owned()
            }
            BinOp::Shl | BinOp::Shr | BinOp::Offset => {
                self.lhs_ty.to_owned() // lhs_ty can be != rhs_ty
            }
            BinOp::Eq | BinOp::Lt | BinOp::Le | BinOp::Ne | BinOp::Ge | BinOp::Gt => {
                TrackedTy::Present(tcx.types.bool)
            }
        }
    }
}

impl<'tcx> HasTrackedTy<'tcx> for Rvalue<'tcx> {
    fn tracked_ty(
        &self,
        type_tracker: &mut TypeTracker<'tcx>,
        upvar_tracker: UpvarTrackerRef<'tcx>,
        body: &Body<'tcx>,
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx> {
        match *self {
            Rvalue::Use(ref operand) => {
                operand.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx)
            }
            Rvalue::Repeat(ref operand, count) => {
                let tracked_ty =
                    operand.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx);
                tracked_ty.map(|ty| tcx.mk_array_with_const_len(ty, count))
            }
            Rvalue::ThreadLocalRef(did) => TrackedTy::from_ty(tcx.thread_local_ptr_ty(did)),
            Rvalue::Ref(reg, bk, ref place) => {
                let place_tracked_ty =
                    place.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx);
                place_tracked_ty.map(|place_ty| {
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
                let place_tracked_ty =
                    place.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx);
                place_tracked_ty.map(|place_ty| {
                    tcx.mk_ptr(ty::TypeAndMut {
                        ty: place_ty,
                        mutbl: mutability,
                    })
                })
            }
            Rvalue::Len(..) => TrackedTy::from_ty(tcx.types.usize),
            Rvalue::Cast(.., ref operand, _) => {
                operand.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx)
            }
            Rvalue::BinaryOp(op, box (ref lhs, ref rhs)) => {
                let lhs_tracked_ty =
                    lhs.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx);
                let rhs_tracked_ty =
                    rhs.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx);
                BinOpWithTys::new(op, lhs_tracked_ty, rhs_tracked_ty).tracked_ty(
                    type_tracker,
                    upvar_tracker.clone(),
                    body,
                    def_id,
                    tcx,
                )
            }
            Rvalue::CheckedBinaryOp(op, box (ref lhs, ref rhs)) => {
                let lhs_tracked_ty =
                    lhs.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx);
                let rhs_tracked_ty =
                    rhs.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx);
                let tracked_ty = BinOpWithTys::new(op, lhs_tracked_ty, rhs_tracked_ty).tracked_ty(
                    type_tracker,
                    upvar_tracker.clone(),
                    body,
                    def_id,
                    tcx,
                );
                tracked_ty.map(|ty| tcx.mk_tup(&[ty, tcx.types.bool]))
            }
            Rvalue::UnaryOp(UnOp::Not | UnOp::Neg, ref operand) => {
                operand.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx)
            }
            Rvalue::Discriminant(ref place) => {
                let place_tracked_ty =
                    place.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx);
                place_tracked_ty.map(|ty| ty.discriminant_ty(tcx))
            }
            Rvalue::NullaryOp(NullOp::SizeOf | NullOp::AlignOf, _) => {
                TrackedTy::from_ty(tcx.types.usize)
            }
            Rvalue::Aggregate(ref ak, ref ops) => match **ak {
                AggregateKind::Array(ty) => TrackedTy::from_ty(tcx.mk_array(ty, ops.len() as u64)),
                // TODO: this explicitly drops dependency information, while ideally it shouldn't.
                AggregateKind::Tuple => TrackedTy::from_ty(tcx.mk_tup_from_iter(ops.iter().map(
                    |op| match op.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx)
                    {
                        TrackedTy::Present(ty) => ty,
                        TrackedTy::Erased(ty, _) => ty,
                    },
                ))),
                // TODO: this implicityly drops dependency information.
                AggregateKind::Adt(did, _, substs, _, _) => {
                    TrackedTy::from_ty(tcx.type_of(did).subst(tcx, substs))
                }
                // TODO: we need to track upvars here.
                AggregateKind::Closure(did, substs) => {
                    let closure_ty = tcx.mk_closure(did, substs);
                    let upvar_tys = ops
                        .into_iter()
                        .map(|operand| {
                            operand.tracked_ty(
                                type_tracker,
                                upvar_tracker.clone(),
                                body,
                                def_id,
                                tcx,
                            )
                        })
                        .collect_vec();
                    debug!("making a closure={:?}, upvars={:?}", closure_ty, &upvar_tys);
                    upvar_tracker.borrow_mut().insert(closure_ty, upvar_tys);
                    TrackedTy::from_ty(closure_ty)
                }
                // TODO: this implicityly drops dependency information.
                AggregateKind::Generator(did, substs, movability) => {
                    TrackedTy::from_ty(tcx.mk_generator(did, substs, movability))
                }
            },
            // TODO: this could drop dependency information.
            Rvalue::ShallowInitBox(_, ty) => TrackedTy::from_ty(tcx.mk_box(ty)),
            Rvalue::CopyForDeref(ref place) => {
                place.tracked_ty(type_tracker, upvar_tracker.clone(), body, def_id, tcx)
            }
        }
    }
}
