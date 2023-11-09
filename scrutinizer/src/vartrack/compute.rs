use super::facts;
use super::intern;
use super::tab_delim;

use flowistry::{
    indexed::impls::{LocationOrArg, LocationOrArgSet},
    infoflow::{Direction, FlowAnalysis},
    mir::{aliases::Aliases, engine},
};

use rustc_borrowck::BodyWithBorrowckFacts;
use rustc_hir::def_id::{DefId, LOCAL_CRATE};
use rustc_index::vec::IndexVec;
use rustc_middle::{
    dep_graph::DepContext,
    mir::{BasicBlock, Body, Local, Place},
    ty::TyCtxt,
};

use rustc_utils::PlaceExt;

use std::error;
use std::fmt;
use std::path::Path;

use polonius_engine::Algorithm;
use polonius_engine::Output as PoloniusEngineOutput;

use std::mem::transmute;
use std::rc::Rc;

#[derive(Debug)]
pub struct Error(String);

impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str(&self.0)
    }
}

macro_rules! attempt {
    ($($tokens:tt)*) => {
        (|| Ok({ $($tokens)* }))()
    };
}

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

// This is the core analysis. Everything below this function is plumbing to
// call into rustc's API.
pub fn compute_dependencies<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: DefId,
    arg_local: Local,
) -> LocationOrArgSet {
    let body = tcx.optimized_mir(def_id).to_owned();

    let location_table = LocationTableShim::new(&body);
    let nll_filename = tcx.def_path(def_id).to_filename_friendly_no_crate();

    // TODO: this is really brittle, can we get this info somehow differently?
    let facts_dir = if def_id.krate == LOCAL_CRATE {
        format!("./nll-facts/{}", nll_filename)
    } else {
        let diagnostic_string = tcx.sess().source_map().span_to_diagnostic_string(body.span);
        let split_path = diagnostic_string.rsplit_once("/src").unwrap();
        format!("{}/nll-facts/{}", split_path.0, nll_filename)
    };

    let tables = &mut intern::InternerTables::new();
    let polonius_result: Result<(facts::AllFacts, Output), Error> = attempt! {
        let all_facts =
            tab_delim::load_tab_delimited_facts(tables, &Path::new(&facts_dir))
                .map_err(|e| Error(e.to_string()))?;
        let algorithm = Algorithm::Hybrid;
        let output = Output::compute(&all_facts, algorithm, false);
        (all_facts, output)
    };

    let (input_facts, output_facts) = polonius_result.unwrap();

    let body_with_facts = BodyWithBorrowckFacts {
        body,
        input_facts: unsafe { transmute(input_facts) },
        output_facts: Rc::new(unsafe { transmute(output_facts) }),
        location_table: unsafe { transmute(location_table) },
    };

    let aliases = Aliases::build(tcx, def_id, &body_with_facts);
    let location_domain = aliases.location_domain().clone();

    let results = {
        let analysis = FlowAnalysis::new(tcx, def_id, &body_with_facts.body, aliases);
        engine::iterate_to_fixpoint(tcx, &body_with_facts.body, location_domain, analysis)
    };

    // We construct a target of the argument at the start of the function.
    let arg_place = Place::make(arg_local, &[], tcx);
    let targets = vec![vec![(arg_place, LocationOrArg::Arg(arg_local))]];

    // Then use Flowistry to compute the locations and places influenced by the target.
    let location_deps =
        flowistry::infoflow::compute_dependencies(&results, targets.clone(), Direction::Forward)
            .remove(0);
    location_deps
}
