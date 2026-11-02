//! Purpose:
//! Maps EIR SSA values needing backing storage to stable stack slots.
//!
//! Called from:
//! - `crate::codegen` function and instruction lowering helpers.
//!
//! Key details:
//! - Scalar constants with recipes or registers need no backing slots.
//! - Other non-void values retain slots, including runtime ownership markers.

use std::collections::HashMap;

use crate::ir::{Function, Immediate, IrType, Op, Ownership, ValueDef, ValueId};
use crate::types::PhpType;
use crate::ir_passes::Allocation;

/// Backing stack slots and dynamic return-ownership markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuePlacement {
    pub slot_of: HashMap<ValueId, usize>,
    pub runtime_return_ownership_slot_of: HashMap<ValueId, usize>,
    pub total_slot_bytes: usize,
}

impl ValuePlacement {
    /// Moves every slot `delta` bytes farther from the frame pointer, leaving `total_slot_bytes`
    /// as the size of the value area itself. The frame layout places the table after the slots it
    /// wants nearer the frame pointer.
    pub fn shift_slots(&mut self, delta: usize) {
        for offset in self.slot_of.values_mut() {
            *offset += delta;
        }
        for offset in self.runtime_return_ownership_slot_of.values_mut() {
            *offset += delta;
        }
    }

    /// Returns the frame-pointer-relative byte offset for a value if it has a slot.
    pub fn slot(&self, value: ValueId) -> Option<usize> {
        self.slot_of.get(&value).copied()
    }

    /// Returns the spill slot for a dynamic PHP-call return ownership marker.
    pub fn runtime_return_ownership_slot(&self, value: ValueId) -> Option<usize> {
        self.runtime_return_ownership_slot_of.get(&value).copied()
    }
}

/// Returns true when a value receives a path-specific runtime ownership marker.
fn publishes_runtime_ownership_status(func: &Function, value: ValueId) -> bool {
    let Some(value) = func.value(value) else {
        return false;
    };
    let ValueDef::Instruction { inst, .. } = value.def else {
        return false;
    };
    let Some(instruction) = func.instruction(inst) else {
        return false;
    };
    if matches!(instruction.op, Op::Call | Op::FunctionVariantCall) {
        return true;
    }
    if instruction.op != Op::Cast
        || instruction.immediate != Some(Immediate::CastTarget(IrType::Str))
    {
        return false;
    }
    let Some(source) = instruction.operands.first().copied() else {
        return false;
    };
    func.value(source).is_some_and(|source| {
        matches!(source.php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
    })
}

/// Allocates backing slots, omitting constants handled by the allocation recipe table.
pub fn allocate(func: &Function, allocation: &Allocation) -> ValuePlacement {
    let mut slot_of = HashMap::new();
    let mut runtime_return_ownership_slot_of = HashMap::new();
    let mut offset = 0usize;
    for (index, value) in func.values.iter().enumerate() {
        let value_id = ValueId::from_raw(index as u32);
        let bytes = bytes_for(value.ir_type);
        if bytes == 0 || allocation.is_constant(value_id) {
            continue;
        }
        offset += bytes;
        slot_of.insert(value_id, offset);
        if value.ownership == Ownership::MaybeOwned
            && publishes_runtime_ownership_status(func, value_id)
        {
            offset += 8;
            runtime_return_ownership_slot_of.insert(value_id, offset);
        }
    }
    ValuePlacement {
        slot_of,
        runtime_return_ownership_slot_of,
        total_slot_bytes: align_to_16(offset),
    }
}

/// One stack slot a value (or its ownership marker) needs, with the span it must stay intact.
struct SlotRequest {
    key: SlotKey,
    bytes: usize,
    start: u32,
    end: u32,
    weight: u32,
}

/// What a [`SlotRequest`] stores.
#[derive(Clone, Copy)]
enum SlotKey {
    Value(ValueId),
    OwnershipMarker(ValueId),
}

/// Returns whether slot sharing has been switched off for A/B comparisons.
fn slot_sharing_disabled() -> bool {
    std::env::var_os("ELEPHC_NO_SLOT_SHARING").is_some()
}

/// Allocates frame slots for the values the register allocator left on the stack.
///
/// `allocate` gives EVERY non-void value its own slot, including the ones the allocator keeps in a
/// register for their whole life (those slots are never read) and values whose live ranges do not
/// overlap. A large function carries thousands of them, which pushes everything behind them past
/// the 256-byte reach of the one-instruction AArch64 `ldur`/`stur` and costs a separate `sub` for
/// each access.
///
/// With `share` set, values whose linear live intervals are strictly disjoint use one slot, and
/// the slots are laid out by total use count so the hottest sit nearest the frame pointer. The
/// intervals are the ones the register allocator trusts, so `share` must only be set where
/// [`crate::ir_passes::live_intervals_are_sound`] holds. Intervals that merely touch do not share:
/// an instruction may write its result while it still reads an operand, and an edge copy reads
/// every argument before it writes any block parameter.
pub fn allocate_with(
    func: &Function,
    allocation: &crate::ir_passes::Allocation,
    share: bool,
) -> ValuePlacement {
    let share = share && !slot_sharing_disabled();
    if !share {
        return allocate_skipping_registers(func, allocation);
    }
    let liveness = crate::ir_passes::compute_liveness(func);
    let intervals = crate::ir_passes::build_intervals(func, &liveness);
    let mut by_value: HashMap<ValueId, (u32, u32, u32)> = HashMap::with_capacity(intervals.len());
    for interval in &intervals {
        by_value.insert(interval.value, (interval.start, interval.end, interval.weight));
    }
    let mut requests: Vec<SlotRequest> = Vec::new();
    let mut unreachable_values: Vec<(ValueId, usize)> = Vec::new();
    for (index, value) in func.values.iter().enumerate() {
        let value_id = ValueId::from_raw(index as u32);
        let bytes = bytes_for(value.ir_type);
        if bytes == 0 || allocation.register_of(value_id).is_some() {
            continue;
        }
        let needs_marker = value.ownership == Ownership::MaybeOwned
            && publishes_runtime_ownership_status(func, value_id);
        match by_value.get(&value_id) {
            Some(&(start, end, weight)) => {
                requests.push(SlotRequest {
                    key: SlotKey::Value(value_id),
                    bytes,
                    start,
                    end,
                    weight: weight + 1,
                });
                if needs_marker {
                    requests.push(SlotRequest {
                        key: SlotKey::OwnershipMarker(value_id),
                        bytes: 8,
                        start,
                        end,
                        weight: weight + 1,
                    });
                }
            }
            // A value defined in an unreachable block has no interval; it keeps a private slot.
            None => {
                unreachable_values.push((value_id, bytes));
                if needs_marker {
                    unreachable_values.push((value_id, 0));
                }
            }
        }
    }
    requests.sort_by_key(|request| (request.start, request.end));

    // Linear scan: `active` holds (end, slot) of slots still in use; `free` the reusable ones.
    struct Slot {
        bytes: usize,
        weight: u64,
    }
    let mut slots: Vec<Slot> = Vec::new();
    let mut slot_of_request: Vec<usize> = vec![0; requests.len()];
    let mut active: Vec<(u32, usize)> = Vec::new();
    let mut free: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    let class = |bytes: usize| usize::from(bytes > 8);
    for (index, request) in requests.iter().enumerate() {
        let mut keep = Vec::with_capacity(active.len());
        for (end, slot) in active.drain(..) {
            if end < request.start {
                free[class(slots[slot].bytes)].push(slot);
            } else {
                keep.push((end, slot));
            }
        }
        active = keep;
        let slot = match free[class(request.bytes)].pop() {
            Some(slot) => slot,
            None => {
                slots.push(Slot { bytes: request.bytes, weight: 0 });
                slots.len() - 1
            }
        };
        slots[slot].weight += u64::from(request.weight);
        slot_of_request[index] = slot;
        active.push((request.end, slot));
    }

    // Hottest slots first: they land nearest the frame pointer.
    let mut order: Vec<usize> = (0..slots.len()).collect();
    order.sort_by(|a, b| slots[*b].weight.cmp(&slots[*a].weight).then(a.cmp(b)));
    let mut offset_of_slot = vec![0usize; slots.len()];
    let mut offset = 0usize;
    for slot in order {
        offset += slots[slot].bytes;
        offset_of_slot[slot] = offset;
    }

    let mut slot_of = HashMap::new();
    let mut runtime_return_ownership_slot_of = HashMap::new();
    for (index, request) in requests.iter().enumerate() {
        let slot_offset = offset_of_slot[slot_of_request[index]];
        match request.key {
            SlotKey::Value(value) => {
                slot_of.insert(value, slot_offset);
            }
            SlotKey::OwnershipMarker(value) => {
                runtime_return_ownership_slot_of.insert(value, slot_offset);
            }
        }
    }
    for (value, bytes) in unreachable_values {
        if bytes == 0 {
            offset += 8;
            runtime_return_ownership_slot_of.insert(value, offset);
        } else {
            offset += bytes;
            slot_of.insert(value, offset);
        }
    }
    ValuePlacement {
        slot_of,
        runtime_return_ownership_slot_of,
        total_slot_bytes: align_to_16(offset),
    }
}

/// `allocate`, minus the values the register allocator keeps in a register for their whole life.
fn allocate_skipping_registers(
    func: &Function,
    allocation: &crate::ir_passes::Allocation,
) -> ValuePlacement {
    let mut slot_of = HashMap::new();
    let mut runtime_return_ownership_slot_of = HashMap::new();
    let mut offset = 0usize;
    for (index, value) in func.values.iter().enumerate() {
        let value_id = ValueId::from_raw(index as u32);
        let bytes = bytes_for(value.ir_type);
        if bytes == 0 || allocation.register_of(value_id).is_some() {
            continue;
        }
        offset += bytes;
        slot_of.insert(value_id, offset);
        if value.ownership == Ownership::MaybeOwned
            && publishes_runtime_ownership_status(func, value_id)
        {
            offset += 8;
            runtime_return_ownership_slot_of.insert(value_id, offset);
        }
    }
    ValuePlacement {
        slot_of,
        runtime_return_ownership_slot_of,
        total_slot_bytes: align_to_16(offset),
    }
}

/// Returns the slot size for one EIR storage type.
pub fn bytes_for(ty: IrType) -> usize {
    match ty {
        IrType::I64 | IrType::F64 | IrType::Heap(_) => 8,
        IrType::Str | IrType::TaggedScalar => 16,
        IrType::Void => 0,
    }
}

/// Rounds a byte count up to the next 16-byte stack-alignment boundary.
fn align_to_16(bytes: usize) -> usize {
    (bytes + 15) & !15
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for Phase 04 EIR value placement.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - These tests verify the stack-slot contract before instruction lowering uses it.

    use crate::ir_passes::Allocation;
    use crate::ir::{
        Builder, DataId, Function, Immediate, IrHeapKind, IrType, LocalSlotId, Op, Ownership,
    };
    use crate::types::PhpType;

    use super::{allocate, allocate_with, bytes_for};
    use crate::ir::Terminator;

    /// Builds `((1 + 2) + (3 + 4)) + ...`-style chains where every temporary dies at its next use.
    fn chain_function(links: usize) -> (Function, Vec<crate::ir::ValueId>) {
        let mut function = Function::new("chain".to_string(), IrType::I64, PhpType::Int);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let mut acc = builder.emit_const_i64(1);
        let mut values = vec![acc];
        for step in 0..links {
            let operand = builder.emit_const_i64(step as i64 + 2);
            acc = builder.emit_iadd(acc, operand);
            values.push(operand);
            values.push(acc);
        }
        builder.terminate(Terminator::Return { value: Some(acc) });
        (function, values)
    }

    /// A chain of dying temporaries needs a handful of slots, not one per value.
    #[test]
    fn sharing_reuses_slots_of_values_with_disjoint_intervals() {
        let (function, values) = chain_function(40);
        let plain = allocate_with(&function, &Allocation::all_spilled(), false);
        let shared = allocate_with(&function, &Allocation::all_spilled(), true);
        assert_eq!(plain.slot_of.len(), values.len());
        assert!(plain.total_slot_bytes >= values.len() * 8);
        assert!(
            shared.total_slot_bytes <= 48,
            "40 dying temporaries should share a few slots, got {} bytes",
            shared.total_slot_bytes
        );
        assert_eq!(shared.slot_of.len(), values.len(), "every value still has a slot");
    }

    /// Two values live at once never share, and an operand never shares with its own result.
    #[test]
    fn sharing_keeps_overlapping_and_adjacent_intervals_apart() {
        let (function, values) = chain_function(6);
        let shared = allocate_with(&function, &Allocation::all_spilled(), true);
        // `acc` is read by the add that defines the next `acc`: operand and result overlap at that
        // instruction, so consecutive accumulators must differ.
        let accumulators: Vec<_> = values.iter().skip(2).step_by(2).copied().collect();
        for pair in accumulators.windows(2) {
            assert_ne!(shared.slot(pair[0]), shared.slot(pair[1]), "operand and result shared a slot");
        }
        // The constant and the accumulator feeding one add are live together.
        let first_const = values[0];
        let second_const = values[1];
        assert_ne!(shared.slot(first_const), shared.slot(second_const));
    }

    /// Values the register allocator keeps in a register leave no slot behind.
    #[test]
    fn register_allocated_values_get_no_slot() {
        let (function, values) = chain_function(3);
        let mut registers = std::collections::HashMap::new();
        registers.insert(values[1], "x20");
        let allocation = crate::ir_passes::Allocation::from_assignments_for_tests(registers);
        let placement = allocate_with(&function, &allocation, false);
        assert!(placement.slot(values[1]).is_none());
        assert!(placement.slot(values[0]).is_some());
    }

    /// Generators and functions with handlers are never shared: their intervals are not exact.
    #[test]
    fn sharing_is_refused_where_intervals_are_not_exact() {
        let (mut function, _) = chain_function(4);
        assert!(crate::ir_passes::live_intervals_are_sound(&function));
        function.flags.is_generator = true;
        assert!(!crate::ir_passes::live_intervals_are_sound(&function));
    }

    /// Verifies one integer value gets one 8-byte slot and a 16-byte aligned frame area.
    #[test]
    fn allocates_one_i64_value_slot() {
        let mut function = Function::new("test".to_string(), IrType::I64, PhpType::Int);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(7);

        let placement = allocate(&function, &Allocation::all_spilled());

        assert_eq!(placement.slot(value), Some(8));
        assert_eq!(placement.total_slot_bytes, 16);
    }

    /// Verifies string values reserve both pointer and length words in placement.
    #[test]
    fn allocates_string_value_as_two_words() {
        let mut function = Function::new("test".to_string(), IrType::Str, PhpType::Str);
        let data_id = crate::ir::DataId::from_raw(0);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_str(data_id);

        let placement = allocate(&function, &Allocation::all_spilled());

        assert_eq!(placement.slot(value), Some(16));
        assert_eq!(placement.total_slot_bytes, 16);
    }

    /// Verifies void storage does not consume a spill slot.
    #[test]
    fn void_values_have_no_slot_size() {
        assert_eq!(bytes_for(IrType::Void), 0);
    }

    /// Verifies iterator handles keep an ordinary opaque-handle spill slot.
    #[test]
    fn allocates_iter_start_value_as_iterator_state() {
        let mut function = Function::new("test".to_string(), IrType::I64, PhpType::Int);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let array = builder
            .emit(
                Op::ArrayNew,
                Vec::new(),
                None,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
                Ownership::Owned,
            )
            .expect("array_new produces a value");
        let iterator = builder
            .emit(
                Op::IterStart,
                vec![array],
                Some(Immediate::IterStart(crate::ir::IterStartMetadata::new(
                    LocalSlotId::from_raw(0),
                    false,
                    None,
                    None,
                ))),
                IrType::Heap(IrHeapKind::Iterable),
                PhpType::Iterable,
                Ownership::MaybeOwned,
            )
            .expect("iter_start produces a value");

        let placement = allocate(&function, &Allocation::all_spilled());

        assert_eq!(placement.slot(array), Some(8));
        assert_eq!(placement.slot(iterator), Some(16));
        assert_eq!(placement.total_slot_bytes, 16);
    }

    /// Verifies only ambiguous direct PHP-call results reserve a marker spill slot.
    #[test]
    fn allocates_runtime_ownership_slot_for_maybe_owned_call_result() {
        let mut function = Function::new(
            "test".to_string(),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let result = builder
            .emit(
                Op::Call,
                Vec::new(),
                Some(crate::ir::Immediate::Data(DataId::from_raw(0))),
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::MaybeOwned,
            )
            .expect("call produces a value");

        let placement = allocate(&function, &Allocation::all_spilled());

        assert_eq!(placement.slot(result), Some(8));
        assert_eq!(placement.runtime_return_ownership_slot(result), Some(16));
        assert_eq!(placement.total_slot_bytes, 16);
    }

    /// Verifies a Mixed-to-string cast reserves a spill for its per-tag ownership marker.
    #[test]
    fn allocates_runtime_ownership_slot_for_mixed_string_cast() {
        let mut function = Function::new("test".to_string(), IrType::Str, PhpType::Str);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let source = builder
            .emit(
                Op::LoadGlobal,
                Vec::new(),
                Some(Immediate::GlobalName(DataId::from_raw(0))),
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Borrowed,
            )
            .expect("load_global produces a value");
        let result = builder
            .emit(
                Op::Cast,
                vec![source],
                Some(Immediate::CastTarget(IrType::Str)),
                IrType::Str,
                PhpType::Str,
                Ownership::MaybeOwned,
            )
            .expect("cast produces a value");

        let placement = allocate(&function, &Allocation::all_spilled());

        assert_eq!(placement.runtime_return_ownership_slot(result), Some(32));
    }
}
