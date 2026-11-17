//! Purpose:
//! Replaces `count(parse_url(...))` with the allocation-free `ParseUrlCount` helper when
//! the returned array is never observed as a value.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the length and count sinks.
//!
//! Key details:
//! - Only the full-array form (no component selector) is folded; a component result is a
//!   string/int/null/false value, where `count()` has different semantics.
//! - The parser still runs and returns the component bitmask count, or status zero for an
//!   invalid URL. The lowering raises PHP's count TypeError in that case.
//! - The temporary array slot and release scaffolding are retired.

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget,
    RuntimeFnId, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::neutralize_to_nop;

/// Rewrites count-only `parse_url()` observations.
pub struct ParseUrlCountSink;

impl IrPass for ParseUrlCountSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "parse_url_count_sink"
    }

    /// Skips functions with no count candidate.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_count_call)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_count_call(inst))
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if specialize(function, candidate) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns whether `inst` is a one-operand `count()` call.
fn is_count_call(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::Count
            )))
        )
        && inst.operands.len() == 1
        && inst.result.is_some()
}

/// Specializes one count call after proving the parsed array has no other observer.
fn specialize(function: &mut Function, count_id: InstId) -> bool {
    let Some(count) = function.instruction(count_id) else {
        return false;
    };
    let Some(parse_value) = count.operands.first().copied() else {
        return false;
    };
    let Some((parse_id, parse)) = defining_instruction_with_id(function, parse_value) else {
        return false;
    };
    if parse.op != Op::RuntimeCall
        || !matches!(
            parse.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ParseUrl
            )))
        )
        || parse.operands.len() != 1
    {
        return false;
    }
    let Some(mut removable) = removable_scaffolding(function, parse_value, count_id) else {
        return false;
    };
    let Some(url) = parse.operands.first().copied() else {
        return false;
    };
    let url = detach_argument_pin(function, url, parse_id).unwrap_or(url);

    let Some(count) = function.instruction_mut(count_id) else {
        return false;
    };
    count.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        RuntimeFnId::ParseUrlCount,
    )));
    count.operands = vec![url];
    count.result_type = IrType::I64;
    count.result_php_type = PhpType::Int;
    count.result_ownership = Ownership::NonHeap;
    count.effects = RuntimeFnId::ParseUrlCount.intrinsic_effects();
    removable.push(parse_id);
    for id in removable {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    prune_empty_owner_slots(function);
    true
}

/// Returns the parse result's removable scaffolding, or `None` on any other observer.
fn removable_scaffolding(
    function: &Function,
    parse_value: ValueId,
    count_id: InstId,
) -> Option<Vec<InstId>> {
    let mut removable = Vec::new();
    let mut slot = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&parse_value) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == count_id {
            continue;
        }
        match inst.op {
            Op::Release => removable.push(id),
            Op::StoreLocal => {
                if inst.operands.as_slice() == [parse_value] {
                    if let Some(Immediate::LocalSlot(found)) = inst.immediate {
                        slot = Some(found);
                    }
                    removable.push(id);
                } else {
                    return None;
                }
            }
            Op::Borrow => removable.push(id),
            _ => return None,
        }
    }
    let slot = slot?;
    for (raw, inst) in function.instructions.iter().enumerate() {
        let Some(Immediate::LocalSlot(candidate)) = inst.immediate else {
            continue;
        };
        if candidate != slot {
            continue;
        }
        match inst.op {
            Op::ReleaseLocalSlot
            | Op::StoreLocal
            | Op::PushCallOperandOwner
            | Op::PopCallOperandOwner
            | Op::UnsetLocal => removable.push(InstId::from_raw(raw as u32)),
            _ => return None,
        }
    }
    Some(removable)
}

/// Drops owner push/pop/unset scaffolding for slots with no stored value left.
fn prune_empty_owner_slots(function: &mut Function) {
    use std::collections::BTreeSet;
    let mut stored = BTreeSet::new();
    for inst in &function.instructions {
        if inst.op == Op::StoreLocal {
            if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                stored.insert(slot);
            }
        }
    }
    let mut empty = BTreeSet::new();
    for inst in &function.instructions {
        if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
            if !stored.contains(&slot)
                && matches!(
                    inst.op,
                    Op::PushCallOperandOwner
                        | Op::PopCallOperandOwner
                        | Op::UnsetLocal
                        | Op::ReleaseLocalSlot
                )
            {
                empty.insert(slot);
            }
        }
    }
    let removable: Vec<InstId> = function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, inst)| {
            matches!(
                inst.op,
                Op::PushCallOperandOwner
                    | Op::PopCallOperandOwner
                    | Op::UnsetLocal
                    | Op::ReleaseLocalSlot
            ) && matches!(inst.immediate, Some(Immediate::LocalSlot(slot)) if empty.contains(&slot))
        })
        .map(|(raw, _)| InstId::from_raw(raw as u32))
        .collect();
    for id in removable {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
}

/// Retires a local lifetime pin whose value is consumed only by the count rewrite.
fn detach_argument_pin(function: &mut Function, operand: ValueId, producer: InstId) -> Option<ValueId> {
    let (acquire_id, acquire) = defining_instruction_with_id(function, operand)?;
    if acquire.op != Op::Acquire
        || acquire.immediate != Some(Immediate::Bool(true))
        || acquire.operands.len() != 1
    {
        return None;
    }
    let source = acquire.operands[0];
    let Some(slot) = store_slot(function, operand) else {
        return None;
    };
    let mut removable = vec![acquire_id];
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&operand) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == producer {
            continue;
        }
        match inst.op {
            Op::Release | Op::StoreLocal | Op::Borrow => removable.push(id),
            _ => return None,
        }
    }
    for (raw, inst) in function.instructions.iter().enumerate() {
        let Some(Immediate::LocalSlot(candidate)) = inst.immediate else {
            continue;
        };
        if candidate != slot {
            continue;
        }
        match inst.op {
            Op::ReleaseLocalSlot
            | Op::StoreLocal
            | Op::PushCallOperandOwner
            | Op::PopCallOperandOwner
            | Op::UnsetLocal => removable.push(InstId::from_raw(raw as u32)),
            _ => return None,
        }
    }
    for id in removable {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    Some(source)
}

/// Returns the local slot written by the `StoreLocal` that stores `value`.
fn store_slot(function: &Function, value: ValueId) -> Option<crate::ir::LocalSlotId> {
    for inst in &function.instructions {
        if inst.op == Op::StoreLocal && inst.operands.as_slice() == [value] {
            if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                return Some(slot);
            }
        }
    }
    None
}

/// Returns the defining instruction and its id when `value` is instruction-defined.
fn defining_instruction_with_id(
    function: &Function,
    value: ValueId,
) -> Option<(InstId, &crate::ir::Instruction)> {
    let crate::ir::ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some((inst, function.instruction(inst)?))
}
