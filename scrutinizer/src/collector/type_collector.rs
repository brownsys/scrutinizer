use log::trace;
use rustc_middle::mir::{
    BasicBlock, Body, Local, Location, Place, Statement, StatementKind, Terminator, TerminatorKind,
};
use rustc_middle::ty::{self, TyCtxt};
use rustc_mir_dataflow::{Analysis, AnalysisDomain, CallReturnPlaces, JoinSemiLattice};
use rustc_utils::BodyExt;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

use super::callee::Callee;
use super::dataflow_shim::iterate_to_fixpoint;
use super::fn_info::FnInfo;
use super::has_tracked_ty::HasTrackedTy;
use super::normalized_place::NormalizedPlace;
use super::storage::FnInfoStorageRef;
use super::tracked_ty::TrackedTy;
use super::type_tracker::TypeTracker;
use super::upvar_tracker::{TrackedUpvars, UpvarTrackerRef};
use super::virtual_stack::{VirtualStack, VirtualStackItem};

pub struct TypeCollector<'tcx> {
    storage_ref: FnInfoStorageRef<'tcx>,
    upvars_ref: UpvarTrackerRef<'tcx>,
    virtual_stack: VirtualStack<'tcx>,
    current_fn: Callee<'tcx>,
    tcx: TyCtxt<'tcx>,
}

impl<'tcx> TypeCollector<'tcx> {
    pub fn new(
        current_fn: Callee<'tcx>,
        storage_ref: FnInfoStorageRef<'tcx>,
        virtual_stack: VirtualStack<'tcx>,
        upvars_ref: UpvarTrackerRef<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Self {
        TypeCollector {
            storage_ref,
            upvars_ref,
            virtual_stack,
            current_fn,
            tcx,
        }
    }
    pub fn run(mut self) -> TypeTracker<'tcx> {
        self.virtual_stack.push(VirtualStackItem::new(
            self.current_fn.instance().def_id(),
            self.current_fn.tracked_args().to_owned(),
        ));

        match self
            .storage_ref
            .borrow()
            .get_regular(self.current_fn.instance())
        {
            Some(places) => return TypeTracker { places },
            None => {}
        }

        let tcx = self.tcx;
        let body = tcx.optimized_mir(self.current_fn.instance().def_id());

        File::create(format!(
            "mir_dumps/{:?}.rs",
            self.current_fn.instance().def_id()
        ))
        .and_then(|mut file| file.write_all(body.to_string(tcx).unwrap().as_bytes()))
        .unwrap();

        let mut cursor = iterate_to_fixpoint(self.into_engine(tcx, body)).into_results_cursor(body);
        body.basic_blocks
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
            let ty = place.ty(body, self.tcx).ty;
            if ty.is_closure() {
                let resolved_closure_ty = self
                    .current_fn
                    .instance()
                    .subst_mir_and_normalize_erasing_regions(
                        self.tcx,
                        ty::ParamEnv::reveal_all(),
                        ty,
                    );
                self.upvars_ref.borrow_mut().insert(
                    ty,
                    TrackedUpvars {
                        upvars: vec![],
                        resolved_ty: resolved_closure_ty,
                    },
                );
            }
            (place, TrackedTy::from_ty(ty))
        }));

        let mut type_tracker = TypeTracker::new(tracked_types);

        // Augment with types from tracked arguments.
        self.current_fn
            .tracked_args()
            .iter()
            .enumerate()
            .for_each(|(i, tracked_ty)| {
                type_tracker.augment_with_args(
                    Local::from_usize(i + 1),
                    tracked_ty,
                    body,
                    def_id,
                    self.tcx,
                );
            });

        // Augment with trypes from tracked upvars.
        if self.current_fn.is_closure() {
            let upvars = self.current_fn.expect_upvars();
            type_tracker.augment_closure_with_upvars(upvars, body, def_id, self.tcx);
        };
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
                self.upvars_ref.clone(),
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

            let place_ty_ref = state.places.get_mut(&normalized_place).unwrap();
            place_ty_ref.join(&rvalue_tracked_ty);

            state.propagate(
                &normalized_place,
                &rvalue_tracked_ty,
                self.tcx.optimized_mir(self.current_fn.instance().def_id()),
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
            let outer_def_id = self.current_fn.instance().def_id();
            let outer_body = self.tcx.optimized_mir(outer_def_id);

            // Apply substitutions to the type in case it contains generics.
            let fn_ty = self
                .current_fn
                .instance()
                .subst_mir_and_normalize_erasing_regions(
                    self.tcx,
                    ty::ParamEnv::reveal_all(),
                    func.ty(outer_body, self.tcx),
                );

            // TODO: handle different call types (e.g. FnPtr).
            if let ty::FnDef(def_id, substs) = fn_ty.kind() {
                let arg_tys: Vec<TrackedTy<'tcx>> = args
                    .iter()
                    .map(|arg| {
                        arg.place()
                            .and_then(|place| {
                                Some(NormalizedPlace::from_place(&place, self.tcx, outer_def_id))
                            })
                            .and_then(|place| state.places.get(&place))
                            .and_then(|ty| Some(ty.to_owned()))
                            .unwrap_or(TrackedTy::from_ty(arg.ty(outer_body, self.tcx)))
                    })
                    .collect();
                // Calculate argument types, account for possible erasure.
                let plausible_fns = Callee::resolve(
                    def_id.to_owned(),
                    substs,
                    &arg_tys,
                    self.upvars_ref.clone(),
                    self.tcx,
                );

                if !plausible_fns.is_empty() {
                    for fn_data in plausible_fns.into_iter() {
                        let fn_data = if !self.tcx.is_closure(fn_data.instance().def_id()) {
                            let resolved_instance = self
                                .current_fn
                                .instance()
                                .subst_mir_and_normalize_erasing_regions(
                                    self.tcx,
                                    ty::ParamEnv::reveal_all(),
                                    fn_data.instance().to_owned(),
                                );
                            Callee::new_function(
                                resolved_instance,
                                fn_data.tracked_args().to_owned(),
                            )
                        } else {
                            match self.upvars_ref.borrow().get(
                                &self
                                    .tcx
                                    .type_of(fn_data.instance().def_id())
                                    .subst_identity(),
                            ) {
                                Some(upvars) => {
                                    let resolved_instance = match upvars.resolved_ty.kind() {
                                        ty::TyKind::Closure(def_id, substs) => {
                                            ty::Instance::resolve(
                                                self.tcx,
                                                ty::ParamEnv::reveal_all(),
                                                *def_id,
                                                substs,
                                            )
                                            .unwrap()
                                            .unwrap()
                                        }
                                        _ => unreachable!(""),
                                    };

                                    Callee::new_closure(
                                        resolved_instance,
                                        fn_data.tracked_args().to_owned(),
                                        fn_data.expect_upvars().to_owned(),
                                    )
                                }
                                None => Callee::new_function(
                                    fn_data.instance().to_owned(),
                                    fn_data.tracked_args().to_owned(),
                                ),
                            }
                        };

                        let def_id = fn_data.instance().def_id();

                        let current_seen_item =
                            VirtualStackItem::new(def_id, fn_data.tracked_args().to_owned());

                        if self.virtual_stack.contains(&current_seen_item) {
                            continue;
                        };

                        let normalized_return_place =
                            NormalizedPlace::from_place(&Place::return_place(), self.tcx, def_id);

                        let return_ty = if self.tcx.is_mir_available(def_id) {
                            let body = self.tcx.optimized_mir(def_id);
                            dbg!(def_id);

                            // Swap the current instance and continue recursively.
                            let new_analysis = TypeCollector::new(
                                fn_data.clone(),
                                self.storage_ref.clone(),
                                self.virtual_stack.clone(),
                                self.upvars_ref.clone(),
                                self.tcx,
                            );
                            let results = new_analysis.run();
                            let fn_call_info = FnInfo::Regular {
                                parent: self.current_fn.instance().to_owned(),
                                instance: fn_data.instance().to_owned(),
                                places: results.places.clone(),
                                span: body.span,
                            };
                            self.storage_ref.borrow_mut().add_fn(fn_call_info);

                            let inferred_return_ty = results
                                .places
                                .get(&normalized_return_place)
                                .unwrap()
                                .to_owned();

                            let provided_return_ty =
                                TrackedTy::from_ty(normalized_return_place.ty(body, self.tcx).ty);

                            match provided_return_ty {
                                TrackedTy::Present(..) => provided_return_ty,
                                TrackedTy::Erased(..) => inferred_return_ty,
                            }
                        } else {
                            let fn_call_info = FnInfo::Extern {
                                parent: self.current_fn.instance().to_owned(),
                                instance: fn_data.instance().to_owned(),
                                tracked_args: arg_tys.clone(),
                            };
                            self.storage_ref.borrow_mut().add_fn(fn_call_info);
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
                            "handling return terminator current_fn={:?} place={:?} ty={:?}",
                            self.current_fn.instance().def_id(),
                            normalized_destination,
                            return_ty
                        );

                        let destination_place_ty_ref =
                            state.places.get_mut(&normalized_destination).unwrap();
                        destination_place_ty_ref.join(&return_ty);

                        state.propagate(
                            &normalized_destination,
                            &return_ty,
                            outer_body,
                            outer_def_id,
                            self.tcx,
                        );
                    }
                } else {
                    self.storage_ref.borrow_mut().add_fn(FnInfo::Ambiguous {
                        parent: self.current_fn.instance().to_owned(),
                        def_id: def_id.to_owned(),
                        tracked_args: arg_tys,
                    });
                }
            } else {
                self.storage_ref
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
