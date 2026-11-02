//! Purpose:
//! Checks frame storage for checked arithmetic observed by numeric comparisons.
//!
//! Called from:
//! - The library test harness through `codegen::stack_numeric`.
//!
//! Key details:
//! - Unknown comparison tags and observations in another block keep heap storage.

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
