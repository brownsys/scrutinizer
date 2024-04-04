#![feature(box_patterns)]
#![feature(rustc_private)]

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
extern crate rustc_span;
extern crate rustc_trait_selection;

use scrutils::{
    precheck, run_analysis, select_functions, select_pprs, Collector, ImportantLocals,
    PurityAnalysisResult,
};

use clap::Parser;
use log::trace;
use regex::Regex;
use rustc_middle::ty;
use rustc_plugin::{CrateFilter, RustcPlugin, RustcPluginArgs, Utf8Path};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Write;

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
        compiler_args: Vec<String>,
        plugin_args: Self::Args,
    ) -> rustc_interface::interface::Result<()> {
        let mut callbacks = ScrutinizerCallbacks { args: plugin_args };
        let compiler = rustc_driver::RunCompiler::new(&compiler_args, &mut callbacks);
        compiler.run()
    }
}

struct ScrutinizerCallbacks {
    args: Config,
}

impl rustc_driver::Callbacks for ScrutinizerCallbacks {
    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        queries: &'tcx rustc_interface::Queries<'tcx>,
    ) -> rustc_driver::Compilation {
        queries.global_ctxt().unwrap().enter(|tcx| {
            let result = scrutinizer(tcx, &self.args);
            let result_string = serde_json::to_string_pretty(&result).unwrap();
            File::create(self.args.output_file.to_owned())
                .and_then(|mut file| file.write_all(result_string.as_bytes()))
                .unwrap();
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
                let body = instance.subst_mir_and_normalize_erasing_regions(
                    tcx,
                    ty::ParamEnv::reveal_all(),
                    tcx.instance_mir(instance.def).to_owned(),
                );
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
