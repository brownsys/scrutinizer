use serde::ser::{Serialize, SerializeStructVariant};

use std::collections::HashMap;

use rustc_hir::def_id::DefId;
use rustc_middle::mir::Local;
use rustc_span::Span;

use super::arg_ty::RefinedTy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FnCallInfo<'tcx> {
    WithBody {
        def_id: DefId,
        from: DefId,
        span: Span,
        refined_tys: HashMap<Local, RefinedTy<'tcx>>,
        raw_ptr_deref: bool,
    },
    WithoutBody {
        def_id: DefId,
        from: DefId,
        arg_tys: Vec<RefinedTy<'tcx>>,
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
                ref refined_tys,
                ref raw_ptr_deref,
            } => {
                let refined_tys: HashMap<usize, &RefinedTy> =
                    HashMap::from_iter(refined_tys.iter().map(|(k, v)| (k.as_usize(), v)));
                let mut tv = serializer.serialize_struct_variant("FnCallInfo", 0, "WithBody", 5)?;
                tv.serialize_field("from", format!("{:?}", from).as_str())?;
                tv.serialize_field("def_id", format!("{:?}", def_id).as_str())?;
                tv.serialize_field("refined_tys", &refined_tys)?;
                tv.serialize_field("span", format!("{:?}", span).as_str())?;
                tv.serialize_field("raw_ptr_deref", &raw_ptr_deref)?;
                tv.end()
            }
            FnCallInfo::WithoutBody {
                ref def_id,
                ref from,
                ref arg_tys,
            } => {
                let mut tv =
                    serializer.serialize_struct_variant("FnCallInfo", 1, "WithoutBody", 3)?;
                tv.serialize_field("from", format!("{:?}", from).as_str())?;
                tv.serialize_field("def_id", format!("{:?}", def_id).as_str())?;
                tv.serialize_field("arg_tys", &arg_tys)?;
                tv.end()
            }
        }
    }
}
