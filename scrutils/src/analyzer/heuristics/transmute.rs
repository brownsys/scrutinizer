use regex::Regex;
use rustc_middle::mir::TerminatorKind;
use rustc_middle::mir::{visit::Visitor, Body, Location, Mutability, Terminator};
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
    fn visit_terminator(&mut self, terminator: &Terminator<'tcx>, location: Location) {
        if let TerminatorKind::Call { func, .. } = &terminator.kind {
            if let Some((def_id, generic_args)) = func.const_fn_def() {
                let def_path_str = format!("{:?}", def_id);
                let transmute_def_path =
                    Regex::new(r"core\[\w*\]::intrinsics::\{extern#0\}::transmute").unwrap();
                if transmute_def_path.is_match(&def_path_str) {
                    if contains_mut_ref(&generic_args[1].as_type().unwrap(), self.tcx) {
                        self.has_transmute = true;
                    }
                }
            }
        }
        self.super_terminator(terminator, location);
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
