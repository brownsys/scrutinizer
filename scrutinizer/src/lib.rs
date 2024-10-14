#![feature(rustc_private, box_patterns, min_specialization)]

extern crate rustc_abi;
extern crate rustc_borrowck;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_index;
extern crate rustc_infer;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_mir_dataflow;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_trait_selection;

use scrutils::{
    dump_mir_and_borrowck_facts, substituted_mir, precheck, run_analysis, select_functions,
    select_pprs, Collector, ImportantLocals, PurityAnalysisResult,
};

use chrono::offset::Local;
use clap::Parser;
use log::trace;
use regex::Regex;
use rustc_middle::ty;
use rustc_plugin::{CrateFilter, RustcPlugin, RustcPluginArgs, Utf8Path};
use rustc_session::EarlyErrorHandler;
use rustc_span::def_id::LOCAL_CRATE;
use rustc_utils::mir::borrowck_facts;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::process::{exit, Command};
use std::time::Instant;

pub struct ScrutinizerPlugin;

// To parse CLI arguments, we use Clap.
#[derive(Parser, Serialize, Deserialize)]
pub struct ScrutinizerPluginArgs {
    #[arg(short, long, default_value("scrutinizer-config.toml"))]
    config_path: String,
}

fn default_mode() -> String {
    "functions".to_string()
}

fn default_only_inconsistent() -> bool {
    false
}

fn default_output_file() -> String {
    "analysis.result.json".to_string()
}

fn default_shallow() -> bool {
    false
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default = "default_only_inconsistent")]
    only_inconsistent: bool,
    #[serde(default = "default_output_file")]
    output_file: String,
    #[serde(default = "default_shallow")]
    shallow: bool,

    target_filter: Option<String>,
    important_args: Option<Vec<usize>>,
    allowlist: Option<Vec<String>>,
    trusted_stdlib: Option<Vec<String>>,
}

enum CrateHandling {
    JustCompile,
    CompileAndDump,
    Analyze,
}

fn how_to_handle_this_crate(
    _plugin_args: &Config,
    compiler_args: &mut Vec<String>,
) -> CrateHandling {
    let crate_name = compiler_args
        .iter()
        .enumerate()
        .find_map(|(i, s)| (s == "--crate-name").then_some(i))
        .and_then(|i| compiler_args.get(i + 1))
        .cloned();

    match &crate_name {
        Some(krate) if krate == "build_script_build" => CrateHandling::JustCompile,
        _ if std::env::var("CARGO_PRIMARY_PACKAGE").is_ok() => CrateHandling::Analyze,
        Some(_) => CrateHandling::CompileAndDump,
        _ => CrateHandling::JustCompile,
    }
}

impl RustcPlugin for ScrutinizerPlugin {
    type Args = Config;

    fn version(&self) -> Cow<'static, str> {
        env!("CARGO_PKG_VERSION").into()
    }

    fn driver_name(&self) -> Cow<'static, str> {
        "scrutinizer-driver".into()
    }

    fn args(&self, _target_dir: &Utf8Path) -> RustcPluginArgs<Self::Args> {
        let args = ScrutinizerPluginArgs::parse_from(env::args().skip(1));
        let config =
            toml::from_str(fs::read_to_string(args.config_path).unwrap().as_str()).unwrap();
        let filter = CrateFilter::AllCrates;
        RustcPluginArgs {
            args: config,
            filter,
        }
    }

    fn run(
        self,
        mut compiler_args: Vec<String>,
        plugin_args: Self::Args,
    ) -> rustc_interface::interface::Result<()> {
        let mut callbacks = match how_to_handle_this_crate(&plugin_args, &mut compiler_args) {
            CrateHandling::JustCompile => {
                Box::new(NoopCallbacks) as Box<dyn rustc_driver::Callbacks + Send>
            }
            CrateHandling::CompileAndDump => Box::new(DumpOnlyCallbacks),
            CrateHandling::Analyze => Box::new(ScrutinizerCallbacks { args: plugin_args }),
        };
        rustc_driver::RunCompiler::new(&compiler_args, callbacks.as_mut()).run()
    }

    fn modify_cargo(&self, cargo: &mut Command, _args: &Self::Args) {
        // Find the default target triplet.
        let output = Command::new("rustc")
            .arg("-vV")
            .output()
            .expect("Cannot get default rustc target");
        let stdout = String::from_utf8(output.stdout).expect("Cannot parse stdout");

        let mut target = String::from("");
        for part in stdout.split("\n") {
            if part.starts_with("host: ") {
                target = part.chars().skip("host: ".len()).collect();
            }
        }
        if target.len() == 0 {
            panic!("Bad output");
        }

        // Add -Zalways-encode-mir to RUSTFLAGS
        let mut old_rustflags = String::from("");
        for (key, val) in cargo.get_envs() {
            if key == "RUSTFLAGS" {
                if let Some(val) = val {
                    old_rustflags = format!("{}", val.to_str().unwrap());
                }
            }
        }
        cargo.env(
            "RUSTFLAGS",
            format!("-Zalways-encode-mir {}", old_rustflags),
        );
        cargo.arg("-Zbuild-std=std,core,alloc,proc_macro");
        cargo.arg(format!("--target={}", target));
    }
}

struct NoopCallbacks;

impl rustc_driver::Callbacks for NoopCallbacks {}

struct DumpOnlyCallbacks;

impl rustc_driver::Callbacks for DumpOnlyCallbacks {
    fn config(&mut self, config: &mut rustc_interface::Config) {
        // You MUST configure rustc to ensure `get_body_with_borrowck_facts` will work.
        borrowck_facts::enable_mir_simplification();
        config.override_queries = Some(borrowck_facts::override_queries);
    }

    fn after_expansion<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        queries: &'tcx rustc_interface::Queries<'tcx>,
    ) -> rustc_driver::Compilation {
        queries.global_ctxt().unwrap().enter(|tcx| {
            dump_mir_and_borrowck_facts(tcx);
        });
        rustc_driver::Compilation::Continue
    }
}

struct ScrutinizerCallbacks {
    args: Config,
}

#[derive(Serialize)]
struct Output<'tcx> {
    results: Vec<PurityAnalysisResult<'tcx>>,
    elapsed: f32,
    crate_name: String,
}

impl rustc_driver::Callbacks for ScrutinizerCallbacks {
    fn config(&mut self, config: &mut rustc_interface::Config) {
        // You MUST configure rustc to ensure `get_body_with_borrowck_facts` will work.
        borrowck_facts::enable_mir_simplification();
        config.override_queries = Some(borrowck_facts::override_queries);
    }

    fn after_expansion<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        queries: &'tcx rustc_interface::Queries<'tcx>,
    ) -> rustc_driver::Compilation {
        queries.global_ctxt().unwrap().enter(|tcx| {
            dump_mir_and_borrowck_facts(tcx);
        });
        rustc_driver::Compilation::Continue
    }

    fn after_analysis<'tcx>(
        &mut self,
        _handler: &EarlyErrorHandler,
        _compiler: &rustc_interface::interface::Compiler,
        queries: &'tcx rustc_interface::Queries<'tcx>,
    ) -> rustc_driver::Compilation {
        queries.global_ctxt().unwrap().enter(|tcx| {
            let now = Instant::now();
            let results = scrutinizer(tcx, &self.args);
            let elapsed = now.elapsed();

            let output = Output {
                results,
                elapsed: elapsed.as_secs_f32(),
                crate_name: format!("{}", tcx.crate_name(LOCAL_CRATE))

            };

            let output_string = serde_json::to_string_pretty(&output).unwrap();
            let file_name = format!("{}.{}", Local::now(), self.args.output_file);
            File::create(file_name)
                .and_then(|mut file| file.write_all(output_string.as_bytes()))
                .unwrap();
            let inconsistent: Vec<_> = output
                .results
                .iter()
                .filter(|res| res.is_inconsistent())
                .map(|res| res.def_id())
                .collect();
            if !inconsistent.is_empty() {
                println!("Scrutinizer failed to verify the purity of the following regions: {:?}. See more information in {:?}.", inconsistent, self.args.output_file);
                exit(-1);
            }
        });

        rustc_driver::Compilation::Continue
    }
}

// The entry point of analysis.
fn scrutinizer<'tcx>(tcx: ty::TyCtxt<'tcx>, args: &Config) -> Vec<PurityAnalysisResult<'tcx>> {
    let instances = if args.mode == "function" {
        select_functions(tcx)
    } else if args.mode == "ppr" {
        select_pprs(tcx)
    } else {
        panic!("undefined mode")
    };

    instances
        .into_iter()
        .filter(|(instance, _)| {
            args.target_filter.is_none()
                || tcx
                    .def_path_str(instance.def_id())
                    .contains(args.target_filter.as_ref().unwrap().as_str())
        })
        .map(|(instance, annotated_pure)| analyze_instance(instance, annotated_pure, tcx, args))
        .filter(|result| {
            if args.only_inconsistent {
                result.is_inconsistent()
            } else {
                true
            }
        })
        .collect()
}

fn analyze_instance<'tcx>(
    instance: ty::Instance<'tcx>,
    annotated_pure: bool,
    tcx: ty::TyCtxt<'tcx>,
    args: &Config,
) -> PurityAnalysisResult<'tcx> {
    trace!("started analyzing instance {:?}", &instance);

    let def_id = instance.def_id();

    match precheck(instance, tcx) {
        Err(reason) => {
            return PurityAnalysisResult::error(def_id, reason, annotated_pure);
        }
        _ => {}
    };

    let collector = Collector::collect(instance, tcx, args.shallow);

    // Calculate important locals.
    let important_locals = {
        // Parse important arguments.
        let important_args = if args.important_args.is_none() {
            // If no important arguments are provided, assume all are important.
            let arg_count = {
                let body = substituted_mir(&instance, tcx);
                body.arg_count
            };
            (1..=arg_count).collect()
        } else {
            args.important_args.as_ref().unwrap().to_owned()
        };
        ImportantLocals::from_important_args(important_args, def_id, tcx)
    };

    let allowlist = args
        .allowlist
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .map(|re| Regex::new(re).unwrap())
        .collect();

    let trusted_stdlib = args
        .trusted_stdlib
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .map(|re| Regex::new(re).unwrap())
        .collect();

    run_analysis(
        collector.get_function_info_storage(),
        collector.get_closure_info_storage(),
        important_locals,
        annotated_pure,
        &allowlist,
        &trusted_stdlib,
        tcx,
    )
}
