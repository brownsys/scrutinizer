use itertools::Itertools;
use regex::Regex;
use std::cell::RefCell;
use std::rc::Rc;

use rustc_hir::def_id::DefId;
use rustc_middle::mir::{
    visit::Visitor, Body, Local, Location, Operand, Place, Terminator, TerminatorKind,
};
use rustc_middle::ty::{
    subst::GenericArgKind, subst::SubstsRef, FnDef, Instance, ParamEnv, Ty, TyCtxt, TyKind,
};
use rustc_utils::PlaceExt;

use flowistry::indexed::impls::LocationOrArg;
use flowistry::infoflow::Direction;

use super::raw_ptr::has_raw_ptr_deref;
use super::storage::FnCallStorage;
use super::types::{ArgTy, FnCallInfo};
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
                        let backward_deps = self.extract_backwards_deps(arg, &location);
                        let arg_ty = arg.ty(self.current_body, self.tcx);

                        if backward_deps.is_empty() {
                            ArgTy::Simple(arg_ty)
                        } else {
                            ArgTy::WithClosureInfluences(arg_ty, backward_deps)
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
        let instances = {
            // Resolve function instance.
            let maybe_instance =
                Instance::resolve(self.tcx, ParamEnv::reveal_all(), def_id, substs).unwrap();
            let def_id = match maybe_instance {
                Some(instance) => instance.def_id(),
                None => def_id,
            };
            // All possible closure shims that we need to analyze.
            let closure_shims = vec![
                Regex::new(r"core\[\w*\]::ops::function::FnMut::call_mut").unwrap(),
                Regex::new(r"core\[\w*\]::ops::function::FnOnce::call_once").unwrap(),
                Regex::new(r"core\[\w*\]::ops::function::Fn::call").unwrap(),
            ];
            let def_path_str = format!("{:?}", def_id);

            if closure_shims.iter().any(|lib| lib.is_match(&def_path_str))
                && !self.tcx.is_mir_available(def_id)
            {
                // Extract closure influences, as we have encountered an opaque closure shim.
                let closure_influences = self.extract_closure_influences(&arg_tys);
                // Check if there are any closure influences, return intact shim if not.
                if closure_influences.is_empty() {
                    vec![(maybe_instance, def_id)]
                } else {
                    closure_influences
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

    fn extract_closure_influences(
        &self,
        arg_tys: &Vec<ArgTy<'tcx>>,
    ) -> Vec<(Option<Instance<'tcx>>, DefId)> {
        arg_tys
            .iter()
            .map(|arg_ty| match arg_ty {
                ArgTy::Simple(ty) => {
                    let maybe_ty = ty.walk().find(|ty| match ty.unpack() {
                        GenericArgKind::Type(ty) => ty.is_closure(),
                        _ => false,
                    });
                    if let Some(ty) = maybe_ty {
                        vec![ty.expect_ty()]
                    } else {
                        vec![]
                    }
                }
                ArgTy::WithClosureInfluences(ty, influences) => {
                    let mut new_influences = influences.to_owned();
                    new_influences.push(ty.to_owned());
                    new_influences
                        .iter()
                        .filter_map(|ty| {
                            ty.walk().find(|ty| match ty.unpack() {
                                GenericArgKind::Type(ty) => ty.is_closure(),
                                _ => false,
                            })
                        })
                        .map(|ty| ty.expect_ty())
                        .collect()
                }
            })
            .flatten()
            .filter_map(|closure| match closure.kind() {
                TyKind::Closure(def_id, substs) => {
                    Instance::resolve(self.tcx, ParamEnv::reveal_all(), def_id.to_owned(), substs)
                        .unwrap()
                        .and_then(|instance| Some((Some(instance), instance.def_id())))
                }
                _ => None,
            })
            .unique()
            .collect()
    }

    fn extract_backwards_deps(&self, arg: &Operand<'tcx>, location: &Location) -> Vec<Ty<'tcx>> {
        let backward_deps = arg.place().and_then(|place| {
            let targets = vec![vec![(place, LocationOrArg::Location(location.to_owned()))]];
            Some(compute_dependent_locals(
                self.tcx,
                self.current_def_id,
                targets,
                Direction::Backward,
            ))
        });
        // Retrieve backwards dependencies' types.
        backward_deps
            .into_iter()
            .map(|backward_deps_for_local| {
                backward_deps_for_local
                    .into_iter()
                    .map(|local| {
                        let mut dependent_types =
                            if local.index() != 0 && local.index() <= self.current_arg_tys.len() {
                                match self.current_arg_tys[local.index() - 1] {
                                    ArgTy::Simple(ty) => vec![ty],
                                    ArgTy::WithClosureInfluences(ty, ref influences) => {
                                        let mut new_influences = influences.to_owned();
                                        new_influences.push(ty);
                                        new_influences
                                    }
                                }
                            } else {
                                vec![]
                            };
                        dependent_types.push(self.current_body.local_decls[local].ty);
                        dependent_types
                    })
                    .flatten()
                    .filter(|ty| ty.contains_closure())
                    .collect::<Vec<_>>()
            })
            .flatten()
            .unique()
            .collect()
    }
}
