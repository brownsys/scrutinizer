mod body_cache;
mod encoder;

pub use body_cache::{dump_mir_and_borrowck_facts, load_body_and_facts, BodyCache};

use rustc_hir::{def::DefKind, def_id::DefId};
use rustc_middle::{
    mir::Body,
    ty::{self, Instance, TyCtxt},
};

pub fn is_mir_available<'tcx>(def_id: DefId, tcx: TyCtxt<'tcx>) -> bool {
    load_body_and_facts(tcx, def_id).is_ok()
}

pub fn num_args_in_body<'tcx>(def_id: DefId, tcx: TyCtxt<'tcx>) -> usize {
    load_body_and_facts(tcx, def_id).unwrap().owned_body().arg_count
}

pub fn substituted_mir<'tcx>(instance: &Instance<'tcx>, tcx: TyCtxt<'tcx>) -> Body<'tcx> {
    let instance_body = match instance.def {
        ty::InstanceDef::Item(def) => {
            let def_kind = tcx.def_kind(def);
            match def_kind {
                DefKind::Const
                | DefKind::Static(..)
                | DefKind::AssocConst
                | DefKind::Ctor(..)
                | DefKind::AnonConst
                | DefKind::InlineConst => tcx.mir_for_ctfe(def).clone(),
                _ => {
                    let def_id = instance.def_id();
                    let cached_body = load_body_and_facts(tcx, def_id).unwrap();
                    tcx.erase_regions(cached_body.owned_body())
                }
            }
        }
        ty::InstanceDef::VTableShim(..)
        | ty::InstanceDef::ReifyShim(..)
        | ty::InstanceDef::Intrinsic(..)
        | ty::InstanceDef::FnPtrShim(..)
        | ty::InstanceDef::Virtual(..)
        | ty::InstanceDef::ClosureOnceShim { .. }
        | ty::InstanceDef::DropGlue(..)
        | ty::InstanceDef::CloneShim(..)
        | ty::InstanceDef::ThreadLocalShim(..)
        | ty::InstanceDef::FnPtrAddrShim(..) => tcx.mir_shims(instance.def).clone(),
    };
    instance.subst_mir_and_normalize_erasing_regions(
        tcx,
        ty::ParamEnv::reveal_all(),
        ty::EarlyBinder::bind(instance_body),
    )
}
