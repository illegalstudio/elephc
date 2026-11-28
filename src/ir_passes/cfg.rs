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

pub(crate) use crate::ir::control_flow::explicit_successors as successors;
use crate::ir::{BlockId, Function, Op, Terminator, ValueId};

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

/// Returns true when the CFG has an edge to a block at or before its source in
/// reverse postorder, the cheap necessary condition for a natural loop.
pub(crate) fn has_back_edge(func: &Function) -> bool {
    let order = reverse_postorder(func);
    let mut position = vec![usize::MAX; func.blocks.len()];
    for (index, block) in order.iter().enumerate() {
        if let Some(slot) = position.get_mut(block.as_raw() as usize) {
            *slot = index;
        }
    }
    func.blocks.iter().any(|block| {
        let Some(term) = &block.terminator else {
            return false;
        };
        let source = position[block.id.as_raw() as usize];
        successors(term).iter().any(|successor| {
            position
                .get(successor.as_raw() as usize)
                .copied()
                .unwrap_or(usize::MAX)
                <= source
        })
    })
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
    crate::ir::control_flow::reverse_postorder(func.entry, func.blocks.len(), |block| {
        func.block(block)
            .and_then(|block| block.terminator.as_ref())
            .map_or_else(Vec::new, successors)
    })
}

/// Borrows explicit successor targets together with their parallel SSA arguments.
pub(super) fn successor_edges(term: &mut Terminator) -> Vec<(&mut BlockId, &mut Vec<ValueId>)> {
    match term {
        Terminator::Br { target, args } => vec![(target, args)],
        Terminator::CondBr { then_target, then_args, else_target, else_args, .. } => {
            vec![(then_target, then_args), (else_target, else_args)]
        }
        Terminator::Switch { cases, default, default_args, .. } => {
            let mut edges: Vec<_> = cases.iter_mut().map(|case| (&mut case.target, &mut case.args)).collect();
            edges.push((default, default_args));
            edges
        }
        Terminator::GeneratorSuspend { resume, resume_args, .. } => vec![(resume, resume_args)],
        Terminator::Return { .. } | Terminator::Throw { .. }
        | Terminator::Fatal { .. } | Terminator::Unreachable => Vec::new(),
    }
}
