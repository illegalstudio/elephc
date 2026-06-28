//! Purpose:
//! Tests for common-subexpression elimination: per-block redundancy, dominance-
//! aware cross-block redundancy, the non-dominating diamond case where CSE must
//! not fire, constant deduplication, float signed-zero distinction, and the
//! exception-handler skip.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built by hand with `crate::ir::Builder`. Instruction table
//!   indices follow emission order, so a neutralized instruction is checked by
//!   its position.

use crate::ir::{
    validate_function, Builder, Function, Immediate, IrType, Op, Ownership, Terminator, ValueId,
};
use crate::ir_passes::cse::Cse;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Runs CSE over a function and reports whether it changed anything.
fn run_cse(function: &mut Function) -> bool {
    Cse.run(function, &mut crate::ir::DataPool::default())
}

/// Returns the value returned by the function's single-return entry path.
fn return_value(function: &Function, block_index: usize) -> Option<ValueId> {
    match function.blocks[block_index].terminator.as_ref() {
        Some(Terminator::Return { value }) => *value,
        _ => None,
    }
}

/// Two identical adds in one block: the second is redundant and is redirected to
/// the first, which is what per-block value numbering must catch.
#[test]
fn per_block_redundant_add_is_eliminated() {
    let mut function = Function::new("local".to_string(), IrType::I64, PhpType::Int);
    let (first_add, second_add);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let a = builder.emit_const_i64(5);
        let b = builder.emit_const_i64(3);
        first_add = builder.emit_iadd(a, b);
        second_add = builder.emit_iadd(a, b);
        builder.terminate(Terminator::Return { value: Some(second_add) });
    }
    assert!(run_cse(&mut function), "the duplicate add must be eliminated");
    assert_eq!(function.instructions[3].op, Op::Nop, "the second add is neutralized");
    assert_eq!(return_value(&function, 0), Some(first_add), "uses redirect to the first add");
    assert_ne!(first_add, second_add);
    assert!(validate_function(&function).is_ok());
}

/// An identical add in a dominated successor block reuses the value computed in
/// the dominating entry block.
#[test]
fn cross_block_dominating_add_is_reused() {
    let mut function = Function::new("cross".to_string(), IrType::I64, PhpType::Int);
    let entry_add;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let body = builder.create_named_block("body", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let a = builder.emit_const_i64(5);
        let b = builder.emit_const_i64(3);
        entry_add = builder.emit_iadd(a, b);
        builder.terminate(Terminator::Br { target: body, args: vec![] });

        builder.position_at_end(body);
        let body_add = builder.emit_iadd(a, b);
        builder.terminate(Terminator::Return { value: Some(body_add) });
    }
    assert!(run_cse(&mut function), "the dominated add must be reused");
    assert_eq!(function.instructions[3].op, Op::Nop, "the body add is neutralized");
    assert_eq!(return_value(&function, 1), Some(entry_add), "body returns the entry value");
    assert!(validate_function(&function).is_ok());
}

/// In a diamond, an add in each arm and in the merge is not eliminated: neither
/// arm dominates the other or the merge, so none of the values is available.
#[test]
fn non_dominating_diamond_does_not_cse() {
    let mut function = Function::new("diamond".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let then_block = builder.create_named_block("then", vec![]);
        let else_block = builder.create_named_block("els", vec![]);
        let merge = builder.create_named_block("merge", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let a = builder.emit_const_i64(5);
        let b = builder.emit_const_i64(3);
        let cond = builder.emit_const_i64(1);
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: then_block,
            then_args: vec![],
            else_target: else_block,
            else_args: vec![],
        });

        builder.position_at_end(then_block);
        let _p = builder.emit_iadd(a, b);
        builder.terminate(Terminator::Br { target: merge, args: vec![] });

        builder.position_at_end(else_block);
        let _q = builder.emit_iadd(a, b);
        builder.terminate(Terminator::Br { target: merge, args: vec![] });

        builder.position_at_end(merge);
        let r = builder.emit_iadd(a, b);
        builder.terminate(Terminator::Return { value: Some(r) });
    }
    assert!(!run_cse(&mut function), "no add dominates another, so nothing is eliminated");
    let adds = function.instructions.iter().filter(|i| i.op == Op::IAdd).count();
    assert_eq!(adds, 3, "all three independent adds survive");
    assert!(validate_function(&function).is_ok());
}

/// Duplicate bare constants are NOT deduplicated: nullary materializations are
/// left for the backend to rematerialize rather than kept live by CSE. Only a
/// computation built on them (the add) is eliminated.
#[test]
fn duplicate_constants_are_left_for_rematerialization() {
    let mut function = Function::new("consts".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let a = builder.emit_const_i64(42);
        let b = builder.emit_const_i64(42);
        let first_add = builder.emit_iadd(a, b);
        let second_add = builder.emit_iadd(a, b);
        builder.terminate(Terminator::Return { value: Some(second_add) });
        let _ = first_add;
    }
    assert!(run_cse(&mut function), "the duplicate add is eliminated");
    assert_eq!(function.instructions[0].op, Op::ConstI64, "the first constant is kept");
    assert_eq!(function.instructions[1].op, Op::ConstI64, "the duplicate constant is kept");
    assert_eq!(function.instructions[3].op, Op::Nop, "the duplicate add is neutralized");
    assert!(validate_function(&function).is_ok());
}

/// A function containing an exception-handling opcode is skipped wholesale,
/// because cross-block dominance over the terminator graph is unsound across the
/// implicit handler edges.
#[test]
fn function_with_exception_handler_is_skipped() {
    let mut function = Function::new("guarded".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        // A handler marker makes the function ineligible for the pass.
        builder.emit(
            Op::TryPushHandler,
            vec![],
            Some(Immediate::I64(0)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        let a = builder.emit_const_i64(5);
        let b = builder.emit_const_i64(3);
        let _first = builder.emit_iadd(a, b);
        let second = builder.emit_iadd(a, b);
        builder.terminate(Terminator::Return { value: Some(second) });
    }
    assert!(!run_cse(&mut function), "exception-handler functions are skipped");
    let adds = function.instructions.iter().filter(|i| i.op == Op::IAdd).count();
    assert_eq!(adds, 2, "both adds survive untouched");
}
