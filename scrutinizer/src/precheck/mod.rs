use itertools::Itertools;
use rustc_middle::mir::Mutability;
use rustc_middle::ty::{self, TyCtxt};

use crate::common::TrackedTy;

pub fn precheck<'tcx>(instance: ty::Instance<'tcx>, tcx: TyCtxt<'tcx>) -> Result<(), String> {
    let body = instance.subst_mir_and_normalize_erasing_regions(
        tcx,
        ty::ParamEnv::reveal_all(),
        tcx.instance_mir(instance.def).to_owned(),
    );

    // Create initial argument types.
    let arg_tys = (1..=body.arg_count)
        .map(|local| {
            let arg_ty = body.local_decls[local.into()].ty;
            TrackedTy::from_ty(arg_ty)
        })
        .collect_vec();

    // Check for unresolved generic types or consts.
    let contains_unresolved_generics = arg_tys.iter().any(|arg| match arg {
        TrackedTy::Present(..) => false,
        TrackedTy::Erased(..) => true,
    });

    if contains_unresolved_generics {
        return Err(String::from("erased args detected"));
    }

    // Check for mutable arguments.
    let contains_mutable_args = arg_tys.iter().any(|arg| {
        let main_ty = match arg {
            TrackedTy::Present(ty) => ty,
            TrackedTy::Erased(..) => unreachable!(),
        };
        if let ty::TyKind::Ref(.., mutbl) = main_ty.kind() {
            return mutbl.to_owned() == Mutability::Mut;
        } else {
            return false;
        }
    });

    if contains_mutable_args {
        return Err(String::from("mutable arguments detected"));
    }

    Ok(())
}
