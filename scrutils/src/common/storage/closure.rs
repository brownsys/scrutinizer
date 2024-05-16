use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::DefId;
use serde::{ser::SerializeMap, Serialize};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::common::{ClosureInfo, TrackedTy};

#[derive(Clone)]
pub struct ClosureInfoStorage<'tcx> {
    storage: Rc<RefCell<ClosureInfoStorageInternal<'tcx>>>,
}

#[derive(Clone, Debug)]
struct ClosureInfoStorageInternal<'tcx> {
    closures: HashMap<DefId, ClosureInfo<'tcx>>,
}

impl<'tcx> ClosureInfoStorage<'tcx> {
    pub fn new() -> Self {
        Self {
            storage: Rc::new(RefCell::new(ClosureInfoStorageInternal {
                closures: HashMap::new(),
            })),
        }
    }

    pub fn get(&self, def_id: &DefId) -> Option<ClosureInfo<'tcx>> {
        let storage = self.storage.borrow();
        storage
            .closures
            .get(&def_id)
            .and_then(|closure| Some(closure.to_owned()))
    }

    // TODO: enforce that only closures are passed to this function.
    pub fn update_with(
        &self,
        closure_ty: Ty<'tcx>,
        outer_instance: &ty::Instance<'tcx>,
        upvars: Vec<TrackedTy<'tcx>>,
        tcx: TyCtxt<'tcx>,
    ) {
        let mut storage = self.storage.borrow_mut();
        if let ty::TyKind::Closure(closure_def_id, ..) = closure_ty.kind() {
            let resolved_closure_ty = outer_instance.subst_mir_and_normalize_erasing_regions(
                tcx,
                ty::ParamEnv::reveal_all(),
                closure_ty,
            );
            storage
                .closures
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
        let storage = self.storage.borrow();
        let mut state = serializer.serialize_map(Some(storage.closures.len()))?;
        for (k, v) in &storage.closures {
            state.serialize_entry(format!("{:?}", k).as_str(), format!("{:?}", v).as_str())?;
        }
        state.end()
    }
}
