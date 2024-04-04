use rustc_hir::ItemKind;
use rustc_middle::ty::{self, TyCtxt};

pub fn select_functions<'tcx>(tcx: TyCtxt<'tcx>) -> Vec<(ty::Instance<'tcx>, bool)> {
    tcx.hir()
        .items()
        .filter_map(|item_id| {
            let hir = tcx.hir();
            let item = hir.item(item_id);
            let def_id = item.owner_id.to_def_id();
            let annotated_pure = tcx
                .get_attr(def_id, rustc_span::symbol::Symbol::intern("doc"))
                .and_then(|attr| attr.doc_str())
                .and_then(|symbol| Some(symbol == rustc_span::symbol::Symbol::intern("pure")))
                .unwrap_or(false);

            if let ItemKind::Fn(..) = &item.kind {
                // Sanity check for generics.
                let has_generics = ty::InternalSubsts::identity_for_item(tcx, def_id)
                    .iter()
                    .any(|param| match param.unpack() {
                        ty::GenericArgKind::Lifetime(..) => false,
                        ty::GenericArgKind::Type(..) | ty::GenericArgKind::Const(..) => true,
                    });

                if has_generics {
                    return None;
                }

                // Retrieve the instance, as we know it exists.
                Some((ty::Instance::mono(tcx, def_id), annotated_pure))
            } else {
                None
            }
        })
        .collect()
}
