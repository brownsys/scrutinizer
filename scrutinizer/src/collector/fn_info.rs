use rustc_hir::def_id::DefId;
use rustc_middle::ty;
use rustc_span::Span;
use serde::ser::{Serialize, SerializeStructVariant};
use std::collections::HashMap;

use super::normalized_place::NormalizedPlace;
use super::tracked_ty::TrackedTy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FnInfo<'tcx> {
    Regular {
        parent: ty::Instance<'tcx>,
        instance: ty::Instance<'tcx>,
        places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>,
        span: Span,
    },
    Extern {
        parent: ty::Instance<'tcx>,
        instance: ty::Instance<'tcx>,
        tracked_args: Vec<TrackedTy<'tcx>>,
    },
    Ambiguous {
        parent: ty::Instance<'tcx>,
        def_id: DefId,
        tracked_args: Vec<TrackedTy<'tcx>>,
    },
}

impl<'tcx> Serialize for FnInfo<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            FnInfo::Regular {
                ref parent,
                ref instance,
                ref places,
                ref span,
            } => {
                let mut tv = serializer.serialize_struct_variant("FnInfo", 0, "Regular", 4)?;
                tv.serialize_field("parent", format!("{:?}", parent).as_str())?;
                tv.serialize_field("instance", format!("{:?}", instance).as_str())?;
                tv.serialize_field("span", format!("{:?}", span).as_str())?;
                tv.serialize_field("places", &places)?;
                tv.end()
            }
            FnInfo::Extern {
                ref parent,
                ref instance,
                ref tracked_args,
            } => {
                let mut tv = serializer.serialize_struct_variant("FnInfo", 1, "Extern", 3)?;
                tv.serialize_field("parent", format!("{:?}", parent).as_str())?;
                tv.serialize_field("instance", format!("{:?}", instance).as_str())?;
                tv.serialize_field("tracked_args", &tracked_args)?;
                tv.end()
            }
            FnInfo::Ambiguous {
                ref parent,
                ref def_id,
                ref tracked_args,
                ..
            } => {
                let mut tv = serializer.serialize_struct_variant("Ambiguous", 2, "Ambiguous", 3)?;
                tv.serialize_field("parent", format!("{:?}", parent).as_str())?;
                tv.serialize_field("def_id", format!("{:?}", def_id).as_str())?;
                tv.serialize_field("tracked_args", &tracked_args)?;
                tv.end()
            }
        }
    }
}
