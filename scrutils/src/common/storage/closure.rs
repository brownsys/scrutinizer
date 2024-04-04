use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::DefId;
use serde::{ser::SerializeMap, Serialize};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::common::{ClosureInfo, TrackedTy};

pub type ClosureInfoStorageRef<'tcx> = Rc<RefCell<ClosureInfoStorage<'tcx>>>;

#[derive(Clone, Debug)]
pub struct ClosureInfoStorage<'tcx> {
    closures: HashMap<DefId, ClosureInfo<'tcx>>,
}

impl<'tcx> ClosureInfoStorage<'tcx> {
    pub fn new() -> Self {
        Self {
            closures: HashMap::new(),
        }
    }

    pub fn get(&self, def_id: &DefId) -> Option<&ClosureInfo<'tcx>> {
        self.closures.get(&def_id)
    }

    // TODO: enforce that only closures are passed to this function.
    pub fn update_with(
        &mut self,
        closure_ty: Ty<'tcx>,
        outer_instance: &ty::Instance<'tcx>,
        upvars: Vec<TrackedTy<'tcx>>,
        tcx: TyCtxt<'tcx>,
    ) {
        if let ty::TyKind::Closure(closure_def_id, ..) = closure_ty.kind() {
            let resolved_closure_ty = outer_instance.subst_mir_and_normalize_erasing_regions(
                tcx,
                ty::ParamEnv::reveal_all(),
                closure_ty,
            );
            self.closures
                .entry(closure_def_id.to_owned())
                .and_modify(|closure_ref| {
                    if closure_ref.upvars.is_empty() {
                        closure_ref.upvars.extend(upvars.clone().into_iter())
                    } else if !upvars.is_empty() {
                        assert!(upvars.len() == closure_ref.upvars.len());
                        upvars.iter().zip(closure_ref.upvars.iter_mut()).for_each(
                            |(new_upvar, old_upvar)| {
                                old_upvar.join(new_upvar);
                            },
                        );
                    }
                })
                .or_insert(ClosureInfo {
                    upvars,
                    with_substs: resolved_closure_ty,
                });
        }
    }
}

impl<'tcx> Serialize for ClosureInfoStorage<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_map(Some(self.closures.len()))?;
        for (k, v) in &self.closures {
            state.serialize_entry(format!("{:?}", k).as_str(), format!("{:?}", v).as_str())?;
        }
        state.end()
    }
}
