use regex::Regex;
use rustc_hir::def_id::DefId;
use rustc_middle::mir::Terminator;
use std::cell::RefCell;
use std::rc::Rc;

use super::fn_info::FnInfo;
use super::tracked_ty::TrackedTy;

pub type SeenStorage<'tcx> = Rc<RefCell<Vec<SeenStorageItem<'tcx>>>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeenStorageItem<'tcx> {
    pub def_id: DefId,
    pub tracked_args: Vec<TrackedTy<'tcx>>,
}

pub type FnInfoStorageRef<'tcx> = Rc<RefCell<FnInfoStorage<'tcx>>>;

#[derive(Clone)]
pub struct FnInfoStorage<'tcx> {
    def_id: DefId,
    fn_calls: Vec<FnInfo<'tcx>>,
    unhandled: Vec<Terminator<'tcx>>,
}

impl<'tcx> FnInfoStorage<'tcx> {
    pub fn new(def_id: DefId) -> FnInfoStorage<'tcx> {
        Self {
            def_id,
            fn_calls: vec![],
            unhandled: vec![],
        }
    }

    pub fn add_call(&mut self, new_call: FnInfo<'tcx>) {
        self.fn_calls.push(new_call);
    }

    pub fn add_unhandled(&mut self, new_unhandled: Terminator<'tcx>) {
        self.unhandled.push(new_unhandled);
    }

    pub fn get_fn_call(&self, def_id: DefId) -> Option<FnInfo<'tcx>> {
        self.fn_calls
            .iter()
            .find(|call| match call {
                FnInfo::Regular {
                    def_id: call_def_id,
                    ..
                }
                | FnInfo::Ambiguous {
                    def_id: call_def_id,
                    ..
                }
                | FnInfo::Extern {
                    def_id: call_def_id,
                    ..
                } => call_def_id.to_owned() == def_id,
            })
            .cloned()
    }

    fn check_fn_call_purity(&self, fn_call: &FnInfo) -> bool {
        let allowed_libs = vec![
            Regex::new(r"core\[\w*\]::intrinsics").unwrap(),
            Regex::new(r"core\[\w*\]::panicking").unwrap(),
            Regex::new(r"alloc\[\w*\]::alloc").unwrap(),
        ];
        match fn_call {
            FnInfo::Regular {
                def_id,
                raw_ptr_deref,
                ..
            } => {
                let def_path_str = format!("{:?}", def_id);
                !raw_ptr_deref || (allowed_libs.iter().any(|lib| lib.is_match(&def_path_str)))
            }
            FnInfo::Ambiguous { def_id, .. } | FnInfo::Extern { def_id, .. } => {
                let def_path_str = format!("{:?}", def_id);
                allowed_libs.iter().any(|lib| lib.is_match(&def_path_str))
            }
        }
    }

    pub fn check_purity(&self) -> bool {
        self.fn_calls
            .iter()
            .all(|fn_call| self.check_fn_call_purity(fn_call))
            && self.unhandled.is_empty()
    }

    pub fn dump(&self, annotated_pure: bool) -> PurityAnalysisResult<'tcx> {
        let (passing_calls, failing_calls) = self
            .fn_calls
            .clone()
            .into_iter()
            .partition(|fn_call| self.check_fn_call_purity(fn_call));
        if !self.check_purity() {
            let reason = if !self.unhandled.is_empty() {
                String::from("unhandled terminator")
            } else if !self
                .fn_calls
                .iter()
                .all(|fn_call| self.check_fn_call_purity(fn_call))
            {
                String::from("unable to ascertain purity of inner function call")
            } else {
                unreachable!()
            };
            PurityAnalysisResult::new(
                self.def_id,
                annotated_pure,
                self.check_purity(),
                reason,
                passing_calls,
                failing_calls,
                self.unhandled.clone(),
            )
        } else {
            PurityAnalysisResult::new(
                self.def_id,
                annotated_pure,
                self.check_purity(),
                String::new(),
                passing_calls,
                failing_calls,
                self.unhandled.clone(),
            )
        }
    }
}
