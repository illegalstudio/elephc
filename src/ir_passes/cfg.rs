//! Purpose:
//! Control-flow-graph helpers shared by the IR-level passes: successor lookup,
//! predecessor lists, reverse-postorder block ordering, and exception-handler
//! detection.
//!
//! Called from:
//! - `crate::ir_passes::liveness`, `crate::ir_passes::intervals`,
//!   `crate::ir_passes::dominance`, `crate::ir_passes::branch_simplify`, and
//!   `crate::ir_passes::cse`.
//!
//! Key details:
//! - Reverse postorder visits a definition before its dominated uses on
//!   reducible CFGs, which is the order the linear-scan numbering relies on.

use std::collections::HashSet;

use crate::ir::{BlockId, Function, Op, Terminator};

/// Returns true when the function uses any exception-handling opcode.
///
/// Such functions have handler blocks reachable only through implicit edges (a
/// `try_push_handler` token names the handler block id), absent from the
/// terminator graph. Passes that reason about reachability or dominance over the
/// terminator graph alone (branch simplification, dominance-aware CSE) are
/// unsound across those implicit edges and conservatively skip these functions.
pub(super) fn has_exception_handlers(func: &Function) -> bool {
    func.instructions.iter().any(|inst| {
        matches!(
            inst.op,
            Op::TryPushHandler
                | Op::TryPopHandler
                | Op::CatchCurrent
                | Op::CatchBind
                | Op::FinallyEnter
                | Op::FinallyExit
        )
    })
}

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

/// Builds the predecessor list for every block: `preds[b]` holds each block
/// whose terminator branches to `b`, in block-index order.
///
/// Indexed by raw block id over all blocks (including unreachable ones), so a
/// block with no incoming edges has an empty list. Used by dominance analysis,
/// which filters out unreachable predecessors.
pub(super) fn predecessors(func: &Function) -> Vec<Vec<BlockId>> {
    let mut preds: Vec<Vec<BlockId>> = vec![Vec::new(); func.blocks.len()];
    for block in &func.blocks {
        if let Some(term) = &block.terminator {
            for succ in successors(term) {
                preds[succ.as_raw() as usize].push(block.id);
            }
        }
    }
    preds
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
