use rustc_index::vec::IndexVec;
use rustc_middle::mir::{BasicBlock, Body};

// This is needed to interoperate with rustc's LocationTable, which is pub(crate) by default.
#[allow(dead_code)]
pub(super) struct LocationTableShim {
    num_points: usize,
    statements_before_block: IndexVec<BasicBlock, usize>,
}

impl LocationTableShim {
    pub(super) fn new(body: &Body<'_>) -> Self {
        let mut num_points = 0;
        let statements_before_block = body
            .basic_blocks
            .iter()
            .map(|block_data| {
                let v = num_points;
                num_points += (block_data.statements.len() + 1) * 2;
                v
            })
            .collect();

        Self {
            num_points,
            statements_before_block,
        }
    }
}
