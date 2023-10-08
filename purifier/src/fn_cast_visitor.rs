use rustc_hir as hir;
use rustc_middle::mir as mir;
use rustc_middle::ty as ty;
use rustc_middle::mir::visit::Visitor;

struct FnCastVisitor<'tcx> {
    tcx: ty::TyCtxt<'tcx>,
    body: &'tcx mir::Body<'tcx>,
    fn_like_items: Vec<(hir::def_id::DefId, ty::subst::SubstsRef<'tcx>)>,
}

fn extract_fn_like_items<'tcx>(visitor: &mut FnCastVisitor<'tcx>, op: mir::Operand<'tcx>) {
    if let Some(fn_like_item) = op.ty(visitor.body, visitor.tcx).walk().find(|arg| {
        if let ty::subst::GenericArgKind::Type(ty) = arg.unpack() {
            ty.is_closure() || ty.is_fn()
        } else {
            false
        }
    }) {
        match fn_like_item.expect_ty().kind() {
            ty::Closure(def_id, substs) => {
                visitor.fn_like_items.push((def_id.to_owned(), substs));
            }
            ty::FnDef(def_id, substs) => {
                visitor.fn_like_items.push((def_id.to_owned(), substs));
            }
            _ => {}
        }
    }
}

impl<'tcx> mir::visit::Visitor<'tcx> for FnCastVisitor<'tcx> {
    fn visit_statement(&mut self, statement: &mir::Statement<'tcx>, location: mir::Location) {
        if let Some((ref place, ref rvalue)) = statement.kind.as_assign() {
            match rvalue {
                mir::Rvalue::Cast(_, op, _) | mir::Rvalue::Use(op) => {
                    extract_fn_like_items(self, op.to_owned());
                }
                mir::Rvalue::Aggregate(kind, _) => {
                    if let mir::AggregateKind::Closure(def_id, substs) = **kind {
                        self.fn_like_items.push((def_id.to_owned(), substs));
                    }
                }
                _ => {}
            }
        };
        self.super_statement(statement, location);
    }
}

pub fn get_all_fn_casts<'tcx>(tcx: ty::TyCtxt<'tcx>, body: &'tcx mir::Body<'tcx>)
                              -> Vec<(hir::def_id::DefId, ty::subst::SubstsRef<'tcx>)> {
    let mut uncast_visitor = FnCastVisitor { tcx, body, fn_like_items: vec![] };
    uncast_visitor.visit_body(body);
    dbg!(uncast_visitor.fn_like_items.clone());
    uncast_visitor.fn_like_items
}