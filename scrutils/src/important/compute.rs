use crate::important::aliases::Aliases;
use crate::important::nll_facts::{create_location_table, load_facts_for_flowistry};

use rustc_hir::def_id::{DefId, LOCAL_CRATE};
use rustc_middle::{
    dep_graph::DepContext,
    mir::{Local, Place, StatementKind, TerminatorKind},
    ty::TyCtxt,
};

use std::path::Path;

use flowistry::{
    indexed::impls::LocationOrArg,
    infoflow::{Direction, FlowAnalysis},
    mir::engine,
};

extern crate polonius_engine;

// This function computes all locals that depend on the argument local for a given def_id.
pub fn compute_dependent_locals<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: DefId,
    targets: Vec<Vec<(Place<'tcx>, LocationOrArg)>>,
    direction: Direction,
) -> Vec<Local> {
    // Retrieve optimized MIR body.
    // For foreign crate items, it would be saved during the crate's compilation.
    let body = tcx.promoted_mir(def_id)[0].1;

    // Create the shimmed LocationTable, which is identical to the original LocationTable.
    let location_table = create_location_table(&body);

    // Find the directory with precomputed borrow checker facts for a given DefId.
    let facts_dir = {
        let def_path = tcx.def_path(def_id);
        let nll_filename = def_path.to_filename_friendly_no_crate();
        if def_id.krate == LOCAL_CRATE {
            format!("./nll-facts/{}", nll_filename)
        } else {
            let core_def_id = {
                let mut core_candidate = def_id;
                while tcx.opt_parent(core_candidate).is_some() {
                    core_candidate = tcx.opt_parent(core_candidate).unwrap();
                }
                core_candidate
            };
            let diagnostic_string = tcx
                .sess()
                .source_map()
                .span_to_diagnostic_string(tcx.def_span(core_def_id));
            let split_path = diagnostic_string.rsplit_once("/src").unwrap();
            format!("{}/nll-facts/{}", split_path.0, nll_filename)
        }
    };

    let flowistry_facts =
        load_facts_for_flowistry(&location_table, &Path::new(&facts_dir)).unwrap();

    // Run analysis on the body with with borrow checker facts.
    let results = {
        let aliases = Aliases::build(tcx, def_id, &body, &flowistry_facts);
        let location_domain = aliases.location_domain().clone();
        let analysis =
            FlowAnalysis::new(tcx, def_id, &body, unsafe { std::mem::transmute(aliases) });
        engine::iterate_to_fixpoint(tcx, &body, location_domain, analysis)
    };

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
        .filter_map(|dep| match dep {
            LocationOrArg::Location(location) => {
                let stmt_or_terminator = body.stmt_at(*location);
                if stmt_or_terminator.is_left() {
                    stmt_or_terminator.left().and_then(|stmt| {
                        if let StatementKind::Assign(assign) = &stmt.kind {
                            let (place, _) = **assign;
                            Some(place.local)
                        } else {
                            None
                        }
                    })
                } else {
                    stmt_or_terminator.right().and_then(|terminator| {
                        if let TerminatorKind::Call { destination, .. } = &terminator.kind {
                            Some(destination.local)
                        } else {
                            None
                        }
                    })
                }
            }
            LocationOrArg::Arg(local) => Some(*local),
        })
        .collect()
}
