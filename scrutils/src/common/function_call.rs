use itertools::Itertools;
use rustc_middle::mir::Operand;
use rustc_middle::ty;
use rustc_span::def_id::DefId;
use serde::ser::SerializeStruct;
use serde::Serialize;

#[derive(Clone, Debug, Hash, PartialEq)]
pub enum FunctionCall<'tcx> {
    WithBody {
        instance: ty::Instance<'tcx>,
        args: Vec<Operand<'tcx>>,
    },
    WithoutBody {
        def_id: DefId,
        args: Vec<Operand<'tcx>>,
    },
}

impl<'tcx> Eq for FunctionCall<'tcx> {}

impl<'tcx> FunctionCall<'tcx> {
    pub fn new_with_body(instance: ty::Instance<'tcx>, args: Vec<Operand<'tcx>>) -> Self {
        Self::WithBody { instance, args }
    }
    pub fn new_without_body(def_id: DefId, args: Vec<Operand<'tcx>>) -> Self {
        Self::WithoutBody { def_id, args }
    }
    pub fn args(&self) -> &Vec<Operand<'tcx>> {
        match self {
            Self::WithBody { args, .. } | Self::WithoutBody { args, .. } => &args,
        }
    }
    pub fn def_id(&self) -> DefId {
        match self {
            Self::WithBody { instance, .. } => instance.def_id(),
            Self::WithoutBody { def_id, .. } => def_id.to_owned(),
        }
    }
}

impl<'tcx> Serialize for FunctionCall<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("FunctionCall", 2)?;
        state.serialize_field("def_id", format!("{:?}", self.def_id()).as_str())?;
        state.serialize_field(
            "args",
            &self
                .args()
                .iter()
                .map(|arg| format!("{:?}", arg))
                .collect_vec(),
        )?;
        state.end()
    }
}
