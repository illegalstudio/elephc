//! Purpose:
//! Removes the temporary-owner scaffold around a raw `array_sum()` reducer whose
//! receiver is a stable local array.
//!
//! Called from:
//! - The EIR fixed-point pass driver after `array_sum_int_sink`.
//!
//! Key details:
//! - The raw reducer reads a borrowed packed array and cannot throw; a non-escaping PHP
//!   local already owns the receiver through the call, so the acquire temporary, the
//!   call-operand owner frame and both temporary-slot releases are redundant.
//! - The source load must dominate the call in the same block, the source slot must not be
//!   address-escaping or written between the load and the call, and the temporary slot
//!   must have no use other than this call's scaffold.
//! - Any unexpected use, reference alias, intervening write or foreign temporary slot use
//!   keeps the original scaffold.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    DataPool, Function, Immediate, InstId, LocalKind, LocalSlotId, Op, Ownership,
    RuntimeCallTarget, RuntimeFnId, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::neutralize_to_nop;

const LOAD: usize = 0;
const ACQUIRE: usize = 1;
const STORE: usize = 2;
const PUSH: usize = 3;
const CALL: usize = 4;
const POP: usize = 5;
const RELEASE_BEFORE: usize = 6;
const RELEASE_AFTER: usize = 7;

/// Rewrites a raw `array_sum_int_value` call over a stable local to a direct borrow.
pub struct ArraySumIntDirect;

impl IrPass for ArraySumIntDirect {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "array_sum_int_direct"
    }

    /// Skips functions that have no raw reducer call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_raw_sum_call)
    }

    /// Specializes every independently proven reducer call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_raw_sum_call(inst))
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        if candidates.is_empty() {
            return false;
        }
        let escaping = super::by_ref_alias::address_escaping_slots(function);
        let mut changed = false;
        for candidate in candidates {
            if specialize(function, candidate, &escaping) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns whether `inst` is a one-operand raw array-sum reducer call.
fn is_raw_sum_call(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ArraySumInt
            )))
        )
        && inst.operands.len() == 1
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Int
}

/// Returns the defining instruction and its id when `value` is instruction-defined.
fn defining_instruction_with_id(function: &Function, value: ValueId) -> Option<(InstId, &crate::ir::Instruction)> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some((inst, function.instruction(inst)?))
}

/// Returns the block index and instruction position of every instruction.
fn instruction_positions(function: &Function) -> HashMap<InstId, (usize, usize)> {
    let mut positions = HashMap::new();
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (position, &id) in block.instructions.iter().enumerate() {
            positions.insert(id, (block_index, position));
        }
    }
    positions
}

/// Specializes one raw reducer call after proving the complete source and temporary graph.
fn specialize(
    function: &mut Function,
    candidate: InstId,
    escaping: &HashSet<LocalSlotId>,
) -> bool {
    let Some(call) = function.instruction(candidate) else {
        return false;
    };
    let Some(&acquired) = call.operands.first() else {
        return false;
    };
    let Some((acquire_id, acquire)) = defining_instruction_with_id(function, acquired) else {
        return false;
    };
    if acquire.op != Op::Acquire || acquire.operands.len() != 1 {
        return false;
    }
    let loaded = acquire.operands[0];
    let Some(loaded_value) = function.value(loaded) else {
        return false;
    };
    if !matches!(loaded_value.ownership, Ownership::Borrowed | Ownership::MaybeOwned) {
        return false;
    }
    let Some((load_id, load)) = defining_instruction_with_id(function, loaded) else {
        return false;
    };
    let Some(Immediate::LocalSlot(source_slot)) = load.immediate else {
        return false;
    };
    if load.op != Op::LoadLocal {
        return false;
    }
    let Some(source_local) = function.locals.get(source_slot.as_raw() as usize) else {
        return false;
    };
    if source_local.kind != LocalKind::PhpLocal || escaping.contains(&source_slot) {
        return false;
    }
    if !matches!(
        source_local.php_type.codegen_repr(),
        PhpType::Array(element) if element.codegen_repr() == PhpType::Int
    ) {
        return false;
    }

    let Some((store_id, temp_slot)) = acquire_store(function, acquired) else {
        return false;
    };
    let Some(temp_local) = function.locals.get(temp_slot.as_raw() as usize) else {
        return false;
    };
    if temp_local.kind != LocalKind::HiddenTemp {
        return false;
    }
    let Some((push_id, pop_id, releases)) = temp_scaffold(function, temp_slot, store_id) else {
        return false;
    };
    if releases.is_empty() || releases.len() > 2 {
        return false;
    }
    let Some(positions) = call_positions(
        function,
        candidate,
        load_id,
        acquire_id,
        store_id,
        push_id,
        pop_id,
        &releases,
    ) else {
        return false;
    };
    let (call_block, call_position) = positions[CALL];
    let (load_block, load_position) = positions[LOAD];
    if call_block != load_block || load_position >= call_position {
        return false;
    }
    let block = &function.blocks[call_block];
    for &id in &block.instructions[load_position + 1..call_position] {
        let Some(inst) = function.instruction(id) else {
            return false;
        };
        if super::checked_int_sink::instruction_mentions_slot(inst, source_slot) {
            return false;
        }
    }
    if !acquired_uses_are_scaffold(function, acquired, store_id, candidate) {
        return false;
    }

    let Some(call) = function.instruction_mut(candidate) else {
        return false;
    };
    call.operands[0] = loaded;
    let mut removable = vec![acquire_id, store_id, push_id, pop_id];
    removable.extend(releases);
    for id in removable {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    function.locals[temp_slot.as_raw() as usize].kind = LocalKind::BorrowedTemp;
    true
}

/// Returns the instruction positions of the complete scaffold in role order.
fn call_positions(
    function: &Function,
    call: InstId,
    load: InstId,
    acquire: InstId,
    store: InstId,
    push: InstId,
    pop: InstId,
    releases: &[InstId],
) -> Option<[(usize, usize); 8]> {
    let positions = instruction_positions(function);
    let ids = [load, acquire, store, push, call, pop];
    let mut result = [(0usize, 0usize); 8];
    for (index, id) in ids.into_iter().enumerate() {
        result[index] = *positions.get(&id)?;
    }
    let mut release_positions: Vec<_> = releases
        .iter()
        .map(|id| positions.get(id).copied())
        .collect::<Option<Vec<_>>>()?;
    release_positions.sort_by_key(|(_, position)| *position);
    let (before, after) = match release_positions.as_slice() {
        [only] => ((usize::MAX, 0), *only),
        [first, second] => (*first, *second),
        _ => return None,
    };
    result[RELEASE_BEFORE] = before;
    result[RELEASE_AFTER] = after;
    let (load_block, load_position) = result[LOAD];
    let (acquire_block, acquire_position) = result[ACQUIRE];
    let (store_block, store_position) = result[STORE];
    let (push_block, push_position) = result[PUSH];
    let (call_block, call_position) = result[CALL];
    let (pop_block, pop_position) = result[POP];
    let (before_block, before_position) = result[RELEASE_BEFORE];
    let (after_block, after_position) = result[RELEASE_AFTER];
    let before_ok = before_block == usize::MAX
        || (before_block == call_block && before_position < store_position);
    if load_block != call_block
        || acquire_block != call_block
        || store_block != call_block
        || push_block != call_block
        || pop_block != call_block
        || after_block != call_block
        || !before_ok
        || !(load_position < acquire_position
            && acquire_position < store_position
            && store_position < push_position
            && push_position < call_position
            && call_position < pop_position
            && pop_position < after_position)
    {
        return None;
    }
    Some(result)
}

/// Returns the temporary store and its slot for the acquired receiver value.
fn acquire_store(function: &Function, acquired: ValueId) -> Option<(InstId, LocalSlotId)> {
    let mut found = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if inst.op != Op::StoreLocal || inst.operands.as_slice() != [acquired] {
            continue;
        }
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            continue;
        };
        if found.replace((InstId::from_raw(raw as u32), slot)).is_some() {
            return None;
        }
    }
    found
}

/// Returns the owner push/pop and every release for the temporary slot.
fn temp_scaffold(
    function: &Function,
    temp_slot: LocalSlotId,
    expected_store: InstId,
) -> Option<(InstId, InstId, Vec<InstId>)> {
    let mut push = None;
    let mut pop = None;
    let mut releases = Vec::new();
    let mut stores = 0;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !super::checked_int_sink::instruction_mentions_slot(inst, temp_slot) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::StoreLocal => {
                if id != expected_store {
                    return None;
                }
                stores += 1;
            }
            Op::PushCallOperandOwner => {
                if push.replace(id).is_some() {
                    return None;
                }
            }
            Op::PopCallOperandOwner => {
                if pop.replace(id).is_some() {
                    return None;
                }
            }
            Op::ReleaseLocalSlot => releases.push(id),
            _ => return None,
        }
    }
    if stores != 1 {
        return None;
    }
    Some((push?, pop?, releases))
}

/// Rejects every acquire-result use other than the temporary store and the reducer call.
fn acquired_uses_are_scaffold(
    function: &Function,
    acquired: ValueId,
    store: InstId,
    call: InstId,
) -> bool {
    let mut uses = 0;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&acquired) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id != store && id != call {
            return false;
        }
        uses += 1;
    }
    uses == 2
}
