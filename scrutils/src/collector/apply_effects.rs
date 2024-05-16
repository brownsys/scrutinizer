use log::warn;
use rustc_middle::mir::{
    BasicBlock, Location, Operand, Statement, StatementKind, Terminator, TerminatorKind,
};
use rustc_middle::ty::{self};
use rustc_mir_dataflow::{Analysis, CallReturnPlaces};

use crate::collector::collector::Collector;
use crate::collector::collector_domain::CollectorDomain;
use crate::collector::has_tracked_ty::HasTrackedTy;
use crate::common::{ArgTys, NormalizedPlace, TrackedTy};

/// This impl block implements functions responsible for interoperation with rustc_mir_dataflow.
impl<'tcx> Analysis<'tcx> for Collector<'tcx> {
    /// Update tracked type values based on a statement (we are only concerened about assignments).
    fn apply_statement_effect(
        &self,
        state: &mut CollectorDomain<'tcx>,
        statement: &Statement<'tcx>,
        _location: Location,
    ) {
        if let StatementKind::Assign(box (place, rvalue)) = statement.to_owned().kind {
            // Get lvalue and rvalue, update the analysis domain.
            let rvalue_tracked_ty = rvalue.tracked_ty(
                state,
                self.get_closure_info_storage(),
                self.get_current_function().instance(),
                self.get_tcx(),
            );
            let normalized_place = NormalizedPlace::from_place(
                &place,
                self.get_tcx(),
                self.get_current_function().instance().def_id(),
            );

            state.update_with(
                normalized_place,
                rvalue_tracked_ty,
                &self.get_substituted_body(),
                self.get_current_function().instance().def_id(),
                self.get_tcx(),
            );
        }
    }

    /// Update tracked type values based on a terminator.
    fn apply_terminator_effect<'mir>(
        &self,
        state: &mut CollectorDomain<'tcx>,
        terminator: &'mir Terminator<'tcx>,
        _location: Location,
    ) {
        match terminator.kind {
            // Process a simple function call.
            TerminatorKind::Call {
                ref func,
                ref args,
                ref destination,
                ..
            } => {
                // Figure out function type and arguments, dispatch to the function call collector.
                let function_ty = func.ty(&self.get_substituted_body(), self.get_tcx());
                let arg_tys = state.construct_args(
                    args,
                    self.get_current_function().instance().def_id(),
                    &self.get_substituted_body(),
                    self.get_tcx(),
                );
                self.collect_function_call(function_ty, args, arg_tys, state, Some(destination));
            }
            // Process a drop which could hide some leaking functionality.
            TerminatorKind::Drop { place, .. } => {
                // Figure out what we are dropping.
                let place_ty = place.ty(&self.get_substituted_body(), self.get_tcx());
                if let ty::TyKind::Adt(adt_def, substs) = place_ty.ty.kind() {
                    // Retrieve the destructor if exists.
                    let destructor = self.get_tcx().adt_destructor(adt_def.did());
                    // Process destructor as a regular call if it exists.
                    if let Some(destructor) = destructor {
                        // Get the function type and populate with in-scope substitutions.
                        let destructor_function_ty = self
                            .get_tcx()
                            .type_of(destructor.did)
                            .subst(self.get_tcx(), substs);
                        // Destructor has only one argument -- the object itself.
                        let destructor_args = &vec![Operand::Copy(place)];
                        // This is a bit unwieldy, but it just creates a &mut PlaceTy type.
                        let destructor_arg_tys =
                            ArgTys::new(vec![TrackedTy::from_ty(self.get_tcx().mk_mut_ref(
                                self.get_tcx().mk_region_from_kind(ty::RegionKind::ReErased),
                                place.ty(&self.get_substituted_body(), self.get_tcx()).ty,
                            ))]);

                        self.collect_function_call(
                            destructor_function_ty,
                            destructor_args,
                            destructor_arg_tys,
                            state,
                            None,
                        );
                    }
                } else {
                    // TODO: some other drops beyond adt's could also leak.
                    warn!("dropping a non-adt object: {}", place_ty.ty);
                }
            }
            _ => {}
        }
    }

    // TODO: do we care about this method?
    fn apply_call_return_effect(
        &self,
        _state: &mut CollectorDomain<'tcx>,
        _block: BasicBlock,
        _return_places: CallReturnPlaces<'_, 'tcx>,
    ) {
    }
}
