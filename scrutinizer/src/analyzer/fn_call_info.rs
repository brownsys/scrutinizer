use rustc_hir::def_id::DefId;
use rustc_span::Span;
use serde::ser::{Serialize, SerializeStructVariant};

use super::tracked_ty::TrackedTy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FnCallInfo<'tcx> {
    WithBody {
        from: DefId,
        def_id: DefId,
        tracked_args: Vec<TrackedTy<'tcx>>,
        span: Span,
        raw_ptr_deref: bool,
    },
    WithoutBody {
        from: DefId,
        def_id: DefId,
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
                ref from,
                ref span,
                ref tracked_args,
                ref raw_ptr_deref,
            } => {
                let mut tv = serializer.serialize_struct_variant("FnCallInfo", 0, "WithBody", 5)?;
                tv.serialize_field("from", format!("{:?}", from).as_str())?;
                tv.serialize_field("def_id", format!("{:?}", def_id).as_str())?;
                tv.serialize_field("tracked_args", &tracked_args)?;
                tv.serialize_field("span", format!("{:?}", span).as_str())?;
                tv.serialize_field("raw_ptr_deref", &raw_ptr_deref)?;
                tv.end()
            }
            FnCallInfo::WithoutBody {
                ref def_id,
                ref from,
            } => {
                let mut tv =
                    serializer.serialize_struct_variant("FnCallInfo", 1, "WithoutBody", 3)?;
                tv.serialize_field("from", format!("{:?}", from).as_str())?;
                tv.serialize_field("def_id", format!("{:?}", def_id).as_str())?;
                tv.end()
            }
        }
    }
}
