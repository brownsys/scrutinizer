use itertools::Itertools;
use rustc_middle::mir::{AggregateKind, BinOp, CastKind, NullOp, Operand, Place, Rvalue, UnOp};
use rustc_middle::ty::{self, TyCtxt};
use std::collections::HashSet;

use crate::body_cache::substituted_mir;
use crate::collector::collector_domain::CollectorDomain;
use crate::common::storage::ClosureInfoStorageRef;
use crate::common::{NormalizedPlace, TrackedTy};

pub trait HasTrackedTy<'tcx> {
    fn tracked_ty(
        &self,
        type_tracker: &mut CollectorDomain<'tcx>,
        closure_info_storage: ClosureInfoStorageRef<'tcx>,
        instance: &ty::Instance<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx>;
}

impl<'tcx> HasTrackedTy<'tcx> for Place<'tcx> {
    fn tracked_ty(
        &self,
        type_tracker: &mut CollectorDomain<'tcx>,
        _closure_info_storage: ClosureInfoStorageRef<'tcx>,
        instance: &ty::Instance<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx> {
        let body = substituted_mir(instance, tcx);
        type_tracker
            .get(&NormalizedPlace::from_place(self, tcx, instance.def_id()))
            .and_then(|ty| Some(ty.to_owned()))
            .unwrap_or(TrackedTy::from_ty(self.ty(&body, tcx).ty).to_owned())
    }
}

impl<'tcx> HasTrackedTy<'tcx> for Operand<'tcx> {
    fn tracked_ty(
        &self,
        type_tracker: &mut CollectorDomain<'tcx>,
        closure_info_storage: ClosureInfoStorageRef<'tcx>,
        instance: &ty::Instance<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx> {
        match self {
            &Operand::Copy(ref l) | &Operand::Move(ref l) => {
                l.tracked_ty(type_tracker, closure_info_storage, instance, tcx)
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
        _type_tracker: &mut CollectorDomain<'tcx>,
        _closure_info_storage: ClosureInfoStorageRef<'tcx>,
        _instance: &ty::Instance<'tcx>,
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
            | BinOp::BitOr
            | BinOp::AddUnchecked
            | BinOp::MulUnchecked
            | BinOp::ShlUnchecked
            | BinOp::ShrUnchecked
            | BinOp::SubUnchecked => {
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
        type_tracker: &mut CollectorDomain<'tcx>,
        closure_info_storage: ClosureInfoStorageRef<'tcx>,
        instance: &ty::Instance<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx> {
        match *self {
            Rvalue::Use(ref operand) => {
                operand.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx)
            }
            Rvalue::Repeat(ref operand, count) => {
                let tracked_ty =
                    operand.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
                tracked_ty.map(|ty| ty::Ty::new_array_with_const_len(tcx, ty, count))
            }
            Rvalue::ThreadLocalRef(did) => {
                let ty = tcx.thread_local_ptr_ty(did);
                TrackedTy::from_ty(ty)
            }
            Rvalue::Ref(reg, bk, ref place) => {
                let place_tracked_ty =
                    place.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
                place_tracked_ty.map(|place_ty| {
                    ty::Ty::new_ref(
                        tcx,
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
                    place.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
                place_tracked_ty.map(|place_ty| {
                    ty::Ty::new_ptr(
                        tcx,
                        ty::TypeAndMut {
                            ty: place_ty,
                            mutbl: mutability,
                        },
                    )
                })
            }
            Rvalue::Len(..) => TrackedTy::from_ty(tcx.types.usize),
            Rvalue::Cast(cast_kind, ref operand, ref ty) => {
                let tracked_ty =
                    operand.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
                match cast_kind {
                    CastKind::PointerFromExposedAddress | CastKind::PointerExposeAddress => {
                        TrackedTy::from_ty(ty.to_owned())
                    }
                    CastKind::Transmute => {
                        if ty.is_fn_ptr() {
                            tracked_ty
                        } else {
                            TrackedTy::from_ty(ty.to_owned())
                        }
                    }
                    _ => tracked_ty,
                }
            }
            Rvalue::BinaryOp(op, box (ref lhs, ref rhs)) => {
                let lhs_tracked_ty =
                    lhs.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
                let rhs_tracked_ty =
                    rhs.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
                BinOpWithTys::new(op, lhs_tracked_ty, rhs_tracked_ty).tracked_ty(
                    type_tracker,
                    closure_info_storage.clone(),
                    instance,
                    tcx,
                )
            }
            Rvalue::CheckedBinaryOp(op, box (ref lhs, ref rhs)) => {
                let lhs_tracked_ty =
                    lhs.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
                let rhs_tracked_ty =
                    rhs.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
                let tracked_ty = BinOpWithTys::new(op, lhs_tracked_ty, rhs_tracked_ty).tracked_ty(
                    type_tracker,
                    closure_info_storage.clone(),
                    instance,
                    tcx,
                );
                tracked_ty.map(|ty| ty::Ty::new_tup(tcx, &[ty, tcx.types.bool]))
            }
            Rvalue::UnaryOp(UnOp::Not | UnOp::Neg, ref operand) => {
                operand.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx)
            }
            Rvalue::Discriminant(ref place) => {
                let place_tracked_ty =
                    place.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
                place_tracked_ty.map(|ty| ty.discriminant_ty(tcx))
            }
            Rvalue::NullaryOp(NullOp::SizeOf | NullOp::AlignOf | NullOp::OffsetOf(_), _) => {
                TrackedTy::from_ty(tcx.types.usize)
            }
            Rvalue::Aggregate(ref ak, ref ops) => match **ak {
                AggregateKind::Array(ty) => {
                    TrackedTy::from_ty(ty::Ty::new_array(tcx, ty, ops.len() as u64))
                }
                AggregateKind::Tuple => {
                    let op_tys = ops
                        .iter()
                        .map(|op| {
                            op.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx)
                        })
                        .collect_vec();
                    let all_present = op_tys.iter().all(|ty| {
                        if let TrackedTy::Present(..) = ty {
                            true
                        } else {
                            false
                        }
                    });
                    let transformed_ty = if all_present {
                        let ops = op_tys.iter().map(|ty| match ty {
                            TrackedTy::Present(ty) => ty.to_owned(),
                            _ => unreachable!(),
                        });
                        TrackedTy::from_ty(ty::Ty::new_tup_from_iter(tcx, ops))
                    } else {
                        let deps = HashSet::from_iter(
                            op_tys
                                .iter()
                                .map(|ty| match ty {
                                    TrackedTy::Present(ty) => vec![ty.to_owned()],
                                    TrackedTy::Erased(.., deps) => {
                                        deps.iter().cloned().collect_vec()
                                    }
                                })
                                .multi_cartesian_product()
                                .map(|tuple| ty::Ty::new_tup_from_iter(tcx, tuple.into_iter())),
                        );
                        TrackedTy::Erased(deps)
                    };
                    transformed_ty
                }
                AggregateKind::Adt(did, _, args, _, _) => TrackedTy::from_ty(instance.subst_mir(
                    tcx,
                    ty::EarlyBinder::bind(&tcx.type_of(did).instantiate(tcx, args)),
                )),
                AggregateKind::Closure(did, substs) => {
                    let closure_ty = ty::Ty::new_closure(tcx, did, substs);
                    let upvar_tys = ops
                        .into_iter()
                        .map(|operand| {
                            operand.tracked_ty(
                                type_tracker,
                                closure_info_storage.clone(),
                                instance,
                                tcx,
                            )
                        })
                        .collect_vec();
                    closure_info_storage
                        .borrow_mut()
                        .update_with(closure_ty, instance, upvar_tys, tcx);
                    TrackedTy::from_ty(closure_ty)
                }
                AggregateKind::Generator(..) => {
                    panic!("generators are not supported")
                }
            },
            Rvalue::ShallowInitBox(_, ty) => TrackedTy::from_ty(ty::Ty::new_box(tcx, ty)),
            Rvalue::CopyForDeref(ref place) => {
                place.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx)
            }
        }
    }
}
