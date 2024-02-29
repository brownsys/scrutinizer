use rustc_middle::mir::Body;
use rustc_middle::ty::{self, Ty};
use rustc_span::{def_id::DefId, Span};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::{FunctionCall, FunctionInfo, NormalizedPlace, TrackedTy};

pub type FunctionInfoStorageRef<'tcx> = Rc<RefCell<FunctionInfoStorage<'tcx>>>;

#[derive(Clone)]
pub struct FunctionInfoStorage<'tcx> {
    origin: ty::Instance<'tcx>,
    fns: Vec<FunctionInfo<'tcx>>,
}

impl<'tcx> FunctionInfoStorage<'tcx> {
    pub fn new(origin: ty::Instance<'tcx>) -> FunctionInfoStorage<'tcx> {
        Self {
            origin,
            fns: vec![],
        }
    }

    pub fn add_with_body(
        &mut self,
        parent: ty::Instance<'tcx>,
        instance: ty::Instance<'tcx>,
        places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>,
        calls: HashSet<FunctionCall<'tcx>>,
        body: Body<'tcx>,
        span: Span,
        unhandled: HashSet<Ty<'tcx>>,
    ) -> FunctionInfo<'tcx> {
        let fn_info = FunctionInfo::WithBody {
            parent,
            instance,
            places,
            calls,
            body,
            span,
            unhandled,
        };
        if !self.fns.contains(&fn_info) {
            self.fns.push(fn_info.clone());
        }
        fn_info
    }

    pub fn add_without_body(
        &mut self,
        parent: ty::Instance<'tcx>,
        def_id: DefId,
        tracked_args: Vec<TrackedTy<'tcx>>,
    ) -> FunctionInfo<'tcx> {
        let fn_info = FunctionInfo::WithoutBody {
            parent,
            def_id,
            tracked_args,
        };
        if !self.fns.contains(&fn_info) {
            self.fns.push(fn_info.clone());
        }
        fn_info
    }

    pub fn get_with_body(&self, instance: &ty::Instance<'tcx>) -> Option<&FunctionInfo<'tcx>> {
        self.fns.iter().find(|func| {
            if let FunctionInfo::WithBody {
                instance: func_instance,
                ..
            } = func
            {
                instance == func_instance
            } else {
                false
            }
        })
    }

    pub fn get_without_body(&self, def_id: &DefId) -> Option<&FunctionInfo<'tcx>> {
        self.fns.iter().find(|func| match func {
            FunctionInfo::WithoutBody {
                def_id: func_def_id,
                ..
            } => func_def_id == def_id,
            _ => false,
        })
    }

    pub fn get_by_call(&self, call: &FunctionCall<'tcx>) -> &FunctionInfo<'tcx> {
        match call {
            FunctionCall::WithBody { instance, .. } => self.get_with_body(instance),
            FunctionCall::WithoutBody { def_id, .. } => self.get_without_body(def_id),
        }
        .unwrap()
    }

    pub fn origin(&self) -> &ty::Instance<'tcx> {
        &self.origin
    }
}
