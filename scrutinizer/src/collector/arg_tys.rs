use itertools::Itertools;

use super::TrackedTy;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgTys<'tcx> {
    arg_tys: Vec<TrackedTy<'tcx>>,
}

impl<'tcx> ArgTys<'tcx> {
    pub fn new(arg_tys: Vec<TrackedTy<'tcx>>) -> Self {
        ArgTys { arg_tys }
    }

    pub fn as_closure(&self) -> Self {
        let mut closure_args = vec![self.arg_tys[0].clone()];
        closure_args.extend(self.arg_tys[1].spread_tuple().into_iter());
        ArgTys {
            arg_tys: closure_args,
        }
    }

    pub fn as_vec(&self) -> &Vec<TrackedTy<'tcx>> {
        &self.arg_tys
    }

    pub fn merge(inferred_args: ArgTys<'tcx>, provided_args: ArgTys<'tcx>) -> ArgTys<'tcx> {
        assert!(inferred_args.arg_tys.len() == provided_args.arg_tys.len());
        let merged_arg_tys = inferred_args
            .arg_tys
            .into_iter()
            .zip(provided_args.arg_tys.into_iter())
            .map(|(inferred, provided)| match provided {
                TrackedTy::Present(..) => provided,
                TrackedTy::Erased(..) => inferred,
            })
            .collect_vec();
        ArgTys {
            arg_tys: merged_arg_tys,
        }
    }
}
