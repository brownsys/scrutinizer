use std::cell::RefCell;
use std::rc::Rc;

use rustc_hir as hir;
use rustc_middle::mir as mir;
use rustc_middle::ty as ty;

use rustc_middle::mir::HasLocalDecls;
use crate::raw_ptr_deref_visitor::RawPtrDerefVisitor;

#[derive(Debug)]
struct FnCallInfo<'tcx> {
    def_id: hir::def_id::DefId,
    arg_tys: Vec<ty::Ty<'tcx>>,
    call_span: rustc_span::Span,
    body_span: Option<rustc_span::Span>,
    // Whether we were able to retrieve and check the MIR for the function body.
    body_checked: bool,
    // Whether body contains raw pointer dereference.
    raw_ptr_deref: bool,
}

pub struct FnVisitor<'tcx> {
    tcx: ty::TyCtxt<'tcx>,
    // Maintain single list of function calls.
    fn_calls: Rc<RefCell<Vec<FnCallInfo<'tcx>>>>,
    current_body: &'tcx mir::Body<'tcx>,
    current_instance: ty::Instance<'tcx>,
}

impl<'tcx> mir::visit::Visitor<'tcx> for FnVisitor<'tcx> {
    fn visit_terminator(
        &mut self,
        terminator: &mir::Terminator<'tcx>,
        location: mir::Location,
    ) {
        match &terminator.kind {
            mir::TerminatorKind::Call { func, args, fn_span, .. } => {
                if let Some((def_id, _)) = func.const_fn_def() {
                    // To avoid visiting the same function body twice, check whether we have seen it.
                    if !self.encountered_def_id(def_id) {
                        // Attempt to resolve the instance via monomorphization.
                        let func_ty = func.ty(self.current_body, self.tcx);
                        let func_ty = self.current_instance.subst_mir_and_normalize_erasing_regions(
                            self.tcx,
                            ty::ParamEnv::reveal_all(),
                            func_ty,
                        );
                        if let ty::FnDef(callee_def_id, substs) = func_ty.kind() {
                            let instance = ty::Instance::expect_resolve(self.tcx, ty::ParamEnv::reveal_all(), *callee_def_id, substs);
                            let def_id = instance.def.def_id();
                            // Retrieve argument types.
                            let local_decls = self.current_body.local_decls();
                            let arg_tys = args.iter().map(|arg| arg.ty(local_decls, self.tcx)).collect::<Vec<_>>();
                            if self.tcx.is_mir_available(def_id) {
                                let body = self.tcx.optimized_mir(def_id);

                                let mut ptr_deref_visitor = RawPtrDerefVisitor::new(self.tcx, body.local_decls(), def_id);
                                ptr_deref_visitor.visit_body(body);
                                self.add_call(FnCallInfo {
                                    def_id,
                                    arg_tys,
                                    call_span: *fn_span,
                                    body_span: Some(body.span),
                                    body_checked: true,
                                    raw_ptr_deref: ptr_deref_visitor.check(),
                                });

                                // Swap the current instance and body and continue recursively.
                                let mut visitor = self.with_new_body_and_instance(body, instance);
                                visitor.visit_body(body);
                            } else {
                                // Otherwise, we are unable to verify the purity due to external reference or dynamic dispatch.
                                self.add_call(FnCallInfo { def_id, arg_tys, call_span: *fn_span, body_span: None, body_checked: false, raw_ptr_deref: false });
                            }
                        } else {
                            panic!("Other type of call encountered.");
                        }
                    }
                }
            }
            _ => {}
        }
        self.super_terminator(terminator, location);
    }
}

impl<'tcx> FnVisitor<'tcx> {
    pub fn new(tcx: ty::TyCtxt<'tcx>, current_body: &'tcx mir::Body<'tcx>, current_instance: ty::Instance<'tcx>) -> Self {
        Self { tcx, fn_calls: Rc::new(RefCell::new(Vec::new())), current_body, current_instance }
    }

    fn with_new_body_and_instance(&self, new_body: &'tcx mir::Body<'tcx>, new_instance: ty::Instance<'tcx>) -> Self {
        Self { tcx: self.tcx, fn_calls: self.fn_calls.clone(), current_body: new_body, current_instance: new_instance }
    }

    fn add_call(&mut self, new_call: FnCallInfo<'tcx>) {
        self.fn_calls.borrow_mut().push(new_call);
    }

    fn encountered_def_id(&self, def_id: hir::def_id::DefId) -> bool {
        self.fn_calls.borrow().iter().any(|fn_call_info| fn_call_info.def_id == def_id)
    }

    pub fn dump_passing(&self) {
        for fn_call in self.fn_calls.borrow().iter() {
            if self.check_fn_call_purity(fn_call) {
                dbg!(fn_call);
            }
        }
    }

    pub fn dump_violating(&self) {
        for fn_call in self.fn_calls.borrow().iter() {
            if !self.check_fn_call_purity(fn_call) {
                dbg!(fn_call);
                match fn_call.body_span {
                    Some(span) => {
                        let body_snippet = self.tcx
                            .sess
                            .source_map()
                            .span_to_snippet(span)
                            .unwrap();
                        dbg!(body_snippet);
                    }
                    None => ()
                }
            }
        }
    }

    fn check_fn_call_purity(&self, fn_call: &FnCallInfo) -> bool {
        fn_call.body_checked && !fn_call.raw_ptr_deref
    }

    pub fn check_purity(&self) -> bool {
        self.fn_calls.borrow().iter().all(|fn_call| {
            self.check_fn_call_purity(fn_call)
        })
    }
}
