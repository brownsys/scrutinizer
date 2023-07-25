#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use std::{borrow::Cow, env};
use std::cell::RefCell;
use std::rc::Rc;

use clap::Parser;

use rustc_hir as hir;
use rustc_middle::mir as mir;
use rustc_middle::ty as ty;

use rustc_middle::mir::visit::Visitor;
use rustc_middle::mir::HasLocalDecls;

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

#[derive(Debug)]
struct FnCallInfo<'tcx> {
    def_id: hir::def_id::DefId,
    arg_tys: Vec<ty::Ty<'tcx>>,
    // Whether we were able to retrieve and check the MIR for the function body.
    body_checked: bool,
}

struct FnVisitor<'tcx> {
    tcx: ty::TyCtxt<'tcx>,
    // Maintain single list of function calls.
    fn_calls: Rc<RefCell<Vec<FnCallInfo<'tcx>>>>,
    current_body: &'tcx mir::Body<'tcx>,
}

impl<'tcx> mir::visit::Visitor<'tcx> for FnVisitor<'tcx> {
    fn visit_terminator(
        &mut self,
        terminator: &mir::Terminator<'tcx>,
        location: mir::Location,
    ) {
        match &terminator.kind {
            mir::TerminatorKind::Call { func, args, .. } => {
                if let Some((def_id, _)) = func.const_fn_def() {
                    // To avoid visiting the same function body twice.
                    if !self.fn_calls.borrow().iter().any(|fn_call_info| fn_call_info.def_id == def_id) {
                        let local_decls = self.current_body.local_decls();
                        let arg_tys = args.iter().map(|arg| arg.ty(local_decls, self.tcx)).collect::<Vec<_>>();
                        self.fn_calls.borrow_mut().push(FnCallInfo { def_id, arg_tys, body_checked: self.tcx.is_mir_available(def_id) });
                        if self.tcx.is_mir_available(def_id) {
                            let mir = self.tcx.optimized_mir(def_id);
                            // Swap the current body and continue recursively.
                            let mut visitor = self.with_new_body(mir);
                            visitor.visit_body(mir);
                        }
                    }
                }
            }
            _ => {}
        }
        self.super_terminator(terminator, location);
    }
}

impl<'tcx> FnVisitor<'tcx> {
    fn new(tcx: ty::TyCtxt<'tcx>, current_body: &'tcx mir::Body<'tcx>) -> Self {
        FnVisitor { tcx, fn_calls: Rc::new(RefCell::new(Vec::new())), current_body }
    }

    fn with_new_body(&self, new_body: &'tcx mir::Body<'tcx>) -> Self {
        FnVisitor { tcx: self.tcx, fn_calls: self.fn_calls.clone(), current_body: new_body }
    }

    fn dump(&self) {
        for fn_call in self.fn_calls.borrow().iter() {
            dbg!(fn_call);
        }
    }
}

// The entry point of analysis.
fn pure_func(tcx: ty::TyCtxt, args: &PureFuncPluginArgs) {
    let hir = tcx.hir();
    for item_id in hir.items() {
        let item = hir.item(item_id);
        let def_id = item.owner_id.to_def_id();
        // Find the desired function by name.
        if item.ident.name == rustc_span::symbol::Symbol::intern(args.function.as_str()) {
            let mir = tcx.optimized_mir(def_id);
            let mut visitor = FnVisitor::new(tcx, mir);
            visitor.visit_body(mir);
            visitor.dump();
        }
    }
}