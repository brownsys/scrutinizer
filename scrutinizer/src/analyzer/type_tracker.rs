use log::trace;
use rustc_middle::mir::{
    BasicBlock, Body, Local, Location, Place, PlaceElem, Statement, StatementKind, Terminator,
    TerminatorKind,
};
use rustc_middle::ty::{self, TyCtxt};
use rustc_mir_dataflow::{
    fmt::DebugWithContext, Analysis, AnalysisDomain, CallReturnPlaces, JoinSemiLattice,
};
use rustc_utils::{BodyExt, PlaceExt};
use std::collections::HashMap;

use super::dataflow_shim::iterate_to_fixpoint;
use super::fn_call_info::FnCallInfo;
use super::fn_data::FnData;
use super::normalized_place::NormalizedPlace;
use super::raw_ptr::HasRawPtrDeref;
use super::storage::FnCallStorageRef;
use super::tracked_ty::{HasTrackedTy, TrackedTy};
use super::upvar_tracker::UpvarTrackerRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeTracker<'tcx> {
    pub places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>,
}

impl<'tcx> TypeTracker<'tcx> {
    pub fn new(places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>) -> Self {
        TypeTracker { places }
    }
}

impl<'tcx> DebugWithContext<TypeCollector<'tcx>> for TypeTracker<'tcx> {}

impl<'tcx> JoinSemiLattice for TypeTracker<'tcx> {
    fn join(&mut self, other: &Self) -> bool {
        let updated_places = other.places.iter().fold(false, |acc, (key, other_value)| {
            let self_value = self.places.get_mut(key).unwrap();
            let updated = self_value.join(other_value);
            acc || updated
        });
        updated_places
    }
}

pub struct TypeCollector<'tcx> {
    storage_ref: FnCallStorageRef<'tcx>,
    upvars_ref: UpvarTrackerRef<'tcx>,
    current_fn: FnData<'tcx>,
    tcx: TyCtxt<'tcx>,
}

impl<'tcx> TypeCollector<'tcx> {
    pub fn new(
        current_fn: FnData<'tcx>,
        storage_ref: FnCallStorageRef<'tcx>,
        upvars_ref: UpvarTrackerRef<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Self {
        TypeCollector {
            storage_ref,
            current_fn,
            upvars_ref,
            tcx,
        }
    }
    pub fn run(self) -> TypeTracker<'tcx> {
        let tcx = self.tcx;
        let body = tcx.optimized_mir(self.current_fn.instance().def_id());
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
        // Collect original types for all places.
        let all_places = body.all_places(self.tcx, self.current_fn.instance().def_id());
        let mut tracked_types = HashMap::from_iter(all_places.map(|place| {
            let place =
                NormalizedPlace::from_place(&place, self.tcx, self.current_fn.instance().def_id());
            let ty = place.ty(body, self.tcx).ty;
            (place, TrackedTy::from_ty(ty))
        }));
        // Augment with types from tracked arguments.
        self.current_fn
            .tracked_args()
            .iter()
            .enumerate()
            .for_each(|(i, tracked_ty)| {
                let place = NormalizedPlace::from_place(
                    &Place::from_local(Local::from_usize(i + 1), self.tcx),
                    self.tcx,
                    self.current_fn.instance().def_id(),
                );
                tracked_types.get_mut(&place).unwrap().join(tracked_ty);
            });
        // Augment with trypes from tracked upvars.
        if let Some(upvars) = self.current_fn.upvars() {
            let closure_place = Place::make(Local::from_usize(1), &[PlaceElem::Deref], self.tcx);
            let closure_interior_places =
                closure_place.interior_places(self.tcx, body, self.current_fn.instance().def_id());
            upvars
                .iter()
                .zip(closure_interior_places.iter().skip(1))
                .for_each(|(upvar, interior_place)| {
                    let normalized_interior_place = NormalizedPlace::from_place(
                        interior_place,
                        self.tcx,
                        self.current_fn.instance().def_id(),
                    );
                    tracked_types
                        .get_mut(&normalized_interior_place)
                        .unwrap()
                        .join(upvar);
                });
        }
        TypeTracker::new(tracked_types)
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
        let body = self.tcx.optimized_mir(self.current_fn.instance().def_id());
        let statement_owned = statement.to_owned();
        if let StatementKind::Assign(box (place, rvalue)) = statement_owned.kind {
            let rvalue_tracked_ty = rvalue.tracked_ty(
                state,
                self.upvars_ref.clone(),
                body,
                self.current_fn.instance().def_id(),
                self.tcx,
            );
            let normalized_place =
                NormalizedPlace::from_place(&place, self.tcx, self.current_fn.instance().def_id());
            let place_ty_ref = state.places.get_mut(&normalized_place).unwrap();
            place_ty_ref.join(&rvalue_tracked_ty);
            trace!(
                "handling statement current_fn={:?} place={:?} ty={:?} upd={:?}",
                self.current_fn.instance().def_id(),
                normalized_place,
                rvalue_tracked_ty,
                place_ty_ref
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

            // Attempt to resolve the callee instance via monomorphization.
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
                let plausible_fns = FnData::resolve(
                    def_id.to_owned(),
                    substs,
                    &arg_tys,
                    self.upvars_ref.clone(),
                    self.tcx,
                );

                if !plausible_fns.is_empty() {
                    for fn_data in plausible_fns
                        .into_iter()
                        .filter(|func| !self.tcx.is_const_fn_raw(func.instance().def_id()))
                    {
                        let def_id = fn_data.instance().def_id();
                        // Only if we have not seen this call before.
                        let body = self.tcx.optimized_mir(def_id);

                        let fn_call_info = FnCallInfo::WithBody {
                            def_id,
                            from: self.current_fn.instance().def_id(),
                            span: body.span,
                            tracked_args: fn_data.tracked_args().to_owned(),
                            raw_ptr_deref: body.has_raw_ptr_deref(self.tcx),
                        };

                        if self.storage_ref.borrow().encountered_fn_call(&fn_call_info) {
                            continue;
                        }
                        self.storage_ref.borrow_mut().add_call(fn_call_info);

                        // Swap the current instance and continue recursively.
                        let new_analysis = TypeCollector::new(
                            fn_data,
                            self.storage_ref.clone(),
                            self.upvars_ref.clone(),
                            self.tcx,
                        );
                        let results = new_analysis.run();

                        trace!("results for {:?} are {:?}", def_id, results);

                        let normalized_destination = NormalizedPlace::from_place(
                            &destination,
                            self.tcx,
                            self.current_fn.instance().def_id(),
                        );

                        let destination_place_ty_ref =
                            state.places.get_mut(&normalized_destination).unwrap();

                        let normalized_return_place =
                            NormalizedPlace::from_place(&Place::return_place(), self.tcx, def_id);

                        let inferred_return_ty =
                            results.places.get(&normalized_return_place).unwrap();
                        let provided_return_ty =
                            TrackedTy::from_ty(normalized_return_place.ty(body, self.tcx).ty);

                        let return_ty = match provided_return_ty {
                            TrackedTy::Present(..) => &provided_return_ty,
                            TrackedTy::Erased(..) => &inferred_return_ty,
                        };

                        destination_place_ty_ref.join(return_ty);
                        trace!(
                            "handling return terminator current_fn={:?} place={:?} ty={:?}",
                            self.current_fn.instance().def_id(),
                            normalized_destination,
                            return_ty
                        );
                    }
                } else {
                    self.storage_ref
                        .borrow_mut()
                        .add_call(FnCallInfo::WithoutBody {
                            def_id: def_id.to_owned(),
                            from: self.current_fn.instance().def_id(),
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
