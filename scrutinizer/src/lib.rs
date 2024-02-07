#![feature(box_patterns)]
#![feature(rustc_private)]

// mod analyzer;
mod collector;
mod util;
// mod vartrack;

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

use clap::Parser;
use collector::{ArgTys, Callee, FnInfo, FnInfoStorage, TrackedTy, TypeCollector, VirtualStack};
use itertools::Itertools;
use log::{error, trace};
use rustc_hir::{ItemId, ItemKind};
use rustc_middle::ty;
use rustc_plugin::{CrateFilter, RustcPlugin, RustcPluginArgs, Utf8Path};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Write;
use std::rc::Rc;

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
) -> Vec<Vec<FnInfo<'tcx>>> {
    tcx.hir()
        .items()
        .filter_map(|item_id| analyze_item(item_id, tcx.to_owned(), args))
        .collect()
}

fn analyze_item<'tcx>(
    item_id: ItemId,
    tcx: ty::TyCtxt<'tcx>,
    args: &ScrutinizerPluginArgs,
) -> Option<Vec<FnInfo<'tcx>>> {
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
        if let ItemKind::Fn(..) = &item.kind {
            // Retrieve body.
            let body = tcx.optimized_mir(def_id);

            // Create initial argument types.
            let arg_tys = (1..=body.arg_count)
                .map(|local| {
                    let arg_ty = body.local_decls[local.into()].ty;
                    TrackedTy::from_ty(arg_ty)
                })
                .collect_vec();

            // // Check for unresolved generic types or consts.
            // let contains_unresolved_generics = arg_tys.iter().any(|arg| match arg {
            //     TrackedTy::Present(..) => false,
            //     TrackedTy::Erased(..) => true,
            // });

            // if contains_unresolved_generics {
            //     return Some(PurityAnalysisResult::new(
            //         def_id,
            //         annotated_pure,
            //         false,
            //         String::from("unresolved generics detected"),
            //         vec![],
            //         vec![],
            //         vec![],
            //     ));
            // }

            // // Check for mutable arguments.
            // let contains_mutable_args = arg_tys.iter().any(|arg| {
            //     let main_ty = match arg {
            //         TrackedTy::Present(ty) => ty,
            //         TrackedTy::Erased(ty, ..) => ty,
            //     };
            //     if let ty::TyKind::Ref(.., mutbl) = main_ty.kind() {
            //         return mutbl.to_owned() == Mutability::Mut;
            //     } else {
            //         return false;
            //     }
            // });

            // if contains_mutable_args {
            //     return Some(PurityAnalysisResult::new(
            //         def_id,
            //         annotated_pure,
            //         false,
            //         String::from("mutable arguments detected"),
            //         vec![],
            //         vec![],
            //         vec![],
            //     ));
            // }

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

            let fn_storage = Rc::new(RefCell::new(FnInfoStorage::new(instance)));
            let upvar_storage = Rc::new(RefCell::new(HashMap::new()));
            let virtual_stack = VirtualStack::new();

            let current_fn = Callee::new_function(instance, ArgTys::new(arg_tys));

            let results = TypeCollector::new(
                current_fn.clone(),
                virtual_stack,
                fn_storage.clone(),
                upvar_storage,
                tcx,
            )
            .run();

            let fn_call_info = FnInfo::Regular {
                parent: instance,
                instance,
                places: results.places,
                span: body.span,
            };

            fn_storage.borrow_mut().add_fn(fn_call_info);
            let dump = fn_storage.borrow().dump();
            Some(dump)
        } else {
            None
        }
    } else {
        None
    }
}
