use rustc_middle::mir::{Body, Operand, Place};
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_mir_dataflow::{Analysis, JoinSemiLattice};
use rustc_span::def_id::DefId;
use rustc_utils::BodyExt;
use std::fs;

use crate::collector::collector_domain::CollectorDomain;
use crate::collector::dataflow_shim::iterate_to_fixpoint;
use crate::collector::structs::{PartialFunctionInfo, VirtualStack, VirtualStackItem};
use crate::common::storage::{ClosureInfoStorage, FunctionInfoStorage};
use crate::common::{ArgTys, FunctionCall, FunctionInfo, NormalizedPlace, TrackedTy};

/// Dumps MIR of a given body, used primarly for debugging test failures.
fn dump_mir<'tcx>(def_id: DefId, body: &Body<'tcx>, tcx: TyCtxt<'tcx>) {
    fs::create_dir_all("mir_dumps").unwrap();
    fs::write(
        format!("mir_dumps/{:?}.rs", def_id,),
        body.to_string(tcx).unwrap(),
    )
    .unwrap();
}

/// Main data structure used for call graph generation aka collecting.
#[derive(Clone)]
pub struct Collector<'tcx> {
    /// Currently analyzed function.
    current_function: PartialFunctionInfo<'tcx>,
    /// Directory of all visited functions.
    function_storage: FunctionInfoStorage<'tcx>,
    /// Directory of all visited closures.
    closure_storage: ClosureInfoStorage<'tcx>,
    /// Used to detect recursive calls and halt early.
    virtual_stack: VirtualStack<'tcx>,
    /// Set to true if only in-crate collection is performed.
    shallow: bool,
    /// Reference to global TyCtxt.
    tcx: TyCtxt<'tcx>,
}

impl<'tcx> Collector<'tcx> {
    /// Initialize all fields in the collector.
    fn new(instance: ty::Instance<'tcx>, shallow: bool, tcx: TyCtxt<'tcx>) -> Self {
        let current_function = PartialFunctionInfo::new_function(instance, tcx);
        let virtual_stack = VirtualStack::new();
        let function_storage = FunctionInfoStorage::new(instance);
        let closure_storage = ClosureInfoStorage::new();
        Collector {
            current_function,
            virtual_stack,
            function_storage,
            closure_storage,
            shallow,
            tcx,
        }
    }

    /// Clone the collector, preserving all the fields except for the function.
    fn swap_function(&self, new_function: PartialFunctionInfo<'tcx>) -> Self {
        let mut collector = self.clone();
        collector.current_function = new_function;
        collector
    }

    /// Current function getter.
    pub fn get_current_function(&self) -> &PartialFunctionInfo<'tcx> {
        &self.current_function
    }

    /// Retrieves instance body, applying in-scope substitutions.
    pub fn get_substituted_body(&self) -> Body<'tcx> {
        let instance = self.current_function.instance();
        self.current_function
            .substitute(self.tcx.instance_mir(instance.def).to_owned(), self.tcx)
    }

    /// Function info storage getter.
    pub fn get_function_info_storage(&self) -> FunctionInfoStorage<'tcx> {
        self.function_storage.clone()
    }

    /// Closure info storage getter.
    pub fn get_closure_info_storage(&self) -> ClosureInfoStorage<'tcx> {
        self.closure_storage.clone()
    }

    /// TyCtxt getter.
    pub fn get_tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }
}

impl<'tcx> Collector<'tcx> {
    /// Public entry point of collection.
    pub fn collect(instance: ty::Instance<'tcx>, tcx: TyCtxt<'tcx>, shallow: bool) -> Self {
        let mut collector = Collector::new(instance, shallow, tcx);

        let results = collector.run();

        let fn_info = FunctionInfo::new_with_body(
            instance,
            results.places().to_owned(),
            results.calls().to_owned(),
            collector.get_substituted_body(),
            results.unhandled().to_owned(),
        );

        collector.function_storage.insert(fn_info.clone());
        collector
    }

    /// Run a previously set-up collector instance.
    fn run(&mut self) -> CollectorDomain<'tcx> {
        // Short-circut if the function has already been visited.
        if let Some(function_info) = self
            .function_storage
            .get_with_body(self.current_function.instance())
        {
            return CollectorDomain::from_regular_fn_info(&function_info);
        }

        // Push the function onto the virtual stack.
        self.virtual_stack
            .push(VirtualStackItem::new(&self.current_function));

        // Dump function MIR (helps with debugging).
        dump_mir(
            self.current_function.instance().def_id(),
            &self.get_substituted_body(),
            self.tcx,
        );

        // Perform substitution.
        let substituted_body = self.get_substituted_body();

        // Run dataflow analysis.
        let mut cursor = iterate_to_fixpoint(
            self.clone()
                .into_engine(self.tcx, &self.get_substituted_body()),
        )
        .into_results_cursor(&substituted_body);

        // Retrieve end results for each basic block.
        let bb_results = substituted_body
            .basic_blocks
            .iter_enumerated()
            .map(|(bb, _)| {
                cursor.seek_to_block_end(bb);
                cursor.get().to_owned()
            });

        // Bring results from multiple basic blocks together.
        let total_results = bb_results
            .reduce(|mut acc, elt| {
                acc.join(&elt);
                acc
            })
            .unwrap();

        total_results
    }

    /// Analyze a callee, possibly overapproximated.
    fn collect_plausible_callee(
        &self,
        plausible_callee: PartialFunctionInfo<'tcx>,
        args: &Vec<Operand<'tcx>>,
        arg_tys: &ArgTys<'tcx>,
        domain: &mut CollectorDomain<'tcx>,
        destination: Option<&Place<'tcx>>,
    ) {
        // Skip if the call is repeated.
        let current_seen_item = VirtualStackItem::new(&plausible_callee);
        if self.virtual_stack.contains(&current_seen_item) {
            return;
        };

        // Determine the return type while determining the callee body.
        let return_ty: TrackedTy<'_> = match plausible_callee.instance().def {
            // Callee has no body, add to the storage, register call, get generic return type.
            ty::InstanceDef::Intrinsic(..) | ty::InstanceDef::Virtual(..) => {
                self.function_storage.insert(FunctionInfo::new_without_body(
                    plausible_callee.instance().def_id().to_owned(),
                    arg_tys.as_vec().to_owned(),
                ));
                domain.add_call(FunctionCall::new_without_body(
                    plausible_callee.instance().def_id(),
                    args.to_owned(),
                ));
                TrackedTy::from_ty(
                    self.tcx
                        .fn_sig(plausible_callee.instance().def_id())
                        .subst(self.tcx, plausible_callee.instance().substs)
                        .output()
                        .skip_binder(),
                )
            }
            // Callee has a body, request it and continue recursively, get specific return type.
            _ => {
                let body = plausible_callee.substitute(
                    self.tcx
                        .instance_mir(plausible_callee.instance().def)
                        .to_owned(),
                    self.tcx,
                );

                // Swap the current instance and continue recursively.
                let results = self.swap_function(plausible_callee.clone()).run();

                self.function_storage.insert(FunctionInfo::new_with_body(
                    plausible_callee.instance().to_owned(),
                    results.places().to_owned(),
                    results.calls().to_owned(),
                    body.to_owned(),
                    results.unhandled().to_owned(),
                ));

                domain.add_call(FunctionCall::new_with_body(
                    plausible_callee.instance().to_owned(),
                    args.to_owned(),
                ));

                results.return_type(plausible_callee.instance().def_id(), &body, self.tcx)
            }
        };

        // Update the return type.
        if let Some(destination) = destination {
            let normalized_destination = NormalizedPlace::from_place(
                &destination,
                self.tcx,
                self.current_function.instance().def_id(),
            );

            domain.update_with(
                normalized_destination,
                return_ty,
                &self.get_substituted_body(),
                self.current_function.instance().def_id(),
                self.tcx,
            );
        }
    }

    /// Run resolution and collection on a function call.
    pub(super) fn collect_function_call(
        &self,
        function_ty: Ty<'tcx>,
        args: &Vec<Operand<'tcx>>,
        arg_tys: ArgTys<'tcx>,
        domain: &mut CollectorDomain<'tcx>,
        destination: Option<&Place<'tcx>>,
    ) {
        // Apply substitutions to the type in case it contains generics.
        let function_ty = self.current_function.substitute(function_ty, self.tcx);

        // We only support direct function calls for now.
        if let ty::FnDef(def_id, substs) = function_ty.kind() {
            // Return if performing shallow analysis and encountered a non-local def_id.
            if !def_id.is_local() && self.shallow {
                return;
            }

            // Calculate argument types, account for possible erasure.
            let plausible_callees = self.current_function.resolve(
                def_id.to_owned(),
                substs,
                &arg_tys,
                self.closure_storage.clone(),
                self.tcx,
            );

            // Analyze possible callees if non-empty.
            if !plausible_callees.is_empty() {
                for plausible_callee in plausible_callees.into_iter() {
                    self.collect_plausible_callee(
                        plausible_callee,
                        args,
                        &arg_tys,
                        domain,
                        destination,
                    );
                }
            } else {
                // Give up -- we cannot determine a function type here.
                self.function_storage.insert(FunctionInfo::new_without_body(
                    def_id.to_owned(),
                    arg_tys.as_vec().to_owned(),
                ));
                domain.add_call(FunctionCall::new_without_body(
                    def_id.to_owned(),
                    args.to_owned(),
                ));
            }
        } else {
            // This happens if we weren't able to unwrap the call type (e.g. FnPtr).
            domain.add_unhandled(function_ty.to_owned());
        }
    }
}
