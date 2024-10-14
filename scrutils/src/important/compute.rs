use log::trace;
use rustc_hir::def_id::DefId;
use rustc_middle::{
    mir::{Local, Place, StatementKind, TerminatorKind},
    ty::TyCtxt,
};

use flowistry::{
    infoflow::{Direction, FlowAnalysis},
    mir::{engine, placeinfo::PlaceInfo, FlowistryInput},
};

use crate::body_cache::BodyCache;
use either::Either;
use rustc_utils::mir::location_or_arg::LocationOrArg;

// This function computes all locals that depend on the argument local for a given def_id.
pub fn compute_dependent_locals<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: DefId,
    targets: Vec<Vec<(Place<'tcx>, LocationOrArg)>>,
    direction: Direction,
) -> Vec<Local> {
    let cache = BodyCache::new(tcx);
    let body_with_facts = cache
        .get(def_id)
        .expect("Failed to read body information from disk.");

    let place_info = PlaceInfo::build(tcx, def_id, body_with_facts);
    let location_domain = place_info.location_domain().clone();

    let body = body_with_facts.body();

    let results = {
        let analysis = FlowAnalysis::new(tcx, def_id, body, place_info);
        engine::iterate_to_fixpoint(tcx, body, location_domain, analysis)
    };

    trace!("computing location dependencies for {:?}, {:?}", def_id, targets);
    // Use Flowistry to compute the locations and places influenced by the target.
    let location_deps =
        flowistry::infoflow::compute_dependencies(&results, targets.clone(), direction)
            .into_iter()
            .reduce(|acc, e| {
                let mut new_acc = acc.clone();
                new_acc.union(&e);
                new_acc
            })
            .unwrap();

    // Merge location dependencies and extract locals from them.
    location_deps
        .iter()
        .map(|dep| match dep {
            LocationOrArg::Location(location) => {
                let stmt_or_terminator = body_with_facts.body().stmt_at(*location);
                match stmt_or_terminator {
                    Either::Left(stmt) => match &stmt.kind {
                        StatementKind::Assign(assign) => {
                            let (place, _) = **assign;
                            vec![place.local]
                        }
                        _ => {
                            unimplemented!()
                        }
                    },
                    Either::Right(terminator) => match &terminator.kind {
                        TerminatorKind::Call { destination, .. } => {
                            vec![destination.local]
                        }
                        TerminatorKind::SwitchInt { .. } => vec![],
                        _ => {
                            unimplemented!()
                        }
                    },
                }
            }
            LocationOrArg::Arg(local) => vec![*local],
        })
        .flatten()
        .collect()
}
