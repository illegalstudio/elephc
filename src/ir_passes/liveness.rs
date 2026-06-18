//! Purpose:
//! Backward dataflow liveness analysis over EIR functions. Computes per-block
//! live-in and live-out value sets, the foundation for live-interval
//! construction in the linear-scan register allocator.
//!
//! Called from:
//! - `crate::ir_passes` interval construction and register allocation.
//!
//! Key details:
//! - SSA-lite with block parameters: a block parameter is "defined" at block
//!   entry; branch arguments at a terminator are "uses" at that terminator.
//! - Iterates to a fixed point so loops converge; the CFG is small so a simple
//!   worklist over blocks is sufficient.

use std::collections::{HashMap, HashSet};

use crate::ir::{BasicBlock, BlockId, Function, Terminator, ValueId};

/// Per-block liveness result: which values are live entering and leaving each
/// block. Live-in/live-out are keyed by `BlockId` for every block in the
/// function.
pub struct LivenessInfo {
    live_in: HashMap<BlockId, HashSet<ValueId>>,
    live_out: HashMap<BlockId, HashSet<ValueId>>,
}

impl LivenessInfo {
    /// Returns the set of values live on entry to `block`.
    pub fn live_in_of(&self, block: BlockId) -> &HashSet<ValueId> {
        self.live_in
            .get(&block)
            .expect("liveness queried for unknown block")
    }

    /// Returns the set of values live on exit from `block`.
    pub fn live_out_of(&self, block: BlockId) -> &HashSet<ValueId> {
        self.live_out
            .get(&block)
            .expect("liveness queried for unknown block")
    }
}

/// Per-block local liveness facts, independent of control flow: the values
/// defined in the block and the upward-exposed uses (used before any definition
/// in the same block).
struct BlockFacts {
    /// Values defined in the block: instruction results plus block parameters.
    defs: HashSet<ValueId>,
    /// Values used in the block that are not defined in the block.
    upward_uses: HashSet<ValueId>,
    /// Successor blocks reached from this block's terminator.
    successors: Vec<BlockId>,
}

/// Computes per-block liveness for `func` via backward dataflow to a fixed
/// point.
///
/// Block parameters are treated as definitions at block entry, so values passed
/// as branch arguments are uses in the predecessor's terminator and do not leak
/// backwards through the successor's parameters. SSA single-definition lets us
/// derive upward-exposed uses by simply excluding block-local definitions.
pub fn compute_liveness(func: &Function) -> LivenessInfo {
    let facts: HashMap<BlockId, BlockFacts> = func
        .blocks
        .iter()
        .map(|block| (block.id, block_facts(func, block)))
        .collect();

    let mut live_in: HashMap<BlockId, HashSet<ValueId>> = func
        .blocks
        .iter()
        .map(|block| (block.id, HashSet::new()))
        .collect();
    let mut live_out: HashMap<BlockId, HashSet<ValueId>> = live_in.clone();

    // Iterate backwards over the block list until the sets stop changing. The
    // CFG is small, so repeated full sweeps converge quickly without an
    // explicit predecessor worklist.
    let mut changed = true;
    while changed {
        changed = false;
        for block in func.blocks.iter().rev() {
            let f = &facts[&block.id];

            let mut new_out = HashSet::new();
            for succ in &f.successors {
                new_out.extend(live_in[succ].iter().copied());
            }

            let mut new_in = f.upward_uses.clone();
            for value in new_out.iter().copied() {
                if !f.defs.contains(&value) {
                    new_in.insert(value);
                }
            }

            if new_out != live_out[&block.id] || new_in != live_in[&block.id] {
                changed = true;
                live_out.insert(block.id, new_out);
                live_in.insert(block.id, new_in);
            }
        }
    }

    LivenessInfo { live_in, live_out }
}

/// Computes the control-flow-independent liveness facts for a single block:
/// its definitions, its upward-exposed uses, and its successors.
fn block_facts(func: &Function, block: &BasicBlock) -> BlockFacts {
    let mut defs: HashSet<ValueId> = block.params.iter().copied().collect();
    let mut uses: HashSet<ValueId> = HashSet::new();

    for inst_id in &block.instructions {
        let inst = func
            .instruction(*inst_id)
            .expect("block references a valid instruction");
        for operand in &inst.operands {
            uses.insert(*operand);
        }
        if let Some(result) = inst.result {
            defs.insert(result);
        }
    }

    if let Some(term) = &block.terminator {
        for value in terminator_uses(term) {
            uses.insert(value);
        }
    }

    let upward_uses = uses.difference(&defs).copied().collect();
    let successors = block
        .terminator
        .as_ref()
        .map(crate::ir_passes::cfg::successors)
        .unwrap_or_default();

    BlockFacts {
        defs,
        upward_uses,
        successors,
    }
}

/// Collects every value used by a terminator: branch arguments, conditions,
/// switch scrutinees, returned/thrown values, and generator suspend operands.
pub(super) fn terminator_uses(term: &Terminator) -> Vec<ValueId> {
    match term {
        Terminator::Br { args, .. } => args.clone(),
        Terminator::CondBr {
            cond,
            then_args,
            else_args,
            ..
        } => {
            let mut uses = vec![*cond];
            uses.extend(then_args.iter().copied());
            uses.extend(else_args.iter().copied());
            uses
        }
        Terminator::Switch {
            scrutinee,
            cases,
            default_args,
            ..
        } => {
            let mut uses = vec![*scrutinee];
            for case in cases {
                uses.extend(case.args.iter().copied());
            }
            uses.extend(default_args.iter().copied());
            uses
        }
        Terminator::Return { value } => value.iter().copied().collect(),
        Terminator::Throw { value } => vec![*value],
        Terminator::GeneratorSuspend {
            key,
            value,
            resume_args,
            ..
        } => {
            let mut uses: Vec<ValueId> = key.iter().copied().collect();
            uses.extend(value.iter().copied());
            uses.extend(resume_args.iter().copied());
            uses
        }
        Terminator::Fatal { .. } | Terminator::Unreachable => Vec::new(),
    }
}
