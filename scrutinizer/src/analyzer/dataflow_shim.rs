use rustc_data_structures::work_queue::WorkQueue;
use rustc_index::vec::IndexVec;
use rustc_middle::mir::{traversal, BasicBlock, Body};
use rustc_middle::ty::TyCtxt;
use rustc_mir_dataflow::{Analysis, AnalysisDomain, Direction, Engine, JoinSemiLattice, Results};
use std::mem::transmute;

use super::type_tracker::TypeTracker;

#[allow(dead_code)]
pub struct EngineShim<'a, 'tcx, A>
where
    A: Analysis<'tcx>,
{
    tcx: TyCtxt<'tcx>,
    body: &'a Body<'tcx>,
    entry_sets: IndexVec<BasicBlock, A::Domain>,
    pass_name: Option<&'static str>,
    analysis: A,
    apply_trans_for_block: Option<Box<dyn Fn(BasicBlock, &mut A::Domain)>>,
}

#[allow(dead_code)]
pub struct ResultsShim<'tcx, A>
where
    A: Analysis<'tcx>,
{
    analysis: A,
    entry_sets: IndexVec<BasicBlock, A::Domain>,
}

pub fn iterate_to_fixpoint<'a, 'tcx>(
    engine: Engine<'a, 'tcx, TypeTracker<'tcx>>,
) -> Results<'tcx, TypeTracker<'tcx>> {
    let engine: EngineShim<'a, 'tcx, TypeTracker> = unsafe { transmute(engine) };
    let EngineShim {
        analysis,
        body,
        mut entry_sets,
        tcx,
        apply_trans_for_block,
        ..
    } = engine;

    let mut dirty_queue: WorkQueue<BasicBlock> = WorkQueue::with_none(body.basic_blocks.len());

    if <TypeTracker<'tcx> as AnalysisDomain>::Direction::IS_FORWARD {
        for (bb, _) in traversal::reverse_postorder(body) {
            dirty_queue.insert(bb);
        }
    } else {
        // Reverse post-order on the reverse CFG may generate a better iteration order for
        // backward dataflow analyses, but probably not enough to matter.
        for (bb, _) in traversal::postorder(body) {
            dirty_queue.insert(bb);
        }
    }

    // `state` is not actually used between iterations;
    // this is just an optimization to avoid reallocating
    // every iteration.
    let mut state = analysis.bottom_value(body);
    while let Some(bb) = dirty_queue.pop() {
        let bb_data = &body[bb];

        // Set the state to the entry state of the block.
        // This is equivalent to `state = entry_sets[bb].clone()`,
        // but it saves an allocation, thus improving compile times.
        state.clone_from(&entry_sets[bb]);

        // Apply the block transfer function, using the cached one if it exists.
        match &apply_trans_for_block {
            Some(apply) => apply(bb, &mut state),
            None => <TypeTracker<'tcx> as AnalysisDomain>::Direction::apply_effects_in_block(
                &analysis, &mut state, bb, bb_data,
            ),
        }

        <TypeTracker<'tcx> as AnalysisDomain>::Direction::join_state_into_successors_of(
            &analysis,
            tcx,
            body,
            &mut state,
            (bb, bb_data),
            |target: BasicBlock, state: &<TypeTracker<'tcx> as AnalysisDomain>::Domain| {
                let set_changed = entry_sets[target].join(state);
                if set_changed {
                    dirty_queue.insert(target);
                }
            },
        );
    }

    let results: Results<'tcx, TypeTracker<'tcx>> = unsafe {
        transmute(ResultsShim {
            analysis,
            entry_sets,
        })
    };

    results
}
