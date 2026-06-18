//! Purpose:
//! Control-flow-graph helpers shared by the IR-level passes: successor lookup
//! and reverse-postorder block ordering.
//!
//! Called from:
//! - `crate::ir_passes::liveness` and `crate::ir_passes::intervals`.
//!
//! Key details:
//! - Reverse postorder visits a definition before its dominated uses on
//!   reducible CFGs, which is the order the linear-scan numbering relies on.

use std::collections::HashSet;

use crate::ir::{BlockId, Function, Terminator};

/// Returns the successor blocks branched to by a terminator, in branch order.
pub(super) fn successors(term: &Terminator) -> Vec<BlockId> {
    match term {
        Terminator::Br { target, .. } => vec![*target],
        Terminator::CondBr {
            then_target,
            else_target,
            ..
        } => vec![*then_target, *else_target],
        Terminator::Switch { cases, default, .. } => {
            let mut targets: Vec<BlockId> = cases.iter().map(|case| case.target).collect();
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

/// Computes the reverse-postorder traversal of `func`'s reachable blocks
/// starting from the entry block.
///
/// Uses an explicit work stack rather than recursion so deeply nested functions
/// cannot overflow. Blocks unreachable from the entry are omitted.
pub(super) fn reverse_postorder(func: &Function) -> Vec<BlockId> {
    let mut visited: HashSet<BlockId> = HashSet::new();
    let mut postorder: Vec<BlockId> = Vec::new();
    let mut stack: Vec<(BlockId, bool)> = vec![(func.entry, false)];

    while let Some((block_id, processed)) = stack.pop() {
        if processed {
            postorder.push(block_id);
            continue;
        }
        if !visited.insert(block_id) {
            continue;
        }
        stack.push((block_id, true));
        if let Some(block) = func.block(block_id) {
            if let Some(term) = &block.terminator {
                for succ in successors(term) {
                    if !visited.contains(&succ) {
                        stack.push((succ, false));
                    }
                }
            }
        }
    }

    postorder.reverse();
    postorder
}
