use rustc_hir::def_id::DefId;
use rustc_middle::ty::Ty;
use rustc_span::Span;

#[derive(Debug, Clone)]
pub enum ArgTy<'tcx> {
    Simple(Ty<'tcx>),
    WithClosureInfluences(Ty<'tcx>, Vec<Ty<'tcx>>),
}

#[derive(Debug, Clone)]
pub enum FnCallInfo<'tcx> {
    WithBody {
        def_id: DefId,
        arg_tys: Vec<ArgTy<'tcx>>,
        call_span: Span,
        body_span: Span,
        // Whether body contains raw pointer dereference.
        raw_ptr_deref: bool,
    },
    WithoutBody {
        def_id: DefId,
        arg_tys: Vec<ArgTy<'tcx>>,
        call_span: Span,
    },
}
