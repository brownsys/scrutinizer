use rustc_hir::def_id::DefId;
use rustc_infer::infer::{DefiningAnchor, TyCtxtInferExt};

use rustc_middle::ty::{self, TyCtxt};
use rustc_trait_selection::traits::{Obligation, ObligationCause, SelectionContext};

use super::ty_ext::TyExt;

pub trait SubstsExt<'tcx> {
    fn maybe_invalid_for(
        &self,
        def_id: DefId,
        param_env: ty::ParamEnv<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> bool;
}

impl<'tcx> SubstsExt<'tcx> for ty::SubstsRef<'tcx> {
    fn maybe_invalid_for(
        &self,
        def_id: DefId,
        param_env: ty::ParamEnv<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> bool {
        let not_ty = self.iter().any(|subst| match subst.unpack() {
            ty::GenericArgKind::Type(_) => false,
            _ => true,
        });
        // Check that every substitution contains no params itself.
        let contains_erased = self.iter().all(|subst| {
            subst.walk().any(|ty| match ty.unpack() {
                ty::GenericArgKind::Type(ty) => ty.contains_erased(),
                _ => false,
            })
        });
        if not_ty || contains_erased {
            true
        } else {
            let not_selectable = {
                if let Some(trait_def_id) = tcx.trait_of_item(def_id) {
                    let trait_ref = ty::TraitRef::from_method(
                        tcx,
                        trait_def_id,
                        tcx.normalize_erasing_regions(param_env, self),
                    );
                    let infcx = tcx
                        .infer_ctxt()
                        .ignoring_regions()
                        .with_opaque_type_inference(DefiningAnchor::Bubble)
                        .build();
                    let mut selcx = SelectionContext::new(&infcx);
                    let obligation_cause = ObligationCause::dummy();
                    let obligation = Obligation::new(
                        tcx,
                        obligation_cause,
                        param_env,
                        ty::Binder::dummy(trait_ref),
                    );
                    selcx.select(&obligation).is_err()
                } else {
                    false
                }
            };
            not_selectable
        }
    }
}
