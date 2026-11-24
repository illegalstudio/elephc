//! Purpose:
//! Computes dominance for validation with both ordinary and exception-handler edges.
//!
//! Called from:
//! - The EIR validator's cross-block definition and use checks.
//!
//! Key details:
//! - Reachable blocks own a half-open interval in dominator-tree preorder.
//! - Dead continuations have no range and keep their existing validation rules.

use std::ops::Range;

use crate::ir::control_flow::{explicit_successors, immediate_dominators, reverse_postorder};
use crate::ir::{BlockId, Function, Immediate, Op};

/// Answers reachability and dominance without materializing a bitset per block.
pub(super) struct DominatorTree {
    ranges: Vec<Range<usize>>,
}

impl DominatorTree {
    /// Builds the reachable tree from the validator's exception-aware graph.
    pub(super) fn new(function: &Function) -> Self {
        let block_count = function.blocks.len();
        let mut predecessors = vec![Vec::new(); block_count];
        let mut successors = vec![Vec::new(); block_count];
        for block in &function.blocks {
            let targets = &mut successors[block.id.as_raw() as usize];
            if let Some(term) = &block.terminator {
                targets.extend(explicit_successors(term));
            }
            // A handler token names an implicit edge from its push block, which
            // dominates the corresponding protected region.
            for &inst in &block.instructions {
                if let Some(instruction) = function.instruction(inst) {
                    if let (Op::TryPushHandler, Some(Immediate::I64(token))) =
                        (instruction.op, &instruction.immediate)
                    {
                        targets.push(BlockId::from_raw(*token as u32));
                    }
                }
            }
            for target in targets {
                // Invalid explicit targets are diagnosed by terminator validation.
                if let Some(incoming) = predecessors.get_mut(target.as_raw() as usize) {
                    incoming.push(block.id);
                }
            }
        }

        let order = reverse_postorder(function.entry, block_count, |block| {
            successors[block.as_raw() as usize].iter().copied()
        });
        let parents = immediate_dominators(function.entry, &predecessors, &order);
        let mut children = vec![Vec::new(); block_count];
        for &block in order.iter().skip(1) {
            let parent = parents[block.as_raw() as usize].expect("reachable block has a dominator");
            children[parent.as_raw() as usize].push(block);
        }

        let mut ranges = vec![0..0; block_count];
        let mut next = 1;
        let mut stack = vec![(function.entry, false)];
        while let Some((block, leaving)) = stack.pop() {
            let range = &mut ranges[block.as_raw() as usize];
            if leaving {
                range.end = next;
            } else {
                range.start = next;
                next += 1;
                stack.push((block, true));
                stack.extend(children[block.as_raw() as usize].iter().map(|&child| (child, false)));
            }
        }
        Self { ranges }
    }

    /// Reports whether a block's use sites can execute.
    pub(super) fn is_reachable(&self, block: BlockId) -> bool {
        self.ranges.get(block.as_raw() as usize).is_some_and(|range| !range.is_empty())
    }

    /// A block dominates another when the latter lies in the former's subtree.
    pub(super) fn dominates(&self, definition: BlockId, used_in: BlockId) -> bool {
        match (self.ranges.get(definition.as_raw() as usize), self.ranges.get(used_in.as_raw() as usize)) {
            (Some(definition), Some(used_in)) => definition.contains(&used_in.start),
            _ => false,
        }
    }
}
