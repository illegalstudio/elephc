//! Purpose:
//! Regression tests for checked integer arithmetic specialization at integer-only sinks.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Hand-built EIR covers casts, typed stores, ref cells, acquire/release scaffolding,
//!   checked opcodes, and conservative rejection of Mixed, terminator, and pinned uses.

use crate::ir::{
    validate_function, Builder, DataPool, Function, Immediate, IrHeapKind, IrType, LocalKind,
    Op, Ownership, Terminator, ValueId,
};
use crate::ir_passes::checked_int_sink::CheckedIntSink;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Runs checked-int sink specialization once with an unused literal pool.
fn specialize(function: &mut Function) -> bool {
    CheckedIntSink.run(function, &mut DataPool::default())
}

/// Emits one boxed checked binary operation with the canonical Mixed metadata.
fn emit_checked(
    builder: &mut Builder<'_>,
    op: Op,
    lhs: ValueId,
    rhs: ValueId,
) -> ValueId {
    builder
        .emit(
            op,
            vec![lhs, rhs],
            None,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
        )
        .expect("checked arithmetic result")
}

/// Emits an explicit PHP integer cast of a boxed Mixed value.
fn emit_int_cast(builder: &mut Builder<'_>, value: ValueId) -> ValueId {
    builder
        .emit(
            Op::Cast,
            vec![value],
            Some(Immediate::CastTarget(IrType::I64)),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .expect("integer cast result")
}

/// Checked add/sub/mul followed only by an integer cast become scalar ToInt ops.
#[test]
fn specializes_all_checked_integer_opcodes_at_cast_sinks() {
    for (source_op, specialized_op) in [
        (Op::ICheckedAdd, Op::ICheckedAddToInt),
        (Op::ICheckedSub, Op::ICheckedSubToInt),
        (Op::ICheckedMul, Op::ICheckedMulToInt),
    ] {
        let mut function = Function::new("cast_sink".to_string(), IrType::I64, PhpType::Int);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let lhs = builder.emit_const_i64(7);
            let rhs = builder.emit_const_i64(2);
            let checked = emit_checked(&mut builder, source_op, lhs, rhs);
            let cast = emit_int_cast(&mut builder, checked);
            let _ = builder.emit(
                Op::Release,
                vec![checked],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: Some(cast) });
        }

        assert!(
            CheckedIntSink.is_applicable(&function),
            "{source_op:?} should reach an integer sink"
        );
        assert!(specialize(&mut function), "{source_op:?} should specialize");
        assert_eq!(function.instructions[2].op, specialized_op);
        assert_eq!(function.instructions[2].result_type, IrType::I64);
        assert_eq!(function.instructions[2].result_php_type, PhpType::Int);
        assert_eq!(function.instructions[2].result_ownership, Ownership::NonHeap);
        assert!(function.instructions[2].effects.is_pure());
        assert_eq!(function.instructions[3].op, Op::Nop, "cast is removed");
        assert_eq!(function.instructions[4].op, Op::Nop, "release is removed");
        assert!(
            validate_function(&function).is_ok(),
            "specialized IR invalid: {:?}",
            validate_function(&function)
        );
        assert!(!specialize(&mut function), "the rewrite is idempotent");
    }
}

/// Acquire/release scaffolding around a checked value is removed before an integer store.
#[test]
fn specializes_typed_local_store_through_acquire_release() {
    let mut function = Function::new("store_sink".to_string(), IrType::Void, PhpType::Void);
    let (checked, acquired);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("result".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let lhs = builder.emit_const_i64(7);
        let rhs = builder.emit_const_i64(2);
        checked = emit_checked(&mut builder, Op::ICheckedAdd, lhs, rhs);
        acquired = builder
            .emit(
                Op::Acquire,
                vec![checked],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .expect("acquired result");
        builder.emit_store_local(slot, acquired);
        let _ = builder.emit(
            Op::Release,
            vec![checked],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: None });
    }

    assert!(specialize(&mut function));
    assert_eq!(function.instructions[2].op, Op::ICheckedAddToInt);
    assert_eq!(function.instructions[3].op, Op::Nop, "acquire is removed");
    assert_eq!(function.instructions[4].operands, vec![checked]);
    assert_eq!(function.instructions[5].op, Op::Nop, "release is removed");
    assert_ne!(checked, acquired, "fixture exercised a real acquire result");
    assert!(
        validate_function(&function).is_ok(),
        "specialized store IR invalid: {:?}",
        validate_function(&function)
    );
}

/// A ref-cell store whose carried alias type is Int is an integer-only sink.
#[test]
fn specializes_integer_ref_cell_store() {
    let mut function = Function::new("ref_cell_sink".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("result".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let lhs = builder.emit_const_i64(7);
        let rhs = builder.emit_const_i64(2);
        let checked = emit_checked(&mut builder, Op::ICheckedAdd, lhs, rhs);
        let _ = builder.emit(
            Op::StoreRefCell,
            vec![checked],
            Some(Immediate::LocalSlot(slot)),
            IrType::Void,
            PhpType::Int,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: None });
    }

    assert!(specialize(&mut function));
    assert_eq!(function.instructions[2].op, Op::ICheckedAddToInt);
    assert!(
        validate_function(&function).is_ok(),
        "specialized ref-cell IR invalid: {:?}",
        validate_function(&function)
    );
}

/// Several explicit integer casts may safely share one specialized producer.
#[test]
fn specializes_one_producer_with_multiple_integer_sinks() {
    let mut function = Function::new("multiple_sinks".to_string(), IrType::Void, PhpType::Void);
    let (checked, first_slot, second_slot);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        first_slot = builder.add_local(
            Some("first".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        second_slot = builder.add_local(
            Some("second".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let lhs = builder.emit_const_i64(7);
        let rhs = builder.emit_const_i64(2);
        checked = emit_checked(&mut builder, Op::ICheckedAdd, lhs, rhs);
        let first = emit_int_cast(&mut builder, checked);
        let second = emit_int_cast(&mut builder, checked);
        builder.emit_store_local(first_slot, first);
        builder.emit_store_local(second_slot, second);
        let _ = builder.emit(
            Op::Release,
            vec![checked],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: None });
    }

    assert!(specialize(&mut function));
    assert_eq!(function.instructions[2].op, Op::ICheckedAddToInt);
    assert_eq!(function.instructions[3].op, Op::Nop);
    assert_eq!(function.instructions[4].op, Op::Nop);
    assert_eq!(function.instructions[5].operands, vec![checked]);
    assert_eq!(function.instructions[6].operands, vec![checked]);
    assert_eq!(function.instructions[7].op, Op::Nop);
    assert!(
        validate_function(&function).is_ok(),
        "multiple-sink IR invalid: {:?}",
        validate_function(&function)
    );
}

/// Direct output observes the overflow-promoted Mixed type and therefore keeps boxing.
#[test]
fn rejects_mixed_output_observer() {
    let mut function = Function::new("mixed_output".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let lhs = builder.emit_const_i64(7);
        let rhs = builder.emit_const_i64(2);
        let checked = emit_checked(&mut builder, Op::ICheckedAdd, lhs, rhs);
        let _ = builder.emit(
            Op::EchoValue,
            vec![checked],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        let _ = builder.emit(
            Op::Release,
            vec![checked],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: None });
    }

    assert!(
        !CheckedIntSink.is_applicable(&function),
        "a mixed-only observer should skip the sink pass"
    );
    assert!(!specialize(&mut function));
    assert_eq!(function.instructions[2].op, Op::ICheckedAdd);
    assert!(
        validate_function(&function).is_ok(),
        "mixed output fixture invalid: {:?}",
        validate_function(&function)
    );
}

/// A Mixed slot no reader observes as a box is narrowed to `int` along with its producer.
///
/// The per-producer phase cannot reach this: the store only counts as an integer sink once
/// the slot is `I64`, and the slot is `mixed` only because this producer writes it. Proving
/// the slot and the producer together is what removes the per-write allocation behind `$i++`.
#[test]
fn narrows_mixed_local_store_without_boxed_readers() {
    let mut function = Function::new("mixed_store".to_string(), IrType::Void, PhpType::Void);
    let slot;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        slot = builder.add_local(
            Some("result".to_string()),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::PhpLocal,
        );
        let lhs = builder.emit_const_i64(7);
        let rhs = builder.emit_const_i64(2);
        let checked = emit_checked(&mut builder, Op::ICheckedAdd, lhs, rhs);
        let acquired = builder
            .emit(
                Op::Acquire,
                vec![checked],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .expect("acquired result");
        builder.emit_store_local(slot, acquired);
        builder.terminate(Terminator::Return { value: None });
    }

    assert!(specialize(&mut function));
    assert_eq!(function.instructions[2].op, Op::ICheckedAddToInt);
    assert_eq!(function.instructions[3].op, Op::Nop);
    let narrowed = &function.locals[slot.as_raw() as usize];
    assert_eq!(narrowed.ir_type, IrType::I64);
    assert_eq!(narrowed.php_type, PhpType::Int);
    assert!(
        validate_function(&function).is_ok(),
        "narrowed store fixture invalid: {:?}",
        validate_function(&function)
    );
}

/// A Mixed slot whose boxed value still escapes keeps its boxed frame storage.
///
/// This is the boundary the narrowing must not cross: a reader that observes the cell itself
/// — rather than an already-`int` load — can still see PHP's overflow-to-float promotion.
#[test]
fn rejects_mixed_local_store_observed_by_a_boxed_reader() {
    let mut function =
        Function::new("mixed_escape".to_string(), IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    let slot;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        slot = builder.add_local(
            Some("result".to_string()),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::PhpLocal,
        );
        let lhs = builder.emit_const_i64(7);
        let rhs = builder.emit_const_i64(2);
        let checked = emit_checked(&mut builder, Op::ICheckedAdd, lhs, rhs);
        let acquired = builder
            .emit(
                Op::Acquire,
                vec![checked],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .expect("acquired result");
        builder.emit_store_local(slot, acquired);
        let loaded =
            builder.emit_load_local(slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        builder.terminate(Terminator::Return {
            value: Some(loaded),
        });
    }

    assert!(!specialize(&mut function));
    assert_eq!(function.instructions[2].op, Op::ICheckedAdd);
    assert_eq!(function.instructions[3].op, Op::Acquire);
    let untouched = &function.locals[slot.as_raw() as usize];
    assert_eq!(untouched.ir_type, IrType::Heap(IrHeapKind::Mixed));
    assert!(
        validate_function(&function).is_ok(),
        "mixed escape fixture invalid: {:?}",
        validate_function(&function)
    );
}

/// Returning the dynamic result can expose overflow promotion and therefore rejects the rewrite.
#[test]
fn rejects_checked_value_used_by_terminator() {
    let mut function = Function::new(
        "mixed_return".to_string(),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    );
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let lhs = builder.emit_const_i64(7);
        let rhs = builder.emit_const_i64(2);
        let checked = emit_checked(&mut builder, Op::ICheckedAdd, lhs, rhs);
        builder.terminate(Terminator::Return {
            value: Some(checked),
        });
    }

    assert!(!specialize(&mut function));
    assert_eq!(function.instructions[2].op, Op::ICheckedAdd);
    assert!(
        validate_function(&function).is_ok(),
        "mixed return fixture invalid: {:?}",
        validate_function(&function)
    );
}

/// An acquire carrying lifetime-pin metadata is never erased by sink specialization.
#[test]
fn rejects_lifetime_pinned_acquire() {
    let mut function = Function::new("pinned".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let lhs = builder.emit_const_i64(7);
        let rhs = builder.emit_const_i64(2);
        let checked = emit_checked(&mut builder, Op::ICheckedAdd, lhs, rhs);
        let _ = builder.emit(
            Op::Acquire,
            vec![checked],
            Some(Immediate::Bool(true)),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
        );
        let _ = emit_int_cast(&mut builder, checked);
        builder.terminate(Terminator::Return { value: None });
    }

    assert!(!specialize(&mut function));
    assert_eq!(function.instructions[2].op, Op::ICheckedAdd);
    assert_eq!(function.instructions[3].op, Op::Acquire);
    assert!(
        validate_function(&function).is_ok(),
        "lifetime-pin fixture invalid: {:?}",
        validate_function(&function)
    );
}
