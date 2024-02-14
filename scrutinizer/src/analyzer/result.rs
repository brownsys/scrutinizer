use itertools::Itertools;
use rustc_hir::def_id::DefId;
use rustc_middle::mir::Terminator;
use serde::ser::{Serialize, SerializeStruct};

use crate::collector::{ClosureInfoStorage, FnInfo};

pub struct PurityAnalysisResult<'tcx> {
    def_id: DefId,
    annotated_pure: bool,
    status: bool,
    reason: String,
    passing: Vec<FnInfo<'tcx>>,
    failing: Vec<FnInfo<'tcx>>,
    closures: ClosureInfoStorage<'tcx>,
    unhandled: Vec<Terminator<'tcx>>,
}

impl<'tcx> Serialize for PurityAnalysisResult<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("PurityAnalysisResult", 5)?;
        state.serialize_field("def_id", format!("{:?}", self.def_id).as_str())?;
        state.serialize_field("annotated_pure", &self.annotated_pure)?;
        state.serialize_field("status", &self.status)?;
        if !self.status {
            state.serialize_field("reason", &self.reason)?;
        }
        state.serialize_field("passing", &self.passing)?;
        state.serialize_field("failing", &self.failing)?;
        state.serialize_field("closures", &self.closures)?;
        state.serialize_field(
            "unhandled",
            &self
                .unhandled
                .iter()
                .map(|terminator| format!("{:?}", terminator))
                .collect_vec(),
        )?;
        state.end()
    }
}

impl<'tcx> PurityAnalysisResult<'tcx> {
    pub fn new(
        def_id: DefId,
        annotated_pure: bool,
        status: bool,
        reason: String,
        passing: Vec<FnInfo<'tcx>>,
        failing: Vec<FnInfo<'tcx>>,
        closures: ClosureInfoStorage<'tcx>,
        unhandled: Vec<Terminator<'tcx>>,
    ) -> Self {
        Self {
            def_id,
            annotated_pure,
            status,
            reason,
            passing,
            failing,
            closures,
            unhandled,
        }
    }

    pub fn is_inconsistent(&self) -> bool {
        self.annotated_pure != self.status
    }
}
