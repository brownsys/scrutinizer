use rustc_middle::mir::visit::Visitor;
use rustc_middle::mir::{Body, Location, Terminator, TerminatorKind};
use rustc_middle::ty::{self, Ty,TyCtxt};

struct PPRCollector<'tcx> {
    tcx: TyCtxt<'tcx>,
    body: Body<'tcx>,
    pprs: Vec<Ty<'tcx>>,
}

pub trait CollectPPRs<'tcx> {
    fn collect_pprs(&self, tcx: TyCtxt<'tcx>) -> Vec<Ty<'tcx>>;
}

impl<'tcx> CollectPPRs<'tcx> for Body<'tcx> {
    fn collect_pprs(&self, tcx: TyCtxt<'tcx>) -> Vec<Ty<'tcx>> {
        let mut ppr_collector = PPRCollector {
            tcx,
            body: self.to_owned(),
            pprs: vec![],
        };
        ppr_collector.visit_body(self);
        ppr_collector.pprs
    }
}

impl<'tcx> Visitor<'tcx> for PPRCollector<'tcx> {
    fn visit_terminator(&mut self, terminator: &Terminator<'tcx>, _: Location) {
        if let TerminatorKind::Call { func, args, .. } = terminator.kind.to_owned() {
            let func_ty = func.ty(&self.body, self.tcx);
            if let ty::TyKind::FnDef(def_id, ..) = func_ty.kind() {
                let ppr_str = self.tcx.def_path_str(def_id.to_owned());
                if ppr_str == "alohomora::pure::PrivacyPureRegion::<F>::new" {
                    self.pprs.push(args[0].ty(&self.body, self.tcx));
                }
            }
        }
    }
}
