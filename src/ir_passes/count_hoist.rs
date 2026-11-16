//! Purpose:
//! Hoists a typed-array `count()` out of a loop condition when the loop cannot
//! change the counted storage.
//!
//! Called from:
//! - The EIR fixed-point driver after loop simplification.
//!
//! Key details:
//! - Only a single-argument count in the loop header, fed by a load of one local.
//! - The slot is a plain PHP local and every loop-block use is read-only; stores,
//!   mutating array/hash ops, by-reference calls, aliases, eval and runtime includes
//!   keep the count in the loop.
//! - The receiver load and the count move together to the existing preheader, so the
//!   condition still evaluates the length once before the first body iteration.
//! - Exception-handler functions are skipped, matching LICM's implicit-edge conservatism.

use std::collections::HashSet;

use crate::ir::{
    DataPool, Function, Immediate, InstId, Instruction, LocalKind, LocalSlotId, Op,
    PassOrigin, RuntimeCallTarget, RuntimeFnId, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::cfg::has_exception_handlers;
use super::dominance::compute_dominance;
use super::driver::IrPass;
use super::licm::{build_def_block, move_instruction, recompute_instruction_defs};
use super::loops::{compute_loops, NaturalLoop};

/// Hoists a loop-invariant typed-array count into the loop preheader.
pub struct CountHoist;

impl IrPass for CountHoist {
    /// Names the pass in fixed-point validation errors.
    fn name(&self) -> &'static str {
        "count_hoist"
    }

    /// Moves eligible header count/load pairs into their loop preheader.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if has_exception_handlers(function)
            || function.flags.is_generator
            || function.flags.is_fiber_wrapper
            || super::local_scope::has_dynamic_eval(function)
        {
            return false;
        }
        let dominance = compute_dominance(function);
        let loops = compute_loops(function, &dominance);
        if loops.is_empty() {
            return false;
        }
        let mut order: Vec<usize> = (0..loops.loops().len()).collect();
        order.sort_by_key(|&index| std::cmp::Reverse(loops.loops()[index].depth));
        let mut def_block = build_def_block(function);
        let mut changed = false;
        for index in order {
            let loop_ref = &loops.loops()[index];
            let Some(preheader) = loop_ref.preheader else {
                continue;
            };
            for (load, count) in find_count_moves(function, loop_ref) {
                move_instruction(function, load, preheader, &mut def_block);
                move_instruction(function, count, preheader, &mut def_block);
                mark_origin(function, load);
                mark_origin(function, count);
                changed = true;
            }
        }
        if changed {
            recompute_instruction_defs(function);
        }
        changed
    }
}

/// Records that the hoist moved `id`, so EIR dumps attribute the move to this pass.
fn mark_origin(function: &mut Function, id: InstId) {
    if let Some(inst) = function.instruction_mut(id) {
        inst.origin = Some(PassOrigin::CountHoist);
    }
}

/// Returns the `(receiver load, count)` pairs a loop may hoist together.
fn find_count_moves(function: &Function, loop_ref: &NaturalLoop) -> Vec<(InstId, InstId)> {
    let Some(header) = function.block(loop_ref.header) else {
        return Vec::new();
    };
    let mut moves = Vec::new();
    for &id in &header.instructions {
        let Some(inst) = function.instruction(id) else {
            continue;
        };
        if !is_single_argument_count(inst) {
            continue;
        }
        let Some(&argument) = inst.operands.first() else {
            continue;
        };
        let Some(load_id) = defining_id(function, argument) else {
            continue;
        };
        let Some(load) = function.instruction(load_id) else {
            continue;
        };
        if load.op != Op::LoadLocal {
            continue;
        }
        let Some(Immediate::LocalSlot(slot)) = load.immediate else {
            continue;
        };
        if slot_is_loop_invariant(function, loop_ref, slot) {
            moves.push((load_id, id));
        }
    }
    moves
}

/// Requires a one-argument `count()` call.
pub(super) fn is_single_argument_count(inst: &Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && inst.operands.len() == 1
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::Count
            )))
        )
}

/// Proves that every loop-block use of the slot reads an invariant container.
pub(super) fn slot_is_loop_invariant(
    function: &Function,
    loop_ref: &NaturalLoop,
    slot: LocalSlotId,
) -> bool {
    let Some(local) = function.locals.get(slot.as_raw() as usize) else {
        return false;
    };
    if local.kind != LocalKind::PhpLocal
        || !matches!(
            local.php_type.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. }
        )
    {
        return false;
    }
    // A runtime include hands the frame to evaluated code; an alias could rebind the
    // local without appearing as a load in this loop.
    if function.instructions.iter().any(|inst| {
        inst.op == Op::RuntimeCall && matches!(inst.immediate, Some(Immediate::Data(_)))
    }) {
        return false;
    }
    let mut loads = HashSet::new();
    for &block_id in &loop_ref.blocks {
        let Some(block) = function.block(block_id) else {
            return false;
        };
        for &id in &block.instructions {
            let Some(inst) = function.instruction(id) else {
                return false;
            };
            if inst.immediate != Some(Immediate::LocalSlot(slot)) {
                continue;
            }
            match inst.op {
                Op::LoadLocal => {
                    if let Some(result) = inst.result {
                        loads.insert(result);
                    }
                }
                // Any store, retirement, element-address or reference operation in the
                // loop can change the slot or its storage.
                _ => return false,
            }
        }
    }
    if loads.is_empty() {
        return false;
    }
    for &block_id in &loop_ref.blocks {
        let Some(block) = function.block(block_id) else {
            return false;
        };
        for &id in &block.instructions {
            let Some(inst) = function.instruction(id) else {
                return false;
            };
            if inst
                .operands
                .iter()
                .any(|operand| loads.contains(operand) && !read_only_container_consumer(inst, *operand))
            {
                return false;
            }
        }
    }
    true
}

/// Returns true when `value` reaches `inst` only as a container to read.
fn read_only_container_consumer(inst: &Instruction, value: ValueId) -> bool {
    match inst.op {
        Op::RuntimeCall if is_single_argument_count(inst) => true,
        Op::ArrayGet
        | Op::ArrayGetUnchecked
        | Op::ArrayGetSilent
        | Op::ArrayLen
        | Op::ArrayIsset
        | Op::ArrayKeyExists
        | Op::HashGet
        | Op::HashGetSilent
        | Op::HashLen
        | Op::HashIsset => inst.operands.first() == Some(&value),
        _ => false,
    }
}

/// Returns the instruction that defines `value`, or `None` for a block parameter.
pub(super) fn defining_id(function: &Function, value: ValueId) -> Option<InstId> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some(inst)
}
