#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_arena;
extern crate rustc_ast;
extern crate rustc_ast_pretty;
extern crate rustc_attr;
extern crate rustc_data_structures;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_hir_pretty;
extern crate rustc_index;
extern crate rustc_infer;
extern crate rustc_lexer;
extern crate rustc_middle;
extern crate rustc_mir_dataflow;
extern crate rustc_parse;
extern crate rustc_span;
extern crate rustc_target;
extern crate rustc_trait_selection;

use regex::Regex;
use rustc_hir::{Item, ItemKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty;
use scrutils::{precheck, run_analysis, Collector, ImportantLocals, PurityAnalysisResult};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;

dylint_linting::declare_late_lint! {
    /// ### What it does
    ///
    /// ### Why is this bad?
    ///
    /// ### Known problems
    /// Remove if none.
    ///
    /// ### Example
    /// ```rust
    /// // example code where a warning is issued
    /// ```
    /// Use instead:
    /// ```rust
    /// // example code that does not raise a warning
    /// ```
    pub SCRUTINIZER_LINT,
    Warn,
    "description goes here"
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

impl<'tcx> LateLintPass<'tcx> for ScrutinizerLint {
    // A list of things you might check can be found here:
    // https://doc.rust-lang.org/stable/nightly-rustc/rustc_lint/trait.LateLintPass.html
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        let args = toml::from_str(
            fs::read_to_string("scrutinizer-config.toml")
                .unwrap()
                .as_str(),
        )
        .unwrap();
        let tcx = cx.tcx;
        let selected_item = {
            let def_id = item.owner_id.to_def_id();
            let annotated_pure = tcx
                .get_attr(def_id, rustc_span::symbol::Symbol::intern("doc"))
                .and_then(|attr| attr.doc_str())
                .and_then(|symbol| Some(symbol == rustc_span::symbol::Symbol::intern("pure")))
                .unwrap_or(false);

            if let ItemKind::Fn(..) = &item.kind {
                // Sanity check for generics.
                let has_generics = ty::InternalSubsts::identity_for_item(tcx, def_id)
                    .iter()
                    .any(|param| match param.unpack() {
                        ty::GenericArgKind::Lifetime(..) => false,
                        ty::GenericArgKind::Type(..) | ty::GenericArgKind::Const(..) => true,
                    });

                if has_generics {
                    None
                } else {
                    // Retrieve the instance, as we know it exists.
                    Some((ty::Instance::mono(tcx, def_id), annotated_pure))
                }
            } else {
                None
            }
        };
        if let Some((instance, annotated_pure)) = selected_item {
            let result = analyze_instance(
                instance,
                annotated_pure,
                tcx,
                &args,
            );
            if result.is_inconsistent() {
                let output_string = serde_json::to_string_pretty(&result).unwrap();
                println!("{}", output_string);
            }
    
        }
    }
}

fn analyze_instance<'tcx>(
    instance: ty::Instance<'tcx>,
    annotated_pure: bool,
    tcx: ty::TyCtxt<'tcx>,
    args: &Config,
) -> PurityAnalysisResult<'tcx> {
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

#[test]
fn ui() {
    dylint_testing::ui_test(
        env!("CARGO_PKG_NAME"),
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui"),
    );
}
