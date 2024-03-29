use rustc_hir::def_id::DefId;
use serde::{ser::SerializeStruct, Serialize};

use crate::common::storage::ClosureInfoStorage;
use crate::common::FunctionInfo;
use crate::important::ImportantLocals;

#[derive(Serialize)]
pub struct FunctionWithMetadata<'tcx> {
    function: FunctionInfo<'tcx>,
    important_locals: ImportantLocals,
    raw_pointer_deref: bool,
    whitelisted: bool,
    has_transmute: bool,
}

impl<'tcx> FunctionWithMetadata<'tcx> {
    pub fn new(
        function: FunctionInfo<'tcx>,
        important_locals: ImportantLocals,
        raw_pointer_deref: bool,
        whitelisted: bool,
        has_transmute: bool,
    ) -> Self {
        FunctionWithMetadata {
            function,
            important_locals,
            raw_pointer_deref,
            whitelisted,
            has_transmute,
        }
    }
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

    pub fn error(def_id: DefId, reason: String, annotated_pure: bool) -> Self {
        Self::new(
            def_id,
            annotated_pure,
            false,
            reason,
            vec![],
            vec![],
            ClosureInfoStorage::new(),
        )
    }

    pub fn is_inconsistent(&self) -> bool {
        self.annotated_pure != self.status
    }
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
