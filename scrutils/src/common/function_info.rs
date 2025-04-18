use itertools::Itertools;
use rustc_hir::def_id::DefId;
use rustc_middle::mir::Body;
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;
use serde::ser::{Serialize, SerializeStruct};
use std::collections::{HashMap, HashSet};

use crate::common::function_call::FunctionCall;
use crate::common::normalized_place::NormalizedPlace;
use crate::common::tracked_ty::TrackedTy;

#[derive(Debug, Clone)]
pub enum FunctionInfo<'tcx> {
    WithBody {
        instance: ty::Instance<'tcx>,
        places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>,
        calls: HashSet<FunctionCall<'tcx>>,
        body: Body<'tcx>,
        span: Span,
        unhandled: HashSet<Ty<'tcx>>,
    },
    WithoutBody {
        def_id: DefId,
        tracked_args: Vec<TrackedTy<'tcx>>,
    },
}

impl<'tcx> FunctionInfo<'tcx> {
    pub fn new_with_body(
        instance: ty::Instance<'tcx>,
        places: HashMap<NormalizedPlace<'tcx>, TrackedTy<'tcx>>,
        calls: HashSet<FunctionCall<'tcx>>,
        body: Body<'tcx>,
        span: Span,
        unhandled: HashSet<Ty<'tcx>>,
    ) -> Self {
        FunctionInfo::WithBody {
            instance,
            places,
            calls,
            body,
            span,
            unhandled,
        }
    }

    pub fn new_without_body(def_id: DefId, tracked_args: Vec<TrackedTy<'tcx>>) -> Self {
        FunctionInfo::WithoutBody {
            def_id,
            tracked_args,
        }
    }

    pub fn def_id(&self) -> DefId {
        match self {
            FunctionInfo::WithBody { instance, .. } => instance.def_id(),
            FunctionInfo::WithoutBody { def_id, .. } => def_id.to_owned(),
        }
    }

    pub fn instance(&self) -> Option<ty::Instance<'tcx>> {
        match self {
            FunctionInfo::WithBody { instance, .. } => Some(instance.to_owned()),
            FunctionInfo::WithoutBody { .. } => None,
        }
    }

    pub fn calls(&self) -> Option<&HashSet<FunctionCall<'tcx>>> {
        match self {
            FunctionInfo::WithBody { calls, .. } => Some(calls),
            _ => None,
        }
    }
}

impl<'tcx> Eq for FunctionInfo<'tcx> {}

impl<'tcx> PartialEq for FunctionInfo<'tcx> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::WithBody {
                    instance: l_instance,
                    places: l_places,
                    calls: l_calls,
                    span: l_span,
                    unhandled: l_unhandled,
                    ..
                },
                Self::WithBody {
                    instance: r_instance,
                    places: r_places,
                    calls: r_calls,
                    span: r_span,
                    unhandled: r_unhandled,
                    ..
                },
            ) => {
                l_instance == r_instance
                    && l_places == r_places
                    && l_calls == r_calls
                    && l_span == r_span
                    && l_unhandled == r_unhandled
            }
            (
                Self::WithoutBody {
                    def_id: l_def_id,
                    tracked_args: l_tracked_args,
                },
                Self::WithoutBody {
                    def_id: r_def_id,
                    tracked_args: r_tracked_args,
                },
            ) => l_def_id == r_def_id && l_tracked_args == r_tracked_args,
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
                ref instance,
                ref calls,
                ref span,
                ref unhandled,
                ..
            } => {
                let mut tv = serializer.serialize_struct("FunctionInfo", 5)?;
                tv.serialize_field("def_id", format!("{:?}", instance.def_id()).as_str())?;
                tv.serialize_field("calls", calls)?;
                tv.serialize_field("span", format!("{:?}", span).as_str())?;
                tv.serialize_field(
                    "unhandled",
                    &unhandled.iter().map(|ty| format!("{:?}", ty)).collect_vec(),
                )?;
                tv.serialize_field("has_body", &true)?;
                tv.end()
            }
            FunctionInfo::WithoutBody {
                ref def_id,
                ref tracked_args,
            } => {
                let mut tv = serializer.serialize_struct("FunctionInfo", 3)?;
                tv.serialize_field("def_id", format!("{:?}", def_id).as_str())?;
                tv.serialize_field("tracked_args", &tracked_args)?;
                tv.serialize_field("has_body", &false)?;
                tv.end()
            }
        }
    }
}
