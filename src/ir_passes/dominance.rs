//! Purpose:
//! Dominator-tree analysis over EIR functions. Computes the immediate dominator
//! of every reachable block and answers dominance queries, the foundation for
//! the cross-block optimizations that follow (common-subexpression elimination,
//! natural-loop detection, loop-invariant code motion).
//!
//! Called from:
//! - Future cross-block EIR passes in `crate::ir_passes`. This is a read-only
//!   sidecar analysis, like `liveness`/`intervals`, not a transformation in the
//!   fixed-point driver.
//!
//! Key details:
//! - Shares the immediate-dominator computation with the validator, using the
//!   optimizer's explicit-edge graph.
//! - Only blocks reachable from the entry participate. Unreachable blocks (which
//!   `branch_simplify` neutralizes in place but leaves in the table) are excluded
//!   from the dominator tree, and dominance queries about them return `false`.
//! - The internal `idom` table is self-rooted (`idom[entry] == entry`,
//!   `idom[unreachable] == itself`) so the intersect and the dominance walk
//!   terminate without special cases; the public `immediate_dominator` maps the
//!   entry and unreachable blocks back to `None`.

use crate::ir::control_flow::immediate_dominators;
use crate::ir::{BlockId, Function};

use super::cfg::{predecessors, reverse_postorder};

/// Dominator-tree result for one function: the immediate dominator of every
/// block plus the children lists for top-down dominator-tree traversal.
pub struct DominanceInfo {
    /// The function entry block (the dominator-tree root).
    entry: BlockId,
    /// Immediate dominator per block, indexed by raw block id. The entry maps to
    /// itself and unreachable blocks map to themselves; both are reported as
    /// having no immediate dominator by `immediate_dominator`.
    idom: Vec<BlockId>,
    /// Whether each block is reachable from the entry.
    reachable: Vec<bool>,
    /// Dominator-tree children per block, indexed by raw block id.
    children: Vec<Vec<BlockId>>,
}

impl DominanceInfo {
    /// Returns the immediate dominator of `block`, or `None` for the entry block
    /// and for unreachable blocks (which are not in the dominator tree).
    pub fn immediate_dominator(&self, block: BlockId) -> Option<BlockId> {
        let raw = block.as_raw() as usize;
        if block == self.entry || !self.reachable[raw] {
            return None;
        }
        Some(self.idom[raw])
    }

    /// Returns true when `a` dominates `b`: every path from the entry to `b`
    /// passes through `a`. Dominance is reflexive (`a` always dominates `a`).
    /// A non-reflexive query involving an unreachable block is false.
    pub fn dominates(&self, a: BlockId, b: BlockId) -> bool {
        if a == b {
            return true;
        }
        if !self.is_reachable(a) || !self.is_reachable(b) {
            return false;
        }
        // Walk b's idom chain up to the root; a dominates b iff it appears on it.
        let mut current = b;
        loop {
            if current == a {
                return true;
            }
            if current == self.entry {
                return false;
            }
            current = self.idom[current.as_raw() as usize];
        }
    }

    /// Returns true when `a` strictly dominates `b` (`a` dominates `b` and
    /// `a != b`).
    pub fn strictly_dominates(&self, a: BlockId, b: BlockId) -> bool {
        a != b && self.dominates(a, b)
    }

    /// Returns true when `block` is reachable from the entry.
    pub fn is_reachable(&self, block: BlockId) -> bool {
        self.reachable[block.as_raw() as usize]
    }

    /// Returns the dominator-tree children of `block`: the blocks whose immediate
    /// dominator is `block`, in ascending block-id order.
    pub fn children(&self, block: BlockId) -> &[BlockId] {
        &self.children[block.as_raw() as usize]
    }

    /// Returns the nearest common dominator of `a` and `b` — the deepest block in
    /// the dominator tree that dominates both — or `None` if either is
    /// unreachable. The entry block is always a common dominator, so a reachable
    /// pair always yields `Some`.
    pub fn nearest_common_dominator(&self, a: BlockId, b: BlockId) -> Option<BlockId> {
        if !self.is_reachable(a) || !self.is_reachable(b) {
            return None;
        }
        // Collect a's dominator chain (itself up to the entry), then walk b's
        // chain until it meets that set.
        let mut ancestors = vec![false; self.idom.len()];
        let mut current = a;
        loop {
            ancestors[current.as_raw() as usize] = true;
            if current == self.entry {
                break;
            }
            current = self.idom[current.as_raw() as usize];
        }
        let mut current = b;
        loop {
            if ancestors[current.as_raw() as usize] {
                return Some(current);
            }
            if current == self.entry {
                return Some(self.entry);
            }
            current = self.idom[current.as_raw() as usize];
        }
    }
}

/// Computes the dominator tree of `func` via the Cooper–Harvey–Kennedy iterative
/// algorithm.
///
/// Returns a [`DominanceInfo`] with the immediate dominator of every reachable
/// block. Unreachable blocks are recorded as such and excluded from the tree.
pub fn compute_dominance(func: &Function) -> DominanceInfo {
    let block_count = func.blocks.len();
    let entry = func.entry;

    let rpo = reverse_postorder(func);
    let parents = immediate_dominators(entry, &predecessors(func), &rpo);
    let reachable = parents.iter().map(Option::is_some).collect();

    // Self-root the entry and unreachable blocks so later walks terminate.
    let idom: Vec<_> = parents.into_iter().enumerate()
        .map(|(raw, parent)| parent.unwrap_or_else(|| BlockId::from_raw(raw as u32)))
        .collect();
    let mut children: Vec<Vec<BlockId>> = vec![Vec::new(); block_count];
    for &block in rpo.iter().skip(1) {
        let parent = idom[block.as_raw() as usize];
        children[parent.as_raw() as usize].push(block);
    }

    DominanceInfo {
        entry,
        idom,
        reachable,
        children,
    }
}
