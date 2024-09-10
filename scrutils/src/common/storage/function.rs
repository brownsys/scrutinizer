use rustc_middle::ty;
use rustc_span::def_id::DefId;
use std::cell::RefCell;
use std::rc::Rc;

use crate::common::{FunctionCall, FunctionInfo};

pub type FunctionInfoStorageRef<'tcx> = Rc<RefCell<FunctionInfoStorage<'tcx>>>;

#[derive(Clone, Debug)]
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

    pub fn insert(&mut self, function_info: FunctionInfo<'tcx>) {
        if !self.fns.contains(&function_info) {
            self.fns.push(function_info.clone());
        }
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

    pub fn all(&self) -> &Vec<FunctionInfo<'tcx>> {
        &self.fns
    }
}
