#![feature(rustc_private)]

mod analyzer;
mod vartrack;

use analyzer::{FnData, FnVisitor, PurityAnalysisResult};
use analyzer::{ImportantLocals, RefinedTy};
use vartrack::compute_dependent_locals;

extern crate rustc_borrowck;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_index;
extern crate rustc_infer;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;
extern crate rustc_trait_selection;

use std::{borrow::Cow, env, fs::File, io::Write};

use clap::Parser;
use flowistry::indexed::impls::LocationOrArg;
use flowistry::infoflow::Direction;
use serde::{Deserialize, Serialize};

use rustc_hir::{GenericParamKind, ItemId, ItemKind, TyKind};
use rustc_middle::mir::{visit::Visitor, Local, Mutability, Place};
use rustc_middle::ty;
use rustc_plugin::{CrateFilter, RustcPlugin, RustcPluginArgs, Utf8Path};
use rustc_utils::PlaceExt;

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
        if let ItemKind::Fn(fn_sig, generics, _) = &item.kind {
            // Check for unresolved generic types or consts.
            let contains_unresolved_generics =
                generics.params.iter().any(|generic| match generic.kind {
                    GenericParamKind::Lifetime { .. } => false,
                    GenericParamKind::Type { .. } => true,
                    GenericParamKind::Const { .. } => true,
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
            let contains_mutable_args = fn_sig.decl.inputs.iter().any(|arg| {
                if let TyKind::Ref(_, mut_ty) = &arg.kind {
                    return mut_ty.mutbl == Mutability::Mut;
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
                    .collect::<Vec<_>>()];
                ImportantLocals::new(compute_dependent_locals(
                    tcx,
                    def_id,
                    targets,
                    Direction::Forward,
                ))
            };

            // Retrieve body.
            let body = tcx.optimized_mir(def_id);

            // Create initial argument types.
            let arg_tys: Vec<RefinedTy> = (1..=body.arg_count)
                .map(|local| {
                    let arg_ty = body.local_decls[local.into()].ty;
                    RefinedTy::Simple(arg_ty)
                })
                .collect();

            // Retrieve the instance, as we know it exists.
            let instance = ty::Instance::mono(tcx, def_id);

            // Construct the visitor.
            let mut visitor = FnVisitor::new(
                def_id,
                tcx,
                FnData::new(arg_tys, instance, important_locals, tcx),
            );

            // Begin the traversal.
            visitor.visit_body(body);

            // Dump traversal results.
            Some(visitor.get_storage_clone().dump(annotated_pure))
        } else {
            None
        }
    } else {
        None
    }
}
