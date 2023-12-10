use std::cell::RefCell;
use std::rc::Rc;

use rustc_hir::def_id::DefId;
use rustc_middle::mir::{visit::Visitor, Body, Local, Location, Place, Terminator, TerminatorKind};
use rustc_middle::ty::{subst::SubstsRef, FnDef, Instance, ParamEnv, TyCtxt};
use rustc_utils::PlaceExt;

use flowistry::indexed::impls::LocationOrArg;
use flowistry::infoflow::Direction;

use super::raw_ptr::has_raw_ptr_deref;
use super::storage::FnCallStorage;
use super::types::{ArgTy, FnCallInfo};
use super::util::{
    extract_callable_deps, extract_callable_influences, is_type_erased_closure_call,
};
use crate::vartrack::compute_dependent_locals;

pub struct FnVisitor<'tcx> {
    tcx: TyCtxt<'tcx>,
    // Maintain single list of function calls and unhandled terminators.
    storage: Rc<RefCell<FnCallStorage<'tcx>>>,
    current_arg_tys: Vec<ArgTy<'tcx>>,
    current_def_id: DefId,
    current_body: &'tcx Body<'tcx>,
    current_instance: Instance<'tcx>,
    current_deps: Vec<Local>,
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
            // Attempt to resolve the instance via monomorphization.
            let func_ty = self
                .current_instance
                .subst_mir_and_normalize_erasing_regions(
                    self.tcx,
                    ParamEnv::reveal_all(),
                    func.ty(self.current_body, self.tcx),
                );

            if let FnDef(def_id, substs) = func_ty.kind() {
                // Retrieve argument types.
                let arg_tys: Vec<ArgTy> = args
                    .iter()
                    .map(|arg| {
                        let backward_deps = extract_callable_deps(
                            arg,
                            &location,
                            &self.current_arg_tys,
                            self.current_def_id,
                            self.current_body,
                            self.tcx,
                        );
                        let arg_ty = arg.ty(self.current_body, self.tcx);

                        if backward_deps.is_empty() {
                            ArgTy::Simple(arg_ty)
                        } else {
                            ArgTy::WithCallableInfluences(arg_ty, backward_deps)
                        }
                    })
                    .collect();
                // Select all arguments that appear in this function call.
                let important_args: Vec<usize> = args
                    .iter()
                    .enumerate()
                    .filter_map(|(i, arg)| {
                        arg.place()
                            .and_then(|place| place.as_local())
                            .and_then(|local| {
                                if self.current_deps.contains(&local) {
                                    // Need to add 1 because arguments' locals start with 1.
                                    Some(i + 1)
                                } else {
                                    None
                                }
                            })
                    })
                    .collect();
                // Only check there are some important args.
                if !important_args.is_empty() {
                    self.visit_fn_call(*def_id, substs, arg_tys.clone(), *fn_span, important_args);
                }
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
        substs: SubstsRef<'tcx>,
        arg_tys: Vec<ArgTy<'tcx>>,
        call_span: rustc_span::Span,
        important_args: Vec<usize>,
    ) {
        // All instances that need to be analyzed, including influences.
        let instances = {
            // Resolve function instance.
            let maybe_instance =
                Instance::resolve(self.tcx, ParamEnv::reveal_all(), def_id, substs).unwrap();
            let def_id = match maybe_instance {
                Some(instance) => instance.def_id(),
                None => def_id,
            };
            // All possible closure shims that we need to analyze.
            if is_type_erased_closure_call(def_id, self.tcx) {
                let closure_arg_ty = arg_tys[0].to_owned();
                // Extract closure influences, as we have encountered an opaque closure shim.
                let maybe_callable_influences =
                    extract_callable_influences(&closure_arg_ty, self.tcx);
                // Check if there are any closure influences, return intact shim if not.
                match maybe_callable_influences {
                    Ok(callable_influences) => {
                        if callable_influences.is_empty() {
                            vec![(maybe_instance, def_id)]
                        } else {
                            callable_influences
                                .into_iter()
                                .map(|instance| (Some(instance), instance.def_id()))
                                .collect()
                        }
                    }
                    Err(_) => vec![(maybe_instance, def_id)],
                }
            } else {
                vec![(maybe_instance, def_id)]
            }
        };

        for (maybe_instance, def_id) in instances.into_iter() {
            // Only if we have not seen this call before.
            if !self.storage.borrow().encountered_def_id(def_id) {
                if self.tcx.is_const_fn_raw(def_id) {
                    return;
                }
                if self.tcx.is_mir_available(def_id) {
                    let body = self.tcx.optimized_mir(def_id);
                    // Construct targets of the arguments.
                    let targets = vec![important_args
                        .iter()
                        .map(|arg| {
                            let arg_local = Local::from_usize(*arg);
                            let arg_place = Place::make(arg_local, &[], self.tcx);
                            (arg_place, LocationOrArg::Arg(arg_local))
                        })
                        .collect()];

                    // Compute new dependencies for all important args.
                    let deps =
                        compute_dependent_locals(self.tcx, def_id, targets, Direction::Forward);

                    self.storage.borrow_mut().add_call(FnCallInfo::WithBody {
                        def_id,
                        arg_tys: arg_tys.clone(),
                        call_span,
                        body_span: body.span,
                        raw_ptr_deref: has_raw_ptr_deref(self.tcx, body),
                    });

                    let instance = maybe_instance.unwrap();
                    // Swap the current instance and body and continue recursively.
                    let mut visitor = self.update(arg_tys.clone(), def_id, body, instance, deps);
                    visitor.visit_body(body);
                } else {
                    // Otherwise, we are unable to verify the purity due to external reference or dynamic dispatch.
                    self.storage.borrow_mut().add_call(FnCallInfo::WithoutBody {
                        def_id,
                        arg_tys: arg_tys.clone(),
                        call_span,
                    });
                }
            }
        }
    }

    pub fn new(
        def_id: DefId,
        tcx: TyCtxt<'tcx>,
        current_arg_tys: Vec<ArgTy<'tcx>>,
        current_def_id: DefId,
        current_body: &'tcx Body<'tcx>,
        current_instance: Instance<'tcx>,
        current_deps: Vec<Local>,
    ) -> Self {
        Self {
            tcx,
            storage: Rc::new(RefCell::new(FnCallStorage::new(def_id))),
            current_arg_tys,
            current_def_id,
            current_body,
            current_instance,
            current_deps,
        }
    }

    fn update(
        &self,
        new_arg_tys: Vec<ArgTy<'tcx>>,
        new_def_id: DefId,
        new_body: &'tcx Body<'tcx>,
        new_instance: Instance<'tcx>,
        new_deps: Vec<Local>,
    ) -> Self {
        Self {
            tcx: self.tcx,
            storage: self.storage.clone(),
            current_arg_tys: new_arg_tys,
            current_def_id: new_def_id,
            current_body: new_body,
            current_instance: new_instance,
            current_deps: new_deps,
        }
    }

    pub fn get_storage_clone(&self) -> FnCallStorage<'tcx> {
        self.storage.borrow().to_owned()
    }
}
