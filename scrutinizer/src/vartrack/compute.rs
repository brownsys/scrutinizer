use super::facts;
use super::intern;
use super::tab_delim;

use rustc_borrowck::BodyWithBorrowckFacts;
use rustc_hir::def_id::{DefId, LOCAL_CRATE};
use rustc_index::vec::IndexVec;
use rustc_middle::{
    dep_graph::DepContext,
    mir::{BasicBlock, Body, Local, Place, StatementKind, TerminatorKind},
    ty::TyCtxt,
};

use std::mem::transmute;
use std::path::Path;
use std::rc::Rc;

use flowistry::{
    indexed::impls::LocationOrArg,
    infoflow::{Direction, FlowAnalysis},
    mir::{aliases::Aliases, engine},
};

use polonius_engine::Algorithm;
use polonius_engine::Output as PoloniusEngineOutput;

// This is needed to interoperate with rustc's LocationTable, which is pub(crate) by default.
pub struct LocationTableShim {
    num_points: usize,
    statements_before_block: IndexVec<BasicBlock, usize>,
}

impl LocationTableShim {
    pub fn new(body: &Body<'_>) -> Self {
        let mut num_points = 0;
        let statements_before_block = body
            .basic_blocks
            .iter()
            .map(|block_data| {
                let v = num_points;
                num_points += (block_data.statements.len() + 1) * 2;
                v
            })
            .collect();

        Self {
            num_points,
            statements_before_block,
        }
    }
}

pub type Output = PoloniusEngineOutput<facts::LocalFacts>;

// This function computes all locals that depend on the argument local for a given def_id.
pub fn compute_dependent_locals<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: DefId,
    targets: Vec<Vec<(Place<'tcx>, LocationOrArg)>>,
    direction: Direction,
) -> Vec<Local> {
    // Retrieve optimized MIR body.
    // For foreign crate items, it would be saved during the crate's compilation.
    let body = tcx.optimized_mir(def_id).to_owned();

    // Create the shimmed LocationTable, which is identical to the original LocationTable.
    let location_table = LocationTableShim::new(&body);

    // Find the directory with precomputed borrow checker facts for a given DefId.
    // TODO: this mechanism is quite brittle, need a more robust approach.
    let facts_dir = {
        let nll_filename = tcx.def_path(def_id).to_filename_friendly_no_crate();
        if def_id.krate == LOCAL_CRATE {
            format!("./nll-facts/{}", nll_filename)
        } else {
            let diagnostic_string = tcx.sess().source_map().span_to_diagnostic_string(body.span);
            let split_path = diagnostic_string.rsplit_once("/src").unwrap();
            format!("{}/nll-facts/{}", split_path.0, nll_filename)
        }
    };

    // Run polonius on the borrow checker facts.
    let (input_facts, output_facts) = {
        let tables = &mut intern::InternerTables::new();
        let all_facts =
            tab_delim::load_tab_delimited_facts(tables, &Path::new(&facts_dir)).unwrap();
        let algorithm = Algorithm::Hybrid;
        let output = Output::compute(&all_facts, algorithm, false);
        (all_facts, output)
    };

    // Construct a body with borrow checker facts required for Flowistry.
    let body_with_facts = BodyWithBorrowckFacts {
        body,
        input_facts: unsafe { transmute(input_facts) },
        output_facts: Rc::new(unsafe { transmute(output_facts) }),
        location_table: unsafe { transmute(location_table) },
    };

    // Run analysis on the body with with borrow checker facts.
    let results = {
        let aliases = Aliases::build(tcx, def_id, &body_with_facts);
        let location_domain = aliases.location_domain().clone();
        let analysis = FlowAnalysis::new(tcx, def_id, &body_with_facts.body, aliases);
        engine::iterate_to_fixpoint(tcx, &body_with_facts.body, location_domain, analysis)
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
                let stmt_or_terminator = body_with_facts.body.stmt_at(*location);
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
