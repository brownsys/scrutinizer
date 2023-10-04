use regex::Regex;

use std::cell::RefCell;
use std::rc::Rc;

use rustc_hir as hir;
use rustc_middle::mir as mir;
use rustc_middle::ty as ty;

use rustc_middle::mir::HasLocalDecls;
use crate::raw_ptr_deref_visitor::has_raw_ptr_deref;

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
    unhandled_terminators: Rc<RefCell<Vec<mir::Terminator<'tcx>>>>,
    current_body: &'tcx mir::Body<'tcx>,
    current_instance: ty::Instance<'tcx>,
}

impl<'tcx> mir::visit::Visitor<'tcx> for FnVisitor<'tcx> {
    fn visit_terminator(
        &mut self,
        terminator: &mir::Terminator<'tcx>,
        location: mir::Location,
    ) {
        if let mir::TerminatorKind::Call { func, args, fn_span, .. } = &terminator.kind {
            // Attempt to resolve the instance via monomorphization.
            let func_ty = self.current_instance.subst_mir_and_normalize_erasing_regions(
                self.tcx,
                ty::ParamEnv::reveal_all(),
                func.ty(self.current_body, self.tcx),
            );

            if let ty::FnDef(callee_def_id, substs) = func_ty.kind() {
                let instance = ty::Instance::expect_resolve(self.tcx, ty::ParamEnv::reveal_all(), *callee_def_id, substs);

                // Introspect all interesting types.
                match instance.def.def_id_if_not_guaranteed_local_codegen() {
                    None => { dbg!(instance); }
                    _ => {}
                }

                // Retrieve argument types.
                let arg_tys = args.iter().map(|arg| {
                    arg.ty(self.current_body, self.tcx)
                }).collect::<Vec<_>>();

                let def_id = instance.def_id();
                // Carve out an exception for Fn(...) -> ... casted to another type.
                // let def_id = match instance.def {
                //     ty::InstanceDef::FnPtrShim { .. } |
                //     ty::InstanceDef::Virtual { .. } |
                //     ty::InstanceDef::ClosureOnceShim { .. } => {
                //         if let Some(place) = args[0].place() {
                //             if let ty::TyKind::Closure(def_id, ..) = place.ty(self.current_body, self.tcx).ty.kind() {
                //                 *def_id
                //             } else {
                //                 if let Some(original_ty) = uncast(self.tcx, place, self.current_body) {
                //                     if let ty::TyKind::Closure(def_id, ..) = original_ty.kind() {
                //                         *def_id
                //                     } else {
                //                         instance.def.def_id()
                //                     }
                //                 } else {
                //                     instance.def.def_id()
                //                 }
                //             }
                //         } else {
                //             instance.def.def_id()
                //         }
                //     }
                //     _ => instance.def.def_id()
                // };

                // To avoid visiting the same function body twice, check whether we have seen it.
                if !self.encountered_def_id(def_id) {
                    if self.tcx.is_const_fn_raw(def_id) {
                        return;
                    }

                    if self.tcx.is_mir_available(def_id) {
                        let body = self.tcx.optimized_mir(def_id);

                        self.add_call(FnCallInfo {
                            def_id,
                            arg_tys,
                            call_span: *fn_span,
                            body_span: Some(body.span),
                            body_checked: true,
                            raw_ptr_deref: has_raw_ptr_deref(self.tcx, body),
                        });

                        // Swap the current instance and body and continue recursively.
                        let mut visitor = self.with_new_body_and_instance(body, instance);
                        visitor.visit_body(body);
                    } else {
                        // Otherwise, we are unable to verify the purity due to external reference or dynamic dispatch.
                        self.add_call(FnCallInfo { def_id, arg_tys, call_span: *fn_span, body_span: None, body_checked: false, raw_ptr_deref: false });
                    }
                }
            } else {
                self.unhandled_terminators.borrow_mut().push(terminator.to_owned());
            }
        }
        self.super_terminator(terminator, location);
    }
}

impl<'tcx> FnVisitor<'tcx> {
    pub fn new(tcx: ty::TyCtxt<'tcx>, current_body: &'tcx mir::Body<'tcx>, current_instance: ty::Instance<'tcx>) -> Self {
        Self {
            tcx,
            fn_calls: Rc::new(RefCell::new(Vec::new())),
            current_body,
            current_instance,
            unhandled_terminators: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn with_new_body_and_instance(&self, new_body: &'tcx mir::Body<'tcx>, new_instance: ty::Instance<'tcx>) -> Self {
        Self {
            tcx:
            self.tcx,
            fn_calls: self.fn_calls.clone(),
            current_body: new_body,
            current_instance: new_instance,
            unhandled_terminators: self.unhandled_terminators.clone(),
        }
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
                println!("--> Passing function call: {:#?}", fn_call);
                match fn_call.body_span {
                    Some(span) => {
                        let body_snippet = self.tcx
                            .sess
                            .source_map()
                            .span_to_snippet(span)
                            .unwrap();
                        println!("Body snippet: {:?}", body_snippet);
                    }
                    None => ()
                }
            }
        }
    }

    pub fn dump_violating(&self) {
        for fn_call in self.fn_calls.borrow().iter() {
            if !self.check_fn_call_purity(fn_call) {
                println!("--> Violating function call: {:#?}", fn_call);
                match fn_call.body_span {
                    Some(span) => {
                        let body_snippet = self.tcx
                            .sess
                            .source_map()
                            .span_to_snippet(span)
                            .unwrap();
                        println!("Body snippet: {:?}", body_snippet);
                    }
                    None => ()
                }
            }
        }
    }

    pub fn dump_unhandled_terminators(&self) {
        for unhandled_terminator in self.unhandled_terminators.borrow().iter() {
            println!("--> Unhandled terminator: {:#?}", unhandled_terminator);
        }
    }

    fn check_fn_call_purity(&self, fn_call: &FnCallInfo) -> bool {
        let allowed_libs =
            vec![Regex::new(r"core\[\w*\]::intrinsics").unwrap(),
                 Regex::new(r"core\[\w*\]::panicking").unwrap()];

        let def_path_str = format!("{:?}", fn_call.def_id);
        (fn_call.body_checked && !fn_call.raw_ptr_deref) ||
            (allowed_libs.iter().any(|lib| lib.is_match(&def_path_str)))
    }

    pub fn check_purity(&self) -> bool {
        self.fn_calls.borrow().iter().all(|fn_call| {
            self.check_fn_call_purity(fn_call)
        }) && self.unhandled_terminators.borrow().is_empty()
    }
}
