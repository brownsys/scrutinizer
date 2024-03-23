use rustc_middle::mir::{visit::Visitor, Body, CastKind, Location, Mutability, Rvalue};
use rustc_middle::ty::{self, Ty, TyCtxt, TypeSuperVisitable, TypeVisitable, TypeVisitor};

use std::ops::ControlFlow;

struct TransmuteVisitor<'tcx> {
    tcx: TyCtxt<'tcx>,
    has_transmute: bool,
}

pub trait HasTransmute<'tcx> {
    fn has_transmute(&self, tcx: TyCtxt<'tcx>) -> bool;
}

impl<'tcx> HasTransmute<'tcx> for Body<'tcx> {
    fn has_transmute(&self, tcx: TyCtxt<'tcx>) -> bool {
        let mut ptr_deref_visitor = TransmuteVisitor {
            tcx,
            has_transmute: false,
        };
        ptr_deref_visitor.visit_body(self);
        ptr_deref_visitor.has_transmute
    }
}

impl<'a, 'tcx> Visitor<'tcx> for TransmuteVisitor<'tcx> {
    fn visit_rvalue(&mut self, rvalue: &Rvalue<'tcx>, location: Location) {
        if let Rvalue::Cast(cast_kind, _, ty) = rvalue {
            if let CastKind::Transmute = cast_kind {
                if contains_mut_ref(ty, self.tcx) {
                    self.has_transmute = true;
                }
            }
        };
        self.super_rvalue(rvalue, location);
    }
}

pub fn contains_mut_ref<'tcx>(ty: &Ty<'tcx>, tcx: TyCtxt<'tcx>) -> bool {
    struct ContainsMutRefVisitor<'tcx> {
        tcx: TyCtxt<'tcx>,
        has_mut_ref: bool,
    }

    impl<'tcx> TypeVisitor<TyCtxt<'tcx>> for ContainsMutRefVisitor<'tcx> {
        type BreakTy = ();

        fn visit_ty(&mut self, t: Ty<'tcx>) -> ControlFlow<Self::BreakTy> {
            if let ty::TyKind::Adt(adt_def, substs) = t.kind() {
                for field in adt_def.all_fields() {
                    field.ty(self.tcx, substs).visit_with(self)?;
                }
            }

            if let Some(Mutability::Mut) = t.ref_mutability() {
                self.has_mut_ref = true;
            }
            t.super_visit_with(self)
        }
    }

    let mut visitor = ContainsMutRefVisitor {
        tcx,
        has_mut_ref: false,
    };
    ty.visit_with(&mut visitor);
    visitor.has_mut_ref
}
