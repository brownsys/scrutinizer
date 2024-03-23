use itertools::Itertools;
use rustc_hir::def_id::DefId;
use rustc_middle::mir::Body;
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;
use serde::ser::{Serialize, SerializeStructVariant};
use std::collections::{HashMap, HashSet};

use super::function_call::FunctionCall;
use super::normalized_place::NormalizedPlace;
use super::tracked_ty::TrackedTy;

#[derive(Debug, Clone)]
pub enum FunctionInfo<'tcx> {
    WithBody {
        parent: ty::Instance<'tcx>,
        instance: ty::Instance<'tcx>,
        places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>,
        calls: HashSet<FunctionCall<'tcx>>,
        body: Body<'tcx>,
        span: Span,
        unhandled: HashSet<Ty<'tcx>>,
    },
    WithoutBody {
        parent: ty::Instance<'tcx>,
        def_id: DefId,
        tracked_args: Vec<TrackedTy<'tcx>>,
    },
}

impl<'tcx> FunctionInfo<'tcx> {
    pub fn def_id(&self) -> DefId {
        match self {
            FunctionInfo::WithBody { instance, .. } => instance.def_id(),
            FunctionInfo::WithoutBody { def_id, .. } => def_id.to_owned(),
        }
    }

    pub fn calls(&self) -> Option<&HashSet<FunctionCall<'tcx>>> {
        match self {
            FunctionInfo::WithBody { calls, .. } => Some(calls),
            _ => None,
        }
    }

    pub fn has_body(&self) -> bool {
        match self {
            FunctionInfo::WithBody { .. } => true,
            FunctionInfo::WithoutBody { .. } => false,
        }
    }
}

impl<'tcx> Eq for FunctionInfo<'tcx> {}

impl<'tcx> PartialEq for FunctionInfo<'tcx> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::WithBody {
                    parent: l_parent,
                    instance: l_instance,
                    places: l_places,
                    calls: l_calls,
                    span: l_span,
                    unhandled: l_unhandled,
                    ..
                },
                Self::WithBody {
                    parent: r_parent,
                    instance: r_instance,
                    places: r_places,
                    calls: r_calls,
                    span: r_span,
                    unhandled: r_unhandled,
                    ..
                },
            ) => {
                l_parent == r_parent
                    && l_instance == r_instance
                    && l_places == r_places
                    && l_calls == r_calls
                    && l_span == r_span
                    && l_unhandled == r_unhandled
            }
            (
                Self::WithoutBody {
                    parent: l_parent,
                    def_id: l_def_id,
                    tracked_args: l_tracked_args,
                },
                Self::WithoutBody {
                    parent: r_parent,
                    def_id: r_def_id,
                    tracked_args: r_tracked_args,
                },
            ) => l_parent == r_parent && l_def_id == r_def_id && l_tracked_args == r_tracked_args,
            _ => false,
        }
    }
}

impl<'tcx> Serialize for FunctionInfo<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            FunctionInfo::WithBody {
                ref parent,
                ref instance,
                ref places,
                ref calls,
                ref span,
                ref unhandled,
                ..
            } => {
                let mut tv =
                    serializer.serialize_struct_variant("FunctionInfo", 0, "WithBody", 6)?;
                tv.serialize_field("parent", format!("{:?}", parent).as_str())?;
                tv.serialize_field("instance", format!("{:?}", instance).as_str())?;
                // tv.serialize_field("places", &places)?;
                tv.serialize_field("calls", calls)?;
                tv.serialize_field("span", format!("{:?}", span).as_str())?;
                tv.serialize_field(
                    "unhandled",
                    &unhandled.iter().map(|ty| format!("{:?}", ty)).collect_vec(),
                )?;
                tv.end()
            }
            FunctionInfo::WithoutBody {
                ref parent,
                ref def_id,
                ref tracked_args,
            } => {
                let mut tv =
                    serializer.serialize_struct_variant("FunctionInfo", 1, "WithoutBody", 3)?;
                tv.serialize_field("parent", format!("{:?}", parent).as_str())?;
                tv.serialize_field("def_id", format!("{:?}", def_id).as_str())?;
                tv.serialize_field("tracked_args", &tracked_args)?;
                tv.end()
            }
        }
    }
}
