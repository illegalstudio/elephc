//! Purpose:
//! Hoists `count(token_get_all($literal))` out of a loop when the source is an
//! immutable compile-time string that parses successfully and the call is only
//! observed through the count.
//!
//! Called from:
//! - The EIR fixed-point pass driver after loop construction and constant hoisting.
//!
//! Key details:
//! - The whole contiguous argument-staging/call/count/release range moves to the loop
//!   preheader, preserving push/pop owner balance and the temporary-slot lifetimes.
//! - The source must parse with the compiler's own lexer/parser, proving
//!   `token_get_all()` cannot throw `ParseError`; no exception-handler function is hoisted.
//! - User calls, memory/GC observers, generators, fibers, and dynamic eval disqualify the
//!   function because moving the allocation/parse changes observable memory timing.

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
use super::string_len_sink::const_local_string;

/// Hoists a literal token_get_all count into the loop preheader.
pub struct TokenGetAllCountHoist;

impl IrPass for TokenGetAllCountHoist {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "token_get_all_count_hoist"
    }

    /// Skips functions without a token_get_all call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            inst.op == Op::Call
                && matches!(inst.immediate, Some(Immediate::Data(_)))
        })
    }

    /// Hoists every independently proven literal count.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        if has_exception_handlers(function)
            || function.flags.is_generator
            || function.flags.is_fiber_wrapper
            || super::local_scope::has_dynamic_eval(function)
            || !function_is_isolated(function, data)
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
            if let Some(range) = find_range(function, data, loop_ref) {
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

/// One contiguous staging/call/count/release instruction range.
struct Range {
    block: BlockId,
    start: usize,
    end: usize,
    call: InstId,
    count: InstId,
}

/// Rejects functions where another user call or a memory/GC observer could see the moved work.
fn function_is_isolated(function: &Function, data: &DataPool) -> bool {
    function.instructions.iter().all(|inst| match inst.op {
        Op::Call => {
            let id = match inst.immediate {
                Some(Immediate::Data(id)) | Some(Immediate::ProfiledData { data: id, .. }) => Some(id),
                _ => None,
            };
            let name = id.and_then(|id| data.function_names.get(id.as_raw() as usize).map(String::as_str));
            name == Some("token_get_all")
        }
        Op::FunctionVariantCall
        | Op::MethodCall
        | Op::StaticMethodCall
        | Op::NullsafeMethodCall
        | Op::LanguageConstructCall
        | Op::EvalLiteralCall
        | Op::EvalFunctionCall
        | Op::EvalFunctionCallArray
        | Op::ExternCall => false,
        Op::RuntimeCall => match direct_target(inst) {
            Some(target) => {
                !is_memory_or_gc_target(target)
            }
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

/// Finds one hoistable range in a natural loop.
fn find_range(function: &Function, data: &DataPool, loop_ref: &NaturalLoop) -> Option<Range> {
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
            if call.op != Op::Call || call.operands.len() != 2 {
                continue;
            }
            let Some(Immediate::Data(name_id)) = call.immediate else {
                continue;
            };
            let name = data.function_names.get(name_id.as_raw() as usize).map(String::as_str);
            if name != Some("token_get_all") {
                continue;
            }
            if const_i64(function, call.operands[1]) != Some(0) {
                continue;
            }
            let source_value = call.operands[0];
            let source_slot = source_slot(function, source_value);
            if source_slot.is_some_and(|slot| !slot_is_invariant(function, slot)) {
                continue;
            }
            let Some((source, _)) = const_local_string(function, data, source_value) else {
                continue;
            };
            let source = String::from_utf8(crate::string_bytes::literal_bytes(&source)).ok()?;
            let valid = source_is_valid_php(&source);
            if !valid {
                continue;
            }
            let call_pos = block.instructions.iter().position(|&id| id == call_id)?;
            if call_pos >= index {
                continue;
            }
            let start = scan_start(function, block, call_pos)?;
            let end = scan_end(function, block, index)?;
            if !range_is_staging(function, block, start, end, call_id, count_id) {
                continue;
            }
            let temps = temp_slots(function, block, start, end, source_slot);
            let outside = slot_used_outside(function, block, start, end, &temps);
            if temps.is_empty() || outside {
                continue;
            }
            return Some(Range { block: block.id, start, end, call: call_id, count: count_id });
        }
    }
    None
}

/// Returns the literal integer value produced by `ConstI64`.
fn const_i64(function: &Function, value: ValueId) -> Option<i64> {
    let inst = defining_instruction(function, value)?;
    match (inst.op, &inst.immediate) {
        (Op::ConstI64, Some(Immediate::I64(value))) => Some(*value),
        _ => None,
    }
}

/// Returns the immutable local behind the token_get_all source operand.
fn source_slot(function: &Function, value: ValueId) -> Option<LocalSlotId> {
    let inst = through_acquire(function, value)?;
    let inst = defining_instruction(function, inst)?;
    if inst.op == Op::LoadLocal {
        if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
            return Some(slot);
        }
    }
    None
}

/// Requires a single entry store, only loads, and scope cleanup.
fn slot_is_invariant(function: &Function, slot: LocalSlotId) -> bool {
    let Some(entry) = function.blocks.first() else {
        return false;
    };
    let mut stored = false;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !matches!(inst.immediate, Some(Immediate::LocalSlot(found)) if found == slot) {
            continue;
        }
        match inst.op {
            Op::StoreLocal if inst.operands.len() == 1 => {
                if stored || !entry.instructions.contains(&InstId::from_raw(raw as u32)) {
                    return false;
                }
                stored = true;
            }
            Op::LoadLocal | Op::ReleaseLocalSlot => {}
            _ => return false,
        }
    }
    stored
}

/// Validates the literal source with the compiler's own front end.
fn source_is_valid_php(source: &str) -> bool {
    if source.len() > 1024 * 1024 {
        return false;
    }
    let Ok(tokens) = crate::lexer::tokenize(source) else {
        return false;
    };
    crate::parser::parse(&tokens).is_ok()
}

/// Finds the first instruction of the call's contiguous staging run.
fn scan_start(function: &Function, block: &crate::ir::BasicBlock, call_pos: usize) -> Option<usize> {
    let mut start = call_pos;
    while start > 0 {
        let id = block.instructions[start - 1];
        let inst = function.instruction(id)?;
        if is_staging(inst) {
            start -= 1;
        } else {
            break;
        }
    }
    while start < call_pos {
        let Some(inst) = function.instruction(block.instructions[start]) else {
            return None;
        };
        if inst.op == Op::Nop {
            start += 1;
        } else {
            break;
        }
    }
    if start == call_pos {
        return None;
    }
    let first = function.instruction(block.instructions[start])?;
    (first.op == Op::PushCallOperandOwner).then_some(start)
}

/// Finds the first instruction after the count that is not post-call cleanup.
fn scan_end(function: &Function, block: &crate::ir::BasicBlock, count_pos: usize) -> Option<usize> {
    let mut end = count_pos + 1;
    while end < block.instructions.len() {
        let inst = function.instruction(block.instructions[end])?;
        if is_post_cleanup(inst) {
            end += 1;
        } else {
            break;
        }
    }
    Some(end)
}

/// Returns whether the range contains exactly one call and one count and only staging ops.
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
        } else if !is_staging(inst) && !is_post_cleanup(inst) {
            return false;
        }
    }
    calls == 1 && counts == 1
}

/// Returns whether the instruction belongs to call argument staging.
fn is_staging(inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.op,
        Op::PushCallOperandOwner
            | Op::PopCallOperandOwner
            | Op::LoadLocal
            | Op::Acquire
            | Op::StoreLocal
            | Op::ReleaseLocalSlot
            | Op::UnsetLocal
            | Op::Nop
            | Op::Borrow
            | Op::Move
    )
}

/// Returns whether the instruction belongs to post-call cleanup.
fn is_post_cleanup(inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.op,
        Op::PopCallOperandOwner
            | Op::UnsetLocal
            | Op::Release
            | Op::ReleaseLocalSlot
            | Op::Nop
    )
}

/// Collects temporary slots touched by the range, excluding the source slot.
fn temp_slots(
    function: &Function,
    block: &crate::ir::BasicBlock,
    start: usize,
    end: usize,
    source_slot: Option<LocalSlotId>,
) -> HashSet<LocalSlotId> {
    let mut slots = HashSet::new();
    for &id in &block.instructions[start..end] {
        let Some(inst) = function.instruction(id) else {
            continue;
        };
        if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
            if Some(slot) != source_slot {
                slots.insert(slot);
            }
        }
    }
    slots
}

/// Rejects a range whose temporary slots are also touched outside the range.
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

/// Moves one contiguous instruction range to the preheader and records its provenance.
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

/// Follows one `Acquire` in front of a value.
fn through_acquire(function: &Function, value: ValueId) -> Option<ValueId> {
    let inst = defining_instruction(function, value)?;
    if inst.op == Op::Acquire && inst.operands.len() == 1 {
        Some(inst.operands[0])
    } else {
        Some(value)
    }
}

/// Returns the defining instruction for one SSA value.
fn defining_instruction(function: &Function, value: ValueId) -> Option<&crate::ir::Instruction> {
    let id = defining_id(function, value)?;
    function.instruction(id)
}
