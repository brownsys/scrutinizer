use regex::Regex;

use std::cell::RefCell;
use std::rc::Rc;

use rustc_hir::def_id::DefId;
use rustc_middle::mir::{visit::Visitor, Body, Local, Location, Place, Terminator, TerminatorKind};
use rustc_middle::ty::{subst::SubstsRef, FnDef, Instance, ParamEnv, TyCtxt};
use rustc_utils::PlaceExt;

use flowistry::indexed::impls::LocationOrArg;
use flowistry::infoflow::Direction;

use super::super::vartrack::compute_dependent_locals;
use super::raw_ptr_deref_visitor::has_raw_ptr_deref;
use super::types::{ArgTy, FnCallInfo};

pub struct FnVisitor<'tcx> {
    tcx: TyCtxt<'tcx>,
    // Maintain single list of function calls.
    fn_calls: Rc<RefCell<Vec<FnCallInfo<'tcx>>>>,
    unhandled_terminators: Rc<RefCell<Vec<Terminator<'tcx>>>>,
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
                let arg_tys = args
                    .iter()
                    .map(|arg| {
                        let backward_deps = arg.place().and_then(|place| {
                            let targets = vec![vec![(place, LocationOrArg::Location(location))]];
                            Some(compute_dependent_locals(
                                self.tcx,
                                self.current_def_id,
                                targets,
                                Direction::Backward,
                            ))
                        });
                        let backward_deps_tys = backward_deps
                            .into_iter()
                            .map(|backward_deps_for_local| {
                                backward_deps_for_local
                                    .into_iter()
                                    .map(|local| {
                                        let mut dependent_types = if local.index() != 0
                                            && local.index() <= self.current_arg_tys.len()
                                        {
                                            match self.current_arg_tys[local.index() - 1] {
                                                ArgTy::Simple(ty) => vec![ty],
                                                ArgTy::WithClosureInfluences(
                                                    ty,
                                                    ref influences,
                                                ) => {
                                                    let mut new_influences = influences.to_owned();
                                                    new_influences.push(ty);
                                                    new_influences
                                                }
                                            }
                                        } else {
                                            vec![]
                                        };
                                        dependent_types
                                            .push(self.current_body.local_decls[local].ty);
                                        dependent_types
                                    })
                                    .flatten()
                                    .filter(|ty| ty.contains_closure())
                                    .collect::<Vec<_>>()
                            })
                            .flatten()
                            .collect::<Vec<_>>();
                        let arg_ty = arg.ty(self.current_body, self.tcx);
                        if backward_deps_tys.is_empty() {
                            ArgTy::Simple(arg_ty)
                        } else {
                            ArgTy::WithClosureInfluences(arg_ty, backward_deps_tys)
                        }
                    })
                    .collect::<Vec<_>>();

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
                    self.visit_fn_call(*def_id, substs, arg_tys, *fn_span, important_args);
                }
            } else {
                self.unhandled_terminators
                    .borrow_mut()
                    .push(terminator.to_owned());
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
        // Resolve function instance.
        let maybe_instance =
            Instance::resolve(self.tcx, ParamEnv::reveal_all(), def_id, substs).unwrap();

        let def_id = match maybe_instance {
            Some(instance) => {
                // Introspect all interesting types.
                match instance.def.def_id_if_not_guaranteed_local_codegen() {
                    None => {
                        dbg!(instance);
                    }
                    _ => {}
                }
                instance.def_id()
            }
            None => def_id,
        };

        // Only if we have not seen this call before.
        // TODO: this is no longer valid, think about handling recursive call chains.
        if !self.encountered_def_id(def_id) {
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
                        return (arg_place, LocationOrArg::Arg(arg_local));
                    })
                    .collect::<Vec<_>>()];

                // Compute new dependencies for all important args.
                let deps = compute_dependent_locals(self.tcx, def_id, targets, Direction::Forward);

                self.add_call(FnCallInfo::WithBody {
                    def_id,
                    arg_tys: arg_tys.clone(),
                    call_span,
                    body_span: body.span,
                    raw_ptr_deref: has_raw_ptr_deref(self.tcx, body),
                });

                if let Some(instance) = maybe_instance {
                    // Swap the current instance and body and continue recursively.
                    let mut visitor = self.update(arg_tys, def_id, body, instance, deps);
                    visitor.visit_body(body);
                }
            } else {
                // Otherwise, we are unable to verify the purity due to external reference or dynamic dispatch.
                self.add_call(FnCallInfo::WithoutBody {
                    def_id,
                    arg_tys,
                    call_span,
                });
            }
        }
    }
    pub fn new(
        tcx: TyCtxt<'tcx>,
        current_arg_tys: Vec<ArgTy<'tcx>>,
        current_def_id: DefId,
        current_body: &'tcx Body<'tcx>,
        current_instance: Instance<'tcx>,
        current_deps: Vec<Local>,
    ) -> Self {
        Self {
            tcx,
            fn_calls: Rc::new(RefCell::new(Vec::new())),
            current_arg_tys,
            current_def_id,
            current_body,
            current_instance,
            current_deps,
            unhandled_terminators: Rc::new(RefCell::new(Vec::new())),
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
            fn_calls: self.fn_calls.clone(),
            current_arg_tys: new_arg_tys,
            current_def_id: new_def_id,
            current_body: new_body,
            current_instance: new_instance,
            current_deps: new_deps,
            unhandled_terminators: self.unhandled_terminators.clone(),
        }
    }

    fn add_call(&mut self, new_call: FnCallInfo<'tcx>) {
        self.fn_calls.borrow_mut().push(new_call);
    }

    fn encountered_def_id(&self, def_id: DefId) -> bool {
        self.fn_calls.borrow().iter().any(|fn_call_info| {
            let fn_call_info_def_id = match fn_call_info {
                FnCallInfo::WithBody { def_id, .. } => def_id,
                FnCallInfo::WithoutBody { def_id, .. } => def_id,
            };
            *fn_call_info_def_id == def_id
        })
    }

    pub fn dump_passing(&self) {
        for fn_call in self.fn_calls.borrow().iter() {
            if self.check_fn_call_purity(fn_call) {
                println!("--> Passing function call: {:#?}", fn_call);
                match fn_call {
                    FnCallInfo::WithBody { body_span, .. } => {
                        let body_snippet = self
                            .tcx
                            .sess
                            .source_map()
                            .span_to_snippet(*body_span)
                            .unwrap();
                        println!("Body snippet: {:?}", body_snippet);
                    }
                    FnCallInfo::WithoutBody { .. } => (),
                }
            }
        }
    }

    pub fn dump_violating(&self) {
        for fn_call in self.fn_calls.borrow().iter() {
            if !self.check_fn_call_purity(fn_call) {
                println!("--> Violating function call: {:#?}", fn_call);
                match fn_call {
                    FnCallInfo::WithBody { body_span, .. } => {
                        let body_snippet = self
                            .tcx
                            .sess
                            .source_map()
                            .span_to_snippet(*body_span)
                            .unwrap();
                        println!("Body snippet: {:?}", body_snippet);
                    }
                    FnCallInfo::WithoutBody { .. } => (),
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
        let allowed_libs = vec![
            Regex::new(r"core\[\w*\]::intrinsics").unwrap(),
            Regex::new(r"core\[\w*\]::panicking").unwrap(),
        ];
        match fn_call {
            FnCallInfo::WithBody {
                def_id,
                raw_ptr_deref,
                ..
            } => {
                let def_path_str = format!("{:?}", def_id);
                !raw_ptr_deref || (allowed_libs.iter().any(|lib| lib.is_match(&def_path_str)))
            }
            FnCallInfo::WithoutBody { def_id, .. } => {
                let def_path_str = format!("{:?}", def_id);
                allowed_libs.iter().any(|lib| lib.is_match(&def_path_str))
            }
        }
    }

    pub fn check_purity(&self) -> bool {
        self.fn_calls
            .borrow()
            .iter()
            .all(|fn_call| self.check_fn_call_purity(fn_call))
            && self.unhandled_terminators.borrow().is_empty()
    }
}
