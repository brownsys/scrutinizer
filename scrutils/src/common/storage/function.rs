use rustc_middle::ty;
use rustc_span::def_id::DefId;
use std::cell::RefCell;
use std::rc::Rc;

use crate::common::{FunctionCall, FunctionInfo};

#[derive(Clone)]
pub struct FunctionInfoStorage<'tcx> {
    storage: Rc<RefCell<FunctionInfoStorageInternal<'tcx>>>,
}

#[derive(Clone)]
struct FunctionInfoStorageInternal<'tcx> {
    origin: ty::Instance<'tcx>,
    fns: Vec<FunctionInfo<'tcx>>,
}

impl<'tcx> FunctionInfoStorage<'tcx> {
    pub fn new(origin: ty::Instance<'tcx>) -> Self {
        Self {
            storage: Rc::new(RefCell::new(FunctionInfoStorageInternal {
                origin,
                fns: vec![],
            })),
        }
    }

    pub fn insert(&self, function_info: FunctionInfo<'tcx>) {
        let mut storage = self.storage.borrow_mut();
        if !storage.fns.contains(&function_info) {
            storage.fns.push(function_info.clone());
        }
    }

    pub fn get_with_body(&self, instance: &ty::Instance<'tcx>) -> Option<FunctionInfo<'tcx>> {
        let storage = self.storage.borrow();
        storage
            .fns
            .iter()
            .find(|func| {
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
            .and_then(|func_info| Some(func_info.to_owned()))
    }

    pub fn get_without_body(&self, def_id: &DefId) -> Option<FunctionInfo<'tcx>> {
        let storage = self.storage.borrow();
        storage
            .fns
            .iter()
            .find(|func| match func {
                FunctionInfo::WithoutBody {
                    def_id: func_def_id,
                    ..
                } => func_def_id == def_id,
                _ => false,
            })
            .and_then(|func_info| Some(func_info.to_owned()))
    }

    pub fn get_by_call(&self, call: &FunctionCall<'tcx>) -> FunctionInfo<'tcx> {
        match call {
            FunctionCall::WithBody { instance, .. } => self.get_with_body(instance),
            FunctionCall::WithoutBody { def_id, .. } => self.get_without_body(def_id),
        }
        .unwrap()
    }

    pub fn origin(&self) -> ty::Instance<'tcx> {
        let storage = self.storage.borrow();
        storage.origin.clone()
    }

    pub fn all(&self) -> Vec<FunctionInfo<'tcx>> {
        let storage = self.storage.borrow();
        storage.fns.clone()
    }
}
