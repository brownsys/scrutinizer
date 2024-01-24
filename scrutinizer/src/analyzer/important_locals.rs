use crate::vartrack::compute_dependent_locals;

use flowistry::indexed::impls::LocationOrArg;
use flowistry::infoflow::Direction;
use itertools::Itertools;
use rustc_hir::def_id::DefId;
use rustc_middle::mir::{Local, Operand, Place};
use rustc_middle::ty::TyCtxt;
use rustc_utils::PlaceExt;
use std::ops::Deref;

// Newtype for a vec of locals.
#[derive(Debug)]
pub struct ImportantLocals(Vec<Local>);

impl Deref for ImportantLocals {
    type Target = Vec<Local>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ImportantLocals {
    pub fn new(important_locals: Vec<Local>) -> Self {
        Self(important_locals)
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
            return ImportantLocals::new(vec![]);
        }
        // Construct targets of the arguments.
        let new_important_arg_targets = {
            let important_args_to_callee = args_from_caller
                .iter()
                .enumerate()
                .filter_map(|(i, arg)| {
                    arg.place()
                        .and_then(|place| place.as_local())
                        .and_then(|local| {
                            if self.contains(&local) {
                                // Need to add 1 because arguments' locals start with 1.
                                Some(i + 1)
                            } else {
                                None
                            }
                        })
                })
                .collect_vec();
            vec![important_args_to_callee
                .iter()
                .map(|arg| {
                    let arg_local = Local::from_usize(*arg);
                    let arg_place = Place::make(arg_local, &[], tcx);
                    (arg_place, LocationOrArg::Arg(arg_local))
                })
                .collect()]
        };
        // Compute new dependencies for all important args.
        ImportantLocals::new(compute_dependent_locals(
            tcx,
            callee_def_id,
            new_important_arg_targets,
            Direction::Forward,
        ))
    }
}
