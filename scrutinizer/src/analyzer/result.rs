use super::types::FnCallInfo;

use serde::ser::{Serialize, SerializeStruct};

use rustc_hir::def_id::DefId;
use rustc_middle::mir::Terminator;

pub struct PurityAnalysisResult<'tcx> {
    def_id: DefId,
    status: bool,
    passing: Vec<FnCallInfo<'tcx>>,
    failing: Vec<FnCallInfo<'tcx>>,
    unhandled: Vec<Terminator<'tcx>>,
}

impl<'tcx> Serialize for PurityAnalysisResult<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("PurityAnalysisResult", 5)?;
        state.serialize_field("def_id", format!("{:?}", self.def_id).as_str())?;
        state.serialize_field("status", &self.status)?;
        state.serialize_field("passing", &self.passing)?;
        state.serialize_field("failing", &self.failing)?;
        state.serialize_field(
            "unhandled",
            &self
                .unhandled
                .iter()
                .map(|terminator| format!("{:?}", terminator))
                .collect::<Vec<_>>(),
        )?;
        state.end()
    }
}

impl<'tcx> PurityAnalysisResult<'tcx> {
    pub fn new(
        def_id: DefId,
        status: bool,
        passing: Vec<FnCallInfo<'tcx>>,
        failing: Vec<FnCallInfo<'tcx>>,
        unhandled: Vec<Terminator<'tcx>>,
    ) -> Self {
        Self {
            def_id,
            status,
            passing,
            failing,
            unhandled,
        }
    }
}
