use log::trace;
use rustc_middle::mir::{
    BasicBlock, Body, Local, Location, Place, Statement, StatementKind, Terminator, TerminatorKind,
};
use rustc_middle::ty::{self, TyCtxt};
use rustc_mir_dataflow::{
    fmt::DebugWithContext, Analysis, AnalysisDomain, CallReturnPlaces, JoinSemiLattice,
};
use rustc_utils::{BodyExt, PlaceExt};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::dataflow_shim::iterate_to_fixpoint;
use super::fn_call_info::FnCallInfo;
use super::fn_data::FnData;
use super::partial_fn_data::PartialFnData;
use super::raw_ptr::HasRawPtrDeref;
use super::storage::FnCallStorage;
use super::tracked_ty::HasTrackedTy;
use super::tracked_ty::TrackedTy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedTypeMap<'tcx> {
    pub map: HashMap<Place<'tcx>, TrackedTy<'tcx>>,
}

impl<'tcx> TrackedTypeMap<'tcx> {
    pub fn new(map: HashMap<Place<'tcx>, TrackedTy<'tcx>>) -> Self {
        TrackedTypeMap { map }
    }
}

impl<'tcx> DebugWithContext<TypeTracker<'tcx>> for TrackedTypeMap<'tcx> {}

impl<'tcx> JoinSemiLattice for TrackedTypeMap<'tcx> {
    fn join(&mut self, other: &Self) -> bool {
        let updated = other.map.iter().fold(false, |acc, (key, other_value)| {
            let self_value = self.map.get_mut(key).unwrap();
            let updated = self_value.join(other_value);
            acc || updated
        });
        updated
    }
}

pub struct TypeTracker<'tcx> {
    storage: Rc<RefCell<FnCallStorage<'tcx>>>,
    current_fn: FnData<'tcx>,
    tcx: TyCtxt<'tcx>,
}

impl<'tcx> TypeTracker<'tcx> {
    pub fn new(
        current_fn: FnData<'tcx>,
        storage: Rc<RefCell<FnCallStorage<'tcx>>>,
        tcx: TyCtxt<'tcx>,
    ) -> Self {
        TypeTracker {
            storage,
            current_fn,
            tcx,
        }
    }
    pub fn run(self) -> TrackedTypeMap<'tcx> {
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

impl<'tcx> AnalysisDomain<'tcx> for TypeTracker<'tcx> {
    type Domain = TrackedTypeMap<'tcx>;

    const NAME: &'static str = "TypeTracker";

    fn bottom_value(&self, body: &Body<'tcx>) -> TrackedTypeMap<'tcx> {
        let all_places = body.all_places(self.tcx, self.current_fn.instance().def_id());
        let mut tracked_type_map = HashMap::from_iter(all_places.map(|place| {
            let place = place.normalize(self.tcx, self.current_fn.instance().def_id());
            (place, TrackedTy::determine(place.ty(body, self.tcx).ty))
        }));
        self.current_fn
            .tracked_args()
            .iter()
            .enumerate()
            .for_each(|(i, tracked_ty)| {
                let place = Place::from_local(Local::from_usize(i + 1), self.tcx)
                    .normalize(self.tcx, self.current_fn.instance().def_id());
                tracked_type_map.get_mut(&place).unwrap().join(tracked_ty);
            });
        TrackedTypeMap::new(tracked_type_map)
    }

    fn initialize_start_block(&self, _body: &Body<'tcx>, _state: &mut Self::Domain) {}
}

impl<'tcx> Analysis<'tcx> for TypeTracker<'tcx> {
    // Required methods
    fn apply_statement_effect(
        &self,
        state: &mut TrackedTypeMap<'tcx>,
        statement: &Statement<'tcx>,
        _location: Location,
    ) {
        let body = self.tcx.optimized_mir(self.current_fn.instance().def_id());
        let statement_owned = statement.to_owned();
        if let StatementKind::Assign(box (place, rvalue)) = statement_owned.kind {
            let rvalue_tracked_ty = rvalue.tracked_ty(state, body, self.tcx);
            let normalized_place = place.normalize(self.tcx, self.current_fn.instance().def_id());
            let place_ty_ref = state.map.get_mut(&normalized_place).unwrap();
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
        state: &mut TrackedTypeMap<'tcx>,
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
            // Attempt to resolve the callee instance via monomorphization.
            let fn_ty = self
                .current_fn
                .instance()
                .subst_mir_and_normalize_erasing_regions(
                    self.tcx,
                    ty::ParamEnv::reveal_all(),
                    func.ty(
                        self.tcx.optimized_mir(self.current_fn.instance().def_id()),
                        self.tcx,
                    ),
                );

            // TODO: handle different call types (e.g. FnPtr).
            if let ty::FnDef(def_id, substs) = fn_ty.kind() {
                // Calculate argument types, account for possible erasure.
                let outer_body = self.tcx.optimized_mir(self.current_fn.instance().def_id());
                let partial_fn_data =
                    PartialFnData::new(def_id, substs, args, state, outer_body, self.tcx);

                let plausible_fns =
                    partial_fn_data.try_resolve(&self.current_fn.important_locals(), self.tcx);

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

                        if self.storage.borrow().encountered_fn_call(&fn_call_info) {
                            continue;
                        }
                        self.storage.borrow_mut().add_call(fn_call_info);

                        // Swap the current instance and continue recursively.
                        let new_analysis =
                            TypeTracker::new(fn_data, self.storage.clone(), self.tcx);
                        let results = new_analysis.run();

                        let normalized_destination =
                            destination.normalize(self.tcx, self.current_fn.instance().def_id());

                        let destination_place_ty_ref =
                            state.map.get_mut(&normalized_destination).unwrap();

                        let normalized_return_place =
                            Place::return_place().normalize(self.tcx, def_id);

                        let inferred_return_ty = results.map.get(&normalized_return_place).unwrap();
                        let provided_return_ty =
                            TrackedTy::determine(normalized_return_place.ty(body, self.tcx).ty);

                        let return_ty = match provided_return_ty {
                            TrackedTy::Simple(..) => &provided_return_ty,
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
                    self.storage.borrow_mut().add_call(FnCallInfo::WithoutBody {
                        def_id: def_id.to_owned(),
                        from: self.current_fn.instance().def_id(),
                        tracked_args: partial_fn_data.get_arg_tys(),
                    });
                }
            } else {
                self.storage
                    .borrow_mut()
                    .add_unhandled(terminator.to_owned());
            }
        }
    }

    fn apply_call_return_effect(
        &self,
        _state: &mut TrackedTypeMap<'tcx>,
        _block: BasicBlock,
        _return_places: CallReturnPlaces<'_, 'tcx>,
    ) {
    }
}
