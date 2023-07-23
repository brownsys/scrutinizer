#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use std::{borrow::Cow, env};
use std::collections::HashSet;

use clap::Parser;

use rustc_hir as hir;
use rustc_middle::mir as mir;
use rustc_middle::ty as ty;

use rustc_middle::mir::visit::Visitor;

use rustc_plugin::{CrateFilter, RustcPlugin, RustcPluginArgs, Utf8Path};

use serde::{Deserialize, Serialize};

// This struct is the plugin provided to the rustc_plugin framework,
// and it must be exported for use by the CLI/driver binaries.
pub struct PureFuncPlugin;

// To parse CLI arguments, we use Clap for this example. But that
// detail is up to you.
#[derive(Parser, Serialize, Deserialize)]
pub struct PureFuncPluginArgs {
    #[arg(short, long)]
    function: String,
}

impl RustcPlugin for PureFuncPlugin {
    type Args = PureFuncPluginArgs;

    fn version(&self) -> Cow<'static, str> {
        env!("CARGO_PKG_VERSION").into()
    }

    fn driver_name(&self) -> Cow<'static, str> {
        "pure-func-driver".into()
    }

    // In the CLI, we ask Clap to parse arguments and also specify a CrateFilter.
    // If one of the CLI arguments was a specific file to analyze, then you
    // could provide a different filter.
    fn args(&self, _target_dir: &Utf8Path) -> RustcPluginArgs<Self::Args> {
        let args = PureFuncPluginArgs::parse_from(env::args().skip(1));
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
        let mut callbacks = PureFuncCallbacks { args: plugin_args };
        let compiler = rustc_driver::RunCompiler::new(&compiler_args, &mut callbacks);
        compiler.run()
    }
}

struct PureFuncCallbacks {
    args: PureFuncPluginArgs,
}

impl rustc_driver::Callbacks for PureFuncCallbacks {
    // At the top-level, the Rustc API uses an event-based interface for
    // accessing the compiler at different stages of compilation. In this callback,
    // all the type-checking has completed.
    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        queries: &'tcx rustc_interface::Queries<'tcx>,
    ) -> rustc_driver::Compilation {
        // We extract a key data structure, the `TyCtxt`, which is all we need
        // for our simple task of printing out item names.
        queries
            .global_ctxt()
            .unwrap()
            .enter(|tcx| pure_func(tcx, &self.args));

        // Note that you should generally allow compilation to continue. If
        // your plugin is being invoked on a dependency, then you need to ensure
        // the dependency is type-checked (its .rmeta file is emitted into target/)
        // so that its dependents can read the compiler outputs.
        rustc_driver::Compilation::Continue
    }
}

struct FnVisitor<'tcx> {
    tcx: ty::TyCtxt<'tcx>,
    fn_def_ids: HashSet<hir::def_id::DefId>,
}

impl<'tcx> mir::visit::Visitor<'tcx> for FnVisitor<'tcx> {
    fn visit_terminator(
        &mut self,
        terminator: &mir::Terminator<'tcx>,
        location: mir::Location,
    ) {
        match &terminator.kind {
            mir::TerminatorKind::Call { func, .. } => {
                if let Some((def_id, args)) = func.const_fn_def() {
                    if !self.fn_def_ids.contains(&def_id) {
                        println!("Function: {:?}, args: {:?}.", def_id, args);
                        self.fn_def_ids.insert(def_id);
                        if self.tcx.is_mir_available(def_id) {
                            let mir = self.tcx.optimized_mir(def_id);
                            self.visit_body(mir);
                        } else {
                            println!("No MIR available for function {:?}.", def_id);
                        }
                    }
                }
            }
            _ => {}
        }
        self.super_terminator(terminator, location);
    }
}

// The entry point of analysis.
fn pure_func(tcx: ty::TyCtxt, args: &PureFuncPluginArgs) {
    let hir = tcx.hir();
    for item_id in hir.items() {
        let item = hir.item(item_id);
        let def_id = item.owner_id.to_def_id();
        if item.ident.name == rustc_span::symbol::Symbol::intern(args.function.as_str()) {
            let mir = tcx.optimized_mir(def_id);
            let mut visitor = FnVisitor { tcx, fn_def_ids: HashSet::new() };
            visitor.visit_body(mir);
        }
    }
}