use log::trace;
use rustc_middle::mir::{
    BasicBlock, Body, Location, Statement, StatementKind, Terminator, TerminatorKind,
};
use rustc_middle::ty::{self, TyCtxt};
use rustc_mir_dataflow::{Analysis, AnalysisDomain, CallReturnPlaces, JoinSemiLattice};
use rustc_utils::BodyExt;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

use super::arg_tys::ArgTys;
use super::callee::Callee;
use super::closure_info::ClosureInfoStorageRef;
use super::dataflow_shim::iterate_to_fixpoint;
use super::has_tracked_ty::HasTrackedTy;
use super::normalized_place::NormalizedPlace;
use super::storage::FnInfoStorageRef;
use super::tracked_ty::TrackedTy;
use super::type_tracker::{Call, TypeTracker};
use super::virtual_stack::{VirtualStack, VirtualStackItem};

#[derive(Clone)]
pub struct TypeCollector<'tcx> {
    virtual_stack: VirtualStack<'tcx>,
    current_fn: Callee<'tcx>,
    substituted_body: Body<'tcx>,
    fn_storage_ref: FnInfoStorageRef<'tcx>,
    closure_storage_ref: ClosureInfoStorageRef<'tcx>,
    tcx: TyCtxt<'tcx>,
}

impl<'tcx> TypeCollector<'tcx> {
    pub fn new(
        current_fn: Callee<'tcx>,
        virtual_stack: VirtualStack<'tcx>,
        fn_storage_ref: FnInfoStorageRef<'tcx>,
        closure_storage_ref: ClosureInfoStorageRef<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Self {
        let substituted_body = current_fn
            .instance()
            .subst_mir_and_normalize_erasing_regions(
                tcx,
                ty::ParamEnv::reveal_all(),
                tcx.optimized_mir(current_fn.instance().def_id()).to_owned(),
            );
        TypeCollector {
            current_fn,
            virtual_stack,
            substituted_body,
            fn_storage_ref,
            closure_storage_ref,
            tcx,
        }
    }
    pub fn run(mut self) -> TypeTracker<'tcx> {
        trace!("running dataflow analysis on {:?}", self.current_fn);

        self.virtual_stack.push(VirtualStackItem::new(
            self.current_fn.instance().def_id(),
            self.current_fn.tracked_args().to_owned(),
        ));

        if let Some(places) = self
            .fn_storage_ref
            .borrow()
            .get_regular(self.current_fn.instance())
        {
            return TypeTracker::new(places);
        }

        File::create(format!(
            "mir_dumps/{:?}.rs",
            self.current_fn.instance().def_id()
        ))
        .and_then(|mut file| {
            file.write_all(
                self.substituted_body
                    .to_string(self.tcx)
                    .unwrap()
                    .as_bytes(),
            )
        })
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
}

impl<'tcx> AnalysisDomain<'tcx> for TypeCollector<'tcx> {
    type Domain = TypeTracker<'tcx>;

    const NAME: &'static str = "TypeCollector";

    fn bottom_value(&self, body: &Body<'tcx>) -> TypeTracker<'tcx> {
        let def_id = self.current_fn.instance().def_id();

        // Collect original types for all places.
        let all_places = body.all_places(self.tcx, def_id);
        let tracked_types = HashMap::from_iter(all_places.map(|place| {
            let place = NormalizedPlace::from_place(&place, self.tcx, def_id);
            let place_ty = place.ty(body, self.tcx).ty;
            self.closure_storage_ref.borrow_mut().try_insert(
                place_ty,
                self.current_fn.instance(),
                vec![],
                self.tcx,
            );
            (place, TrackedTy::from_ty(place_ty))
        }));

        let mut type_tracker = TypeTracker::new(tracked_types);

        if self.current_fn.is_closure() {
            // Augment with types from tracked upvars.
            let upvars = self.current_fn.expect_closure_info();
            type_tracker.augment_closure_with_upvars(upvars, body, def_id, self.tcx);
        }
        type_tracker.augment_with_args(self.current_fn.tracked_args(), body, def_id, self.tcx);
        type_tracker
    }

    fn initialize_start_block(&self, _body: &Body<'tcx>, _state: &mut Self::Domain) {}
}

impl<'tcx> Analysis<'tcx> for TypeCollector<'tcx> {
    fn apply_statement_effect(
        &self,
        state: &mut TypeTracker<'tcx>,
        statement: &Statement<'tcx>,
        _location: Location,
    ) {
        let statement_owned = statement.to_owned();
        if let StatementKind::Assign(box (place, rvalue)) = statement_owned.kind {
            let rvalue_tracked_ty = rvalue.tracked_ty(
                state,
                self.closure_storage_ref.clone(),
                self.current_fn.instance(),
                self.tcx,
            );
            let normalized_place =
                NormalizedPlace::from_place(&place, self.tcx, self.current_fn.instance().def_id());

            trace!(
                "handling statement current_fn={:?} place={:?} ty={:?}",
                self.current_fn.instance().def_id(),
                normalized_place,
                rvalue_tracked_ty,
            );

            state.update_with(
                normalized_place,
                rvalue_tracked_ty,
                &self.substituted_body,
                self.current_fn.instance().def_id(),
                self.tcx,
            );
        }
    }
    fn apply_terminator_effect<'mir>(
        &self,
        state: &mut TypeTracker<'tcx>,
        terminator: &'mir Terminator<'tcx>,
        _location: Location,
    ) {
        if let TerminatorKind::Call {
            func,
            args,
            destination,
            ..
        } = &terminator.kind
        {
            // Apply substitutions to the type in case it contains generics.
            let fn_ty = self
                .current_fn
                .substitute(func.ty(&self.substituted_body, self.tcx), self.tcx);

            // TODO: handle different call types (e.g. FnPtr).
            if let ty::FnDef(def_id, substs) = fn_ty.kind() {
                let arg_tys = ArgTys::from_args(
                    args,
                    self.current_fn.instance().def_id(),
                    &self.substituted_body,
                    state,
                    self.tcx,
                );

                // Calculate argument types, account for possible erasure.
                let plausible_fns = self.current_fn.resolve(
                    def_id.to_owned(),
                    substs,
                    &arg_tys,
                    self.closure_storage_ref.clone(),
                    self.tcx,
                );

                if !plausible_fns.is_empty() {
                    for fn_data in plausible_fns.into_iter() {
                        let def_id = fn_data.instance().def_id();

                        // Skip if the call is repeated.
                        let current_seen_item =
                            VirtualStackItem::new(def_id, fn_data.tracked_args().to_owned());
                        if self.virtual_stack.contains(&current_seen_item) {
                            continue;
                        };

                        state.add_call(Call::new(def_id, args.to_owned()));

                        let return_ty = if self.tcx.is_mir_available(def_id) {
                            let body = fn_data
                                .substitute(self.tcx.optimized_mir(def_id).to_owned(), self.tcx);

                            // Swap the current instance and continue recursively.
                            let results = TypeCollector::new(
                                fn_data.clone(),
                                self.virtual_stack.clone(),
                                self.fn_storage_ref.clone(),
                                self.closure_storage_ref.clone(),
                                self.tcx,
                            )
                            .run();

                            self.fn_storage_ref.borrow_mut().add_with_body(
                                self.current_fn.instance().to_owned(),
                                fn_data.instance().to_owned(),
                                results.places().to_owned(),
                                results.calls().to_owned(),
                                body.to_owned(),
                                body.span,
                            );

                            results.return_type(def_id, &body, self.tcx)
                        } else {
                            self.fn_storage_ref.borrow_mut().add_without_body(
                                self.current_fn.instance().to_owned(),
                                def_id.to_owned(),
                                arg_tys.as_vec().to_owned(),
                                self.tcx,
                            );
                            TrackedTy::from_ty(
                                self.tcx
                                    .fn_sig(def_id)
                                    .subst(self.tcx, fn_data.instance().substs)
                                    .output()
                                    .skip_binder(),
                            )
                        };

                        let normalized_destination = NormalizedPlace::from_place(
                            &destination,
                            self.tcx,
                            self.current_fn.instance().def_id(),
                        );

                        trace!(
                            "handling return terminator current_fn={:?} place={:?} ty={:?} from={:?}",
                            self.current_fn.instance().def_id(),
                            normalized_destination,
                            return_ty,
                            def_id
                        );

                        state.update_with(
                            normalized_destination,
                            return_ty,
                            &self.substituted_body,
                            self.current_fn.instance().def_id(),
                            self.tcx,
                        );
                    }
                } else {
                    state.add_call(Call::new(def_id.to_owned(), args.to_owned()));
                    self.fn_storage_ref.borrow_mut().add_without_body(
                        self.current_fn.instance().to_owned(),
                        def_id.to_owned(),
                        arg_tys.as_vec().to_owned(),
                        self.tcx,
                    );
                }
            } else {
                self.fn_storage_ref
                    .borrow_mut()
                    .add_unhandled(terminator.to_owned());
            }
        }
    }

    fn apply_call_return_effect(
        &self,
        _state: &mut TypeTracker<'tcx>,
        _block: BasicBlock,
        _return_places: CallReturnPlaces<'_, 'tcx>,
    ) {
    }
}
