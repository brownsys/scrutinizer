use itertools::Itertools;
use rustc_middle::mir::{AggregateKind, BinOp, CastKind, NullOp, Operand, Place, Rvalue, UnOp};
use rustc_middle::ty::{self, TyCtxt};
use std::collections::HashSet;

use super::closure_info::ClosureInfoStorageRef;
use super::normalized_place::NormalizedPlace;
use super::tracked_ty::TrackedTy;
use super::type_tracker::TypeTracker;

pub trait HasTrackedTy<'tcx> {
    fn tracked_ty(
        &self,
        type_tracker: &mut TypeTracker<'tcx>,
        closure_info_storage: ClosureInfoStorageRef<'tcx>,
        instance: &ty::Instance<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx>;
}

impl<'tcx> HasTrackedTy<'tcx> for Place<'tcx> {
    fn tracked_ty(
        &self,
        type_tracker: &mut TypeTracker<'tcx>,
        _closure_info_storage: ClosureInfoStorageRef<'tcx>,
        instance: &ty::Instance<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> TrackedTy<'tcx> {
        let def_id = instance.def_id();
        let body = instance.subst_mir_and_normalize_erasing_regions(
            tcx,
            ty::ParamEnv::reveal_all(),
            tcx.optimized_mir(def_id).to_owned(),
        );
        type_tracker
            .get(&NormalizedPlace::from_place(self, tcx, def_id))
            .and_then(|ty| Some(ty.to_owned()))
            .unwrap_or(TrackedTy::from_ty(self.ty(&body, tcx).ty).to_owned())
    }
}

impl<'tcx> HasTrackedTy<'tcx> for Operand<'tcx> {
    fn tracked_ty(
        &self,
        type_tracker: &mut TypeTracker<'tcx>,
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
        _type_tracker: &mut TypeTracker<'tcx>,
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
                tracked_ty.map(|ty| tcx.mk_array_with_const_len(ty, count))
            }
            Rvalue::ThreadLocalRef(did) => {
                let ty = tcx.thread_local_ptr_ty(did);
                // if ty.is_closure() {
                //     let closure_def_id = if let ty::TyKind::Closure(def_id, ..) = ty.kind() {
                //         def_id.to_owned()
                //     } else {
                //         unreachable!();
                //     };
                //     let resolved_closure_ty = instance.subst_mir_and_normalize_erasing_regions(
                //         tcx,
                //         ty::ParamEnv::reveal_all(),
                //         ty,
                //     );
                //     closure_info_storage.borrow_mut().insert(
                //         closure_def_id,
                //         ClosureInfo {
                //             upvars: vec![],
                //             with_substs: resolved_closure_ty,
                //         },
                //     );
                // };
                TrackedTy::from_ty(ty)
            }
            Rvalue::Ref(reg, bk, ref place) => {
                let place_tracked_ty =
                    place.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
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
                    place.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
                place_tracked_ty.map(|place_ty| {
                    tcx.mk_ptr(ty::TypeAndMut {
                        ty: place_ty,
                        mutbl: mutability,
                    })
                })
            }
            Rvalue::Len(..) => TrackedTy::from_ty(tcx.types.usize),
            // TODO: this is a potential point of failure, as not all cast kinds are covered.
            Rvalue::Cast(cast_kind, ref operand, ref ty) => {
                let tracked_ty =
                    operand.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
                match cast_kind {
                    CastKind::PointerFromExposedAddress
                    | CastKind::PointerExposeAddress
                    | CastKind::Transmute => TrackedTy::from_ty(ty.to_owned()),
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
                tracked_ty.map(|ty| tcx.mk_tup(&[ty, tcx.types.bool]))
            }
            Rvalue::UnaryOp(UnOp::Not | UnOp::Neg, ref operand) => {
                operand.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx)
            }
            Rvalue::Discriminant(ref place) => {
                let place_tracked_ty =
                    place.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx);
                place_tracked_ty.map(|ty| ty.discriminant_ty(tcx))
            }
            Rvalue::NullaryOp(NullOp::SizeOf | NullOp::AlignOf, _) => {
                TrackedTy::from_ty(tcx.types.usize)
            }
            Rvalue::Aggregate(ref ak, ref ops) => {
                match **ak {
                    AggregateKind::Array(ty) => {
                        TrackedTy::from_ty(tcx.mk_array(ty, ops.len() as u64))
                    }
                    AggregateKind::Tuple => {
                        let op_tys = ops
                            .iter()
                            .map(|op| {
                                op.tracked_ty(
                                    type_tracker,
                                    closure_info_storage.clone(),
                                    instance,
                                    tcx,
                                )
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
                            TrackedTy::from_ty(tcx.mk_tup_from_iter(ops))
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
                                    .map(|tuple| tcx.mk_tup_from_iter(tuple.into_iter())),
                            );
                            TrackedTy::Erased(deps)
                        };
                        transformed_ty
                    }
                    // TODO: this could implicitly drop dependency information,
                    //       as it does not consider which operands flow into it.
                    AggregateKind::Adt(did, _, substs, _, _) => {
                        TrackedTy::from_ty(instance.subst_mir_and_normalize_erasing_regions(
                            tcx,
                            ty::ParamEnv::reveal_all(),
                            tcx.type_of(did).subst(tcx, substs),
                        ))
                    }
                    AggregateKind::Closure(did, substs) => {
                        let closure_ty = tcx.mk_closure(did, substs);
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
                            .try_insert(closure_ty, instance, upvar_tys, tcx);
                        TrackedTy::from_ty(closure_ty)
                    }
                    AggregateKind::Generator(..) => {
                        panic!("generators are not supported")
                    }
                }
            }
            // TODO: this could implicitly drop dependency information,
            //       as it does not consider which operands flow into it.
            Rvalue::ShallowInitBox(_, ty) => TrackedTy::from_ty(tcx.mk_box(ty)),
            Rvalue::CopyForDeref(ref place) => {
                place.tracked_ty(type_tracker, closure_info_storage.clone(), instance, tcx)
            }
        }
    }
}
