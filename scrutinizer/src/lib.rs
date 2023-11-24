#![feature(rustc_private)]

mod vartrack;
mod visitors;

use vartrack::compute_dependent_locals;
use visitors::FnVisitor;

extern crate rustc_borrowck;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_index;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use std::{borrow::Cow, env};

use clap::Parser;
use flowistry::infoflow::Direction;
use flowistry::indexed::impls::LocationOrArg;
use serde::{Deserialize, Serialize};

use rustc_hir as hir;
use rustc_middle::mir;
use rustc_middle::mir::visit::Visitor;
use rustc_middle::mir::Local;
use rustc_middle::mir::Place;
use rustc_middle::ty;
use rustc_plugin::{CrateFilter, RustcPlugin, RustcPluginArgs, Utf8Path};
use rustc_utils::PlaceExt;

pub struct ScrutinizerPlugin;

// To parse CLI arguments, we use Clap.
#[derive(Parser, Serialize, Deserialize)]
pub struct ScrutinizerPluginArgs {
    #[arg(short, long)]
    function: String,
    #[arg(short, long, num_args(1..), value_delimiter(','))]
    pub important_args: Vec<usize>,
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
        queries
            .global_ctxt()
            .unwrap()
            .enter(|tcx| scrutinizer(tcx, &self.args));

        rustc_driver::Compilation::Continue
    }
}

// The entry point of analysis.
fn scrutinizer(tcx: ty::TyCtxt, args: &ScrutinizerPluginArgs) {
    let hir = tcx.hir();
    for item_id in hir.items() {
        let item = hir.item(item_id);
        let def_id = item.owner_id.to_def_id();
        // Find the desired function by name.
        if item.ident.name == rustc_span::symbol::Symbol::intern(args.function.as_str()) {
            println!("[STARTING ANALYSIS]");

            if let hir::ItemKind::Fn(fn_sig, _, _) = &item.kind {
                let main_body = tcx.optimized_mir(def_id);

                let targets = vec![args
                    .important_args
                    .iter()
                    .map(|arg| {
                        let arg_local = Local::from_usize(*arg);
                        let arg_place = Place::make(arg_local, &[], tcx);
                        return (arg_place, LocationOrArg::Arg(arg_local));
                    })
                    .collect::<Vec<_>>()];

                let deps =
                    compute_dependent_locals(tcx, def_id, targets, Direction::Forward);

                let main_instance = ty::Instance::mono(tcx, def_id);

                println!(
                    "--> Checking for mutable reference params in {}...",
                    args.function
                );
                let mutable = fn_sig.decl.inputs.iter().any(|arg| {
                    if let hir::TyKind::Ref(_, mut_ty) = &arg.kind {
                        if mut_ty.mutbl == mir::Mutability::Mut {
                            return true;
                        }
                    }
                    return false;
                });
                if mutable {
                    println!("--> Cannot ensure the purity of the function, as some of the arguments are mutable refs!");
                    return;
                }

                println!("--> Performing call tree traversal...");

                let mut visitor = FnVisitor::new(tcx, def_id, main_body, main_instance, deps);
                // Begin the traversal.
                visitor.visit_body(main_body);
                // Show all checked bodies encountered.
                println!("--> Dumping all passing function bodies:");
                visitor.dump_passing();
                // Show all unchecked bodies encountered.
                println!("--> Dumping all violating function bodies:");
                visitor.dump_violating();
                // Show all unhandled terminators encountered.
                println!("--> Dumping all unhandled terminators:");
                visitor.dump_unhandled_terminators();
                println!(
                    "--> Body purity check result for function {}: {}",
                    args.function,
                    visitor.check_purity()
                );
            }
        }
    }
}
