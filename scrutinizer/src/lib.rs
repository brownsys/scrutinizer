#![feature(rustc_private)]

mod vartrack;
mod visitors;

use vartrack::compute_dependencies;
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

use rustc_hir as hir;
use rustc_middle::mir;
use rustc_middle::mir::visit::Visitor;
use rustc_middle::mir::Local;
use rustc_middle::ty;

use rustc_plugin::{CrateFilter, RustcPlugin, RustcPluginArgs, Utf8Path};
// use rustc_utils::mir::borrowck_facts;

use serde::{Deserialize, Serialize};

// This struct is the plugin provided to the rustc_plugin framework,
// and it must be exported for use by the CLI/driver binaries.
pub struct ScrutinizerPlugin;

// To parse CLI arguments, we use Clap.
#[derive(Parser, Serialize, Deserialize)]
pub struct ScrutinizerPluginArgs {
    #[arg(short, long)]
    function: String,
}

impl RustcPlugin for ScrutinizerPlugin {
    type Args = ScrutinizerPluginArgs;

    fn version(&self) -> Cow<'static, str> {
        env!("CARGO_PKG_VERSION").into()
    }

    fn driver_name(&self) -> Cow<'static, str> {
        "scrutinizer-driver".into()
    }

    // In the CLI, we ask Clap to parse arguments and also specify a CrateFilter.
    fn args(&self, _target_dir: &Utf8Path) -> RustcPluginArgs<Self::Args> {
        let args = ScrutinizerPluginArgs::parse_from(env::args().skip(1));
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
        let mut callbacks = ScrutinizerCallbacks { args: plugin_args };
        let compiler = rustc_driver::RunCompiler::new(&compiler_args, &mut callbacks);
        compiler.run()
    }
}

struct ScrutinizerCallbacks {
    args: ScrutinizerPluginArgs,
}

impl rustc_driver::Callbacks for ScrutinizerCallbacks {
    // fn config(&mut self, config: &mut rustc_interface::Config) {
    //     // You MUST configure rustc to ensure `get_body_with_borrowck_facts` will work.
    //     borrowck_facts::enable_mir_simplification();
    //     config.override_queries = Some(borrowck_facts::override_queries);
    // }
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

                let sensitive_arg = Local::from_usize(1);
                let deps = compute_dependencies(tcx, def_id, sensitive_arg);
                dbg!(&deps);

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

                let mut visitor = FnVisitor::new(tcx, main_body, main_instance, deps);
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
