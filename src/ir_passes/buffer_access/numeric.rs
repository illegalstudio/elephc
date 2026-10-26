//! Purpose:
//! Identifies numeric box operations that cannot invalidate a hoisted buffer descriptor.
//!
//! Called from:
//! - `buffer_access::hoist_loops`, once on the converged function before rewriting loops.
//!
//! Key details:
//! - The shared numeric-domain proof checks every local writer, initialization and alias.
//! - Numeric allocation does not collect cycles or run PHP; numeric release has no children.
//! - These facts only specialize buffer preservation, never the general EIR effect contract.

use std::collections::HashSet;

use crate::ir::{Function, Immediate, InstId, Instruction, IrHeapKind, IrType, LocalSlotId, Op};
use crate::types::PhpType;

use crate::ir_passes::numeric_domain::{self, NumericInputs};

/// Collects reviewed numeric operations, sharing complete origin proofs across all queries.
pub(super) fn preserving_instructions(function: &Function) -> HashSet<InstId> {
    let mut numeric = NumericInputs::new(function);
    let mut slots: HashSet<LocalSlotId> = HashSet::new();
    for inst in &function.instructions {
        if inst.op != Op::LoadLocal || inst.result_type != IrType::Heap(IrHeapKind::Mixed) {
            continue;
        }
        if let (Some(Immediate::LocalSlot(slot)), Some(value)) = (&inst.immediate, inst.result) {
            if !slots.contains(slot) && numeric.prove(&[value]) {
                slots.insert(*slot);
            }
        }
    }
    function.instructions.iter().enumerate().filter_map(|(index, inst)| {
        let preserves = match inst.op {
            Op::StoreLocal | Op::ReleaseLocalSlot => {
                matches!(inst.immediate, Some(Immediate::LocalSlot(slot)) if slots.contains(&slot))
            }
            Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul
            | Op::MixedBox | Op::MixedClone | Op::Acquire | Op::EnsureOwned
            | Op::Release | Op::ReleaseUnlessAliases => numeric.prove(&inst.operands),
            Op::MixedNumericBinop if numeric_domain::mixed(inst) => numeric.prove(&inst.operands),
            Op::StrIncDec if numeric_domain::increment(inst) => numeric.prove(&inst.operands),
            Op::PhpRelCmp if numeric_domain::comparison(inst) => numeric.prove(&inst.operands),
            Op::Cast if warning_free_numeric_cast(function, inst) => numeric.prove(&inst.operands),
            _ => false,
        };
        preserves.then_some(InstId::from_raw(index as u32))
    }).collect()
}

/// Admits only numeric casts whose storage path cannot emit PHP coercion diagnostics.
fn warning_free_numeric_cast(function: &Function, inst: &Instruction) -> bool {
    let Some(Immediate::CastTarget(target @ (IrType::I64 | IrType::F64))) = inst.immediate
    else { return false; };
    if !matches!(inst.result_php_type.codegen_repr(), PhpType::Int | PhpType::Float | PhpType::Bool) {
        return false;
    }
    // Float-to-int range checks and NaN-to-bool coercions can run PHP warning handlers.
    // Boxed numeric values can hold either tag, so I64-target casts require an actual int.
    target == IrType::F64
        || inst.operands.first().and_then(|&value| function.value(value))
            .is_some_and(|value| value.php_type.codegen_repr() == PhpType::Int)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, LocalKind, MixedNumericOp, Ownership, Terminator};

    /// Builds a boxed local recurrence with a numeric or nonnumeric seed and explicit cleanup.
    fn cycle(numeric_seed: bool) -> Function {
        let mut function = Function::new("numeric_buffer_preservation".into(), IrType::Void, PhpType::Void);
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let slot = b.add_local(Some("sum".into()), IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, LocalKind::PhpLocal);
        let seed = if numeric_seed { b.emit_const_i64(0) } else { b.emit_const_bool(false) };
        b.emit_store_local(slot, seed);
        let loaded = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        let one = b.emit_const_i64(1);
        let next = b.emit(Op::MixedNumericBinop, vec![loaded, one],
            Some(Immediate::MixedNumericOp(MixedNumericOp::Add)),
            IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
        let saved = b.emit(Op::Acquire, vec![next], None,
            IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
        b.emit(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(slot)),
            IrType::Void, PhpType::Void, Ownership::NonHeap);
        b.emit_store_local(slot, saved);
        b.emit(Op::Release, vec![next], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
        b.terminate(Terminator::Return { value: None });
        function
    }

    /// Box allocation, replacement and retirement preserve buffers for a closed numeric cycle.
    #[test]
    fn buffer_numeric_preservation_accepts_boxed_local_cycle() {
        let function = cycle(true);
        let preserving = preserving_instructions(&function);
        for (index, inst) in function.instructions.iter().enumerate() {
            if matches!(inst.op, Op::MixedNumericBinop | Op::Acquire | Op::StoreLocal | Op::ReleaseLocalSlot | Op::Release) {
                assert!(preserving.contains(&InstId::from_raw(index as u32)), "{:?}", inst.op);
            }
        }
    }

    /// A nonnumeric seed invalidates the entire recurrence, including its stores and releases.
    #[test]
    fn buffer_numeric_preservation_rejects_nonnumeric_seed() {
        assert!(preserving_instructions(&cycle(false)).is_empty());
    }

    /// Reference exposure or a missing dominating seed prevents numeric local specialization.
    #[test]
    fn buffer_numeric_preservation_rejects_aliases_and_uninitialized_cycles() {
        let mut aliased = cycle(true);
        let end = aliased.blocks[0].terminator.take().unwrap();
        let mut b = Builder::new(&mut aliased);
        b.position_at_end(crate::ir::BlockId::from_raw(0));
        b.emit(Op::PromoteLocalRefCell, vec![], Some(Immediate::LocalSlot(LocalSlotId::from_raw(0))),
            IrType::Void, PhpType::Void, Ownership::NonHeap);
        b.terminate(end);
        assert!(preserving_instructions(&aliased).is_empty());

        let mut uninitialized = cycle(true);
        let seed = uninitialized.instructions.iter_mut().find(|inst| inst.op == Op::StoreLocal).unwrap();
        crate::ir_passes::rewrite::neutralize_to_nop(seed);
        assert!(preserving_instructions(&uninitialized).is_empty());
    }

    /// Boxed numeric storage keeps integer-target conversions behind a conservative barrier.
    #[test]
    fn buffer_numeric_preservation_rejects_warning_capable_casts() {
        for immediate in [Immediate::CastTarget(IrType::I64), Immediate::ImplicitIntCast, Immediate::StringOffsetCast] {
            let mut function = cycle(true);
            let end = function.blocks[0].terminator.take().unwrap();
            let mut b = Builder::new(&mut function);
            b.position_at_end(crate::ir::BlockId::from_raw(0));
            let value = b.emit_load_local(LocalSlotId::from_raw(0), IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
            b.emit(Op::Cast, vec![value], Some(immediate), IrType::I64, PhpType::Int, Ownership::NonHeap);
            b.terminate(end);
            let inst = InstId::from_raw(function.instructions.len() as u32 - 1);
            assert!(!preserving_instructions(&function).contains(&inst));
        }
    }

    /// Raw float-to-int casts may warn even when the input is proven numeric.
    #[test]
    fn buffer_numeric_preservation_rejects_raw_float_int_casts() {
        for (target, php_type, expected) in [
            (IrType::I64, PhpType::Int, false),
            (IrType::I64, PhpType::Bool, false),
            (IrType::F64, PhpType::Float, true),
        ] {
            let mut function = Function::new("float_cast_preservation".into(), IrType::Void, PhpType::Void);
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", vec![]);
            b.set_entry(entry);
            b.position_at_end(entry);
            let value = b.emit_const_f64(1e20);
            b.emit(Op::Cast, vec![value], Some(Immediate::CastTarget(target)), target, php_type, Ownership::NonHeap);
            b.terminate(Terminator::Return { value: None });
            let cast = InstId::from_raw(function.instructions.len() as u32 - 1);
            assert_eq!(preserving_instructions(&function).contains(&cast), expected);
        }
        for (target, php_type) in [(IrType::I64, PhpType::Int), (IrType::I64, PhpType::Bool), (IrType::F64, PhpType::Float)] {
            let mut function = Function::new("int_cast_preservation".into(), IrType::Void, PhpType::Void);
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", vec![]);
            b.set_entry(entry);
            b.position_at_end(entry);
            let value = b.emit_const_i64(1);
            b.emit(Op::Cast, vec![value], Some(Immediate::CastTarget(target)), target, php_type, Ownership::NonHeap);
            b.terminate(Terminator::Return { value: None });
            let cast = InstId::from_raw(function.instructions.len() as u32 - 1);
            assert!(preserving_instructions(&function).contains(&cast));
        }
    }

    /// Both raw and boxed NaN-to-bool conversions can dispatch a PHP warning handler.
    #[test]
    fn buffer_numeric_preservation_rejects_nan_bool_casts() {
        for boxed in [false, true] {
            let mut function = Function::new("nan_bool_preservation".into(), IrType::Void, PhpType::Void);
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", vec![]);
            b.set_entry(entry);
            b.position_at_end(entry);
            let value = b.emit_const_f64(f64::NAN);
            let value = if boxed {
                b.emit(Op::MixedBox, vec![value], None, IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed, Ownership::Owned).unwrap()
            } else { value };
            b.emit(Op::Cast, vec![value], Some(Immediate::CastTarget(IrType::I64)),
                IrType::I64, PhpType::Bool, Ownership::NonHeap);
            b.terminate(Terminator::Return { value: None });
            let cast = InstId::from_raw(function.instructions.len() as u32 - 1);
            assert!(!preserving_instructions(&function).contains(&cast));
        }
    }
}
