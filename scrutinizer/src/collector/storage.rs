use rustc_middle::mir::Terminator;
use rustc_middle::ty;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::fn_info::FnInfo;
use super::normalized_place::NormalizedPlace;
use super::tracked_ty::TrackedTy;

pub type FnInfoStorageRef<'tcx> = Rc<RefCell<FnInfoStorage<'tcx>>>;

#[derive(Clone)]
pub struct FnInfoStorage<'tcx> {
    origin: ty::Instance<'tcx>,
    fns: Vec<FnInfo<'tcx>>,
    unhandled: Vec<Terminator<'tcx>>,
}

impl<'tcx> FnInfoStorage<'tcx> {
    pub fn new(origin: ty::Instance<'tcx>) -> FnInfoStorage<'tcx> {
        Self {
            origin,
            fns: vec![],
            unhandled: vec![],
        }
    }

    pub fn add_fn(&mut self, new_fn: FnInfo<'tcx>) {
        self.fns.push(new_fn);
    }

    pub fn add_unhandled(&mut self, new_unhandled: Terminator<'tcx>) {
        self.unhandled.push(new_unhandled);
    }

    pub fn dump(&self) -> Vec<FnInfo<'tcx>> {
        self.fns.to_owned()
    }

    pub fn get_regular(
        &self,
        instance: &ty::Instance<'tcx>,
    ) -> Option<HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>> {
        self.fns
            .iter()
            .find(|func| {
                if let FnInfo::Regular {
                    instance: func_instance,
                    ..
                } = func
                {
                    instance == func_instance
                } else {
                    false
                }
            })
            .and_then(|func| {
                if let FnInfo::Regular { places, .. } = func {
                    Some(places.to_owned())
                } else {
                    unreachable!()
                }
            })
    }
}
