use std::cell::RefCell;
use std::rc::Rc;

use rustc_hir::def_id::DefId;
use rustc_middle::mir::{visit::Visitor, Location, Operand, Terminator, TerminatorKind};
use rustc_middle::ty::{self, TyCtxt};

use super::arg_ty::ArgTy;
use super::fn_ty::{FnCallInfo, FnData};
use super::raw_ptr::has_raw_ptr_deref;
use super::storage::FnCallStorage;
use super::util::{calculate_important_locals, find_plausible_substs};

pub struct FnVisitor<'tcx> {
    // Type context.
    tcx: TyCtxt<'tcx>,
    // All seen function calls.
    storage: Rc<RefCell<FnCallStorage<'tcx>>>,
    // Currently visited function.
    current_fn: FnData<'tcx>,
}

impl<'tcx> Visitor<'tcx> for FnVisitor<'tcx> {
    fn visit_terminator(&mut self, terminator: &Terminator<'tcx>, location: Location) {
        if let TerminatorKind::Call {
            func,
            args,
            fn_span,
            ..
        } = &terminator.kind
        {
            // Body of the function where the terminator was found.
            let body = self.tcx.optimized_mir(self.current_fn.instance.def_id());

            // Attempt to resolve the callee instance via monomorphization.
            let fn_ty = self
                .current_fn
                .instance
                .subst_mir_and_normalize_erasing_regions(
                    self.tcx,
                    ty::ParamEnv::reveal_all(),
                    func.ty(body, self.tcx),
                );

            // TODO: handle different call types (e.g. FnPtr).
            if let ty::FnDef(def_id, substs) = fn_ty.kind() {
                self.visit_fn_call(
                    def_id.to_owned(),
                    substs,
                    args,
                    location,
                    fn_span.to_owned(),
                );
            } else {
                self.storage
                    .borrow_mut()
                    .add_unhandled(terminator.to_owned());
            }
        }
        self.super_terminator(terminator, location);
    }
}

impl<'tcx> FnVisitor<'tcx> {
    pub fn visit_fn_call(
        &mut self,
        def_id: DefId,
        substs: ty::SubstsRef<'tcx>,
        args: &Vec<Operand<'tcx>>,
        location: Location,
        call_span: rustc_span::Span,
    ) {
        // Calculate argument types, account for possible erasure.
        let arg_tys: Vec<ArgTy> = args
            .iter()
            .map(|arg| ArgTy::from_operand(arg, &location, &self.current_fn, self.tcx))
            .collect();

        // Resolve function instances that need to be analyzed.
        let maybe_instance =
            ty::Instance::resolve(self.tcx, ty::ParamEnv::reveal_all(), def_id, substs).unwrap();
        let def_id = match maybe_instance {
            Some(instance) => instance.def_id(),
            None => def_id,
        };
        let fns = if self.tcx.is_mir_available(def_id) {
            // Select all arguments that appear in this function call.
            let important_locals = calculate_important_locals(
                args,
                &self.current_fn.important_locals,
                def_id,
                self.tcx,
            );
            vec![FnData {
                arg_tys,
                instance: maybe_instance.unwrap(),
                important_locals,
            }]
        } else {
            let plausible_substs = find_plausible_substs(def_id, &arg_tys, substs, self.tcx);
            if !plausible_substs.is_empty() {
                plausible_substs
                    .into_iter()
                    .filter_map(|instance| {
                        let arg_tys = self
                            .tcx
                            .fn_sig(instance.def_id())
                            .subst(self.tcx, instance.substs)
                            .inputs()
                            .iter()
                            .map(|ty| ArgTy::from_ty(ty.skip_binder().to_owned()))
                            .collect();
                        let important_locals = calculate_important_locals(
                            args,
                            &self.current_fn.important_locals,
                            instance.def_id(),
                            self.tcx,
                        );
                        Some(FnData {
                            arg_tys,
                            instance,
                            important_locals,
                        })
                    })
                    .collect()
            } else {
                // Otherwise, we are unable to verify the purity due to external reference or dynamic dispatch.
                self.storage.borrow_mut().add_call(FnCallInfo::WithoutBody {
                    def_id,
                    arg_tys,
                    call_span,
                });
                return;
            }
        };

        for func in fns.into_iter() {
            let def_id = func.instance.def_id();
            // Only if we have not seen this call before.
            if self.storage.borrow().encountered_def_id(def_id) || self.tcx.is_const_fn_raw(def_id)
            {
                continue;
            }
            let body = self.tcx.optimized_mir(def_id);

            self.storage.borrow_mut().add_call(FnCallInfo::WithBody {
                def_id,
                arg_tys: func.arg_tys.clone(),
                call_span,
                body_span: body.span,
                raw_ptr_deref: has_raw_ptr_deref(self.tcx, body),
            });

            // Swap the current instance and continue recursively.
            let mut visitor = self.clone_with(func);
            visitor.visit_body(body);
        }
    }

    pub fn new(def_id: DefId, tcx: TyCtxt<'tcx>, current_fn: FnData<'tcx>) -> Self {
        Self {
            tcx,
            storage: Rc::new(RefCell::new(FnCallStorage::new(def_id))),
            current_fn,
        }
    }

    fn clone_with(&self, new_fn: FnData<'tcx>) -> Self {
        Self {
            tcx: self.tcx,
            storage: self.storage.clone(),
            current_fn: new_fn,
        }
    }

    pub fn get_storage_clone(&self) -> FnCallStorage<'tcx> {
        self.storage.borrow().to_owned()
    }
}
