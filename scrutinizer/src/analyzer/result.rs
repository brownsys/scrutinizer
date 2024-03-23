use rustc_hir::def_id::DefId;
use serde::{ser::SerializeStruct, Serialize};

use super::{ClosureInfoStorage, FunctionInfo, ImportantLocals};

#[derive(Serialize)]
pub struct FunctionWithMetadata<'tcx> {
    pub function: FunctionInfo<'tcx>,
    pub important_locals: ImportantLocals,
    pub raw_pointer_deref: bool,
    pub whitelisted: bool,
    pub has_transmute: bool,
}

pub struct PurityAnalysisResult<'tcx> {
    def_id: DefId,
    annotated_pure: bool,
    status: bool,
    reason: String,
    passing: Vec<FunctionWithMetadata<'tcx>>,
    failing: Vec<FunctionWithMetadata<'tcx>>,
    closures: ClosureInfoStorage<'tcx>,
}

impl<'tcx> Serialize for PurityAnalysisResult<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("PurityAnalysisResult", 7)?;
        state.serialize_field("def_id", format!("{:?}", self.def_id).as_str())?;
        state.serialize_field("annotated_pure", &self.annotated_pure)?;
        state.serialize_field("status", &self.status)?;
        if !self.status {
            state.serialize_field("reason", &self.reason)?;
        }
        state.serialize_field("passing", &self.passing)?;
        state.serialize_field("failing", &self.failing)?;
        state.serialize_field("closures", &self.closures)?;
        state.end()
    }
}

impl<'tcx> PurityAnalysisResult<'tcx> {
    pub fn new(
        def_id: DefId,
        annotated_pure: bool,
        status: bool,
        reason: String,
        passing: Vec<FunctionWithMetadata<'tcx>>,
        failing: Vec<FunctionWithMetadata<'tcx>>,
        closures: ClosureInfoStorage<'tcx>,
    ) -> Self {
        Self {
            def_id,
            annotated_pure,
            status,
            reason,
            passing,
            failing,
            closures,
        }
    }

    pub fn is_inconsistent(&self) -> bool {
        self.annotated_pure != self.status
    }
}
