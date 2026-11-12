//! Purpose:
//! Identifies numeric locals whose finalized stores only publish frame-owned cells.
//!
//! Called from:
//! - Frame layout after numeric producer and seed storage has been assigned.
//!
//! Key details:
//! - Slot-access safety alone does not prove that every stored producer uses a frame cell.
//! - Every live store must resolve to a selected producer or the slot's scalar seed cell.
//! - Unknown writers retain runtime retirement; transparent aliases reuse the existing proof.

use std::collections::{HashMap, HashSet};

use crate::ir::{Function, Immediate, LocalSlotId, Op, ValueId};

/// Returns proven numeric locals that can never own a heap-allocated numeric cell.
pub(in crate::codegen) fn frame_only_locals(
    function: &Function,
    numeric_locals: &HashSet<LocalSlotId>,
    cells: &HashMap<ValueId, usize>,
    seeds: &HashMap<LocalSlotId, usize>,
) -> HashSet<LocalSlotId> {
    let mut candidates = numeric_locals.clone();
    let mut written = HashSet::new();
    for block in &function.blocks {
        for id in &block.instructions {
            let Some(inst) = function.instruction(*id) else { continue; };
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else { continue; };
            if inst.op != Op::StoreLocal || !candidates.contains(&slot) {
                continue;
            }
            written.insert(slot);
            let frame_backed = inst.operands.first().is_some_and(|&value| {
                let seed = seeds.contains_key(&slot) && function.value(value).is_some_and(|value| {
                    super::scalar_numeric(&value.ir_type, &value.php_type)
                });
                seed || super::is_stack_value(function, cells, value)
            });
            if !frame_backed {
                candidates.remove(&slot);
            }
        }
    }
    candidates.retain(|slot| written.contains(slot));
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::context::FunctionContext;
    use crate::codegen::data_section::DataSection;
    use crate::codegen::emit::Emitter;
    use crate::codegen::platform::Target;
    use crate::codegen::shared_state::SharedCodegenState;
    use crate::ir::{Builder, IrHeapKind, IrType, LocalKind, Module, Ownership, Terminator};
    use crate::types::PhpType;

    /// Keeps six numeric cells live together before observing their payloads.
    fn fixture() -> (Function, Vec<LocalSlotId>, Vec<ValueId>) {
        let mut function = Function::new("numeric_retirement".into(), IrType::I64, PhpType::Int);
        let slots: Vec<_> = (0..6).map(|index| function.add_local(
            Some(format!("sum{index}")), IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed, LocalKind::PhpLocal,
        )).collect();
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let zero = builder.emit_const_i64(0);
        let mut producers = Vec::new();
        for (index, slot) in slots.iter().copied().enumerate() {
            builder.emit_store_local(slot, zero);
            let number = builder.emit_const_i64(index as i64 + 1);
            let result = builder.emit(
                Op::ICheckedAdd, vec![number, number], None,
                IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned,
            ).unwrap();
            let owner = builder.emit(
                Op::Acquire, vec![result], None,
                IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned,
            ).unwrap();
            builder.emit_store_local(slot, owner);
            builder.emit(Op::Release, vec![result], None,
                IrType::Void, PhpType::Void, Ownership::NonHeap);
            producers.push(result);
        }
        let mut sum = zero;
        for slot in &slots {
            let boxed = builder.emit_load_local(*slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
            let payload = builder.emit(
                Op::Cast, vec![boxed], Some(Immediate::CastTarget(IrType::I64)),
                IrType::I64, PhpType::Int, Ownership::NonHeap,
            ).unwrap();
            sum = builder.emit(Op::IAdd, vec![sum, payload], None,
                IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
        }
        builder.terminate(Terminator::Return { value: Some(sum) });
        crate::ir::validate_function(&function).unwrap();
        (function, slots, producers)
    }

    /// Checks retirement calls with physical storage choices on every supported ABI.
    fn check_cleanup(remove_producer: bool, remove_seed: bool, expected_calls: usize) {
        let (function, slots, producers) = fixture();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = Module::new(target);
            let mut layout = crate::codegen::frame::layout_for_function(
                &function, target, false, false, false,
            );
            assert_eq!(layout.numeric_local_slots.len(), 6, "{name}");
            if remove_producer { layout.numeric_cell_offsets.remove(&producers[0]); }
            if remove_seed { layout.numeric_seed_offsets.remove(&slots[0]); }
            assert!(layout.numeric_cell_offsets.values().copied().collect::<HashSet<_>>().len() > 4);
            layout.numeric_frame_only_slots = frame_only_locals(
                &function, &layout.numeric_local_slots,
                &layout.numeric_cell_offsets, &layout.numeric_seed_offsets,
            );
            assert_eq!(layout.numeric_frame_only_slots.len(), 6 - expected_calls, "{name}");
            let offsets = layout.local_offsets.clone();
            let mut emitter = Emitter::new(target);
            let mut data = DataSection::new();
            let mut shared = SharedCodegenState::default();
            {
                let mut ctx = FunctionContext::new(
                    &module, &function, &mut emitter, &mut data, &mut shared,
                    layout, false, false, false, None,
                );
                for slot in &slots {
                    crate::codegen::frame::emit_owned_local_cleanup(
                        &mut ctx, *slot, offsets[slot], &PhpType::Mixed,
                    );
                }
            }
            let asm = emitter.output();
            assert_eq!(asm.matches("__rt_decref_mixed").count(), expected_calls, "{name}:\n{asm}");
        }
    }

    /// Frame-only slots avoid runtime cleanup even beyond the old four-cell inline limit.
    #[test]
    fn frame_only_numeric_retirement_skips_runtime_on_every_target() {
        check_cleanup(false, false, 0);
    }

    /// A single heap-backed producer keeps retirement despite other selected frame writers.
    #[test]
    fn frame_only_numeric_retirement_preserves_heap_fallback_on_every_target() {
        check_cleanup(true, false, 1);
    }

    /// A raw seed must have actual frame storage before its owner can omit retirement.
    #[test]
    fn frame_only_numeric_retirement_requires_selected_seed_storage() {
        check_cleanup(false, true, 1);
    }
}
