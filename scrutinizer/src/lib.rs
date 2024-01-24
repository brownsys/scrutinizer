#![feature(box_patterns)]
#![feature(rustc_private)]

mod analyzer;
mod vartrack;

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

use analyzer::{
    FnCallStorage, FnData, ImportantLocals, PurityAnalysisResult, TrackedTy, TypeTracker,
};
use clap::Parser;
use flowistry::indexed::impls::LocationOrArg;
use flowistry::infoflow::Direction;
use itertools::Itertools;
use log::trace;
use rustc_hir::{ItemId, ItemKind};
use rustc_middle::mir::{Local, Mutability, Place};
use rustc_middle::ty;
use rustc_plugin::{CrateFilter, RustcPlugin, RustcPluginArgs, Utf8Path};
use rustc_utils::PlaceExt;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;
use std::env;
use std::fs::File;
use std::io::Write;
use std::rc::Rc;
use vartrack::compute_dependent_locals;

pub struct ScrutinizerPlugin;

// To parse CLI arguments, we use Clap.
#[derive(Parser, Serialize, Deserialize)]
pub struct ScrutinizerPluginArgs {
    #[arg(short, long, default_value(""))]
    function: String,
    #[arg(short, long, num_args(0..), value_delimiter(','))]
    important_args: Vec<usize>,
    #[arg(short, long, default_value("analysis_results.json"))]
    out_file: String,
}

impl RustcPlugin for ScrutinizerPlugin {
    type Args = ScrutinizerPluginArgs;

    fn version(&self) -> Cow<'static, str> {
        env!("CARGO_PKG_VERSION").into()
    }

    fn driver_name(&self) -> Cow<'static, str> {
        "scrutinizer-driver".into()
    }

    fn args(&self, _target_dir: &Utf8Path) -> RustcPluginArgs<Self::Args> {
        let args = ScrutinizerPluginArgs::parse_from(env::args().skip(1));
        let filter = CrateFilter::AllCrates;
        RustcPluginArgs { args, filter }
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
    args: ScrutinizerPluginArgs,
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
            File::create(self.args.out_file.clone())
                .and_then(|mut file| file.write_all(result_string.as_bytes()))
                .unwrap();
        });

        rustc_driver::Compilation::Continue
    }
}

// The entry point of analysis.
fn scrutinizer<'tcx>(
    tcx: ty::TyCtxt<'tcx>,
    args: &ScrutinizerPluginArgs,
) -> Vec<PurityAnalysisResult<'tcx>> {
    tcx.hir()
        .items()
        .filter_map(|item_id| analyze_item(item_id, tcx.to_owned(), args))
        .filter(|result| result.is_inconsistent())
        .collect()
}

fn analyze_item<'tcx>(
    item_id: ItemId,
    tcx: ty::TyCtxt<'tcx>,
    args: &ScrutinizerPluginArgs,
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
    if args.function.as_str().is_empty()
        || item.ident.name == rustc_span::symbol::Symbol::intern(args.function.as_str())
    {
        if let ItemKind::Fn(fn_sig, ..) = &item.kind {
            // Retrieve body.
            let body = tcx.optimized_mir(def_id);

            // Create initial argument types.
            let arg_tys = (1..=body.arg_count)
                .map(|local| {
                    let arg_ty = body.local_decls[local.into()].ty;
                    TrackedTy::determine(arg_ty)
                })
                .collect_vec();

            // Check for unresolved generic types or consts.
            let contains_unresolved_generics = arg_tys.iter().any(|arg| match arg {
                TrackedTy::Simple(..) => false,
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
                    vec![],
                ));
            }

            // Check for mutable arguments.
            let contains_mutable_args = arg_tys.iter().any(|arg| {
                let main_ty = match arg {
                    TrackedTy::Simple(ty) => ty,
                    TrackedTy::Erased(ty, ..) => ty,
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
                    vec![],
                ));
            }

            // Retrieve the instance, as we know it exists.
            let instance = ty::Instance::mono(tcx, def_id);
            trace!("current instance {:?}", &instance);

            let storage = Rc::new(RefCell::new(FnCallStorage::new(def_id)));

            // Calculate important locals.
            let important_locals = {
                // Parse important arguments.
                let important_args = if args.important_args.is_empty() {
                    // If no important arguments are provided, assume all are important.
                    let n_args = fn_sig.decl.inputs.len();
                    (1..=n_args).collect()
                } else {
                    args.important_args.clone()
                };
                let targets = vec![important_args
                    .iter()
                    .map(|arg| {
                        let arg_local = Local::from_usize(*arg);
                        let arg_place = Place::make(arg_local, &[], tcx);
                        return (arg_place, LocationOrArg::Arg(arg_local));
                    })
                    .collect_vec()];
                ImportantLocals::new(compute_dependent_locals(
                    tcx,
                    def_id,
                    targets,
                    Direction::Forward,
                ))
            };

            TypeTracker::new(
                FnData::new(instance, arg_tys, important_locals),
                storage.clone(),
                tcx,
            )
            .run();

            let storage_owned = storage.borrow().to_owned();
            Some(storage_owned.dump(annotated_pure))
        } else {
            None
        }
    } else {
        None
    }
}
