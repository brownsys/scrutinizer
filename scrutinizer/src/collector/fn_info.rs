use rustc_hir::def_id::DefId;
use rustc_middle::mir::Body;
use rustc_middle::ty;
use rustc_span::Span;
use serde::ser::{Serialize, SerializeStructVariant};
use std::collections::{HashMap, HashSet};

use super::normalized_place::NormalizedPlace;
use super::tracked_ty::TrackedTy;
use super::type_tracker::Call;

#[derive(Debug, Clone)]
pub enum FnInfo<'tcx> {
    Regular {
        parent: ty::Instance<'tcx>,
        instance: ty::Instance<'tcx>,
        places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>,
        calls: HashSet<Call<'tcx>>,
        body: Body<'tcx>,
        span: Span,
    },
    Extern {
        parent: ty::Instance<'tcx>,
        def_id: DefId,
        tracked_args: Vec<TrackedTy<'tcx>>,
    },
    Ambiguous {
        parent: ty::Instance<'tcx>,
        def_id: DefId,
        tracked_args: Vec<TrackedTy<'tcx>>,
    },
}

impl<'tcx> FnInfo<'tcx> {
    pub fn def_id(&self) -> DefId {
        match self {
            FnInfo::Regular { instance, .. } => instance.def_id(),
            FnInfo::Extern { def_id, .. } | FnInfo::Ambiguous { def_id, .. } => def_id.to_owned(),
        }
    }

    pub fn calls(&self) -> Option<&HashSet<Call<'tcx>>> {
        match self {
            FnInfo::Regular { calls, .. } => Some(calls),
            _ => None,
        }
    }
}

impl<'tcx> Eq for FnInfo<'tcx> {}

impl<'tcx> PartialEq for FnInfo<'tcx> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Regular {
                    parent: l_parent,
                    instance: l_instance,
                    places: l_places,
                    calls: l_calls,
                    span: l_span,
                    ..
                },
                Self::Regular {
                    parent: r_parent,
                    instance: r_instance,
                    places: r_places,
                    calls: r_calls,
                    span: r_span,
                    ..
                },
            ) => {
                l_parent == r_parent
                    && l_instance == r_instance
                    && l_places == r_places
                    && l_calls == r_calls
                    && l_span == r_span
            }
            (
                Self::Extern {
                    parent: l_parent,
                    def_id: l_def_id,
                    tracked_args: l_tracked_args,
                },
                Self::Extern {
                    parent: r_parent,
                    def_id: r_def_id,
                    tracked_args: r_tracked_args,
                },
            ) => l_parent == r_parent && l_def_id == r_def_id && l_tracked_args == r_tracked_args,
            (
                Self::Ambiguous {
                    parent: l_parent,
                    def_id: l_def_id,
                    tracked_args: l_tracked_args,
                },
                Self::Ambiguous {
                    parent: r_parent,
                    def_id: r_def_id,
                    tracked_args: r_tracked_args,
                },
            ) => l_parent == r_parent && l_def_id == r_def_id && l_tracked_args == r_tracked_args,
            _ => false,
        }
    }
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
                // ref places,
                ref calls,
                ref span,
                ..
            } => {
                let mut tv = serializer.serialize_struct_variant("FnInfo", 0, "Regular", 4)?;
                tv.serialize_field("parent", format!("{:?}", parent).as_str())?;
                tv.serialize_field("instance", format!("{:?}", instance).as_str())?;
                // tv.serialize_field("places", &places)?;
                tv.serialize_field("calls", calls)?;
                tv.serialize_field("span", format!("{:?}", span).as_str())?;
                tv.end()
            }
            FnInfo::Extern {
                ref parent,
                ref def_id,
                ref tracked_args,
            } => {
                let mut tv = serializer.serialize_struct_variant("FnInfo", 1, "Extern", 3)?;
                tv.serialize_field("parent", format!("{:?}", parent).as_str())?;
                tv.serialize_field("def_id", format!("{:?}", def_id).as_str())?;
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
