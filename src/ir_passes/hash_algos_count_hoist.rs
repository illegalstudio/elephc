//! Purpose:
//! Hoists `count(hash_algos())` out of a loop because the zero-argument catalog is
//! invariant and only its count is observed.
//!
//! Called from:
//! - The EIR fixed-point pass driver after loop construction.
//!
//! Key details:
//! - The contiguous call/store/push/count/pop/unset/release range moves to the preheader,
//!   preserving array ownership and the temporary slot lifetimes.
//! - User calls, memory/GC observers, generators, fibers, and dynamic eval disqualify the
//!   function because moving the allocation changes observable memory timing.
//! - Only a count consumer is accepted; a whole-array observation keeps the call in the loop.

use std::collections::HashSet;

use crate::ir::{
    BlockId, DataPool, Function, Immediate, InstId, LocalSlotId, Op, PassOrigin,
    RuntimeCallTarget, RuntimeFnId, ValueDef, ValueId,
};

use super::cfg::has_exception_handlers;
use super::dominance::compute_dominance;
use super::driver::IrPass;
use super::licm::recompute_instruction_defs;
use super::loops::{compute_loops, NaturalLoop};

/// Hoists a zero-argument `hash_algos()` count into the loop preheader.
pub struct HashAlgosCountHoist;

impl IrPass for HashAlgosCountHoist {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "hash_algos_count_hoist"
    }

    /// Skips functions without a hash_algos call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(direct_target(inst), Some(RuntimeFnId::HashAlgos))
        })
    }

    /// Hoists every independently proven count.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if has_exception_handlers(function)
            || function.flags.is_generator
            || function.flags.is_fiber_wrapper
            || super::local_scope::has_dynamic_eval(function)
            || !function_is_isolated(function)
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
        let mut changed = false;
        for index in order {
            let loop_ref = &loops.loops()[index];
            let Some(preheader) = loop_ref.preheader else {
                continue;
            };
            if let Some(range) = find_range(function, loop_ref) {
                move_range(function, range, preheader);
                changed = true;
                break;
            }
        }
        if changed {
            recompute_instruction_defs(function);
        }
        changed
    }
}

/// One contiguous hash_algos/count/release range.
struct Range {
    block: BlockId,
    start: usize,
    end: usize,
    call: InstId,
    count: InstId,
}

/// Returns the direct runtime target for a call.
fn direct_target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Rejects functions where a user call or memory/GC observer could see the moved array.
fn function_is_isolated(function: &Function) -> bool {
    function.instructions.iter().all(|inst| match inst.op {
        Op::Call
        | Op::FunctionVariantCall
        | Op::MethodCall
        | Op::StaticMethodCall
        | Op::NullsafeMethodCall
        | Op::LanguageConstructCall
        | Op::EvalLiteralCall
        | Op::EvalFunctionCall
        | Op::EvalFunctionCallArray
        | Op::ExternCall => false,
        Op::RuntimeCall => match direct_target(inst) {
            Some(target) => !is_memory_or_gc_target(target),
            None => true,
        },
        _ => true,
    })
}

/// Returns whether the target observes heap/GC timing that hoisting would change.
fn is_memory_or_gc_target(target: RuntimeFnId) -> bool {
    matches!(
        target,
        RuntimeFnId::MemoryGetAllocations
            | RuntimeFnId::MemoryGetFrees
            | RuntimeFnId::MemoryGetPeakUsage
            | RuntimeFnId::MemoryGetUsage
            | RuntimeFnId::MemoryResetPeakUsage
    )
}

/// Finds one hoistable range in a natural loop.
fn find_range(function: &Function, loop_ref: &NaturalLoop) -> Option<Range> {
    for &block_id in &loop_ref.blocks {
        let block = function.block(block_id)?;
        for (index, &count_id) in block.instructions.iter().enumerate() {
            let count = function.instruction(count_id)?;
            if direct_target(count) != Some(RuntimeFnId::Count) || count.operands.len() != 1 {
                continue;
            }
            let Some(call_id) = defining_id(function, count.operands[0]) else {
                continue;
            };
            let call = function.instruction(call_id)?;
            if direct_target(call) != Some(RuntimeFnId::HashAlgos) || !call.operands.is_empty() {
                continue;
            }
            let call_pos = block.instructions.iter().position(|&id| id == call_id)?;
            if call_pos >= index {
                continue;
            }
            let end = scan_end(function, block, index)?;
            if !range_is_staging(function, block, call_pos, end, call_id, count_id) {
                continue;
            }
            let temps = temp_slots(function, block, call_pos, end);
            if temps.is_empty() || slot_used_outside(function, block, call_pos, end, &temps) {
                continue;
            }
            return Some(Range { block: block.id, start: call_pos, end, call: call_id, count: count_id });
        }
    }
    None
}

/// Finds the first instruction after the count that is still call cleanup.
fn scan_end(function: &Function, block: &crate::ir::BasicBlock, count_pos: usize) -> Option<usize> {
    let mut end = count_pos + 1;
    while end < block.instructions.len() {
        let inst = function.instruction(block.instructions[end])?;
        if is_cleanup(inst) {
            end += 1;
        } else {
            break;
        }
    }
    Some(end)
}

/// Returns whether the range contains exactly one call and one count and only staging.
fn range_is_staging(
    function: &Function,
    block: &crate::ir::BasicBlock,
    start: usize,
    end: usize,
    call: InstId,
    count: InstId,
) -> bool {
    let mut calls = 0;
    let mut counts = 0;
    for &id in &block.instructions[start..end] {
        let Some(inst) = function.instruction(id) else {
            return false;
        };
        if id == call {
            calls += 1;
        } else if id == count {
            counts += 1;
        } else if !is_cleanup(inst) && !is_staging(inst) {
            return false;
        }
    }
    calls == 1 && counts == 1
}

/// Returns whether the instruction stores or protects the call result.
fn is_staging(inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.op,
        Op::StoreLocal | Op::PushCallOperandOwner | Op::PopCallOperandOwner | Op::ReleaseLocalSlot
    )
}

/// Returns whether the instruction retires the call result or its temporary slot.
fn is_cleanup(inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.op,
        Op::PopCallOperandOwner | Op::UnsetLocal | Op::Release | Op::ReleaseLocalSlot | Op::Nop
    )
}

/// Collects temporary slots touched by the range.
fn temp_slots(
    function: &Function,
    block: &crate::ir::BasicBlock,
    start: usize,
    end: usize,
) -> HashSet<LocalSlotId> {
    block.instructions[start..end]
        .iter()
        .filter_map(|id| function.instruction(*id))
        .filter_map(|inst| match inst.immediate {
            Some(Immediate::LocalSlot(slot)) => Some(slot),
            _ => None,
        })
        .collect()
}

/// Rejects a range whose temporary slots are also touched outside it.
fn slot_used_outside(
    function: &Function,
    block: &crate::ir::BasicBlock,
    start: usize,
    end: usize,
    slots: &HashSet<LocalSlotId>,
) -> bool {
    let inside: HashSet<InstId> = block.instructions[start..end].iter().copied().collect();
    function.instructions.iter().enumerate().any(|(raw, inst)| {
        let id = InstId::from_raw(raw as u32);
        if inside.contains(&id) {
            return false;
        }
        matches!(inst.immediate, Some(Immediate::LocalSlot(slot)) if slots.contains(&slot))
    })
}

/// Moves one contiguous range to the preheader and records its provenance.
fn move_range(function: &mut Function, range: Range, preheader: BlockId) {
    let Some(block) = function.block_mut(range.block) else {
        return;
    };
    let moved: Vec<InstId> = block.instructions.drain(range.start..range.end).collect();
    let Some(preheader_block) = function.block_mut(preheader) else {
        return;
    };
    preheader_block.instructions.extend(moved);
    for id in [range.call, range.count] {
        if let Some(inst) = function.instruction_mut(id) {
            inst.origin = Some(PassOrigin::Licm);
        }
    }
}

/// Returns the defining instruction identifier for one SSA value.
fn defining_id(function: &Function, value: ValueId) -> Option<InstId> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some(inst)
}
