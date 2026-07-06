//! Purpose:
//! Tests for the constant folding pass: integer/float arithmetic and bitwise
//! folds, shift-count bounds, comparison and predicate folds, chained folds, and
//! the invariant that every rewrite leaves the function valid.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built with `crate::ir::Builder`. Each fold reproduces the
//!   op's runtime lowering exactly (wrapping integers, in-range shifts, IEEE
//!   floats), so the compiled result is unchanged.

use crate::ir::{
    validate_function, Builder, CmpPredicate, DataPool, Function, Immediate, IrHeapKind, IrType,
    Op, Ownership, Terminator, ValueId,
};
use crate::ir_passes::const_fold::ConstFold;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Builds `return (const_a OP const_b)` with an integer binary op and an optional
/// comparison predicate immediate, with the given result PHP type.
fn int_binop_function(
    op: Op,
    a: i64,
    b: i64,
    immediate: Option<Immediate>,
    result_php: PhpType,
) -> Function {
    let mut function = Function::new("binop".to_string(), IrType::I64, result_php.clone());
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let lhs = builder.emit_const_i64(a);
        let rhs = builder.emit_const_i64(b);
        let result = builder
            .emit(op, vec![lhs, rhs], immediate, IrType::I64, result_php, Ownership::NonHeap)
            .expect("binop result");
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    function
}

/// Builds `return (const_a checked_OP const_b)` for checked integer arithmetic.
fn checked_int_binop_function(
    op: Op,
    a: i64,
    b: i64,
    return_ir: IrType,
    return_php: PhpType,
) -> Function {
    let mut function = Function::new("checked_binop".to_string(), return_ir, return_php);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let lhs = builder.emit_const_i64(a);
        let rhs = builder.emit_const_i64(b);
        let result = builder
            .emit(
                op,
                vec![lhs, rhs],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::for_php_type(&PhpType::Mixed),
            )
            .expect("checked binop result");
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    function
}

/// Runs the const-fold pass over a function and reports whether it changed.
fn fold(function: &mut Function) -> bool {
    ConstFold.run(function, &mut DataPool::default())
}

/// `2 + 3` folds in place to the integer constant `5`.
#[test]
fn int_add_folds_to_constant() {
    let mut function = int_binop_function(Op::IAdd, 2, 3, None, PhpType::Int);
    assert!(fold(&mut function), "constant add must fold");
    let folded = &function.instructions[2];
    assert_eq!(folded.op, Op::ConstI64);
    assert_eq!(folded.immediate, Some(Immediate::I64(5)));
    assert!(folded.operands.is_empty(), "const has no operands");
    assert!(validate_function(&function).is_ok());
}

/// `6 * 7` folds to `42` (a product neither identity nor zero handles).
#[test]
fn int_mul_folds_to_constant() {
    let mut function = int_binop_function(Op::IMul, 6, 7, None, PhpType::Int);
    assert!(fold(&mut function));
    assert_eq!(function.instructions[2].immediate, Some(Immediate::I64(42)));
    assert!(validate_function(&function).is_ok());
}

/// `10 - 4` folds to `6`.
#[test]
fn int_sub_folds_to_constant() {
    let mut function = int_binop_function(Op::ISub, 10, 4, None, PhpType::Int);
    assert!(fold(&mut function));
    assert_eq!(function.instructions[2].immediate, Some(Immediate::I64(6)));
}

/// Bitwise `0b1100 & 0b1010`, `|`, and `^` fold to their exact results.
#[test]
fn bitwise_ops_fold() {
    for (op, expected) in [(Op::IBitAnd, 0b1000), (Op::IBitOr, 0b1110), (Op::IBitXor, 0b0110)] {
        let mut function = int_binop_function(op, 0b1100, 0b1010, None, PhpType::Int);
        assert!(fold(&mut function), "{:?} must fold", op);
        assert_eq!(function.instructions[2].immediate, Some(Immediate::I64(expected)));
        assert!(validate_function(&function).is_ok());
    }
}

/// Integer addition overflow wraps to match the native 64-bit `add` lowering.
#[test]
fn int_add_overflow_wraps() {
    let mut function = int_binop_function(Op::IAdd, i64::MAX, 1, None, PhpType::Int);
    assert!(fold(&mut function));
    assert_eq!(function.instructions[2].immediate, Some(Immediate::I64(i64::MIN)));
}

/// Checked integer addition overflow folds to PHP's promoted floating-point result.
#[test]
fn checked_int_add_overflow_folds_to_promoted_float() {
    let expected = (i64::MAX as f64) + 1.0;
    let mut function =
        checked_int_binop_function(Op::ICheckedAdd, i64::MAX, 1, IrType::F64, PhpType::Float);
    assert!(fold(&mut function));
    assert_eq!(function.instructions[2].op, Op::ConstF64);
    assert_eq!(function.instructions[2].immediate, Some(Immediate::F64(expected)));
    assert!(validate_function(&function).is_ok());
}

/// Checked integer multiplication without overflow folds to a plain integer constant.
#[test]
fn checked_int_mul_no_overflow_folds_to_int() {
    let mut function =
        checked_int_binop_function(Op::ICheckedMul, 6, 7, IrType::I64, PhpType::Int);
    assert!(fold(&mut function));
    assert_eq!(function.instructions[2].op, Op::ConstI64);
    assert_eq!(function.instructions[2].immediate, Some(Immediate::I64(42)));
    assert!(validate_function(&function).is_ok());
}

/// A left shift by an in-range count folds (`3 << 4 == 48`).
#[test]
fn shift_in_range_folds() {
    let mut function = int_binop_function(Op::IShl, 3, 4, None, PhpType::Int);
    assert!(fold(&mut function));
    assert_eq!(function.instructions[2].immediate, Some(Immediate::I64(48)));
}

/// An arithmetic right shift preserves sign (`-8 >> 1 == -4`).
#[test]
fn arithmetic_shift_right_preserves_sign() {
    let mut function = int_binop_function(Op::IShrA, -8, 1, None, PhpType::Int);
    assert!(fold(&mut function));
    assert_eq!(function.instructions[2].immediate, Some(Immediate::I64(-4)));
}

/// Out-of-range shift counts (>= 64 or negative) are not folded: the runtime
/// masking/trap behavior is left to lowering.
#[test]
fn out_of_range_shift_is_not_folded() {
    for count in [64, -1] {
        let mut function = int_binop_function(Op::IShl, 1, count, None, PhpType::Int);
        assert!(!fold(&mut function), "shift by {count} must not fold");
        assert_eq!(function.instructions[2].op, Op::IShl);
    }
}

/// Unary negation and bitwise-not fold over a single constant operand.
#[test]
fn unary_int_ops_fold() {
    let mut function = Function::new("unary".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(5);
        let neg = builder
            .emit(Op::INeg, vec![value], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
            .expect("neg");
        let not = builder
            .emit(Op::IBitNot, vec![value], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
            .expect("not");
        builder.terminate(Terminator::Return { value: Some(neg) });
        let _ = not;
    }
    assert!(fold(&mut function));
    assert_eq!(function.instructions[1].immediate, Some(Immediate::I64(-5)), "neg 5");
    assert_eq!(function.instructions[2].immediate, Some(Immediate::I64(!5)), "not 5");
    assert!(validate_function(&function).is_ok());
}

/// Float `1.5 + 2.25`, `* 2.0`, and unary negation fold to exact IEEE results.
#[test]
fn float_ops_fold() {
    let mut function = Function::new("fbinop".to_string(), IrType::F64, PhpType::Float);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let a = builder.emit_const_f64(1.5);
        let b = builder.emit_const_f64(2.25);
        let sum = builder
            .emit(Op::FAdd, vec![a, b], None, IrType::F64, PhpType::Float, Ownership::NonHeap)
            .expect("fadd");
        builder.terminate(Terminator::Return { value: Some(sum) });
    }
    assert!(fold(&mut function));
    assert_eq!(function.instructions[2].op, Op::ConstF64);
    assert_eq!(function.instructions[2].immediate, Some(Immediate::F64(3.75)));
    assert!(validate_function(&function).is_ok());
}

/// Each signed comparison predicate folds two integer constants to a boolean.
#[test]
fn integer_compare_folds_to_bool() {
    let cases = [
        (CmpPredicate::Eq, 3, 3, true),
        (CmpPredicate::Ne, 3, 4, true),
        (CmpPredicate::Slt, 3, 4, true),
        (CmpPredicate::Sle, 4, 4, true),
        (CmpPredicate::Sgt, 5, 4, true),
        (CmpPredicate::Sge, 3, 4, false),
    ];
    for (predicate, a, b, expected) in cases {
        let mut function = int_binop_function(
            Op::ICmp,
            a,
            b,
            Some(Immediate::CmpPredicate(predicate)),
            PhpType::Bool,
        );
        assert!(fold(&mut function), "icmp {:?} must fold", predicate);
        let folded = &function.instructions[2];
        assert_eq!(folded.op, Op::ConstBool);
        assert_eq!(folded.immediate, Some(Immediate::Bool(expected)));
        assert!(validate_function(&function).is_ok());
    }
}

/// `is_null` folds: true for a null constant, false for a concrete integer.
#[test]
fn is_null_folds() {
    let mut function = Function::new("isnull".to_string(), IrType::I64, PhpType::Bool);
    let (null_inst, int_inst);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let null_value = builder.emit_const_null();
        let int_value = builder.emit_const_i64(7);
        null_inst = builder
            .emit(Op::IsNull, vec![null_value], None, IrType::I64, PhpType::Bool, Ownership::NonHeap)
            .expect("is_null(null)");
        int_inst = builder
            .emit(Op::IsNull, vec![int_value], None, IrType::I64, PhpType::Bool, Ownership::NonHeap)
            .expect("is_null(int)");
        builder.terminate(Terminator::Return { value: Some(null_inst) });
        let _ = int_inst;
    }
    assert!(fold(&mut function));
    assert_eq!(function.instructions[2].immediate, Some(Immediate::Bool(true)), "is_null(null)");
    assert_eq!(function.instructions[3].immediate, Some(Immediate::Bool(false)), "is_null(7)");
    assert!(validate_function(&function).is_ok());
}

/// `is_truthy` folds nonzero/zero integers, bools, and floats per PHP truthiness.
#[test]
fn is_truthy_folds() {
    for (make, expected) in [(0_i64, false), (5_i64, true)] {
        let mut function = Function::new("istruthy".to_string(), IrType::I64, PhpType::Bool);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let value = builder.emit_const_i64(make);
            let truthy = builder
                .emit(Op::IsTruthy, vec![value], None, IrType::I64, PhpType::Bool, Ownership::NonHeap)
                .expect("is_truthy");
            builder.terminate(Terminator::Return { value: Some(truthy) });
        }
        assert!(fold(&mut function));
        assert_eq!(function.instructions[1].immediate, Some(Immediate::Bool(expected)));
        assert!(validate_function(&function).is_ok());
    }
}

/// Chained constant operations collapse in a single sweep: `(2 + 3) * 4` folds
/// the add to `5` and then the multiply to `20` using the accumulated constant.
#[test]
fn chained_folds_collapse_in_one_sweep() {
    let mut function = Function::new("chain".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let a = builder.emit_const_i64(2);
        let b = builder.emit_const_i64(3);
        let sum = builder.emit_iadd(a, b);
        let four = builder.emit_const_i64(4);
        let product = builder
            .emit(Op::IMul, vec![sum, four], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
            .expect("mul");
        builder.terminate(Terminator::Return { value: Some(product) });
    }
    assert!(fold(&mut function), "chained constants fold in one run");
    assert_eq!(function.instructions[2].immediate, Some(Immediate::I64(5)), "add folded");
    assert_eq!(function.instructions[4].immediate, Some(Immediate::I64(20)), "mul folded");
    assert!(validate_function(&function).is_ok());
}

/// A non-constant operand (a block parameter) blocks folding and reports no change.
#[test]
fn non_constant_operand_is_not_folded() {
    let mut function = Function::new("param".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![(IrType::I64, PhpType::Int)]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let param = builder.block_param(entry, 0);
        let five = builder.emit_const_i64(5);
        let result = builder
            .emit(Op::IAdd, vec![param, five], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
            .expect("add");
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    assert!(!fold(&mut function), "non-constant operand must not fold");
    assert_eq!(function.instructions[1].op, Op::IAdd);
}

/// Integer `/` and `%` are never folded here — their trap, zero-divisor, and
/// float-promotion semantics are left to lowering (matching identity arithmetic).
#[test]
fn division_and_modulo_are_not_folded() {
    for op in [Op::IDiv, Op::ISDiv, Op::ISMod] {
        let mut function = int_binop_function(op, 8, 2, None, PhpType::Int);
        assert!(!fold(&mut function), "{:?} must not fold", op);
        assert_eq!(function.instructions[2].op, op);
    }
}

/// A function with no constant-operand operations reports no change.
#[test]
fn nothing_to_fold_reports_no_change() {
    let mut function = Function::new("empty".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(1);
        builder.terminate(Terminator::Return { value: Some(value) });
    }
    assert!(!fold(&mut function), "a lone constant has nothing to fold");
    assert_eq!(return_value(&function), Some(ValueId::from_raw(0)));
}

/// Returns the value returned by the function's single-return entry path.
fn return_value(function: &Function) -> Option<ValueId> {
    match function.block(function.entry).unwrap().terminator.as_ref() {
        Some(Terminator::Return { value }) => *value,
        _ => None,
    }
}
