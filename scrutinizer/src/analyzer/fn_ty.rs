use serde::ser::{Serialize, SerializeStructVariant};

use rustc_hir::def_id::DefId;
use rustc_middle::mir::Local;
use rustc_middle::ty;
use rustc_span::Span;

use super::arg_ty::ArgTy;

pub struct FnData<'tcx> {
    pub arg_tys: Vec<ArgTy<'tcx>>,
    pub instance: ty::Instance<'tcx>,
    pub important_locals: Vec<Local>,
}

#[derive(Debug, Clone)]
pub enum FnCallInfo<'tcx> {
    WithBody {
        def_id: DefId,
        arg_tys: Vec<ArgTy<'tcx>>,
        call_span: Span,
        body_span: Span,
        // Whether body contains raw pointer dereference.
        raw_ptr_deref: bool,
    },
    WithoutBody {
        def_id: DefId,
        arg_tys: Vec<ArgTy<'tcx>>,
        call_span: Span,
    },
}

impl<'tcx> Serialize for FnCallInfo<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            FnCallInfo::WithBody {
                ref def_id,
                ref arg_tys,
                ref call_span,
                ref body_span,
                ref raw_ptr_deref,
            } => {
                let mut tv = serializer.serialize_struct_variant("FnCallInfo", 0, "WithBody", 5)?;
                tv.serialize_field("def_id", format!("{:?}", def_id).as_str())?;
                tv.serialize_field("arg_tys", &arg_tys)?;
                tv.serialize_field("call_span", format!("{:?}", call_span).as_str())?;
                tv.serialize_field("body_span", format!("{:?}", body_span).as_str())?;
                tv.serialize_field("raw_ptr_deref", &raw_ptr_deref)?;
                tv.end()
            }
            FnCallInfo::WithoutBody {
                ref def_id,
                ref arg_tys,
                ref call_span,
            } => {
                let mut tv =
                    serializer.serialize_struct_variant("FnCallInfo", 1, "WithoutBody", 3)?;
                tv.serialize_field("def_id", format!("{:?}", def_id).as_str())?;
                tv.serialize_field("arg_tys", &arg_tys)?;
                tv.serialize_field("call_span", format!("{:?}", call_span).as_str())?;
                tv.end()
            }
        }
    }
}
