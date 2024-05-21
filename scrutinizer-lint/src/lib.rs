#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_hir;
extern crate rustc_middle;

use clippy_utils::diagnostics::span_lint_and_help;
use regex::Regex;
use rustc_hir::{Item, ItemKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty;
use scrutils::{
    precheck, run_analysis, select_pprs, Collector, ImportantLocals, PurityAnalysisResult,
};
use serde::{Deserialize, Serialize};

dylint_linting::impl_late_lint! {
    pub SCRUTINIZER_LINT,
    Deny,
    "checks purity of allegedly pure regions",
    ScrutinizerLint::new()
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
enum ScrutinizerMode {
    #[default]
    Lint,
    Test,
}

#[derive(Default, Deserialize, Debug)]
pub struct Config {
    mode: ScrutinizerMode,
    important_args: Option<Vec<usize>>,
    allowlist: Option<Vec<String>>,
    trusted_stdlib: Option<Vec<String>>,
}

struct ScrutinizerLint {
    config: Config,
}

impl ScrutinizerLint {
    pub fn new() -> Self {
        let linter = Self {
            config: dylint_linting::config_or_default(env!("CARGO_PKG_NAME")),
        };
        if linter.config.mode == ScrutinizerMode::Test {
            eprintln!("--LINTSSTART--");
        };
        linter
    }
}

impl Drop for ScrutinizerLint {
    fn drop(&mut self) {
        if self.config.mode == ScrutinizerMode::Test {
            eprintln!("--LINTSEND--");
        }
    }
}

impl<'tcx> LateLintPass<'tcx> for ScrutinizerLint {
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        let tcx = cx.tcx;
        let instances = {
            let def_id = item.owner_id.to_def_id();
            if let ItemKind::Fn(..) = &item.kind {
                match self.config.mode {
                    ScrutinizerMode::Test => {
                        // Sanity check for generics.
                        let has_generics = ty::InternalSubsts::identity_for_item(tcx, def_id)
                            .iter()
                            .any(|param| match param.unpack() {
                                ty::GenericArgKind::Lifetime(..) => false,
                                ty::GenericArgKind::Type(..) | ty::GenericArgKind::Const(..) => {
                                    true
                                }
                            });

                        if has_generics {
                            span_lint_and_help(
                                cx,
                                SCRUTINIZER_LINT,
                                item.span,
                                "static analysis was not able to verify the purity of the region",
                                None,
                                "consider using sandbox or privacy region",
                            );
                            vec![]
                        } else {
                            // Retrieve the instance, as we know it exists.
                            vec![ty::Instance::mono(tcx, def_id)]
                        }
                    }
                    ScrutinizerMode::Lint => select_pprs(tcx, def_id),
                }
            } else {
                vec![]
            }
        };
        for instance in instances.into_iter() {
            let result = analyze_instance(instance, tcx, &self.config);
            if !result.is_pure() {
                span_lint_and_help(
                    cx,
                    SCRUTINIZER_LINT,
                    item.span,
                    "static analysis was not able to verify the purity of the region",
                    None,
                    "consider using sandbox or privacy region",
                );
            }
        }
    }
}

fn analyze_instance<'tcx>(
    instance: ty::Instance<'tcx>,
    tcx: ty::TyCtxt<'tcx>,
    config: &Config,
) -> PurityAnalysisResult<'tcx> {
    let def_id = instance.def_id();

    match precheck(instance, tcx) {
        Err(reason) => {
            return PurityAnalysisResult::error(def_id, reason);
        }
        _ => {}
    };

    let collector = Collector::collect(instance, tcx, false);

    // Calculate important locals.
    let important_locals = {
        // Parse important arguments.
        let important_args = if config.important_args.is_none() {
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
            config.important_args.as_ref().unwrap().to_owned()
        };
        ImportantLocals::from_important_args(important_args, def_id, tcx)
    };

    let allowlist = config
        .allowlist
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .map(|re| Regex::new(re).unwrap())
        .collect();

    let trusted_stdlib = config
        .trusted_stdlib
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .map(|re| Regex::new(re).unwrap())
        .collect();

    let function_info_storage = collector.get_function_info_storage();
    let closure_info_storage = collector.get_closure_info_storage();

    run_analysis(
        function_info_storage,
        closure_info_storage,
        important_locals,
        &allowlist,
        &trusted_stdlib,
        tcx,
    )
}
