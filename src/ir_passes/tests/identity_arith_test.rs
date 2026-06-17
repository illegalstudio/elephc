//! Purpose:
//! Tests for the identity arithmetic folding pass: fold-to-operand and
//! fold-to-zero rewrites, chained folds, excluded non-identities, and the
//! invariant that every rewrite leaves the function valid.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built with `crate::ir::Builder`. Values and instructions are
//!   indexed by their definition order (one value per emitted instruction here).

use crate::ir::{
    validate_function, Builder, Function, Immediate, IrType, Op, Ownership, Terminator, ValueId,
};
use crate::ir_passes::driver::IrPass;
use crate::ir_passes::identity_arith::IdentityArith;
use crate::types::PhpType;

/// Builds `return (const_a OP const_b)` with an integer binary op.
fn int_binop_function(op: Op, a: i64, b: i64) -> Function {
    let mut function = Function::new("binop".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let lhs = builder.emit_const_i64(a);
        let rhs = builder.emit_const_i64(b);
        let result = builder
            .emit(op, vec![lhs, rhs], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
            .expect("binop result");
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    function
}

/// Builds `return (const_a OP const_a)` so both operands are the same value.
fn int_self_binop_function(op: Op, a: i64) -> Function {
    let mut function = Function::new("self_binop".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(a);
        let result = builder
            .emit(op, vec![value, value], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
            .expect("self binop result");
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    function
}

/// Returns the value returned by the function's single-return entry path.
fn return_value(function: &Function) -> Option<ValueId> {
    match function.block(function.entry).unwrap().terminator.as_ref() {
        Some(Terminator::Return { value }) => *value,
        _ => None,
    }
}

/// `x + 0` folds to `x`: the add becomes a `Nop` and the return uses the operand.
#[test]
fn add_zero_folds_to_operand() {
    let mut function = int_binop_function(Op::IAdd, 5, 0);
    let changed = IdentityArith.run(&mut function);
    assert!(changed, "x + 0 must fold");
    assert_eq!(function.instructions[2].op, Op::Nop, "the add is neutralized");
    assert_eq!(return_value(&function), Some(ValueId::from_raw(0)), "returns x");
    assert!(validate_function(&function).is_ok(), "folded IR stays valid");
}

/// `x * 1` folds to `x` through fold-to-operand.
#[test]
fn mul_one_folds_to_operand() {
    let mut function = int_binop_function(Op::IMul, 9, 1);
    assert!(IdentityArith.run(&mut function));
    assert_eq!(function.instructions[2].op, Op::Nop);
    assert_eq!(return_value(&function), Some(ValueId::from_raw(0)));
    assert!(validate_function(&function).is_ok());
}

/// `x << 0` folds to `x`.
#[test]
fn shift_zero_folds_to_operand() {
    let mut function = int_binop_function(Op::IShl, 12, 0);
    assert!(IdentityArith.run(&mut function));
    assert_eq!(function.instructions[2].op, Op::Nop);
    assert_eq!(return_value(&function), Some(ValueId::from_raw(0)));
    assert!(validate_function(&function).is_ok());
}

/// `x | x` folds to `x` (same-operand idempotence).
#[test]
fn or_self_folds_to_operand() {
    let mut function = int_self_binop_function(Op::IBitOr, 6);
    assert!(IdentityArith.run(&mut function));
    assert_eq!(function.instructions[1].op, Op::Nop);
    assert_eq!(return_value(&function), Some(ValueId::from_raw(0)));
    assert!(validate_function(&function).is_ok());
}

/// `x ^ x` folds to `0`: the xor is converted in place to `ConstI64 0`.
#[test]
fn xor_self_folds_to_zero_constant() {
    let mut function = int_self_binop_function(Op::IBitXor, 7);
    assert!(IdentityArith.run(&mut function));
    let folded = &function.instructions[1];
    assert_eq!(folded.op, Op::ConstI64, "xor becomes a constant");
    assert_eq!(folded.immediate, Some(Immediate::I64(0)), "the constant is zero");
    assert_eq!(return_value(&function), Some(ValueId::from_raw(1)), "same result id");
    assert!(validate_function(&function).is_ok());
}

/// `x * 0` folds to the zero constant.
#[test]
fn mul_zero_folds_to_zero_constant() {
    let mut function = int_binop_function(Op::IMul, 5, 0);
    assert!(IdentityArith.run(&mut function));
    let folded = &function.instructions[2];
    assert_eq!(folded.op, Op::ConstI64);
    assert_eq!(folded.immediate, Some(Immediate::I64(0)));
    assert!(validate_function(&function).is_ok());
}

/// `x % 1` folds to the zero constant.
#[test]
fn mod_one_folds_to_zero_constant() {
    let mut function = int_binop_function(Op::ISMod, 13, 1);
    assert!(IdentityArith.run(&mut function));
    assert_eq!(function.instructions[2].op, Op::ConstI64);
    assert_eq!(function.instructions[2].immediate, Some(Immediate::I64(0)));
    assert!(validate_function(&function).is_ok());
}

/// `x * 1.0` folds to `x` for floats.
#[test]
fn float_mul_one_folds_to_operand() {
    let mut function = Function::new("fbinop".to_string(), IrType::F64, PhpType::Float);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let lhs = builder.emit_const_f64(2.5);
        let rhs = builder.emit_const_f64(1.0);
        let result = builder
            .emit(Op::FMul, vec![lhs, rhs], None, IrType::F64, PhpType::Float, Ownership::NonHeap)
            .expect("fmul result");
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    assert!(IdentityArith.run(&mut function));
    assert_eq!(function.instructions[2].op, Op::Nop);
    assert_eq!(return_value(&function), Some(ValueId::from_raw(0)));
    assert!(validate_function(&function).is_ok());
}

/// Chained identities resolve transitively: `a = x + 0; b = a * 1; return b`
/// returns `x` directly and never references a neutralized (dead) value.
#[test]
fn chained_folds_resolve_to_terminal_operand() {
    let mut function = Function::new("chain".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let x = builder.emit_const_i64(7);
        let zero = builder.emit_const_i64(0);
        let a = builder
            .emit(Op::IAdd, vec![x, zero], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
            .expect("add result");
        let one = builder.emit_const_i64(1);
        let b = builder
            .emit(Op::IMul, vec![a, one], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
            .expect("mul result");
        builder.terminate(Terminator::Return { value: Some(b) });
    }
    assert!(IdentityArith.run(&mut function));
    assert_eq!(return_value(&function), Some(ValueId::from_raw(0)), "returns x, not a dead value");
    assert_eq!(function.instructions[2].op, Op::Nop, "add neutralized");
    assert_eq!(function.instructions[4].op, Op::Nop, "mul neutralized");
    assert!(validate_function(&function).is_ok());
}

/// A non-identity (`x + 5`) is left untouched and reports no change.
#[test]
fn non_identity_is_left_unchanged() {
    let mut function = int_binop_function(Op::IAdd, 3, 5);
    let changed = IdentityArith.run(&mut function);
    assert!(!changed, "x + 5 is not an identity");
    assert_eq!(function.instructions[2].op, Op::IAdd, "add is preserved");
    assert!(validate_function(&function).is_ok());
}

/// Integer `x / 0` is never folded — it must trap at runtime.
#[test]
fn divide_by_zero_is_not_folded() {
    let mut function = int_binop_function(Op::IDiv, 8, 0);
    let changed = IdentityArith.run(&mut function);
    assert!(!changed, "x / 0 must be preserved to trap");
    assert_eq!(function.instructions[2].op, Op::IDiv);
}
