//! Purpose:
//! Natural-loop forest over EIR functions. Detects back edges, constructs the
//! natural loop body for each loop header, computes loop nesting (parent and
//! depth), and detects each loop's preheader. The foundation for loop-invariant
//! code motion and other loop optimizations.
//!
//! Called from:
//! - Future loop optimization passes in `crate::ir_passes` (LICM). This is a
//!   read-only sidecar analysis, like `dominance`/`liveness`, not a fixed-point
//!   driver transform.
//!
//! Key details:
//! - A back edge is a CFG edge `latch -> header` whose target dominates its
//!   source (so control returns to a block that already dominates the latch).
//!   Loop detection is therefore a thin layer over the [dominance
//!   analysis](crate::ir_passes::dominance). Back edges sharing one header are a
//!   single natural loop with multiple latches.
//! - The natural loop of a header `h` with latch set `L` is `{h}` plus every
//!   block that can reach some latch in `L` without passing through `h` — found
//!   by a backward walk from the latches over reachable predecessors that stops
//!   at the header. Unreachable blocks never participate.
//! - PHP loops lower to slot-based CFGs (the loop variable lives in a local slot,
//!   not a block parameter), so loop edges carry no block arguments. The init
//!   block that branches into the header is the natural preheader. A preheader is
//!   detected as the unique reachable out-of-loop predecessor of the header whose
//!   sole successor is the header; when entry into the loop is shared or
//!   conditional no preheader exists (an optimization that needs one inserts it).
//! - Nesting is by block-set containment: loop `A` is nested in loop `B` when
//!   `A`'s header lies in `B`'s body and `A != B`; the immediate parent is the
//!   smallest such `B`. Reducible CFGs (what the lowerer emits) yield properly
//!   nested or disjoint loops.

use std::collections::HashSet;

use crate::ir::{BlockId, Function};

use super::cfg::{predecessors, successors};
use super::dominance::DominanceInfo;

/// A natural loop: one header, its back-edge sources, its body, and its place in
/// the loop nesting forest.
pub struct NaturalLoop {
    /// The loop header — the single entry block every iteration passes through.
    pub header: BlockId,
    /// The back-edge sources (latches): blocks with an edge to `header` that
    /// `header` dominates. Sorted ascending.
    pub latches: Vec<BlockId>,
    /// Every block in the loop body, including the header. Sorted ascending so
    /// [`NaturalLoop::contains`] can binary-search.
    pub blocks: Vec<BlockId>,
    /// The loop preheader, when one already exists: the unique reachable
    /// out-of-loop predecessor of the header whose only successor is the header.
    pub preheader: Option<BlockId>,
    /// Index into [`LoopInfo`]'s loop list of the immediate enclosing loop, if any.
    pub parent: Option<usize>,
    /// Nesting depth, 1 for an outermost loop and one more per enclosing loop.
    pub depth: u32,
}

impl NaturalLoop {
    /// Returns true when `block` is in this loop's body.
    pub fn contains(&self, block: BlockId) -> bool {
        self.blocks.binary_search(&block).is_ok()
    }
}

/// Natural-loop forest for one function plus the innermost-loop lookup per block.
pub struct LoopInfo {
    loops: Vec<NaturalLoop>,
    /// Innermost containing loop index per block, indexed by raw block id;
    /// `None` for blocks outside every loop.
    innermost: Vec<Option<usize>>,
}

impl LoopInfo {
    /// Returns every natural loop in the function. The order is unspecified; use
    /// [`NaturalLoop::parent`] and `depth` to walk the nesting forest.
    pub fn loops(&self) -> &[NaturalLoop] {
        &self.loops
    }

    /// Returns true when the function has no loops.
    pub fn is_empty(&self) -> bool {
        self.loops.is_empty()
    }

    /// Returns the loop whose header is `block`, if `block` is a loop header.
    pub fn header_loop(&self, block: BlockId) -> Option<&NaturalLoop> {
        self.loops.iter().find(|lp| lp.header == block)
    }

    /// Returns true when `block` is the header of some loop.
    pub fn is_loop_header(&self, block: BlockId) -> bool {
        self.loops.iter().any(|lp| lp.header == block)
    }

    /// Returns the innermost loop containing `block`, or `None` when `block` is
    /// outside every loop.
    pub fn innermost_loop(&self, block: BlockId) -> Option<&NaturalLoop> {
        self.innermost
            .get(block.as_raw() as usize)
            .copied()
            .flatten()
            .map(|index| &self.loops[index])
    }

    /// Returns the loop nesting depth of `block` (0 when outside every loop).
    pub fn loop_depth(&self, block: BlockId) -> u32 {
        self.innermost_loop(block).map(|lp| lp.depth).unwrap_or(0)
    }

    /// Returns every back edge as a `(latch, header)` pair, in loop then latch
    /// order.
    pub fn back_edges(&self) -> Vec<(BlockId, BlockId)> {
        let mut edges = Vec::new();
        for lp in &self.loops {
            for &latch in &lp.latches {
                edges.push((latch, lp.header));
            }
        }
        edges
    }
}

/// Computes the natural-loop forest of `func` using its dominator tree.
///
/// Back edges are CFG edges whose target dominates their source; loops sharing a
/// header are merged. Returns an empty [`LoopInfo`] for loop-free functions.
pub fn compute_loops(func: &Function, dominance: &DominanceInfo) -> LoopInfo {
    let preds = predecessors(func);

    // Group back-edge sources by the header they target. A back edge is an edge
    // `b -> s` where `s` dominates `b`; both ends must be reachable.
    let mut latches_by_header: Vec<(BlockId, Vec<BlockId>)> = Vec::new();
    for block in &func.blocks {
        if !dominance.is_reachable(block.id) {
            continue;
        }
        let Some(term) = &block.terminator else {
            continue;
        };
        for target in successors(term) {
            if dominance.is_reachable(target) && dominance.dominates(target, block.id) {
                match latches_by_header.iter_mut().find(|(h, _)| *h == target) {
                    Some((_, latches)) => latches.push(block.id),
                    None => latches_by_header.push((target, vec![block.id])),
                }
            }
        }
    }

    // Build the natural loop body for each header.
    let mut loops: Vec<NaturalLoop> = latches_by_header
        .into_iter()
        .map(|(header, mut latches)| {
            latches.sort_unstable();
            latches.dedup();
            let blocks = natural_loop_body(header, &latches, &preds, dominance);
            let preheader = detect_preheader(func, header, &blocks, &preds, dominance);
            NaturalLoop {
                header,
                latches,
                blocks,
                preheader,
                parent: None,
                depth: 1,
            }
        })
        .collect();

    assign_nesting(&mut loops);
    let innermost = innermost_map(func, &loops);

    LoopInfo { loops, innermost }
}

/// Collects the natural loop body of `header` with the given `latches`: the
/// header plus every block that can reach a latch without passing through the
/// header, walking reachable predecessors backward. Returns a sorted block list.
fn natural_loop_body(
    header: BlockId,
    latches: &[BlockId],
    preds: &[Vec<BlockId>],
    dominance: &DominanceInfo,
) -> Vec<BlockId> {
    let mut body: HashSet<BlockId> = HashSet::new();
    body.insert(header);
    let mut stack: Vec<BlockId> = Vec::new();
    for &latch in latches {
        // A self-loop latch is the header itself; it is already in the body.
        if body.insert(latch) {
            stack.push(latch);
        }
    }
    while let Some(block) = stack.pop() {
        for &pred in &preds[block.as_raw() as usize] {
            if !dominance.is_reachable(pred) {
                continue;
            }
            if body.insert(pred) {
                stack.push(pred);
            }
        }
    }
    let mut blocks: Vec<BlockId> = body.into_iter().collect();
    blocks.sort_unstable();
    blocks
}

/// Detects an existing preheader for `header`: the unique reachable out-of-loop
/// predecessor whose only successor is the header. Returns `None` when entry into
/// the loop is shared between several blocks or is conditional.
fn detect_preheader(
    func: &Function,
    header: BlockId,
    blocks: &[BlockId],
    preds: &[Vec<BlockId>],
    dominance: &DominanceInfo,
) -> Option<BlockId> {
    let in_loop = |block: BlockId| blocks.binary_search(&block).is_ok();
    let mut outside: Vec<BlockId> = preds[header.as_raw() as usize]
        .iter()
        .copied()
        .filter(|&pred| dominance.is_reachable(pred) && !in_loop(pred))
        .collect();
    outside.sort_unstable();
    outside.dedup();
    let [only] = outside.as_slice() else {
        return None;
    };
    let term = func.block(*only)?.terminator.as_ref()?;
    let succs = successors(term);
    if succs.len() == 1 && succs[0] == header {
        Some(*only)
    } else {
        None
    }
}

/// Assigns each loop's immediate parent and nesting depth by block-set
/// containment: the parent is the smallest other loop whose body contains this
/// loop's header.
fn assign_nesting(loops: &mut [NaturalLoop]) {
    let sizes: Vec<usize> = loops.iter().map(|lp| lp.blocks.len()).collect();
    let headers: Vec<BlockId> = loops.iter().map(|lp| lp.header).collect();
    for index in 0..loops.len() {
        let mut parent: Option<usize> = None;
        let mut parent_size = usize::MAX;
        for other in 0..loops.len() {
            if other == index {
                continue;
            }
            if loops[other].contains(headers[index]) && sizes[other] < parent_size {
                parent = Some(other);
                parent_size = sizes[other];
            }
        }
        loops[index].parent = parent;
    }
    // Depth follows the parent chain. Computed per loop by walking to the root;
    // the forest is shallow, so repeated walks are cheap.
    for index in 0..loops.len() {
        let mut depth = 1;
        let mut current = loops[index].parent;
        while let Some(p) = current {
            depth += 1;
            current = loops[p].parent;
        }
        loops[index].depth = depth;
    }
}

/// Builds the per-block innermost-loop index: for each block, the containing loop
/// with the greatest nesting depth.
fn innermost_map(func: &Function, loops: &[NaturalLoop]) -> Vec<Option<usize>> {
    let mut innermost: Vec<Option<usize>> = vec![None; func.blocks.len()];
    for (index, lp) in loops.iter().enumerate() {
        for &block in &lp.blocks {
            let slot = &mut innermost[block.as_raw() as usize];
            let deeper = match slot {
                Some(current) => lp.depth > loops[*current].depth,
                None => true,
            };
            if deeper {
                *slot = Some(index);
            }
        }
    }
    innermost
}
