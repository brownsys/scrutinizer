use rustc_middle::mir::Body;
use rustc_mir_dataflow::AnalysisDomain;
use rustc_utils::BodyExt;
use std::collections::HashMap;

use crate::collector::closure_collector::CollectClosures;
use crate::collector::collector::Collector;
use crate::collector::collector_domain::CollectorDomain;
use crate::common::{NormalizedPlace, TrackedTy};

/// This impl block implements functions responsible for interoperation with rustc_mir_dataflow.
impl<'tcx> AnalysisDomain<'tcx> for Collector<'tcx> {
    type Domain = CollectorDomain<'tcx>;

    const NAME: &'static str = "TypeCollector";

    /// Sets up the analysis initial value from a given body.
    fn bottom_value(&self, body: &Body<'tcx>) -> CollectorDomain<'tcx> {
        let def_id = self.get_current_function().instance().def_id();

        // Collect original types for all places.
        let all_places = body.all_places(self.get_tcx(), def_id);
        let tracked_types = HashMap::from_iter(all_places.map(|place| {
            let place = NormalizedPlace::from_place(&place, self.get_tcx(), def_id);
            let place_ty = place.ty(body, self.get_tcx()).ty;
            (place, TrackedTy::from_ty(place_ty))
        }));

        // Collect all closure types in a body.
        body.collect_closures(
            self.get_tcx(),
            self.get_closure_info_storage(),
            self.get_current_function(),
        );

        // Initialize the domain.
        let mut type_tracker = CollectorDomain::new(tracked_types);

        // Augment closure with types from tracked upvars.
        if self.get_current_function().is_closure() {
            let upvars = self.get_current_function().expect_closure();
            type_tracker.augment_closure_with_upvars(upvars, body, def_id, self.get_tcx());
        }

        // Augment with types from tracked args.
        type_tracker.augment_with_args(
            self.get_current_function().tracked_args(),
            body,
            def_id,
            self.get_tcx(),
        );

        type_tracker
    }

    // TODO: do we care about this method?
    fn initialize_start_block(&self, _body: &Body<'tcx>, _state: &mut Self::Domain) {}
}
