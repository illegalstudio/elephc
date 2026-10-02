//! Purpose:
//! Unit tests for EIR integer range propagation and checked-arithmetic specialization.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Hand-built CFGs cover masks, shifts, comparison edges, induction variables, boxed results,
//!   descending loops, and conservative overflow rejection.

use crate::ir::{
    validate_function, Builder, CmpPredicate, DataPool, Function, Immediate, IrHeapKind, IrType,
    LocalKind, MixedNumericOp, Op, Ownership, PassOrigin, Terminator, ValueId,
};
use crate::ir_passes::driver::IrPass;
use crate::ir_passes::integer_range::IntegerRange;
use crate::types::PhpType;

/// Runs integer range specialization once.
pub(super) fn specialize(function: &mut Function) -> bool {
    IntegerRange.run(function, &mut DataPool::default())
}

/// Adds and loads one unconstrained scalar integer local.
pub(super) fn emit_unknown_int(builder: &mut Builder<'_>, name: &str) -> ValueId {
    let slot = builder.add_local(
        Some(name.to_string()),
        IrType::I64,
        PhpType::Int,
        LocalKind::PhpLocal,
    );
    builder.emit_load_local(slot, IrType::I64, PhpType::Int)
}

/// Emits an integer binary operation with scalar result metadata.
pub(super) fn emit_scalar_binop(
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
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .expect("integer operation result")
}

/// Emits an integer comparison with its signed predicate.
pub(super) fn emit_icmp(
    builder: &mut Builder<'_>,
    lhs: ValueId,
    rhs: ValueId,
    predicate: CmpPredicate,
) -> ValueId {
    builder
        .emit(
            Op::ICmp,
            vec![lhs, rhs],
            Some(Immediate::CmpPredicate(predicate)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
        )
        .expect("comparison result")
}

/// A nonnegative mask proves a following checked add cannot overflow.
#[test]
fn mask_proves_checked_add_safe() {
    let mut function = Function::new("mask_add".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let input = emit_unknown_int(&mut builder, "input");
        let mask = builder.emit_const_i64(255);
        let bounded = emit_scalar_binop(&mut builder, Op::IBitAnd, input, mask);
        let one = builder.emit_const_i64(1);
        let sum = emit_scalar_binop(&mut builder, Op::ICheckedAddToInt, bounded, one);
        builder.terminate(Terminator::Return { value: Some(sum) });
    }

    assert!(specialize(&mut function));
    assert_eq!(function.instructions[4].op, Op::IAdd);
    assert_eq!(function.instructions[4].origin, Some(PassOrigin::IntegerRange));
    assert!(
        validate_function(&function).is_ok(),
        "rewritten function is invalid: {:?}",
        validate_function(&function)
    );
    assert!(!specialize(&mut function), "the rewrite is idempotent");
}

/// An unconstrained input plus one keeps PHP's checked overflow path.
#[test]
fn unconstrained_add_remains_checked() {
    let mut function = Function::new("unknown_add".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let input = emit_unknown_int(&mut builder, "input");
        let one = builder.emit_const_i64(1);
        let sum = emit_scalar_binop(&mut builder, Op::ICheckedAddToInt, input, one);
        builder.terminate(Terminator::Return { value: Some(sum) });
    }

    assert!(!specialize(&mut function));
    assert_eq!(function.instructions[2].op, Op::ICheckedAddToInt);
}

/// A comparison edge narrows its operand before checked arithmetic in the taken block.
#[test]
fn comparison_edge_proves_checked_add_safe() {
    let mut function = Function::new("edge_add".to_string(), IrType::I64, PhpType::Int);
    let checked_value;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let then_block = builder.create_named_block("then", vec![]);
        let else_block = builder.create_named_block("else", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let input = emit_unknown_int(&mut builder, "input");
        let limit = builder.emit_const_i64(10);
        let condition = emit_icmp(&mut builder, input, limit, CmpPredicate::Slt);
        builder.terminate(Terminator::CondBr {
            cond: condition,
            then_target: then_block,
            then_args: vec![],
            else_target: else_block,
            else_args: vec![],
        });

        builder.position_at_end(then_block);
        let one = builder.emit_const_i64(1);
        let checked = emit_scalar_binop(&mut builder, Op::ICheckedAddToInt, input, one);
        checked_value = checked;
        builder.terminate(Terminator::Return {
            value: Some(checked),
        });

        builder.position_at_end(else_block);
        builder.terminate(Terminator::Return { value: Some(input) });
    }

    let checked_id = match function.value(checked_value).expect("checked value").def {
        crate::ir::ValueDef::Instruction { inst, .. } => inst,
        _ => unreachable!(),
    };
    assert!(specialize(&mut function));
    assert_eq!(function.instruction(checked_id).expect("checked instruction").op, Op::IAdd);
    assert!(
        validate_function(&function).is_ok(),
        "rewritten function is invalid: {:?}",
        validate_function(&function)
    );
}

/// Mask and left-shift transfer preserve enough precision to prove a multiply safe.
#[test]
fn masks_and_shifts_prove_checked_multiply_safe() {
    let mut function = Function::new("shift_mul".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let input = emit_unknown_int(&mut builder, "input");
        let mask = builder.emit_const_i64(255);
        let bounded = emit_scalar_binop(&mut builder, Op::IBitAnd, input, mask);
        let shift = builder.emit_const_i64(8);
        let shifted = emit_scalar_binop(&mut builder, Op::IShl, bounded, shift);
        let scale = builder.emit_const_i64(1024);
        let product = emit_scalar_binop(&mut builder, Op::ICheckedMulToInt, shifted, scale);
        builder.terminate(Terminator::Return {
            value: Some(product),
        });
    }

    assert!(specialize(&mut function));
    assert_eq!(function.instructions[6].op, Op::IMul);
    assert!(
        validate_function(&function).is_ok(),
        "rewritten function is invalid: {:?}",
        validate_function(&function)
    );
}

/// A bounded loop parameter proves both its body multiply and positive update safe.
#[test]
fn bounded_induction_variable_proves_body_and_update_safe() {
    let mut function = Function::new("induction".to_string(), IrType::I64, PhpType::Int);
    let (product_value, next_value);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let header = builder.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let body = builder.create_named_block("body", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let input = emit_unknown_int(&mut builder, "input");
        let mask = builder.emit_const_i64(1023);
        let bound = emit_scalar_binop(&mut builder, Op::IBitAnd, input, mask);
        let zero = builder.emit_const_i64(0);
        builder.terminate(Terminator::Br {
            target: header,
            args: vec![zero],
        });

        let induction = builder.block_param(header, 0);
        builder.position_at_end(header);
        let condition = emit_icmp(&mut builder, induction, bound, CmpPredicate::Slt);
        builder.terminate(Terminator::CondBr {
            cond: condition,
            then_target: body,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });

        builder.position_at_end(body);
        let four = builder.emit_const_i64(4);
        let product = emit_scalar_binop(&mut builder, Op::ICheckedMulToInt, induction, four);
        product_value = product;
        let one = builder.emit_const_i64(1);
        let next = emit_scalar_binop(&mut builder, Op::ICheckedAddToInt, induction, one);
        next_value = next;
        builder.terminate(Terminator::Br {
            target: header,
            args: vec![next],
        });

        builder.position_at_end(exit);
        builder.terminate(Terminator::Return {
            value: Some(induction),
        });
    }

    let mul_id = match function.value(product_value).expect("product").def {
        crate::ir::ValueDef::Instruction { inst, .. } => inst,
        _ => unreachable!(),
    };
    let add_id = match function.value(next_value).expect("next value").def {
        crate::ir::ValueDef::Instruction { inst, .. } => inst,
        _ => unreachable!(),
    };
    assert!(specialize(&mut function));
    assert_eq!(function.instruction(mul_id).expect("multiply").op, Op::IMul);
    assert_eq!(function.instruction(add_id).expect("update").op, Op::IAdd);
    assert!(
        validate_function(&function).is_ok(),
        "rewritten function is invalid: {:?}",
        validate_function(&function)
    );
}

/// A descending inclusive loop proves its checked subtraction safe.
#[test]
fn descending_induction_variable_proves_subtract_safe() {
    let mut function = Function::new("descending".to_string(), IrType::I64, PhpType::Int);
    let next_value;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let header = builder.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let body = builder.create_named_block("body", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let ten = builder.emit_const_i64(10);
        builder.terminate(Terminator::Br {
            target: header,
            args: vec![ten],
        });

        let induction = builder.block_param(header, 0);
        builder.position_at_end(header);
        let zero = builder.emit_const_i64(0);
        let condition = emit_icmp(&mut builder, induction, zero, CmpPredicate::Sgt);
        builder.terminate(Terminator::CondBr {
            cond: condition,
            then_target: body,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });

        builder.position_at_end(body);
        let one = builder.emit_const_i64(1);
        let next = emit_scalar_binop(&mut builder, Op::ICheckedSubToInt, induction, one);
        next_value = next;
        builder.terminate(Terminator::Br {
            target: header,
            args: vec![next],
        });

        builder.position_at_end(exit);
        builder.terminate(Terminator::Return {
            value: Some(induction),
        });
    }

    let sub_id = match function.value(next_value).expect("next value").def {
        crate::ir::ValueDef::Instruction { inst, .. } => inst,
        _ => unreachable!(),
    };
    assert!(specialize(&mut function));
    assert_eq!(function.instruction(sub_id).expect("subtract").op, Op::ISub);
    assert!(
        validate_function(&function).is_ok(),
        "rewritten function is invalid: {:?}",
        validate_function(&function)
    );
}

/// A proven boxed checked result narrows to I64 when output and release can consume it safely.
#[test]
fn boxed_checked_result_narrows_for_scalar_output() {
    let mut function = Function::new("boxed".to_string(), IrType::Void, PhpType::Void);
    let checked_value;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let input = emit_unknown_int(&mut builder, "input");
        let mask = builder.emit_const_i64(31);
        let bounded = emit_scalar_binop(&mut builder, Op::IBitAnd, input, mask);
        let one = builder.emit_const_i64(1);
        checked_value = builder
            .emit(
                Op::ICheckedAdd,
                vec![bounded, one],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .expect("boxed checked result");
        let _ = builder.emit(
            Op::EchoValue,
            vec![checked_value],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        let _ = builder.emit(
            Op::Release,
            vec![checked_value],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: None });
    }

    let checked_id = match function.value(checked_value).expect("checked value").def {
        crate::ir::ValueDef::Instruction { inst, .. } => inst,
        _ => unreachable!(),
    };
    assert!(specialize(&mut function));
    assert_eq!(function.instruction(checked_id).expect("checked instruction").op, Op::IAdd);
    let value = function.value(checked_value).expect("narrowed value");
    assert_eq!(value.ir_type, IrType::I64);
    assert_eq!(value.php_type, PhpType::Int);
    assert_eq!(value.ownership, Ownership::NonHeap);
    assert!(
        validate_function(&function).is_ok(),
        "rewritten function is invalid: {:?}",
        validate_function(&function)
    );
}

/// A boxed producer feeding numeric-chain fusion is left intact for the later pass.
#[test]
fn boxed_checked_result_preserves_numeric_chain_fusion() {
    let mut function = Function::new("boxed_chain".to_string(), IrType::I64, PhpType::Int);
    let product_value;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let seven = builder.emit_const_i64(7);
        let thirty_one = builder.emit_const_i64(31);
        let three = builder.emit_const_i64(3);
        product_value = builder
            .emit(
                Op::ICheckedMul,
                vec![seven, thirty_one],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .expect("boxed product");
        let sum = builder
            .emit(
                Op::MixedNumericBinop,
                vec![product_value, three],
                Some(Immediate::MixedNumericOp(MixedNumericOp::Add)),
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .expect("boxed sum");
        let cast = builder
            .emit(
                Op::Cast,
                vec![sum],
                Some(Immediate::CastTarget(IrType::I64)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("integer cast");
        for value in [product_value, sum] {
            let _ = builder.emit(
                Op::Release,
                vec![value],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
        }
        builder.terminate(Terminator::Return { value: Some(cast) });
    }

    let product_id = match function.value(product_value).expect("product value").def {
        crate::ir::ValueDef::Instruction { inst, .. } => inst,
        _ => unreachable!(),
    };
    assert!(!specialize(&mut function));
    assert_eq!(function.instruction(product_id).expect("product").op, Op::ICheckedMul);
    assert!(validate_function(&function).is_ok());
}

/// A checked operation at the exact upper boundary remains checked when one endpoint overflows.
#[test]
fn upper_boundary_overflow_is_not_rewritten() {
    let mut function = Function::new("boundary".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let max = builder.emit_const_i64(i64::MAX);
        let one = builder.emit_const_i64(1);
        let sum = emit_scalar_binop(&mut builder, Op::ICheckedAddToInt, max, one);
        builder.terminate(Terminator::Return { value: Some(sum) });
    }

    assert!(!specialize(&mut function));
    assert_eq!(function.instructions[2].op, Op::ICheckedAddToInt);
}
