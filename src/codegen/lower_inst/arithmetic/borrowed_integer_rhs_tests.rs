//! Purpose:
//! Checks the exact-constant gate for borrowed integer operands in Mixed arithmetic.
//!
//! Called from:
//! - Focused arithmetic lowering unit tests.
//!
//! Key details:
//! - Bool/null constants share I64 storage but must retain their original runtime tags.
//! - Loads, expressions and block parameters do not become integer literal proofs.

use super::integer_rhs_constant;
use crate::ir::{Builder, Function, Immediate, IrType, Op, Ownership, Terminator};
use crate::types::PhpType;

/// Integer limits qualify, while representation-compatible constants and expressions fail closed.
#[test]
fn borrowed_integer_rhs_requires_exact_integer_definition() {
    let mut function = Function::new("constant_gate".into(), IrType::Void, PhpType::Void);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let zero = builder.emit_const_i64(0);
    let minimum = builder.emit_const_i64(i64::MIN);
    let maximum = builder.emit_const_i64(i64::MAX);
    let boolean = builder.emit_const_bool(true);
    let null = builder.emit(Op::ConstNull, vec![], None, IrType::I64,
        PhpType::Void, Ownership::NonHeap).unwrap();
    let float = builder.emit_const_f64(1.0);
    let expression = builder.emit(Op::IAdd, vec![minimum, maximum], None, IrType::I64,
        PhpType::Int, Ownership::NonHeap).unwrap();
    // Even a mismatched source-type annotation must not turn ConstBool into an integer definition.
    let annotated_bool = builder.emit(Op::ConstBool, vec![], Some(Immediate::Bool(true)),
        IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
    builder.terminate(Terminator::Return { value: None });
    for (value, expected) in [(zero, Some(0)), (minimum, Some(i64::MIN)),
        (maximum, Some(i64::MAX)), (boolean, None), (null, None), (float, None),
        (expression, None), (annotated_bool, None)]
    {
        assert_eq!(integer_rhs_constant(&function, value), expected);
    }
}
