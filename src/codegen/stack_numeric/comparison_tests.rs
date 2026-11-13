//! Purpose:
//! Checks frame storage for checked arithmetic observed by numeric comparisons.
//!
//! Called from:
//! - The library test harness through `codegen::stack_numeric`.
//!
//! Key details:
//! - Unknown comparison tags and observations in another block keep heap storage.
//! - Strict identity joins the relational predicates only against numeric partners.

use super::*;
use crate::ir::{Builder, CmpPredicate, MixedNumericOp, Terminator};

/// Builds a checked result observed by one comparison, optionally outside its defining block.
fn comparison(op: Op, predicate: CmpPredicate, numeric: bool, later: bool) -> (Function, ValueId) {
    let mut function = Function::new("compared_numeric".into(), IrType::I64, PhpType::Bool);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let one = builder.emit_const_i64(1);
    let immediate = (op == Op::MixedNumericBinop)
        .then_some(Immediate::MixedNumericOp(MixedNumericOp::Add));
    let value = builder.emit(op, vec![one, one], immediate,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    let partner = if numeric { one } else { builder.emit_const_bool(true) };
    if later {
        let next = builder.create_named_block("observe", vec![]);
        builder.terminate(Terminator::Br { target: next, args: vec![] });
        builder.position_at_end(next);
    }
    let result = builder.emit(Op::PhpRelCmp, vec![value, partner],
        Some(Immediate::CmpPredicate(predicate)), IrType::I64, PhpType::Bool,
        Ownership::NonHeap).unwrap();
    builder.emit(Op::Release, vec![value], None, IrType::Void, PhpType::Void,
        Ownership::NonHeap);
    builder.terminate(Terminator::Return { value: Some(result) });
    crate::ir::validate_function(&function).unwrap();
    (function, value)
}

/// Every numeric relational predicate copies a checked result without exposing its pointer.
#[test]
fn stack_numeric_direct_comparison_keeps_checked_results_in_frame() {
    for op in [Op::ICheckedAdd, Op::ICheckedSub, Op::ICheckedMul, Op::MixedNumericBinop] {
        for predicate in [CmpPredicate::Slt, CmpPredicate::Sle, CmpPredicate::Sgt, CmpPredicate::Sge] {
            let (function, value) = comparison(op, predicate, true, false);
            assert!(plan(&function).cells.contains(&value), "{op:?}, {predicate:?}");
        }
    }
}

/// Boolean partners need PHP coercion rules outside the closed numeric comparison proof.
#[test]
fn stack_numeric_direct_comparison_rejects_nonnumeric_partners() {
    let (function, value) = comparison(Op::ICheckedMul, CmpPredicate::Sle, false, false);
    assert!(!plan(&function).cells.contains(&value));
}

/// Comparisons in another block cannot extend a producer's frame-cell lifetime.
#[test]
fn stack_numeric_direct_comparison_rejects_cross_block_observations() {
    let (function, value) = comparison(Op::ICheckedMul, CmpPredicate::Sle, true, true);
    assert!(!plan(&function).cells.contains(&value));
}

/// Builds a checked product observed by one strict identity, optionally against a boolean.
fn identity(op: Op, numeric_partner: bool) -> (Function, ValueId) {
    let mut function = Function::new("identical_numeric".into(), IrType::I64, PhpType::Bool);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let one = builder.emit_const_i64(1);
    let value = builder.emit(Op::ICheckedMul, vec![one, one], None,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    let partner = if numeric_partner { one } else { builder.emit_const_bool(true) };
    let result = builder.emit(op, vec![value, partner], None, IrType::I64, PhpType::Bool,
        Ownership::NonHeap).unwrap();
    builder.emit(Op::Release, vec![value], None, IrType::Void, PhpType::Void,
        Ownership::NonHeap);
    builder.terminate(Terminator::Return { value: Some(result) });
    crate::ir::validate_function(&function).unwrap();
    (function, value)
}

/// `===` and `!==` against an integer copy tags and payloads without exposing the cell.
#[test]
fn stack_numeric_strict_identity_keeps_checked_results_in_frame() {
    for op in [Op::StrictEq, Op::StrictNotEq] {
        let (function, value) = identity(op, true);
        let frame = plan(&function);
        assert!(frame.cells.contains(&value), "{op:?}");
        let compare = function.instructions.iter().find(|inst| inst.op == op).unwrap();
        assert!(numeric_identity(&function, &frame.proven_values, compare), "{op:?}");
    }
}

/// A boolean partner is outside the closed numeric identity proof.
#[test]
fn stack_numeric_strict_identity_rejects_nonnumeric_partners() {
    let (function, value) = identity(Op::StrictEq, false);
    let frame = plan(&function);
    assert!(!frame.cells.contains(&value));
    let compare = function.instructions.iter().find(|inst| inst.op == Op::StrictEq).unwrap();
    assert!(!numeric_identity(&function, &frame.proven_values, compare));
}
