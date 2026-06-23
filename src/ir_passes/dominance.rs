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
//! - Uses the Cooper–Harvey–Kennedy iterative algorithm ("A Simple, Fast
//!   Dominance Algorithm"): walk blocks in reverse postorder, recomputing each
//!   block's idom as the intersection of its already-processed predecessors'
//!   idoms until a fixed point. The intersect walks the partial idom tree using
//!   postorder numbers as "fingers". This converges for arbitrary (including
//!   irreducible) reducible CFGs and is simple and fast on the small functions
//!   EIR produces.
//! - Only blocks reachable from the entry participate. Unreachable blocks (which
//!   `branch_simplify` neutralizes in place but leaves in the table) are excluded
//!   from the dominator tree, and dominance queries about them return `false`.
//! - The internal `idom` table is self-rooted (`idom[entry] == entry`,
//!   `idom[unreachable] == itself`) so the intersect and the dominance walk
//!   terminate without special cases; the public `immediate_dominator` maps the
//!   entry and unreachable blocks back to `None`.

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

    // Reverse postorder of reachable blocks (entry first), with a postorder
    // number per block where a higher number is closer to the entry — the order
    // the intersect's finger walk relies on.
    let rpo = reverse_postorder(func);
    let mut reachable = vec![false; block_count];
    let mut postorder_number = vec![0u32; block_count];
    for (rpo_index, &block) in rpo.iter().enumerate() {
        let raw = block.as_raw() as usize;
        reachable[raw] = true;
        postorder_number[raw] = (rpo.len() - 1 - rpo_index) as u32;
    }

    let preds = predecessors(func);

    // `idom[b]` is `None` until the block has been assigned a dominator. The
    // entry is its own dominator (the tree root sentinel).
    let mut idom: Vec<Option<BlockId>> = vec![None; block_count];
    idom[entry.as_raw() as usize] = Some(entry);

    let mut changed = true;
    while changed {
        changed = false;
        // Process every reachable block except the entry, in reverse postorder,
        // so each block's predecessors are usually already processed.
        for &block in rpo.iter().skip(1) {
            let mut new_idom: Option<BlockId> = None;
            for &pred in &preds[block.as_raw() as usize] {
                if !reachable[pred.as_raw() as usize] || idom[pred.as_raw() as usize].is_none() {
                    continue;
                }
                new_idom = Some(match new_idom {
                    None => pred,
                    Some(current) => intersect(pred, current, &idom, &postorder_number),
                });
            }
            if let Some(new_idom) = new_idom {
                if idom[block.as_raw() as usize] != Some(new_idom) {
                    idom[block.as_raw() as usize] = Some(new_idom);
                    changed = true;
                }
            }
        }
    }

    // Finalize: self-root the entry and any unreachable block so later walks
    // terminate, then build the dominator-tree children lists.
    let idom: Vec<BlockId> = (0..block_count)
        .map(|raw| idom[raw].unwrap_or_else(|| BlockId::from_raw(raw as u32)))
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

/// Returns the nearest common ancestor of two processed blocks in the partial
/// dominator tree, walking the two "fingers" up their idom chains until they
/// meet. Higher postorder numbers are closer to the entry, so the finger with
/// the smaller number is always the one moved up; the entry has the maximum
/// number and is never moved, guaranteeing termination.
fn intersect(
    mut a: BlockId,
    mut b: BlockId,
    idom: &[Option<BlockId>],
    postorder_number: &[u32],
) -> BlockId {
    while a != b {
        while postorder_number[a.as_raw() as usize] < postorder_number[b.as_raw() as usize] {
            a = idom[a.as_raw() as usize].expect("intersect operand has an assigned idom");
        }
        while postorder_number[b.as_raw() as usize] < postorder_number[a.as_raw() as usize] {
            b = idom[b.as_raw() as usize].expect("intersect operand has an assigned idom");
        }
    }
    a
}
