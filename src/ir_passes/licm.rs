//! Purpose:
//! Loop-invariant code motion over EIR. Hoists a pure computation whose operands
//! do not change across a loop out of the loop body into the loop preheader, so
//! it runs once instead of every iteration.
//!
//! Called from:
//! - The fixed-point pass driver in `crate::ir_passes::driver`.
//!
//! Key details:
//! - Uses the [loop forest](crate::ir_passes::loops) and
//!   [dominance](crate::ir_passes::dominance). An instruction in a loop is
//!   loop-invariant when it is eligible (see below) and each operand is either
//!   defined by another instruction being hoisted from the same loop or has a
//!   definition that dominates the preheader (so it is available there). This is
//!   a fixed point: hoisting one instruction can make a dependent one invariant.
//! - Only **pure** (`Effects::PURE`) instructions with at least one operand and a
//!   `NonHeap`/`Persistent` result are eligible. Purity means the result depends
//!   only on the operands and the op neither reads mutable state nor faults, so
//!   evaluating it once in the preheader — unconditionally, even if the original
//!   site was only reached on some iterations — is safe (no speculation hazard).
//!   The ownership bound keeps the move refcount-neutral. Nullary constant/address
//!   materializations are not hoisted: they are cheaper to rematerialize than to
//!   keep live across the loop (the same policy CSE uses).
//! - Hoisting needs a preheader to move into. The loop analysis detects an
//!   existing one (PHP loops lower to slot-based CFGs whose init block is a
//!   natural preheader); loops without a detected preheader are skipped rather
//!   than have one synthesized here.
//! - Loops are processed innermost-first and moves are applied immediately, so a
//!   value hoisted to an inner preheader (which lies in the enclosing loop) can
//!   be hoisted again to the outer preheader in the same run when it is invariant
//!   there too. Instructions are moved between blocks' instruction lists; their
//!   result `ValueDef`s (block + index) are recomputed once at the end so the
//!   value table matches the new layout.
//! - Functions with exception handlers are skipped: their handler blocks are
//!   reachable only through implicit edges absent from the terminator graph, so
//!   dominance/loop reasoning over that graph cannot justify hoisting (the same
//!   restriction CSE and branch simplification use).

use std::collections::{HashMap, HashSet};

use crate::ir::{BlockId, DataPool, Function, InstId, Op, Ownership, ValueDef, ValueId};

use super::cfg::has_exception_handlers;
use super::dominance::{compute_dominance, DominanceInfo};
use super::driver::IrPass;
use super::loops::{compute_loops, NaturalLoop};

/// Loop-invariant code motion pass. See the module docs for the model.
pub struct Licm;

impl IrPass for Licm {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "licm"
    }

    /// Hoists loop-invariant pure computations into loop preheaders, returning
    /// true on any change. The literal pool is unused: the pass only relocates
    /// existing instructions.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if has_exception_handlers(function) {
            return false;
        }
        let dominance = compute_dominance(function);
        let loops = compute_loops(function, &dominance);
        if loops.is_empty() {
            return false;
        }

        // Current defining block of every value, updated as instructions move so
        // later (outer) loops see relocated definitions.
        let mut def_block = build_def_block(function);

        // Process innermost loops first so inner-to-outer hoist chains converge in
        // one run.
        let mut order: Vec<usize> = (0..loops.loops().len()).collect();
        order.sort_by_key(|&index| std::cmp::Reverse(loops.loops()[index].depth));

        let mut changed = false;
        for index in order {
            let loop_ref = &loops.loops()[index];
            let Some(preheader) = loop_ref.preheader else {
                continue;
            };
            let hoistable = find_hoistable(function, loop_ref, preheader, &dominance, &def_block);
            for inst_id in hoistable {
                move_instruction(function, inst_id, preheader, &mut def_block);
                changed = true;
            }
        }

        if changed {
            recompute_instruction_defs(function);
        }
        changed
    }
}

/// Builds the loop-invariant instruction set for one loop and returns it ordered
/// by instruction id (a valid topological order, since SSA definitions precede
/// their uses). The set is grown to a fixed point: an instruction joins once all
/// its operands are available at the preheader, which can be enabled by another
/// instruction joining first.
fn find_hoistable(
    function: &Function,
    loop_ref: &NaturalLoop,
    preheader: BlockId,
    dominance: &DominanceInfo,
    def_block: &HashMap<ValueId, BlockId>,
) -> Vec<InstId> {
    let mut hoistable: HashSet<InstId> = HashSet::new();
    loop {
        let mut added = false;
        for &block in &loop_ref.blocks {
            let Some(basic_block) = function.block(block) else {
                continue;
            };
            for &inst_id in &basic_block.instructions {
                if hoistable.contains(&inst_id) {
                    continue;
                }
                let Some(inst) = function.instruction(inst_id) else {
                    continue;
                };
                if !is_hoist_eligible(inst) {
                    continue;
                }
                let ready = inst
                    .operands
                    .iter()
                    .all(|&op| operand_available(function, op, preheader, dominance, def_block, &hoistable));
                if ready {
                    hoistable.insert(inst_id);
                    added = true;
                }
            }
        }
        if !added {
            break;
        }
    }
    let mut ordered: Vec<InstId> = hoistable.into_iter().collect();
    ordered.sort_unstable_by_key(|id| id.as_raw());
    ordered
}

/// Returns true when `op` is available at the preheader: it is produced by an
/// instruction already chosen for hoisting (which will be placed in the preheader
/// before this one), or its current definition dominates the preheader.
fn operand_available(
    function: &Function,
    op: ValueId,
    preheader: BlockId,
    dominance: &DominanceInfo,
    def_block: &HashMap<ValueId, BlockId>,
    hoistable: &HashSet<InstId>,
) -> bool {
    if let Some(inst) = defining_inst(function, op) {
        if hoistable.contains(&inst) {
            return true;
        }
    }
    match def_block.get(&op) {
        Some(&block) => dominance.dominates(block, preheader),
        None => false,
    }
}

/// Returns true when an instruction may be hoisted: it produces a value, is not a
/// `nop`, is pure (no side effects, no fault, no mutable-state read), has at
/// least one operand (so it is a computation, not a rematerializable constant),
/// and its result carries no owned-heap cleanup.
fn is_hoist_eligible(inst: &crate::ir::Instruction) -> bool {
    inst.result.is_some()
        && inst.op != Op::Nop
        && inst.effects.is_pure()
        && !inst.operands.is_empty()
        && matches!(inst.result_ownership, Ownership::NonHeap | Ownership::Persistent)
}

/// Moves an instruction out of its current block and appends it to the preheader
/// (before the preheader's terminator), updating the current-definition-block map.
fn move_instruction(
    function: &mut Function,
    inst_id: InstId,
    preheader: BlockId,
    def_block: &mut HashMap<ValueId, BlockId>,
) {
    let Some(result) = function.instruction(inst_id).and_then(|inst| inst.result) else {
        return;
    };
    let Some(&current) = def_block.get(&result) else {
        return;
    };
    if current == preheader {
        return;
    }
    if let Some(block) = function.block_mut(current) {
        if let Some(position) = block.instructions.iter().position(|&id| id == inst_id) {
            block.instructions.remove(position);
        }
    }
    if let Some(block) = function.block_mut(preheader) {
        block.instructions.push(inst_id);
    }
    def_block.insert(result, preheader);
}

/// Builds the map from each value to the block that currently defines it (block
/// parameters and instruction results).
fn build_def_block(function: &Function) -> HashMap<ValueId, BlockId> {
    let mut map: HashMap<ValueId, BlockId> = HashMap::new();
    for block in &function.blocks {
        for &param in &block.params {
            map.insert(param, block.id);
        }
        for &inst_id in &block.instructions {
            if let Some(result) = function.instruction(inst_id).and_then(|inst| inst.result) {
                map.insert(result, block.id);
            }
        }
    }
    map
}

/// Returns the instruction that defines `value`, when it is instruction-defined.
fn defining_inst(function: &Function, value: ValueId) -> Option<InstId> {
    match function.value(value)?.def {
        ValueDef::Instruction { inst, .. } => Some(inst),
        ValueDef::BlockParam { .. } => None,
    }
}

/// Recomputes every instruction-result value's definition site from the current
/// block layout, so the value table matches after instructions have moved.
fn recompute_instruction_defs(function: &mut Function) {
    let mut updates: Vec<(ValueId, ValueDef)> = Vec::new();
    for block in &function.blocks {
        for (index, &inst_id) in block.instructions.iter().enumerate() {
            if let Some(result) = function.instruction(inst_id).and_then(|inst| inst.result) {
                updates.push((
                    result,
                    ValueDef::Instruction { block: block.id, index: index as u32, inst: inst_id },
                ));
            }
        }
    }
    for (value, def) in updates {
        if let Some(value_ref) = function.values.get_mut(value.as_raw() as usize) {
            value_ref.def = def;
        }
    }
}
