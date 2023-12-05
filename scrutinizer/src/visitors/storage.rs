use super::result::PurityAnalysisResult;
use super::types::FnCallInfo;

use regex::Regex;

use rustc_hir::def_id::DefId;
use rustc_middle::mir::Terminator;
use rustc_middle::ty::TyCtxt;

#[derive(Clone)]
pub struct FnCallStorage<'tcx> {
    def_id: DefId,
    fn_calls: Vec<FnCallInfo<'tcx>>,
    unhandled: Vec<Terminator<'tcx>>,
}

impl<'tcx> FnCallStorage<'tcx> {
    pub(super) fn new(def_id: DefId) -> FnCallStorage<'tcx> {
        Self {
            def_id,
            fn_calls: vec![],
            unhandled: vec![],
        }
    }

    pub(super) fn add_call(&mut self, new_call: FnCallInfo<'tcx>) {
        self.fn_calls.push(new_call);
    }

    pub(super) fn add_unhandled(&mut self, new_unhandled: Terminator<'tcx>) {
        self.unhandled.push(new_unhandled);
    }

    // TODO: this is no longer valid, think about handling recursive call chains.
    pub(super) fn encountered_def_id(&self, def_id: DefId) -> bool {
        self.fn_calls.iter().any(|fn_call_info| {
            let fn_call_info_def_id = match fn_call_info {
                FnCallInfo::WithBody { def_id, .. } => def_id,
                FnCallInfo::WithoutBody { def_id, .. } => def_id,
            };
            *fn_call_info_def_id == def_id
        })
    }

    pub fn print_passing(&self, tcx: TyCtxt<'tcx>) {
        for fn_call in self.fn_calls.iter() {
            if self.check_fn_call_purity(fn_call) {
                println!("--> Passing function call: {:#?}", fn_call);
                match fn_call {
                    FnCallInfo::WithBody { body_span, .. } => {
                        let body_snippet =
                            tcx.sess.source_map().span_to_snippet(*body_span).unwrap();
                        println!("Body snippet: {:?}", body_snippet);
                    }
                    FnCallInfo::WithoutBody { .. } => (),
                }
            }
        }
    }

    pub fn print_failing(&self, tcx: TyCtxt<'tcx>) {
        for fn_call in self.fn_calls.iter() {
            if !self.check_fn_call_purity(fn_call) {
                println!("--> Failing function call: {:#?}", fn_call);
                match fn_call {
                    FnCallInfo::WithBody { body_span, .. } => {
                        let body_snippet =
                            tcx.sess.source_map().span_to_snippet(*body_span).unwrap();
                        println!("Body snippet: {:?}", body_snippet);
                    }
                    FnCallInfo::WithoutBody { .. } => (),
                }
            }
        }
    }

    pub fn print_unhandled(&self) {
        for unhandled_terminator in self.unhandled.iter() {
            println!("--> Unhandled terminator: {:#?}", unhandled_terminator);
        }
    }

    fn check_fn_call_purity(&self, fn_call: &FnCallInfo) -> bool {
        let allowed_libs = vec![
            Regex::new(r"core\[\w*\]::intrinsics").unwrap(),
            Regex::new(r"core\[\w*\]::panicking").unwrap(),
        ];
        match fn_call {
            FnCallInfo::WithBody {
                def_id,
                raw_ptr_deref,
                ..
            } => {
                let def_path_str = format!("{:?}", def_id);
                !raw_ptr_deref || (allowed_libs.iter().any(|lib| lib.is_match(&def_path_str)))
            }
            FnCallInfo::WithoutBody { def_id, .. } => {
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

    pub fn dump(&self) -> PurityAnalysisResult<'tcx> {
        let (passing_calls, failing_calls) = self
            .fn_calls
            .clone()
            .into_iter()
            .partition(|fn_call| self.check_fn_call_purity(fn_call));
        PurityAnalysisResult::new(
            self.def_id,
            self.check_purity(),
            passing_calls,
            failing_calls,
            self.unhandled.clone(),
        )
    }
}
