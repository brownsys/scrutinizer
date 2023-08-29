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
    current_instance: ty::Instance<'tcx>,
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
                    // To avoid visiting the same function body twice, check whether we have seen it.
                    if !self.encountered_def_id(def_id) {
                        // Attempt to resolve the instance via monomorphization.
                        let func_ty = func.ty(self.current_body, self.tcx);
                        let func_ty = self.current_instance.subst_mir_and_normalize_erasing_regions(
                            self.tcx,
                            ty::ParamEnv::reveal_all(),
                            func_ty,
                        );
                        if let ty::FnDef(callee_def_id, substs) = func_ty.kind() {
                            let instance = ty::Instance::expect_resolve(self.tcx, ty::ParamEnv::reveal_all(), *callee_def_id, substs);
                            let def_id = instance.def.def_id();
                            // Retrieve argument types.
                            let local_decls = self.current_body.local_decls();
                            let arg_tys = args.iter().map(|arg| arg.ty(local_decls, self.tcx)).collect::<Vec<_>>();
                            if self.tcx.is_mir_available(def_id) {
                                self.add_call(FnCallInfo { def_id, arg_tys, body_checked: true });
                                let body = self.tcx.optimized_mir(def_id);
                                // Swap the current instance and body and continue recursively.
                                let mut visitor = self.with_new_body_and_instance(body, instance);
                                visitor.visit_body(body);
                            } else {
                                // Otherwise, we are unable to verify the purity due to external reference or dynamic dispatch.
                                self.add_call(FnCallInfo { def_id, arg_tys, body_checked: false });
                            }
                        } else {
                            panic!("Other type of call encountered.");
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
    fn new(tcx: ty::TyCtxt<'tcx>, current_body: &'tcx mir::Body<'tcx>, current_instance: ty::Instance<'tcx>) -> Self {
        FnVisitor { tcx, fn_calls: Rc::new(RefCell::new(Vec::new())), current_body, current_instance }
    }

    fn with_new_body_and_instance(&self, new_body: &'tcx mir::Body<'tcx>, new_instance: ty::Instance<'tcx>) -> Self {
        FnVisitor { tcx: self.tcx, fn_calls: self.fn_calls.clone(), current_body: new_body, current_instance: new_instance }
    }

    fn add_call(&mut self, new_call: FnCallInfo<'tcx>) {
        self.fn_calls.borrow_mut().push(new_call);
    }

    fn encountered_def_id(&self, def_id: hir::def_id::DefId) -> bool {
        self.fn_calls.borrow().iter().any(|fn_call_info| fn_call_info.def_id == def_id)
    }

    fn dump_passing(&self) {
        for fn_call in self.fn_calls.borrow().iter() {
            if FnVisitor::check_fn_call_purity(fn_call) {
                dbg!(fn_call);
            }
        }
    }

    fn dump_violating(&self) {
        for fn_call in self.fn_calls.borrow().iter() {
            if !FnVisitor::check_fn_call_purity(fn_call) {
                dbg!(fn_call);
            }
        }
    }

    fn check_fn_call_purity(fn_call: &FnCallInfo) -> bool {
        fn_call.body_checked && fn_call.arg_tys.iter().all(|arg_ty| {
            if let Some(mutability) = arg_ty.ref_mutability() {
                match mutability {
                    mir::Mutability::Not => true,
                    mir::Mutability::Mut => false,
                }
            } else {
                !arg_ty.is_mutable_ptr()
            }
        })
    }

    fn check_purity(&self) -> bool {
        self.fn_calls.borrow().iter().all(|fn_call| {
            FnVisitor::check_fn_call_purity(fn_call)
        })
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
            if let hir::ItemKind::Fn(_, _, _) = &item.kind {
                let main_body = tcx.optimized_mir(def_id);
                let main_instance = ty::Instance::mono(tcx, def_id);
                let mut visitor = FnVisitor::new(tcx, main_body, main_instance);
                // Begin the traversal.
                visitor.visit_body(main_body);
                // Show all unchecked bodies encountered.
                visitor.dump_violating();
                println!("Body purity check result for function {}: {}", args.function, visitor.check_purity());
            }
        }
    }
}