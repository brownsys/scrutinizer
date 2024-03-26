#![feature(box_patterns)]
#![feature(rustc_private)]

mod analyzer;
mod collector;
mod important;
mod input_adapters;

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

use analyzer::{run, PurityAnalysisResult};
use collector::{structs::*, Collector};
use important::ImportantLocals;
use input_adapters::CollectPPRs;

use clap::Parser;
use itertools::Itertools;
use log::{error, trace};
use regex::Regex;
use rustc_hir::{ConstContext, ItemId, ItemKind};
use rustc_middle::mir::Mutability;
use rustc_middle::ty;
use rustc_plugin::{CrateFilter, RustcPlugin, RustcPluginArgs, Utf8Path};
use rustc_span::def_id::DefId;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::rc::Rc;

pub struct ScrutinizerPlugin;

// To parse CLI arguments, we use Clap.
#[derive(Parser, Serialize, Deserialize)]
pub struct ScrutinizerPluginArgs {
    #[arg(short, long, default_value("config.toml"))]
    config_path: String,
}

fn default_mode() -> String {
    "functions".to_string()
}

fn default_output_file() -> String {
    "analysis.result.json".to_string()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default)]
    only_inconsistent: bool,
    #[serde(default = "default_output_file")]
    output_file: String,

    target_filter: Option<String>,
    important_args: Option<Vec<usize>>,
    allowlist: Vec<String>,
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
    if args.mode == "function" {
        tcx.hir()
            .items()
            .filter_map(|item_id| analyze_item(item_id, tcx, args))
            .filter(|result| {
                if args.only_inconsistent {
                    result.is_inconsistent()
                } else {
                    true
                }
            })
            .collect()
    } else if args.mode == "ppr" {
        tcx.mir_keys(())
            .iter()
            .map(
                |def_id| match tcx.hir().body_const_context(def_id.to_owned()) {
                    Some(ConstContext::ConstFn) | None => {
                        analyze_pprs_in_body(def_id.to_def_id(), tcx, args)
                    }
                    Some(_) => vec![],
                },
            )
            .flatten()
            .collect()
    } else {
        panic!("undefined mode")
    }
}

fn analyze_pprs_in_body<'tcx>(
    def_id: DefId,
    tcx: ty::TyCtxt<'tcx>,
    args: &Config,
) -> Vec<PurityAnalysisResult<'tcx>> {
    let pprs = tcx.optimized_mir(def_id).collect_pprs(tcx);

    pprs.into_iter()
        .map(|ppr| {
            if let ty::TyKind::Closure(def_id, substs_ref) = ppr.kind() {
                // Retrieve the instance, as we know it exists.
                let instance = ty::Instance::expect_resolve(
                    tcx,
                    ty::ParamEnv::reveal_all(),
                    def_id.to_owned(),
                    substs_ref,
                );
                trace!("current instance {:?}", &instance);

                let def_id = instance.def_id();
                let body = instance.subst_mir_and_normalize_erasing_regions(
                    tcx,
                    ty::ParamEnv::reveal_all(),
                    tcx.instance_mir(instance.def).to_owned(),
                );

                // Create initial argument types.
                let arg_tys = (1..=body.arg_count)
                    .map(|local| {
                        let arg_ty = body.local_decls[local.into()].ty;
                        TrackedTy::from_ty(arg_ty)
                    })
                    .collect_vec();

                // Check for unresolved generic types or consts.
                let contains_unresolved_generics = arg_tys.iter().any(|arg| match arg {
                    TrackedTy::Present(..) => false,
                    TrackedTy::Erased(..) => true,
                });

                if contains_unresolved_generics {
                    return PurityAnalysisResult::new(
                        def_id,
                        true,
                        false,
                        String::from("erased args detected"),
                        vec![],
                        vec![],
                        ClosureInfoStorage::new(),
                    );
                }

                // Check for mutable arguments.
                let contains_mutable_args = arg_tys.iter().any(|arg| {
                    let main_ty = match arg {
                        TrackedTy::Present(ty) => ty,
                        TrackedTy::Erased(..) => unreachable!(),
                    };
                    if let ty::TyKind::Ref(.., mutbl) = main_ty.kind() {
                        return mutbl.to_owned() == Mutability::Mut;
                    } else {
                        return false;
                    }
                });

                if contains_mutable_args {
                    return PurityAnalysisResult::new(
                        def_id,
                        true,
                        false,
                        String::from("mutable arguments detected"),
                        vec![],
                        vec![],
                        ClosureInfoStorage::new(),
                    );
                }

                let fn_storage = Rc::new(RefCell::new(FunctionInfoStorage::new(instance)));
                let upvar_storage = Rc::new(RefCell::new(ClosureInfoStorage::new()));
                let virtual_stack = VirtualStack::new();

                let current_fn = PartialFunctionInfo::new_function(instance, ArgTys::new(arg_tys));

                let results = Collector::new(
                    current_fn.clone(),
                    virtual_stack,
                    fn_storage.clone(),
                    upvar_storage.clone(),
                    tcx,
                )
                .run();

                let fn_info = {
                    let mut borrowed_storage = fn_storage.borrow_mut();
                    borrowed_storage.add_with_body(
                        instance,
                        instance,
                        results.places().to_owned(),
                        results.calls().to_owned(),
                        body.to_owned(),
                        body.span,
                        results.unhandled().to_owned(),
                    )
                };

                // Calculate important locals.
                let important_locals = {
                    // Parse important arguments.
                    let important_args = vec![2];
                    ImportantLocals::from_important_args(important_args, def_id, tcx)
                };

                let allowlist = args
                    .allowlist
                    .iter()
                    .map(|re| Regex::new(re).unwrap())
                    .collect();

                run(
                    &fn_info,
                    fn_storage.clone(),
                    upvar_storage,
                    important_locals,
                    true,
                    &allowlist,
                    tcx,
                )
            } else {
                panic!("passed a non-closure to ppr constructor");
            }
        })
        .collect()
}

fn analyze_item<'tcx>(
    item_id: ItemId,
    tcx: ty::TyCtxt<'tcx>,
    args: &Config,
) -> Option<PurityAnalysisResult<'tcx>> {
    let hir = tcx.hir();
    let item = hir.item(item_id);
    let def_id = item.owner_id.to_def_id();
    let annotated_pure = tcx
        .get_attr(def_id, rustc_span::symbol::Symbol::intern("doc"))
        .and_then(|attr| attr.doc_str())
        .and_then(|symbol| Some(symbol == rustc_span::symbol::Symbol::intern("pure")))
        .unwrap_or(false);

    // Find the desired function by name.
    if args.target_filter.is_none()
        || tcx
            .def_path_str(def_id)
            .contains(args.target_filter.as_ref().unwrap().as_str())
    {
        if let ItemKind::Fn(fn_sig, ..) = &item.kind {
            // Retrieve body.
            let body = tcx.optimized_mir(def_id);

            // Create initial argument types.
            let arg_tys = (1..=body.arg_count)
                .map(|local| {
                    let arg_ty = body.local_decls[local.into()].ty;
                    TrackedTy::from_ty(arg_ty)
                })
                .collect_vec();

            // Check for unresolved generic types or consts.
            let contains_unresolved_generics = arg_tys.iter().any(|arg| match arg {
                TrackedTy::Present(..) => false,
                TrackedTy::Erased(..) => true,
            });

            if contains_unresolved_generics {
                return Some(PurityAnalysisResult::new(
                    def_id,
                    annotated_pure,
                    false,
                    String::from("unresolved generics detected"),
                    vec![],
                    vec![],
                    ClosureInfoStorage::new(),
                ));
            }

            // Check for mutable arguments.
            let contains_mutable_args = arg_tys.iter().any(|arg| {
                let main_ty = match arg {
                    TrackedTy::Present(ty) => ty,
                    TrackedTy::Erased(..) => unreachable!(),
                };
                if let ty::TyKind::Ref(.., mutbl) = main_ty.kind() {
                    return mutbl.to_owned() == Mutability::Mut;
                } else {
                    return false;
                }
            });

            if contains_mutable_args {
                return Some(PurityAnalysisResult::new(
                    def_id,
                    annotated_pure,
                    false,
                    String::from("mutable arguments detected"),
                    vec![],
                    vec![],
                    ClosureInfoStorage::new(),
                ));
            }

            // Has generics.
            let has_generics = ty::InternalSubsts::identity_for_item(tcx, def_id)
                .iter()
                .any(|param| match param.unpack() {
                    ty::GenericArgKind::Lifetime(..) => false,
                    ty::GenericArgKind::Type(..) => {
                        error!("{:?} has type parameters", def_id);
                        true
                    }
                    ty::GenericArgKind::Const(..) => {
                        error!("{:?} has const parameters", def_id);
                        true
                    }
                });

            if has_generics {
                return None;
            }

            // Retrieve the instance, as we know it exists.
            let instance = ty::Instance::mono(tcx, def_id);
            trace!("current instance {:?}", &instance);

            let fn_storage = Rc::new(RefCell::new(FunctionInfoStorage::new(instance)));
            let upvar_storage = Rc::new(RefCell::new(ClosureInfoStorage::new()));
            let virtual_stack = VirtualStack::new();

            let current_fn = PartialFunctionInfo::new_function(instance, ArgTys::new(arg_tys));

            let results = Collector::new(
                current_fn.clone(),
                virtual_stack,
                fn_storage.clone(),
                upvar_storage.clone(),
                tcx,
            )
            .run();

            let fn_info = {
                let mut borrowed_storage = fn_storage.borrow_mut();
                borrowed_storage.add_with_body(
                    instance,
                    instance,
                    results.places().to_owned(),
                    results.calls().to_owned(),
                    body.to_owned(),
                    body.span,
                    results.unhandled().to_owned(),
                )
            };

            // Calculate important locals.
            let important_locals = {
                // Parse important arguments.
                let important_args = if args.important_args.is_none() {
                    // If no important arguments are provided, assume all are important.
                    let n_args = fn_sig.decl.inputs.len();
                    (1..=n_args).collect()
                } else {
                    args.important_args.as_ref().unwrap().to_owned()
                };
                ImportantLocals::from_important_args(important_args, def_id, tcx)
            };

            let allowlist = args
                .allowlist
                .iter()
                .map(|re| Regex::new(re).unwrap())
                .collect();

            let dump = run(
                &fn_info,
                fn_storage.clone(),
                upvar_storage,
                important_locals,
                annotated_pure,
                &allowlist,
                tcx,
            );
            Some(dump)
        } else {
            None
        }
    } else {
        None
    }
}
