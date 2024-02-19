use rustc_middle::mir::Body;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::{def_id::DefId, Span};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::fn_info::FnInfo;
use super::normalized_place::NormalizedPlace;
use super::tracked_ty::TrackedTy;
use super::type_tracker::Call;

pub type FnInfoStorageRef<'tcx> = Rc<RefCell<FnInfoStorage<'tcx>>>;

#[derive(Clone)]
pub struct FnInfoStorage<'tcx> {
    origin: ty::Instance<'tcx>,
    fns: Vec<FnInfo<'tcx>>,
    unhandled: Vec<Ty<'tcx>>,
}

#[derive(Serialize)]
pub struct FilteredCalls<'tcx> {
    regular: Vec<FnInfo<'tcx>>,
    externs: Vec<FnInfo<'tcx>>,
    ambiguous: Vec<FnInfo<'tcx>>,
}

impl<'tcx> FnInfoStorage<'tcx> {
    pub fn new(origin: ty::Instance<'tcx>) -> FnInfoStorage<'tcx> {
        Self {
            origin,
            fns: vec![],
            unhandled: vec![],
        }
    }

    pub fn add_with_body(
        &mut self,
        parent: ty::Instance<'tcx>,
        instance: ty::Instance<'tcx>,
        places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>,
        calls: HashSet<Call<'tcx>>,
        body: Body<'tcx>,
        span: Span,
    ) {
        let fn_info = FnInfo::Regular {
            parent,
            instance,
            places,
            calls,
            body,
            span,
        };
        if !self.fns.contains(&fn_info) {
            self.fns.push(fn_info);
        }
    }

    pub fn add_without_body(
        &mut self,
        parent: ty::Instance<'tcx>,
        def_id: DefId,
        tracked_args: Vec<TrackedTy<'tcx>>,
        tcx: TyCtxt<'tcx>,
    ) {
        let fn_info = if tcx.is_foreign_item(def_id) {
            FnInfo::Extern {
                parent,
                def_id,
                tracked_args,
            }
        } else {
            FnInfo::Ambiguous {
                parent,
                def_id,
                tracked_args,
            }
        };
        if !self.fns.contains(&fn_info) {
            self.fns.push(fn_info);
        }
    }

    pub fn add_unhandled(&mut self, new_unhandled: Ty<'tcx>) {
        self.unhandled.push(new_unhandled);
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

    pub fn origin(&self) -> &ty::Instance<'tcx> {
        &self.origin
    }

    pub fn fns(&self) -> &Vec<FnInfo<'tcx>> {
        &self.fns
    }

    pub fn unhandled(&self) -> &Vec<Ty<'tcx>> {
        &self.unhandled
    }
}
