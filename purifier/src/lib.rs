#![feature(rustc_private)]

mod fn_visitor;
mod raw_ptr_deref_visitor;
mod fn_cast_visitor;

use crate::fn_visitor::FnVisitor;

extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use std::{borrow::Cow, env};

use clap::Parser;

use rustc_hir as hir;
use rustc_middle::mir as mir;
use rustc_middle::ty as ty;
use rustc_middle::mir::visit::Visitor;

use rustc_plugin::{CrateFilter, RustcPlugin, RustcPluginArgs, Utf8Path};

use serde::{Deserialize, Serialize};

// This struct is the plugin provided to the rustc_plugin framework,
// and it must be exported for use by the CLI/driver binaries.
pub struct PurifierPlugin;

// To parse CLI arguments, we use Clap.
#[derive(Parser, Serialize, Deserialize)]
pub struct PurifierPluginArgs {
    #[arg(short, long)]
    function: String,
}

impl RustcPlugin for PurifierPlugin {
    type Args = PurifierPluginArgs;

    fn version(&self) -> Cow<'static, str> {
        env!("CARGO_PKG_VERSION").into()
    }

    fn driver_name(&self) -> Cow<'static, str> {
        "purifier-driver".into()
    }

    // In the CLI, we ask Clap to parse arguments and also specify a CrateFilter.
    fn args(&self, _target_dir: &Utf8Path) -> RustcPluginArgs<Self::Args> {
        let args = PurifierPluginArgs::parse_from(env::args().skip(1));
        let filter = CrateFilter::AllCrates;
        RustcPluginArgs { args, filter }
    }

    // In the driver, we use the Rustc API to start a compiler session
    // for the arguments given to us by rustc_plugin.
    fn run(
        self,
        compiler_args: Vec<String>,
        plugin_args: Self::Args,
    ) -> rustc_interface::interface::Result<()> {
        let mut callbacks = PurifierCallbacks { args: plugin_args };
        let compiler = rustc_driver::RunCompiler::new(&compiler_args, &mut callbacks);
        compiler.run()
    }
}

struct PurifierCallbacks {
    args: PurifierPluginArgs,
}

impl rustc_driver::Callbacks for PurifierCallbacks {
    // At the top-level, the Rustc API uses an event-based interface for
    // accessing the compiler at different stages of compilation. In this callback,
    // all the type-checking has completed.
    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        queries: &'tcx rustc_interface::Queries<'tcx>,
    ) -> rustc_driver::Compilation {
        queries
            .global_ctxt()
            .unwrap()
            .enter(|tcx| purifier(tcx, &self.args));

        rustc_driver::Compilation::Continue
    }
}

// The entry point of analysis.
fn purifier(tcx: ty::TyCtxt, args: &PurifierPluginArgs) {
    let hir = tcx.hir();
    for item_id in hir.items() {
        let item = hir.item(item_id);
        let def_id = item.owner_id.to_def_id();
        // Find the desired function by name.
        if item.ident.name == rustc_span::symbol::Symbol::intern(args.function.as_str()) {
            println!("[STARTING ANALYSIS]");

            if let hir::ItemKind::Fn(fn_sig, _, _) = &item.kind {
                let main_body = tcx.optimized_mir(def_id);
                let main_instance = ty::Instance::mono(tcx, def_id);

                println!("--> Checking for mutable reference params in {}...", args.function);
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

                let mut visitor = FnVisitor::new(tcx, main_body, main_instance);
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
                println!("--> Body purity check result for function {}: {}", args.function, visitor.check_purity());
            }
        }
    }
}