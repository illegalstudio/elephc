//! Purpose:
//! Replaces `count(str_split(...))` with the allocation-free `StrSplitCount` helper when
//! the temporary array is never observed as a value.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the length and position sinks.
//!
//! Key details:
//! - The `str_split()` result may be observed only by the count call, release scaffolding
//!   and its hidden temporary slot.
//! - The count call keeps the split's subject and chunk-length operands, so coercion and
//!   the `$length >= 1` `ValueError` remain at the original point.
//! - The source lifetime pin and slot are retired when present, matching the length-only
//!   transform sinks.

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget,
    RuntimeFnId, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::neutralize_to_nop;

/// Rewrites count-only `str_split()` observations.
pub struct StrSplitCountSink;

impl IrPass for StrSplitCountSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "str_split_count_sink"
    }

    /// Skips functions with no split-count candidate.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_count_call)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_count_call(inst))
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if specialize(function, candidate, data) {
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

/// Returns whether a value is a single literal separator byte with no regex meaning.
fn literal_split_separator(function: &Function, data: &DataPool, value: ValueId) -> bool {
    let Some(definition) = function.value(value) else { return false };
    let crate::ir::ValueDef::Instruction { inst, .. } = definition.def else {
        return false;
    };
    let Some(inst) = function.instruction(inst) else { return false };
    let (Op::ConstStr, Some(Immediate::Data(data_id))) = (inst.op, &inst.immediate) else {
        return false;
    };
    let Some(pattern) = data.strings.get(data_id.as_raw() as usize) else {
        return false;
    };
    let bytes = pattern.as_bytes();
    bytes.len() == 1
        && matches!(bytes[0], b',' | b';' | b':' | b'_' | b'=' | b'@' | b'!' | b'%' | b'&' | b'/')
}

/// Returns the runtime target of a direct or profiled call instruction.
fn direct_target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Specializes one count call after proving the split array has no other observer.
fn specialize(function: &mut Function, count_id: InstId, data: &DataPool) -> bool {
    let Some(count) = function.instruction(count_id) else {
        return false;
    };
    let Some(split_value) = count.operands.first().copied() else {
        return false;
    };
    let Some((split_id, split)) = defining_instruction_with_id(function, split_value) else {
        return false;
    };
    let Some(split_target) = direct_target(split) else {
        return false;
    };
    let (count_target, arity_ok) = match split_target {
        RuntimeFnId::StrSplit => (RuntimeFnId::StrSplitCount, matches!(split.operands.len(), 1 | 2)),
        RuntimeFnId::MbStrSplit => (RuntimeFnId::MbStrSplitCount, matches!(split.operands.len(), 2 | 3)),
        RuntimeFnId::MbSplit => (
            RuntimeFnId::MbSplitCount,
            split.operands.len() == 2
                && split
                    .operands
                    .first()
                    .is_some_and(|pattern| literal_split_separator(function, data, *pattern)),
        ),
        RuntimeFnId::MbEncodingAliases => {
            (RuntimeFnId::MbEncodingAliasesCount, split.operands.len() == 1)
        }
        RuntimeFnId::MbListEncodings => {
            (RuntimeFnId::MbListEncodingsCount, split.operands.is_empty())
        }
        _ => return false,
    };
    if split.op != Op::RuntimeCall || !arity_ok {
        return false;
    }
    let mut operands = split.operands.clone();
    let Some(removable) = removable_split_scaffolding(function, split_value, count_id) else {
        return false;
    };
    if let Some(first) = operands.first().copied() {
        if let Some(source) = detach_argument_pin(function, first, split_id) {
            operands[0] = source;
        }
    }

    let Some(count) = function.instruction_mut(count_id) else {
        return false;
    };
    count.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        count_target,
    )));
    count.operands = operands;
    count.result_type = IrType::I64;
    count.result_php_type = PhpType::Int;
    count.result_ownership = Ownership::NonHeap;
    count.effects = count_target.intrinsic_effects();
    for id in std::iter::once(split_id).chain(removable) {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    prune_empty_owner_slots(function);
    true
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

/// Returns the split result's removable scaffolding, or `None` on any other observer.
fn removable_split_scaffolding(
    function: &Function,
    split_value: ValueId,
    count_id: InstId,
) -> Option<Vec<InstId>> {
    let mut removable = Vec::new();
    let mut slot = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&split_value) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == count_id {
            continue;
        }
        match inst.op {
            Op::Release => removable.push(id),
            Op::StoreLocal => {
                if inst.operands.as_slice() == [split_value] {
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
