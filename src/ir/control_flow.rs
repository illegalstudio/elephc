//! Purpose:
//! Shares dense EIR graph traversal and immediate dominators across analyses.
//!
//! Called from:
//! - EIR validation and the optimizer's control-flow analyses.
//!
//! Key details:
//! - The caller selects its edges, including any implicit exception handlers.
//! - Traversal uses an explicit stack for arbitrarily deep graphs.

use super::{BlockId, Terminator};

/// Returns only the explicit successors named by a terminator, in branch order.
pub(crate) fn explicit_successors(term: &Terminator) -> Vec<BlockId> {
    match term {
        Terminator::Br { target, .. } => vec![*target],
        Terminator::CondBr { then_target, else_target, .. } => vec![*then_target, *else_target],
        Terminator::Switch { cases, default, .. } => {
            let mut targets: Vec<_> = cases.iter().map(|case| case.target).collect();
            targets.push(*default);
            targets
        }
        Terminator::GeneratorSuspend { resume, .. } => vec![*resume],
        Terminator::Return { .. }
        | Terminator::Throw { .. }
        | Terminator::Fatal { .. }
        | Terminator::Unreachable => Vec::new(),
    }
}

/// Visits reachable blocks in reverse postorder, ignoring out-of-range edges for validation.
pub(crate) fn reverse_postorder<I: IntoIterator<Item = BlockId>>(
    entry: BlockId,
    block_count: usize,
    mut successors: impl FnMut(BlockId) -> I,
) -> Vec<BlockId> {
    let mut visited = vec![false; block_count];
    let mut order = Vec::new();
    let mut stack = vec![(entry, false)];
    while let Some((block, leaving)) = stack.pop() {
        if leaving {
            order.push(block);
            continue;
        }
        let Some(seen) = visited.get_mut(block.as_raw() as usize) else {
            continue;
        };
        if *seen {
            continue;
        }
        *seen = true;
        stack.push((block, true));
        stack.extend(successors(block).into_iter().map(|successor| (successor, false)));
    }
    order.reverse();
    order
}

/// Computes immediate dominators in reverse postorder using the Cooper, Harvey and Kennedy algorithm.
/// The entry is its own parent and unreachable blocks have no parent.
pub(crate) fn immediate_dominators(
    entry: BlockId,
    predecessors: &[Vec<BlockId>],
    order: &[BlockId],
) -> Vec<Option<BlockId>> {
    let mut postorder_number = vec![0; predecessors.len()];
    for (number, &block) in order.iter().rev().enumerate() {
        postorder_number[block.as_raw() as usize] = number;
    }
    let mut parents = vec![None; predecessors.len()];
    parents[entry.as_raw() as usize] = Some(entry);
    let mut changed = true;
    while changed {
        changed = false;
        for &block in order.iter().skip(1) {
            let index = block.as_raw() as usize;
            let parent = predecessors[index]
                .iter()
                .copied()
                .filter(|pred| parents[pred.as_raw() as usize].is_some())
                .reduce(|a, b| intersect(a, b, &parents, &postorder_number));
            if let Some(parent) = parent {
                if parents[index] != Some(parent) {
                    parents[index] = Some(parent);
                    changed = true;
                }
            }
        }
    }
    parents
}

/// Walks two processed blocks up the partial dominator tree until their paths meet.
fn intersect(
    mut a: BlockId,
    mut b: BlockId,
    parents: &[Option<BlockId>],
    postorder_number: &[usize],
) -> BlockId {
    while a != b {
        while postorder_number[a.as_raw() as usize] < postorder_number[b.as_raw() as usize] {
            a = parents[a.as_raw() as usize].expect("processed block has a parent");
        }
        while postorder_number[b.as_raw() as usize] < postorder_number[a.as_raw() as usize] {
            b = parents[b.as_raw() as usize].expect("processed block has a parent");
        }
    }
    a
}
