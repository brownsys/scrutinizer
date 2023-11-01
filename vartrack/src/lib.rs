#![feature(rustc_private)]

extern crate rustc_borrowck;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, env};

use flowistry::{indexed::impls::LocationOrArg, infoflow::Direction};
use rustc_borrowck::BodyWithBorrowckFacts;
use rustc_hir::{BodyId, ItemKind};
use rustc_middle::{
    mir::{Local, Place},
    ty::TyCtxt,
};
use rustc_span::Span;
use rustc_utils::{
    mir::borrowck_facts,
    source_map::spanner::{EnclosingHirSpans, Spanner},
    BodyExt, PlaceExt, SpanExt,
};

use rustc_plugin::{CrateFilter, RustcPlugin, RustcPluginArgs, Utf8Path};

pub struct VartrackPlugin;

// To parse CLI arguments, we use Clap.
#[derive(Parser, Serialize, Deserialize)]
pub struct VartrackPluginArgs {
    #[arg(short, long)]
    function: String,
}

impl RustcPlugin for VartrackPlugin {
    type Args = VartrackPluginArgs;

    fn version(&self) -> Cow<'static, str> {
        env!("CARGO_PKG_VERSION").into()
    }

    fn driver_name(&self) -> Cow<'static, str> {
        "vartrack-driver".into()
    }

    fn args(&self, _target_dir: &Utf8Path) -> RustcPluginArgs<Self::Args> {
        let args = VartrackPluginArgs::parse_from(env::args().skip(1));
        let filter = CrateFilter::AllCrates;
        RustcPluginArgs { args, filter }
    }

    fn run(
        self,
        compiler_args: Vec<String>,
        plugin_args: Self::Args,
    ) -> rustc_interface::interface::Result<()> {
        let mut callbacks = VartrackCallbacks { args: plugin_args };
        let compiler = rustc_driver::RunCompiler::new(&compiler_args, &mut callbacks);
        compiler.run()
    }
}

// This is the core analysis. Everything below this function is plumbing to
// call into rustc's API.
fn compute_dependencies<'tcx>(
    tcx: TyCtxt<'tcx>,
    body_id: BodyId,
    body_with_facts: &BodyWithBorrowckFacts<'tcx>,
) {
    println!("Body:\n{}", body_with_facts.body.to_string(tcx).unwrap());

    // This computes the core information flow data structure. But it's not very
    // visualizable, so we need to post-process it with a specific query.
    let results = flowistry::infoflow::compute_flow(tcx, body_id, body_with_facts);

    // We construct a target of the first argument at the start of the function.
    let arg_local = Local::from_usize(1);
    let arg_place = Place::make(arg_local, &[], tcx);
    let targets = vec![vec![(arg_place, LocationOrArg::Arg(arg_local))]];

    // Then use Flowistry to compute the locations and places influenced by the target.
    let location_deps = flowistry::infoflow::compute_dependencies(
        &results,
        targets.clone(),
        Direction::Forward,
    )
        .remove(0);

    // And print out those forward dependencies. Note that while each location has an
    // associated span in the body, i.e. via `body.source_info(location).span`,
    // these spans are pretty limited so we have our own infrastructure for mapping MIR
    // back to source. That's the Spanner class and the location_to_span method.
    println!("The forward dependencies of targets {targets:?} are:");
    let body = &body_with_facts.body;
    let spanner = Spanner::new(tcx, body_id, body);
    let source_map = tcx.sess.source_map();
    for location in location_deps.iter() {
        let spans = Span::merge_overlaps(spanner.location_to_spans(
            *location,
            body,
            EnclosingHirSpans::OuterOnly,
        ));
        println!("Location {location:?}:");
        for span in spans {
            println!(
                "{}",
                textwrap::indent(&source_map.span_to_snippet(span).unwrap(), "    ")
            );
        }
    }
}

struct VartrackCallbacks {
    args: VartrackPluginArgs,
}

impl rustc_driver::Callbacks for VartrackCallbacks {
    fn config(&mut self, config: &mut rustc_interface::Config) {
        // You MUST configure rustc to ensure `get_body_with_borrowck_facts` will work.
        borrowck_facts::enable_mir_simplification();
        config.override_queries = Some(borrowck_facts::override_queries);
    }

    fn after_parsing<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        queries: &'tcx rustc_interface::Queries<'tcx>,
    ) -> rustc_driver::Compilation {
        queries.global_ctxt().unwrap().enter(|tcx| {
            let hir = tcx.hir();

            // Get the matching body
            let body_id = hir
                .items()
                .filter_map(|id| {
                    let item = hir.item(id);
                    match item.kind {
                        ItemKind::Fn(_, _, body) => {
                            if item.ident.name == rustc_span::symbol::Symbol::intern(self.args.function.as_str()) {
                                Some(body)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                })
                .next()
                .unwrap();

            let def_id = hir.body_owner_def_id(body_id);
            let body_with_facts = borrowck_facts::get_body_with_borrowck_facts(tcx, def_id);

            compute_dependencies(tcx, body_id, body_with_facts)
        });
        rustc_driver::Compilation::Stop
    }
}
