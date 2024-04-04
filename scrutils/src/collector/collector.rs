use itertools::Itertools;
use log::{debug, trace, warn};
use rustc_middle::mir::{
    BasicBlock, Body, Location, Operand, Place, Statement, StatementKind, Terminator,
    TerminatorKind,
};
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_mir_dataflow::{Analysis, AnalysisDomain, CallReturnPlaces, JoinSemiLattice};
use rustc_utils::BodyExt;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::rc::Rc;

use crate::collector::closure_collector::CollectClosures;
use crate::collector::collector_domain::CollectorDomain;
use crate::collector::dataflow_shim::iterate_to_fixpoint;
use crate::collector::has_tracked_ty::HasTrackedTy;
use crate::collector::structs::{PartialFunctionInfo, VirtualStack, VirtualStackItem};
use crate::common::storage::{
    ClosureInfoStorage, ClosureInfoStorageRef, FunctionInfoStorage, FunctionInfoStorageRef,
};
use crate::common::{ArgTys, FunctionCall, FunctionInfo, NormalizedPlace, TrackedTy};

#[derive(Clone)]
pub struct Collector<'tcx> {
    virtual_stack: VirtualStack<'tcx>,
    current_function: PartialFunctionInfo<'tcx>,
    substituted_body: Body<'tcx>,
    function_storage_ref: FunctionInfoStorageRef<'tcx>,
    closure_storage_ref: ClosureInfoStorageRef<'tcx>,
    shallow: bool,
    tcx: TyCtxt<'tcx>,
}

impl<'tcx> Collector<'tcx> {
    pub fn get_function_info_storage(&self) -> FunctionInfoStorage<'tcx> {
        self.function_storage_ref.borrow().to_owned()
    }

    pub fn get_closure_info_storage(&self) -> ClosureInfoStorage<'tcx> {
        self.closure_storage_ref.borrow().to_owned()
    }
}

impl<'tcx> Collector<'tcx> {
    pub fn collect(instance: ty::Instance<'tcx>, tcx: TyCtxt<'tcx>, shallow: bool) -> Self {
        let body = instance.subst_mir_and_normalize_erasing_regions(
            tcx,
            ty::ParamEnv::reveal_all(),
            tcx.instance_mir(instance.def).to_owned(),
        );
        let arg_tys = (1..=body.arg_count)
            .map(|local| {
                let arg_ty = body.local_decls[local.into()].ty;
                TrackedTy::from_ty(arg_ty)
            })
            .collect_vec();
        let current_function = PartialFunctionInfo::new_function(instance, ArgTys::new(arg_tys));
        let function_storage_ref = Rc::new(RefCell::new(FunctionInfoStorage::new(instance)));
        let closure_storage_ref = Rc::new(RefCell::new(ClosureInfoStorage::new()));
        let virtual_stack = VirtualStack::new();

        let mut collector = Collector::new(
            current_function,
            virtual_stack,
            function_storage_ref,
            closure_storage_ref,
            shallow,
            tcx,
        );
        let results = collector.run();

        let fn_info = FunctionInfo::new_with_body(
            instance,
            results.places().to_owned(),
            results.calls().to_owned(),
            body.to_owned(),
            body.span,
            results.unhandled().to_owned(),
        );

        collector
            .function_storage_ref
            .borrow_mut()
            .insert(fn_info.clone());

        collector
    }

    fn run(&mut self) -> CollectorDomain<'tcx> {
        trace!("running dataflow analysis on {:?}", self.current_function);

        self.virtual_stack.push(VirtualStackItem::new(
            self.current_function.instance().def_id(),
            self.current_function.tracked_args().to_owned(),
        ));

        if let Some(function_info) = self
            .function_storage_ref
            .borrow()
            .get_with_body(self.current_function.instance())
        {
            return CollectorDomain::from_regular_fn_info(function_info);
        }

        fs::create_dir_all("mir_dumps").unwrap();
        fs::write(
            format!(
                "mir_dumps/{:?}.rs",
                self.current_function.instance().def_id()
            ),
            self.substituted_body.to_string(self.tcx).unwrap(),
        )
        .unwrap();

        let mut cursor =
            iterate_to_fixpoint(self.clone().into_engine(self.tcx, &self.substituted_body))
                .into_results_cursor(&self.substituted_body);

        self.substituted_body
            .basic_blocks
            .iter_enumerated()
            .map(|(bb, _)| {
                cursor.seek_to_block_end(bb);
                cursor.get().to_owned()
            })
            .reduce(|mut acc, elt| {
                acc.join(&elt);
                acc
            })
            .unwrap()
    }

    fn new(
        current_function: PartialFunctionInfo<'tcx>,
        virtual_stack: VirtualStack<'tcx>,
        function_storage_ref: FunctionInfoStorageRef<'tcx>,
        closure_storage_ref: ClosureInfoStorageRef<'tcx>,
        shallow: bool,
        tcx: TyCtxt<'tcx>,
    ) -> Self {
        let substituted_body = current_function
            .instance()
            .subst_mir_and_normalize_erasing_regions(
                tcx,
                ty::ParamEnv::reveal_all(),
                tcx.instance_mir(current_function.instance().def).to_owned(),
            );
        Collector {
            current_function,
            virtual_stack,
            substituted_body,
            function_storage_ref,
            closure_storage_ref,
            shallow,
            tcx,
        }
    }

    fn process_call(
        &self,
        function_ty: Ty<'tcx>,
        args: &Vec<Operand<'tcx>>,
        arg_tys: ArgTys<'tcx>,
        state: &mut CollectorDomain<'tcx>,
        destination: Option<&Place<'tcx>>,
    ) {
        // Apply substitutions to the type in case it contains generics.
        let function_ty = self.current_function.substitute(function_ty, self.tcx);

        // TODO: handle different call types (e.g. FnPtr).
        if let ty::FnDef(def_id, substs) = function_ty.kind() {
            if !def_id.is_local() && self.shallow {
                return;
            }

            // Calculate argument types, account for possible erasure.
            let plausible_functions = self.current_function.resolve(
                def_id.to_owned(),
                substs,
                &arg_tys,
                self.closure_storage_ref.clone(),
                self.tcx,
            );

            if !plausible_functions.is_empty() {
                for function_data in plausible_functions.into_iter() {
                    let def_id = function_data.instance().def_id();

                    // Skip if the call is repeated.
                    let current_seen_item =
                        VirtualStackItem::new(def_id, function_data.tracked_args().to_owned());
                    if self.virtual_stack.contains(&current_seen_item) {
                        continue;
                    };

                    let return_ty: TrackedTy<'_> = match function_data.instance().def {
                        ty::InstanceDef::Intrinsic(..) | ty::InstanceDef::Virtual(..) => {
                            self.function_storage_ref.borrow_mut().insert(
                                FunctionInfo::new_without_body(
                                    def_id.to_owned(),
                                    arg_tys.as_vec().to_owned(),
                                ),
                            );
                            state.add_call(FunctionCall::new_without_body(
                                function_data.instance().def_id(),
                                args.to_owned(),
                            ));
                            TrackedTy::from_ty(
                                self.tcx
                                    .fn_sig(def_id)
                                    .subst(self.tcx, function_data.instance().substs)
                                    .output()
                                    .skip_binder(),
                            )
                        }
                        _ => {
                            let body = function_data.substitute(
                                self.tcx
                                    .instance_mir(function_data.instance().def)
                                    .to_owned(),
                                self.tcx,
                            );

                            // Swap the current instance and continue recursively.
                            let results = Collector::new(
                                function_data.clone(),
                                self.virtual_stack.clone(),
                                self.function_storage_ref.clone(),
                                self.closure_storage_ref.clone(),
                                self.shallow,
                                self.tcx,
                            )
                            .run();

                            self.function_storage_ref.borrow_mut().insert(
                                FunctionInfo::new_with_body(
                                    function_data.instance().to_owned(),
                                    results.places().to_owned(),
                                    results.calls().to_owned(),
                                    body.to_owned(),
                                    body.span,
                                    results.unhandled().to_owned(),
                                ),
                            );
                            state.add_call(FunctionCall::new_with_body(
                                function_data.instance().to_owned(),
                                args.to_owned(),
                            ));

                            results.return_type(def_id, &body, self.tcx)
                        }
                    };

                    if let Some(destination) = destination {
                        let normalized_destination = NormalizedPlace::from_place(
                            &destination,
                            self.tcx,
                            self.current_function.instance().def_id(),
                        );

                        trace!(
                            "handling return terminator current_function={:?} place={:?} ty={:?} from={:?}",
                            self.current_function.instance().def_id(),
                            normalized_destination,
                            return_ty,
                            def_id
                        );

                        state.update_with(
                            normalized_destination,
                            return_ty,
                            &self.substituted_body,
                            self.current_function.instance().def_id(),
                            self.tcx,
                        );
                    }
                }
            } else {
                self.function_storage_ref
                    .borrow_mut()
                    .insert(FunctionInfo::new_without_body(
                        def_id.to_owned(),
                        arg_tys.as_vec().to_owned(),
                    ));
                state.add_call(FunctionCall::new_without_body(
                    def_id.to_owned(),
                    args.to_owned(),
                ));
            }
        } else {
            debug!("unhandled terminator");
            state.add_unhandled(function_ty.to_owned());
        }
    }
}

impl<'tcx> AnalysisDomain<'tcx> for Collector<'tcx> {
    type Domain = CollectorDomain<'tcx>;

    const NAME: &'static str = "TypeCollector";

    fn bottom_value(&self, body: &Body<'tcx>) -> CollectorDomain<'tcx> {
        let def_id = self.current_function.instance().def_id();

        // Collect original types for all places.
        let all_places = body.all_places(self.tcx, def_id);
        let tracked_types = HashMap::from_iter(all_places.map(|place| {
            let place = NormalizedPlace::from_place(&place, self.tcx, def_id);
            let place_ty = place.ty(body, self.tcx).ty;
            (place, TrackedTy::from_ty(place_ty))
        }));

        body.collect_closures(
            self.tcx,
            self.closure_storage_ref.clone(),
            &self.current_function,
        );

        let mut type_tracker = CollectorDomain::new(tracked_types);

        if self.current_function.is_closure() {
            // Augment with types from tracked upvars.
            let upvars = self.current_function.expect_closure();
            type_tracker.augment_closure_with_upvars(upvars, body, def_id, self.tcx);
        }
        type_tracker.augment_with_args(
            self.current_function.tracked_args(),
            body,
            def_id,
            self.tcx,
        );
        type_tracker
    }

    fn initialize_start_block(&self, _body: &Body<'tcx>, _state: &mut Self::Domain) {}
}

impl<'tcx> Analysis<'tcx> for Collector<'tcx> {
    fn apply_statement_effect(
        &self,
        state: &mut CollectorDomain<'tcx>,
        statement: &Statement<'tcx>,
        _location: Location,
    ) {
        if let StatementKind::Assign(box (place, rvalue)) = statement.to_owned().kind {
            let rvalue_tracked_ty = rvalue.tracked_ty(
                state,
                self.closure_storage_ref.clone(),
                self.current_function.instance(),
                self.tcx,
            );
            let normalized_place = NormalizedPlace::from_place(
                &place,
                self.tcx,
                self.current_function.instance().def_id(),
            );

            trace!(
                "handling statement current_function={:?} place={:?} ty={:?}",
                self.current_function.instance().def_id(),
                normalized_place,
                rvalue_tracked_ty,
            );

            state.update_with(
                normalized_place,
                rvalue_tracked_ty,
                &self.substituted_body,
                self.current_function.instance().def_id(),
                self.tcx,
            );
        }
    }
    fn apply_terminator_effect<'mir>(
        &self,
        state: &mut CollectorDomain<'tcx>,
        terminator: &'mir Terminator<'tcx>,
        _location: Location,
    ) {
        match terminator.kind {
            TerminatorKind::Call {
                ref func,
                ref args,
                ref destination,
                ..
            } => {
                let function_ty = func.ty(&self.substituted_body, self.tcx);
                let arg_tys = state.construct_args(
                    args,
                    self.current_function.instance().def_id(),
                    &self.substituted_body,
                    self.tcx,
                );
                self.process_call(function_ty, args, arg_tys, state, Some(destination));
            }
            TerminatorKind::Drop { place, .. } => {
                let place_ty = place.ty(&self.substituted_body, self.tcx);

                if let ty::TyKind::Adt(adt_def, substs) = place_ty.ty.kind() {
                    let adt_def_id = adt_def.did();
                    let destructor = self.tcx.adt_destructor(adt_def_id);

                    if let Some(destructor) = destructor {
                        let destructor_function_ty =
                            self.tcx.type_of(destructor.did).subst(self.tcx, substs);
                        let destructor_args = &vec![Operand::Copy(place)];
                        let destructor_arg_tys =
                            ArgTys::new(vec![TrackedTy::from_ty(self.tcx.mk_mut_ref(
                                self.tcx.mk_region_from_kind(ty::RegionKind::ReErased),
                                place.ty(&self.substituted_body, self.tcx).ty,
                            ))]);

                        self.process_call(
                            destructor_function_ty,
                            destructor_args,
                            destructor_arg_tys,
                            state,
                            None,
                        );
                    }
                } else {
                    warn!("dropping a non-adt type: {}", place_ty.ty);
                }
            }
            _ => {}
        }
    }

    fn apply_call_return_effect(
        &self,
        _state: &mut CollectorDomain<'tcx>,
        _block: BasicBlock,
        _return_places: CallReturnPlaces<'_, 'tcx>,
    ) {
    }
}
