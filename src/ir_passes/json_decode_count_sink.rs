//! Purpose:
//! Folds `count(json_decode($s, true))` into a count-only runtime helper when the decoded
//! Mixed graph has no other observer.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the JSON length sink.
//!
//! Key details:
//! - The decode must have exactly two operands and an associative flag constant `true`.
//! - The decode result may be observed only by one `count()`, one store into a temporary
//!   slot, one release, and that slot's owner/unset scaffolding.
//! - The decode call itself is retargeted to the count-only helper and the separate
//!   `count()` call is retired, so the source is consumed before its owner releases it;
//!   both stable locals and temporary sources are safe.
//! - The helper keeps full JSON validation and falls back to the exact decoder for numeric
//!   roots and escaped/numeric object keys, so `count()` TypeErrors stay php-src exact.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, LocalKind, LocalSlotId, Op, Ownership,
    RuntimeCallTarget, RuntimeFnId, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites the exact count-of-decode graph to the count-only helper.
pub struct JsonDecodeCountSink;

impl IrPass for JsonDecodeCountSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "json_decode_count_sink"
    }

    /// Skips functions with no one-operand `count()` call.
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

/// Returns whether `inst` is a one-operand `count()` call returning int.
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
        && inst.result_php_type.codegen_repr() == PhpType::Int
}

/// Returns the defining instruction of a value, when it is instruction-defined.
fn defining_instruction(function: &Function, value: ValueId) -> Option<(InstId, &crate::ir::Instruction)> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some((inst, function.instruction(inst)?))
}

/// Returns the decode call and its source operand when it is the exact two-argument assoc form.
fn decode_source(function: &Function, value: ValueId) -> Option<(InstId, ValueId)> {
    let (id, decode) = defining_instruction(function, value)?;
    if decode.op != Op::RuntimeCall
        || !matches!(
            decode.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::JsonDecode
            )))
        )
        || decode.operands.len() != 2
        || decode.result != Some(value)
        || decode.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return None;
    }
    let assoc = *decode.operands.get(1)?;
    let (_, assoc_inst) = defining_instruction(function, assoc)?;
    if assoc_inst.op != Op::ConstBool || assoc_inst.immediate != Some(Immediate::Bool(true)) {
        return None;
    }
    Some((id, *decode.operands.first()?))
}

/// Returns the temporary slot and the removable decode-result scaffolding.
fn analyze_decode_uses(
    function: &Function,
    decode_result: ValueId,
    count_call: InstId,
) -> Option<(u32, Vec<InstId>)> {
    let mut store = None;
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&decode_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == count_call {
            continue;
        }
        match inst.op {
            Op::StoreLocal
                if inst.operands.as_slice() == [decode_result]
                    && matches!(inst.immediate, Some(Immediate::LocalSlot(_))) =>
            {
                let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
                    return None;
                };
                if store.replace((slot.as_raw(), id)).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [decode_result] => releases.push(id),
            _ => return None,
        }
    }
    for block in &function.blocks {
        if block
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().contains(&decode_result))
        {
            return None;
        }
    }
    let (slot, store) = store?;
    if releases.len() != 1 {
        return None;
    }
    releases.push(store);
    Some((slot, releases))
}

/// Returns the removable owner/unset scaffolding of the decode result's temporary slot.
fn analyze_slot_uses(function: &Function, slot_raw: u32) -> Option<Vec<InstId>> {
    let mut removable = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        let id = InstId::from_raw(raw as u32);
        let uses_slot = matches!(
            inst.immediate,
            Some(Immediate::LocalSlot(slot)) if slot.as_raw() == slot_raw
        );
        if !uses_slot {
            continue;
        }
        match inst.op {
            Op::StoreLocal => {}
            Op::PushCallOperandOwner | Op::PopCallOperandOwner | Op::UnsetLocal => {
                removable.push(id);
            }
            // The only store into this slot is the removed decode result, so the pre-store
            // retirement would release uninitialized frame bits; remove it with the store.
            Op::ReleaseLocalSlot => {
                removable.push(id);
            }
            _ => return None,
        }
    }
    Some(removable)
}

/// The original Mixed source and the scaffolding that can be retired for it.
struct DirectSource {
    /// The stable Mixed local load that becomes the decode operand.
    base: ValueId,
    /// Cast/acquire and temporary-owner instructions to retire.
    removable: Vec<InstId>,
    /// Temporary slots that become borrowed after their owner scaffolding is removed.
    slots: Vec<LocalSlotId>,
}

/// Returns the defining instruction and its id when `value` is instruction-defined.
fn defining_instruction_with_id(function: &Function, value: ValueId) -> Option<(InstId, &crate::ir::Instruction)> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some((inst, function.instruction(inst)?))
}

/// Returns the block position of each instruction.
fn instruction_positions(function: &Function) -> HashMap<InstId, (usize, usize)> {
    let mut positions = HashMap::new();
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (position, &id) in block.instructions.iter().enumerate() {
            positions.insert(id, (block_index, position));
        }
    }
    positions
}

/// Traces a `Str` decode source back through a Mixed cast/acquire chain to a stable local.
///
/// The post-merge `json_encode()` result is `Mixed`, so `json_decode()` receives an owned
/// `Cast` of a `Mixed` local and then an `Acquire` plus one or two temporary owner slots.
/// The count helper can consume the original Mixed cell directly: its boxed source loader
/// already borrows string payloads and applies the same runtime cast for other tags. Reusing
/// the original load removes a per-iteration cast/copy while the local keeps the value alive
/// through the call.
fn direct_mixed_source(
    function: &Function,
    source: ValueId,
    decode_call: InstId,
    escaping: &HashSet<LocalSlotId>,
) -> Option<DirectSource> {
    let mut current = source;
    let mut chain_ids = Vec::new();
    let mut chain_values = vec![source];
    let mut saw_mixed_cast = false;
    let (load_id, source_slot) = loop {
        let (id, inst) = defining_instruction_with_id(function, current)?;
        match inst.op {
            Op::Acquire if inst.operands.len() == 1 => {
                chain_ids.push(id);
                current = inst.operands[0];
            }
            Op::Cast
                if inst.immediate == Some(Immediate::CastTarget(IrType::Str))
                    && inst.operands.len() == 1 =>
            {
                let input = inst.operands[0];
                let input_type = function.value(input)?.php_type.codegen_repr();
                if !matches!(input_type, PhpType::Mixed | PhpType::Union(_)) {
                    return None;
                }
                chain_ids.push(id);
                chain_values.push(current);
                current = input;
                saw_mixed_cast = true;
            }
            Op::LoadLocal => {
                let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
                    return None;
                };
                break (id, slot);
            }
            _ => return None,
        }
    };
    if !saw_mixed_cast {
        return None;
    }
    let source_local = function.locals.get(source_slot.as_raw() as usize)?;
    if source_local.kind != LocalKind::PhpLocal
        || escaping.contains(&source_slot)
        || !matches!(source_local.php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
    {
        return None;
    }

    let chain_set: HashSet<InstId> = chain_ids.iter().copied().collect();
    let mut removable = chain_ids;
    let mut temp_slots = HashSet::new();
    for value in chain_values {
        for (raw, inst) in function.instructions.iter().enumerate() {
            if !inst.operands.contains(&value) {
                continue;
            }
            let id = InstId::from_raw(raw as u32);
            if chain_set.contains(&id) {
                continue;
            }
            match inst.op {
                Op::StoreLocal => {
                    let Some(Immediate::LocalSlot(temp)) = inst.immediate else {
                        return None;
                    };
                    temp_slots.insert(temp);
                    removable.push(id);
                }
                Op::Release if inst.operands.as_slice() == [value] => removable.push(id),
                Op::Borrow
                    if inst.result.is_some_and(|result| {
                        !function.instructions.iter().any(|other| other.operands.contains(&result))
                            && function.blocks.iter().all(|block| {
                                block
                                    .terminator
                                    .as_ref()
                                    .is_none_or(|term| !term.used_values().contains(&result))
                            })
                    }) =>
                {
                    removable.push(id);
                }
                _ if id == decode_call && value == source => {}
                _ => return None,
            }
        }
    }
    for &temp in &temp_slots {
        for (raw, inst) in function.instructions.iter().enumerate() {
            if !super::checked_int_sink::instruction_mentions_slot(inst, temp) {
                continue;
            }
            match inst.op {
                Op::StoreLocal
                | Op::PushCallOperandOwner
                | Op::PopCallOperandOwner
                | Op::UnsetLocal
                | Op::ReleaseLocalSlot => removable.push(InstId::from_raw(raw as u32)),
                _ => return None,
            }
        }
    }
    let positions = instruction_positions(function);
    let (load_block, load_position) = *positions.get(&load_id)?;
    let (call_block, call_position) = *positions.get(&decode_call)?;
    if load_block != call_block || load_position >= call_position {
        return None;
    }
    for &id in &function.blocks[load_block].instructions[load_position + 1..call_position] {
        let inst = function.instruction(id)?;
        if super::checked_int_sink::instruction_mentions_slot(inst, source_slot) {
            return None;
        }
    }
    let mut slots: Vec<_> = temp_slots.into_iter().collect();
    slots.sort_by_key(|slot| slot.as_raw());
    Some(DirectSource {
        base: current,
        removable,
        slots,
    })
}

/// Specializes one count-of-decode call after proving the complete use graph.
fn specialize(function: &mut Function, candidate: InstId) -> bool {
    let Some(count) = function.instruction(candidate) else {
        return false;
    };
    let Some(count_result) = count.result else {
        return false;
    };
    let Some(&decoded) = count.operands.first() else {
        return false;
    };
    let Some((decode_call, _source)) = decode_source(function, decoded) else {
        return false;
    };
    let Some((slot, mut removable)) = analyze_decode_uses(function, decoded, candidate) else {
        return false;
    };
    let Some(mut slot_scaffolding) = analyze_slot_uses(function, slot) else {
        return false;
    };
    removable.append(&mut slot_scaffolding);

    let escaping = super::by_ref_alias::address_escaping_slots(function);
    let source_rewrite = {
        let Some(decode) = function.instruction(decode_call) else {
            return false;
        };
        let Some(&source) = decode.operands.first() else {
            return false;
        };
        direct_mixed_source(function, source, decode_call, &escaping)
    };
    if let Some(rewrite) = &source_rewrite {
        let Some(decode) = function.instruction_mut(decode_call) else {
            return false;
        };
        decode.operands[0] = rewrite.base;
    }

    let Some(decode) = function.instruction_mut(decode_call) else {
        return false;
    };
    decode.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        RuntimeFnId::JsonDecodeCount,
    )));
    decode.result_type = IrType::I64;
    decode.result_php_type = PhpType::Int;
    decode.result_ownership = Ownership::NonHeap;
    decode.effects = RuntimeFnId::JsonDecodeCount.intrinsic_effects();
    if let Some(value) = function.value_mut(decoded) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    replace_all_uses(
        function,
        &HashMap::from([(count_result, decoded)]),
    );
    if let Some(inst) = function.instruction_mut(candidate) {
        neutralize_to_nop(inst);
    }
    for id in removable {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    if let Some(rewrite) = source_rewrite {
        for id in rewrite.removable {
            if let Some(inst) = function.instruction_mut(id) {
                neutralize_to_nop(inst);
            }
        }
        for slot in rewrite.slots {
            if let Some(local) = function.locals.get_mut(slot.as_raw() as usize) {
                local.kind = LocalKind::BorrowedTemp;
            }
        }
    }
    true
}
