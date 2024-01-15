use std::cell::RefCell;
use std::rc::Rc;

use rustc_hir::def_id::DefId;
use rustc_middle::mir::{visit::Visitor, Location, Operand, Place, Terminator, TerminatorKind};
use rustc_middle::ty::{self, TyCtxt};

use super::fn_call_info::FnCallInfo;
use super::fn_data::FnData;
use super::partial_fn_data::PartialFnData;
use super::raw_ptr::HasRawPtrDeref;
use super::storage::FnCallStorage;

pub struct FnVisitor<'tcx> {
    // Type context.
    tcx: TyCtxt<'tcx>,
    // All seen function calls.
    storage: Rc<RefCell<FnCallStorage<'tcx>>>,
    // Currently visited function.
    current_fn: FnData<'tcx>,
}

impl<'tcx> Visitor<'tcx> for FnVisitor<'tcx> {
    fn visit_terminator(&mut self, terminator: &Terminator<'tcx>, location: Location) {
        if let TerminatorKind::Call {
            func,
            args,
            destination,
            ..
        } = &terminator.kind
        {
            // Body of the function where the terminator was found, always exists.
            let body = self
                .tcx
                .optimized_mir(self.current_fn.get_instance().def_id());

            // Attempt to resolve the callee instance via monomorphization.
            let fn_ty = self
                .current_fn
                .get_instance()
                .subst_mir_and_normalize_erasing_regions(
                    self.tcx,
                    ty::ParamEnv::reveal_all(),
                    func.ty(body, self.tcx),
                );

            // TODO: handle different call types (e.g. FnPtr).
            if let ty::FnDef(def_id, substs) = fn_ty.kind() {
                self.visit_fn_call(def_id.to_owned(), substs, args, location, destination);
            } else {
                self.storage
                    .borrow_mut()
                    .add_unhandled(terminator.to_owned());
            }
        }
        self.super_terminator(terminator, location);
    }
}

impl<'tcx> FnVisitor<'tcx> {
    pub fn visit_fn_call(
        &mut self,
        def_id: DefId,
        substs: ty::SubstsRef<'tcx>,
        args: &Vec<Operand<'tcx>>,
        location: Location,
        ret_destination: &Place,
    ) {
        // Calculate argument types, account for possible erasure.
        let partial_fn_data = PartialFnData::new(
            def_id,
            substs,
            args.to_owned(),
            &location,
            &self.current_fn,
            self.tcx,
        );

        let plausible_fns =
            partial_fn_data.try_resolve(&self.current_fn.important_locals(), self.tcx);
        if !plausible_fns.is_empty() {
            for fn_data in plausible_fns
                .into_iter()
                .filter(|func| !self.tcx.is_const_fn_raw(func.get_instance().def_id()))
            {
                let def_id = fn_data.get_instance().def_id();
                // Only if we have not seen this call before.
                let body = self.tcx.optimized_mir(def_id);

                let fn_call_info = FnCallInfo::WithBody {
                    def_id,
                    from: self.current_fn.get_instance().def_id(),
                    span: body.span,
                    refined_tys: fn_data.get_refined_tys(),
                    raw_ptr_deref: body.has_raw_ptr_deref(self.tcx),
                };

                self.current_fn
                    .refine_ty(ret_destination.as_local().unwrap(), fn_data.return_ty());

                if self.storage.borrow().encountered_fn_call(&fn_call_info) {
                    continue;
                }

                self.storage.borrow_mut().add_call(fn_call_info);

                // Swap the current instance and continue recursively.
                let mut visitor = self.clone_with(fn_data);
                visitor.visit_body(body);
            }
        } else {
            self.storage.borrow_mut().add_call(FnCallInfo::WithoutBody {
                def_id,
                from: self.current_fn.get_instance().def_id(),
                arg_tys: partial_fn_data.get_arg_tys(),
            });
        }
    }

    pub fn new(def_id: DefId, tcx: TyCtxt<'tcx>, current_fn: FnData<'tcx>) -> Self {
        Self {
            tcx,
            storage: Rc::new(RefCell::new(FnCallStorage::new(def_id))),
            current_fn,
        }
    }

    fn clone_with(&self, new_fn: FnData<'tcx>) -> Self {
        Self {
            tcx: self.tcx,
            storage: self.storage.clone(),
            current_fn: new_fn,
        }
    }

    pub fn get_storage_clone(&self) -> FnCallStorage<'tcx> {
        self.storage.borrow().to_owned()
    }
}
