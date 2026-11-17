//! Purpose:
//! Fuses a stored `array_map(fn($v) => $v / C, $ints)` temporary whose only read is
//! `array_sum()` into one packed-int map+sum kernel.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the stored numeric-map lowering and before the
//!   integer sum sinks.
//!
//! Key details:
//! - The mapped local must be a PHP local with exactly one store (from `array_map_int_div`),
//!   exactly one load (the `array_sum` receiver), and only slot-release cleanup mentions.
//!   Any other read, write, alias or escape keeps the materialized map.
//! - The source must be a non-escaping packed `array<int>` local, the divisor a nonzero
//!   `ConstI64`, and every scaffold instruction must live in the call block in source order.
//! - The fused call consumes the same source SSA value the map consumed, so source writes
//!   between the map and the sum reject the rewrite.
//! - The runtime kernel preserves PHP's exact-int-or-double division, `PHP_INT_MIN / -1`
//!   promotion, left-to-right sum order and int-overflow promotion; one owned Mixed cell is
//!   returned exactly like `array_sum()` over the materialized map.

use std::collections::HashSet;

use crate::ir::{
    DataPool, Function, Immediate, InstId, LocalKind, LocalSlotId, Op, RuntimeCallTarget,
    RuntimeFnId, ValueId,
};
use crate::types::PhpType;

use super::array_sum_int_direct::{
    acquire_store, acquired_uses_are_scaffold, call_positions, defining_instruction_with_id,
    instruction_positions, temp_scaffold,
};
use super::driver::IrPass;
use super::rewrite::neutralize_to_nop;

/// Fuses stored int-division maps whose only observer is the sum reducer.
pub struct ArrayMapSumDivFusion;

impl IrPass for ArrayMapSumDivFusion {
    /// Returns the stable pass name used in fixed-point diagnostics.
    fn name(&self) -> &'static str {
        "array_map_sum_div_fusion"
    }

    /// Skips functions with no boxed `array_sum()` call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_sum_call)
    }

    /// Rewrites every independently proven stored-map sum.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if super::cfg::has_exception_handlers(function)
            || function.flags.is_generator
            || function.flags.is_fiber_wrapper
            || super::local_scope::has_dynamic_eval(function)
            || function.instructions.iter().any(|inst| {
                inst.op == Op::RuntimeCall && matches!(inst.immediate, Some(Immediate::Data(_)))
            })
        {
            return false;
        }
        let escaping = super::by_ref_alias::address_escaping_slots(function);
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_sum_call(inst))
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if specialize(function, candidate, &escaping) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns whether `inst` is a one-operand boxed `array_sum()` call.
fn is_sum_call(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ArraySum
            )))
        )
        && inst.operands.len() == 1
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Mixed
}

/// Fuses one stored map+sum graph after proving every consumer and scaffold role.
fn specialize(
    function: &mut Function,
    candidate: InstId,
    escaping: &HashSet<LocalSlotId>,
) -> bool {
    let Some(call) = function.instruction(candidate) else {
        return false;
    };
    let receiver = call.operands[0];
    let (receiver_acquire, receiver_value) =
        match defining_instruction_with_id(function, receiver) {
        Some((id, inst)) if inst.op == Op::Acquire && inst.operands.len() == 1 => {
            (Some(id), inst.operands[0])
        }
        _ => (None, receiver),
    };
    let Some((load_id, load)) = defining_instruction_with_id(function, receiver_value) else {
        return false;
    };
    if load.op != Op::LoadLocal {
        return false;
    }
    let Some(load_value) = load.result else {
        return false;
    };
    if !receiver_load_uses_are_scaffold(function, load_value, receiver_acquire, candidate) {
        return false;
    }
    let Some(Immediate::LocalSlot(mapped_slot)) = load.immediate else {
        return false;
    };
    let Some(mapped_local) = function.locals.get(mapped_slot.as_raw() as usize) else {
        return false;
    };
    if mapped_local.kind != LocalKind::PhpLocal || escaping.contains(&mapped_slot) {
        return false;
    }

    let Some(store_id) = unique_mapped_store(function, mapped_slot, load_id) else {
        return false;
    };
    let Some(stored) = function
        .instruction(store_id)
        .and_then(|inst| inst.operands.first().copied())
    else {
        return false;
    };
    let (map_acquire, map_result) = match defining_instruction_with_id(function, stored) {
        Some((id, inst)) if inst.op == Op::Acquire && inst.operands.len() == 1 => {
            (Some(id), inst.operands[0])
        }
        _ => (None, stored),
    };
    let Some((map_call_id, map_call)) = defining_instruction_with_id(function, map_result) else {
        return false;
    };
    if map_call.op != Op::RuntimeCall
        || map_call.operands.len() != 2
        || !matches!(
            map_call.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ArrayMapIntDiv
            )))
        )
    {
        return false;
    }
    let source = map_call.operands[0];
    let divisor = map_call.operands[1];
    if !source_is_packed_int_local(function, source)
        || !divisor_is_nonzero_literal(function, divisor)
    {
        return false;
    }
    let Some((source_load_id, source_slot)) = source_load(function, source) else {
        return false;
    };
    if !map_uses_are_scaffold(function, map_result, store_id, map_acquire) {
        return false;
    }
    if let Some(acquire) = map_acquire {
        let Some(acquired) = function.instruction(acquire).and_then(|inst| inst.result) else {
            return false;
        };
        if !acquired_uses_only_store(function, acquired, store_id) {
            return false;
        }
    }

    let positions = instruction_positions(function);
    let Some(&(call_block, call_position)) = positions.get(&candidate) else {
        return false;
    };
    let Some(&(load_block, load_position)) = positions.get(&load_id) else {
        return false;
    };
    let Some(&(store_block, store_position)) = positions.get(&store_id) else {
        return false;
    };
    let Some(&(map_block, map_position)) = positions.get(&map_call_id) else {
        return false;
    };
    let Some(&(source_block, source_position)) = positions.get(&source_load_id) else {
        return false;
    };
    if call_block != load_block
        || call_block != store_block
        || call_block != map_block
        || call_block != source_block
        || !(source_position < map_position
            && map_position < store_position
            && store_position < load_position
            && load_position < call_position)
    {
        return false;
    }
    if map_acquire.is_some_and(|acquire| {
        positions
            .get(&acquire)
            .is_none_or(|&(block, position)| block != call_block || position >= store_position)
    }) {
        return false;
    }
    if !source_slot_is_stable(
        function,
        source_slot,
        source_load_id,
        map_call_id,
        call_block,
        source_position,
        call_position,
    ) {
        return false;
    }

    let receiver_scaffold = if let Some(acquire_id) = receiver_acquire {
        let Some(acquired) = function.instruction(acquire_id).and_then(|inst| inst.result) else {
            return false;
        };
        let Some((temp_store, temp_slot)) = acquire_store(function, acquired) else {
            return false;
        };
        let Some(temp_local) = function.locals.get(temp_slot.as_raw() as usize) else {
            return false;
        };
        if temp_local.kind != LocalKind::HiddenTemp {
            return false;
        }
        let Some((push, pop, releases)) = temp_scaffold(function, temp_slot, temp_store) else {
            return false;
        };
        if releases.is_empty() || releases.len() > 2 {
            return false;
        }
        if call_positions(
            function,
            candidate,
            load_id,
            acquire_id,
            temp_store,
            push,
            pop,
            &releases,
        )
        .is_none()
            || !acquired_uses_are_scaffold(function, acquired, temp_store, candidate)
        {
            return false;
        }
        Some((temp_store, push, pop, releases, temp_slot))
    } else {
        None
    };

    let Some(call) = function.instruction_mut(candidate) else {
        return false;
    };
    call.operands = vec![source, divisor];
    call.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        RuntimeFnId::ArrayMapSumIntDiv,
    )));
    call.effects = RuntimeFnId::ArrayMapSumIntDiv.intrinsic_effects();

    let mut removable = vec![map_call_id, store_id, load_id];
    removable.extend(map_acquire);
    removable.extend(map_result_releases(function, map_result));
    removable.extend(receiver_acquire);
    if let Some((temp_store, push, pop, releases, temp_slot)) = receiver_scaffold {
        removable.extend([temp_store, push, pop]);
        removable.extend(releases);
        function.locals[temp_slot.as_raw() as usize].kind = LocalKind::BorrowedTemp;
    }
    for id in removable {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    true
}

/// Returns the single store into `mapped_slot`, rejecting every other use but slot releases.
fn unique_mapped_store(
    function: &Function,
    mapped_slot: LocalSlotId,
    expected_load: InstId,
) -> Option<InstId> {
    let mut store = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !super::checked_int_sink::instruction_mentions_slot(inst, mapped_slot) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::StoreLocal => {
                if store.replace(id).is_some() {
                    return None;
                }
            }
            Op::LoadLocal => {
                if id != expected_load {
                    return None;
                }
            }
            Op::ReleaseLocalSlot => {}
            _ => return None,
        }
    }
    store
}

/// Returns the load and slot backing a packed-int map source value.
fn source_load(function: &Function, source: ValueId) -> Option<(InstId, LocalSlotId)> {
    let (load_id, load) = defining_instruction_with_id(function, source)?;
    if load.op != Op::LoadLocal {
        return None;
    }
    let Some(Immediate::LocalSlot(slot)) = load.immediate else {
        return None;
    };
    Some((load_id, slot))
}

/// Returns whether `source` is a non-escaping packed `array<int>` local load.
fn source_is_packed_int_local(function: &Function, source: ValueId) -> bool {
    let Some((_, slot)) = source_load(function, source) else {
        return false;
    };
    let Some(local) = function.locals.get(slot.as_raw() as usize) else {
        return false;
    };
    local.kind == LocalKind::PhpLocal
        && matches!(
            local.php_type.codegen_repr(),
            PhpType::Array(element) if element.codegen_repr() == PhpType::Int
        )
}

/// Returns whether `divisor` is a nonzero `ConstI64` value.
fn divisor_is_nonzero_literal(function: &Function, divisor: ValueId) -> bool {
    let Some((_, inst)) = defining_instruction_with_id(function, divisor) else {
        return false;
    };
    inst.op == Op::ConstI64
        && matches!(inst.immediate, Some(Immediate::I64(value)) if value != 0)
}

/// Rejects every map-result use except the store scaffold, its acquire and releases.
fn map_uses_are_scaffold(
    function: &Function,
    map_result: ValueId,
    store: InstId,
    acquire: Option<InstId>,
) -> bool {
    let mut seen_acquire = false;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&map_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::Acquire if Some(id) == acquire => {
                if seen_acquire {
                    return false;
                }
                seen_acquire = true;
            }
            Op::Release => {}
            Op::StoreLocal if id == store => {}
            _ => return false,
        }
    }
    acquire.is_none_or(|_| seen_acquire)
        && function.blocks.iter().all(|block| {
            block
                .terminator
                .as_ref()
                .is_none_or(|term| !term.used_values().contains(&map_result))
        })
}

/// Rejects every receiver-load use except its acquire or the reducer call itself.
fn receiver_load_uses_are_scaffold(
    function: &Function,
    value: ValueId,
    acquire: Option<InstId>,
    call: InstId,
) -> bool {
    let mut uses = 0;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&value) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if Some(id) == acquire || id == call {
            uses += 1;
        } else {
            return false;
        }
    }
    uses == 1
}

/// Returns whether `value` is used exactly once, by `store`.
fn acquired_uses_only_store(function: &Function, value: ValueId, store: InstId) -> bool {
    let mut uses = 0;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&value) {
            continue;
        }
        if InstId::from_raw(raw as u32) != store {
            return false;
        }
        uses += 1;
    }
    uses == 1
}

/// Collects every release of the materialized map result.
fn map_result_releases(function: &Function, map_result: ValueId) -> Vec<InstId> {
    function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, inst)| inst.op == Op::Release && inst.operands.as_slice() == [map_result])
        .map(|(raw, _)| InstId::from_raw(raw as u32))
        .collect()
}

/// Proves the source slot keeps its loaded array alive and unmodified through the fused call.
///
/// Every mention must be a plain load, store or slot release, and every load result must reach
/// only value consumers (the map call itself for the source load). No store, unset, release or
/// element mutation may sit between the source load and the reducer call, so the SSA array the
/// map consumed cannot be freed or mutated before the fused kernel reads it.
fn source_slot_is_stable(
    function: &Function,
    source_slot: LocalSlotId,
    source_load: InstId,
    map_call: InstId,
    block_index: usize,
    source_position: usize,
    call_position: usize,
) -> bool {
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !super::checked_int_sink::instruction_mentions_slot(inst, source_slot) {
            continue;
        }
        if !matches!(inst.op, Op::LoadLocal | Op::StoreLocal | Op::ReleaseLocalSlot) {
            return false;
        }
        if inst.op != Op::LoadLocal {
            continue;
        }
        let Some(value) = inst.result else {
            return false;
        };
        let is_map_source = InstId::from_raw(raw as u32) == source_load;
        for (user_raw, user) in function.instructions.iter().enumerate() {
            if !user.operands.contains(&value) {
                continue;
            }
            if is_map_source && InstId::from_raw(user_raw as u32) == map_call {
                continue;
            }
            if !super::by_ref_alias::op_is_value_only_consumer(user.op) {
                return false;
            }
        }
    }
    let Some(block) = function.blocks.get(block_index) else {
        return false;
    };
    if source_position >= call_position || call_position > block.instructions.len() {
        return false;
    }
    !block.instructions[source_position + 1..call_position]
        .iter()
        .any(|&id| {
            function.instruction(id).is_some_and(|inst| {
                super::checked_int_sink::instruction_mentions_slot(inst, source_slot)
            })
        })
}
