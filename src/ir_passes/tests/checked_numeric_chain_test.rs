//! Purpose:
//! Unit tests for fusing boxed checked numeric chains at integer cast sinks.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Hand-built EIR pins operation order, ownership cleanup removal, idempotence, and
//!   conservative rejection when any boxed intermediate has another observable use.

use crate::ir::{
    validate_function, Builder, CheckedNumericChainImmediate, DataPool, Function, Immediate,
    IrHeapKind, IrType, MixedNumericOp, Op, Ownership, Terminator, ValidationError, ValueId,
};
use crate::ir_passes::checked_numeric_chain::CheckedNumericChain;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Runs checked numeric chain fusion once with an unused literal pool.
fn fuse(function: &mut Function) -> bool {
    CheckedNumericChain.run(function, &mut DataPool::default())
}

/// Emits one boxed checked integer operation.
fn emit_checked(builder: &mut Builder<'_>, op: Op, lhs: ValueId, rhs: ValueId) -> ValueId {
    builder
        .emit(
            op,
            vec![lhs, rhs],
            None,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
        )
        .expect("checked integer result")
}

/// Emits one dynamic boxed numeric operation.
fn emit_mixed(
    builder: &mut Builder<'_>,
    operation: MixedNumericOp,
    lhs: ValueId,
    rhs: ValueId,
) -> ValueId {
    builder
        .emit(
            Op::MixedNumericBinop,
            vec![lhs, rhs],
            Some(Immediate::MixedNumericOp(operation)),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
        )
        .expect("mixed numeric result")
}

/// Emits a PHP integer cast of one boxed value.
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

/// Emits ownership release bookkeeping for one boxed value.
fn emit_release(builder: &mut Builder<'_>, value: ValueId) {
    let _ = builder.emit(
        Op::Release,
        vec![value],
        None,
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
}

/// Boolean normalization cannot be replaced by a chain returning an arbitrary integer.
#[test]
fn boolean_cast_is_not_a_numeric_chain_sink() {
    let mut function = Function::new("bool_chain".to_string(), IrType::I64, PhpType::Bool);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let seven = builder.emit_const_i64(7);
        let product = emit_checked(&mut builder, Op::ICheckedMul, seven, seven);
        let sum = emit_mixed(&mut builder, MixedNumericOp::Add, product, seven);
        let boolean = builder.emit(
            Op::Cast, vec![sum], Some(Immediate::CastTarget(IrType::I64)),
            IrType::I64, PhpType::Bool, Ownership::NonHeap,
        ).expect("boolean cast");
        emit_release(&mut builder, product);
        emit_release(&mut builder, sum);
        builder.terminate(Terminator::Return { value: Some(boolean) });
    }
    assert!(validate_function(&function).is_ok());
    assert!(!CheckedNumericChain.is_applicable(&function));
    assert!(!fuse(&mut function));
    assert_eq!(function.instructions[3].op, Op::Cast);
    assert!(validate_function(&function).is_ok());
}

/// Fuses the benchmark-shaped multiply/add region and removes both boxed values.
#[test]
fn fuses_multiply_add_chain_at_integer_cast() {
    let mut function = Function::new("benchmark_chain".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let accumulator = builder.emit_const_i64(7);
        let multiplier = builder.emit_const_i64(31);
        let increment = builder.emit_const_i64(3);
        let product = emit_checked(&mut builder, Op::ICheckedMul, accumulator, multiplier);
        let sum = emit_mixed(&mut builder, MixedNumericOp::Add, product, increment);
        let cast = emit_int_cast(&mut builder, sum);
        emit_release(&mut builder, product);
        emit_release(&mut builder, sum);
        builder.terminate(Terminator::Return { value: Some(cast) });
    }

    assert!(CheckedNumericChain.is_applicable(&function));
    assert!(fuse(&mut function));
    let fused = &function.instructions[5];
    assert_eq!(fused.op, Op::ICheckedNumericChainToInt);
    assert_eq!(
        fused.immediate,
        Some(Immediate::CheckedNumericChain(Box::new(
            CheckedNumericChainImmediate::new(vec![MixedNumericOp::Mul, MixedNumericOp::Add])
        )))
    );
    assert_eq!(fused.operands.len(), 3);
    assert_eq!(function.instructions[3].op, Op::Nop);
    assert_eq!(function.instructions[4].op, Op::Nop);
    assert_eq!(function.instructions[6].op, Op::Nop);
    assert_eq!(function.instructions[7].op, Op::Nop);
    assert!(validate_function(&function).is_ok());
    assert!(!fuse(&mut function), "fusion must be idempotent");
}

/// Retains the complete left-associated operation order in a longer chain.
#[test]
fn retains_add_sub_mul_operation_order() {
    let mut function = Function::new("ordered_chain".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let first = builder.emit_const_i64(100);
        let second = builder.emit_const_i64(5);
        let third = builder.emit_const_i64(7);
        let fourth = builder.emit_const_i64(3);
        let add = emit_checked(&mut builder, Op::ICheckedAdd, first, second);
        let sub = emit_mixed(&mut builder, MixedNumericOp::Sub, add, third);
        let mul = emit_mixed(&mut builder, MixedNumericOp::Mul, sub, fourth);
        let cast = emit_int_cast(&mut builder, mul);
        emit_release(&mut builder, add);
        emit_release(&mut builder, sub);
        emit_release(&mut builder, mul);
        builder.terminate(Terminator::Return { value: Some(cast) });
    }

    assert!(fuse(&mut function));
    assert_eq!(
        function.instructions[7].immediate,
        Some(Immediate::CheckedNumericChain(Box::new(
            CheckedNumericChainImmediate::new(vec![
                MixedNumericOp::Add,
                MixedNumericOp::Sub,
                MixedNumericOp::Mul,
            ])
        )))
    );
    assert_eq!(function.instructions[7].operands.len(), 4);
    assert!(validate_function(&function).is_ok());
}

/// Rejects a chain when a boxed intermediate also reaches visible output.
#[test]
fn rejects_observed_boxed_intermediate() {
    let mut function = Function::new("observed_chain".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let first = builder.emit_const_i64(7);
        let second = builder.emit_const_i64(31);
        let third = builder.emit_const_i64(2);
        let product = emit_checked(&mut builder, Op::ICheckedMul, first, second);
        let _ = builder.emit(
            Op::EchoValue,
            vec![product],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        let sum = emit_mixed(&mut builder, MixedNumericOp::Add, product, third);
        let cast = emit_int_cast(&mut builder, sum);
        emit_release(&mut builder, product);
        emit_release(&mut builder, sum);
        builder.terminate(Terminator::Return { value: Some(cast) });
    }

    assert!(!fuse(&mut function));
    assert_eq!(function.instructions[3].op, Op::ICheckedMul);
    assert_eq!(function.instructions[5].op, Op::MixedNumericBinop);
}

/// Rejects exponentiation because its promotion and domain behavior needs a separate lowering.
#[test]
fn rejects_pow_inside_chain() {
    let mut function = Function::new("pow_chain".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let first = builder.emit_const_i64(7);
        let second = builder.emit_const_i64(31);
        let third = builder.emit_const_i64(2);
        let product = emit_checked(&mut builder, Op::ICheckedMul, first, second);
        let power = emit_mixed(&mut builder, MixedNumericOp::Pow, product, third);
        let cast = emit_int_cast(&mut builder, power);
        emit_release(&mut builder, product);
        emit_release(&mut builder, power);
        builder.terminate(Terminator::Return { value: Some(cast) });
    }

    assert!(!fuse(&mut function));
}

/// Rejects a fused instruction whose leaf operands do not match its operation count.
#[test]
fn validator_rejects_malformed_checked_numeric_chain() {
    let mut function = Function::new("malformed_chain".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let lhs = builder.emit_const_i64(7);
        let rhs = builder.emit_const_i64(31);
        let malformed = builder
            .emit(
                Op::ICheckedNumericChainToInt,
                vec![lhs, rhs],
                Some(Immediate::CheckedNumericChain(Box::new(
                    CheckedNumericChainImmediate::new(vec![
                        MixedNumericOp::Mul,
                        MixedNumericOp::Add,
                    ]),
                ))),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("malformed chain result");
        builder.terminate(Terminator::Return {
            value: Some(malformed),
        });
    }

    assert!(matches!(
        validate_function(&function),
        Err(ValidationError::OperandCountMismatch { .. })
    ));
}
