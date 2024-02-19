use std::collections::HashSet;

use crate::vartrack::compute_dependent_locals;

use flowistry::indexed::impls::LocationOrArg;
use flowistry::infoflow::Direction;
use itertools::Itertools;
use rustc_hir::def_id::DefId;
use rustc_middle::mir::{Local, Operand, Place};
use rustc_middle::ty::TyCtxt;
use rustc_utils::PlaceExt;
use serde::ser::SerializeSeq;
use serde::Serialize;

// Newtype for a vec of locals.
#[derive(Clone, Debug)]
pub struct ImportantLocals {
    locals: HashSet<Local>,
}

impl Serialize for ImportantLocals {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.locals.len()))?;
        for element in self.locals.iter() {
            seq.serialize_element(&element.as_usize())?;
        }
        seq.end()
    }
}

impl ImportantLocals {
    pub fn new(locals: HashSet<Local>) -> Self {
        Self { locals }
    }

    pub fn empty() -> Self {
        Self {
            locals: HashSet::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.locals.is_empty()
    }

    pub fn join(&mut self, other: &Self) -> bool {
        other
            .locals
            .iter()
            .fold(false, |acc, elt| self.locals.insert(elt.to_owned()) || acc)
    }

    // Construct new important locals which influence args.
    pub fn transition(
        &self,
        args_from_caller: &Vec<Operand>,
        callee_def_id: DefId,
        tcx: TyCtxt,
    ) -> Self {
        // Constructors are final and have no important locals.
        if tcx.is_constructor(callee_def_id) {
            return ImportantLocals::new(HashSet::new());
        }
        // Construct targets of the arguments.
        let important_args_to_callee = args_from_caller
            .iter()
            .enumerate()
            .filter_map(|(i, arg)| {
                arg.place()
                    .and_then(|place| place.as_local())
                    .and_then(|local| {
                        if self.locals.contains(&local) {
                            // Need to add 1 because arguments' locals start with 1.
                            Some(Local::from_usize(i + 1))
                        } else {
                            None
                        }
                    })
            })
            .collect_vec();
        if tcx.is_mir_available(callee_def_id) {
            let new_important_arg_targets = vec![important_args_to_callee
                .into_iter()
                .map(|arg_local| {
                    let arg_place = Place::make(arg_local, &[], tcx);
                    (arg_place, LocationOrArg::Arg(arg_local))
                })
                .collect()];
            // Compute new dependencies for all important args.
            ImportantLocals::new(HashSet::from_iter(
                compute_dependent_locals(
                    tcx,
                    callee_def_id,
                    new_important_arg_targets,
                    Direction::Forward,
                )
                .into_iter(),
            ))
        } else {
            ImportantLocals::new(HashSet::from_iter(important_args_to_callee.into_iter()))
        }
    }
}
