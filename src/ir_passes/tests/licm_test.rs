//! Purpose:
//! Tests for loop-invariant code motion: hoisting an invariant pure computation
//! into the preheader, leaving loop-carried and in-loop values in place, hoisting
//! out of a conditional loop block (speculation safety), skipping loops without a
//! preheader, and cascading a doubly-invariant value to the outer preheader.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built by hand with `crate::ir::Builder`. Invariant operands
//!   are produced in the preheader/entry by `iadd` of constants (constants
//!   themselves are not hoistable), giving non-constant SSA values defined
//!   outside the loop.

use crate::ir::{
    validate_function, Builder, BlockId, DataPool, Function, IrType, Terminator, ValueDef, ValueId,
};
use crate::ir_passes::driver::IrPass;
use crate::ir_passes::licm::Licm;
use crate::types::PhpType;

/// Runs LICM over a function and reports whether it changed anything.
fn run_licm(function: &mut Function) -> bool {
    Licm.run(function, &mut DataPool::default())
}

/// Returns the block that defines `value`.
fn def_block_of(function: &Function, value: ValueId) -> BlockId {
    match function.value(value).expect("value exists").def {
        ValueDef::Instruction { block, .. } => block,
        ValueDef::BlockParam { block, .. } => block,
    }
}

/// Returns true when `block` currently contains the instruction defining `value`.
fn block_contains_def(function: &Function, block: BlockId, value: ValueId) -> bool {
    function
        .block(block)
        .map(|b| {
            b.instructions
                .iter()
                .any(|&id| function.instruction(id).and_then(|i| i.result) == Some(value))
        })
        .unwrap_or(false)
}

/// Terminates the current block with `cond_br <fresh const> -> then/els`.
fn cond_br(builder: &mut Builder<'_>, then_block: BlockId, else_block: BlockId) {
    let cond = builder.emit_const_i64(1);
    builder.terminate(Terminator::CondBr {
        cond,
        then_target: then_block,
        then_args: vec![],
        else_target: else_block,
        else_args: vec![],
    });
}

/// An `iadd` of two values defined in the preheader is loop-invariant and is
/// hoisted out of the loop body into the preheader.
#[test]
fn hoists_loop_invariant_computation() {
    let mut function = Function::new("licm".to_string(), IrType::I64, PhpType::Int);
    let (entry, body, invariant);
    {
        let mut b = Builder::new(&mut function);
        entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![]);
        body = b.create_named_block("body", vec![]);
        let latch = b.create_named_block("latch", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);

        b.position_at_end(entry);
        let c1 = b.emit_const_i64(5);
        let c2 = b.emit_const_i64(3);
        let a = b.emit_iadd(c1, c2);
        let c3 = b.emit_const_i64(7);
        let c4 = b.emit_const_i64(2);
        let d = b.emit_iadd(c3, c4);
        b.terminate(Terminator::Br { target: header, args: vec![] });

        b.position_at_end(header);
        cond_br(&mut b, body, exit);

        b.position_at_end(body);
        invariant = b.emit_iadd(a, d);
        b.terminate(Terminator::Br { target: latch, args: vec![] });

        b.position_at_end(latch);
        b.terminate(Terminator::Br { target: header, args: vec![] });

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: Some(a) });
    }

    assert_eq!(def_block_of(&function, invariant), body, "starts in the body");
    assert!(run_licm(&mut function), "the invariant add is hoisted");
    assert_eq!(def_block_of(&function, invariant), entry, "hoisted into the preheader");
    assert!(!block_contains_def(&function, body, invariant), "no longer in the body");
    assert!(validate_function(&function).is_ok(), "hoisted IR is valid");
    assert!(!run_licm(&mut function), "a second run finds nothing (idempotent)");
}

/// A computation using a loop-carried header parameter is not loop-invariant and
/// is left in place.
#[test]
fn does_not_hoist_loop_carried_value() {
    let mut function = Function::new("carried".to_string(), IrType::I64, PhpType::Int);
    let (body, dependent);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        body = b.create_named_block("body", vec![]);
        let latch = b.create_named_block("latch", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);

        b.position_at_end(entry);
        let c1 = b.emit_const_i64(5);
        let c2 = b.emit_const_i64(3);
        let a = b.emit_iadd(c1, c2);
        b.terminate(Terminator::Br { target: header, args: vec![c1] });

        let p = b.block_param(header, 0);
        b.position_at_end(header);
        cond_br(&mut b, body, exit);

        b.position_at_end(body);
        // Uses the loop-carried parameter `p`, so it cannot be hoisted.
        dependent = b.emit_iadd(a, p);
        b.terminate(Terminator::Br { target: latch, args: vec![] });

        b.position_at_end(latch);
        b.terminate(Terminator::Br { target: header, args: vec![a] });

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: Some(a) });
    }

    assert!(!run_licm(&mut function), "nothing is loop-invariant here");
    assert_eq!(def_block_of(&function, dependent), body, "the dependent add stays in the body");
}

/// An invariant computation inside a conditional block of the loop is still
/// hoisted: pure operations are safe to evaluate unconditionally in the preheader.
#[test]
fn hoists_from_conditional_loop_block() {
    let mut function = Function::new("speculate".to_string(), IrType::I64, PhpType::Int);
    let (entry, then_block, invariant);
    {
        let mut b = Builder::new(&mut function);
        entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![]);
        let body = b.create_named_block("body", vec![]);
        then_block = b.create_named_block("then", vec![]);
        let latch = b.create_named_block("latch", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);

        b.position_at_end(entry);
        let c1 = b.emit_const_i64(5);
        let c2 = b.emit_const_i64(3);
        let a = b.emit_iadd(c1, c2);
        b.terminate(Terminator::Br { target: header, args: vec![] });

        b.position_at_end(header);
        cond_br(&mut b, body, exit);

        b.position_at_end(body);
        cond_br(&mut b, then_block, latch);

        b.position_at_end(then_block);
        invariant = b.emit_iadd(a, a);
        b.terminate(Terminator::Br { target: latch, args: vec![] });

        b.position_at_end(latch);
        b.terminate(Terminator::Br { target: header, args: vec![] });

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: Some(a) });
    }

    assert_eq!(def_block_of(&function, invariant), then_block, "starts in the conditional block");
    assert!(run_licm(&mut function), "pure op hoists despite conditional execution");
    assert_eq!(def_block_of(&function, invariant), entry, "hoisted into the preheader");
    assert!(validate_function(&function).is_ok());
}

/// A loop whose header is entered from two outside blocks has no detected
/// preheader, so LICM leaves its invariant computation in place.
#[test]
fn skips_loop_without_preheader() {
    let mut function = Function::new("nopre".to_string(), IrType::I64, PhpType::Int);
    let (body, invariant);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let left = b.create_named_block("left", vec![]);
        let right = b.create_named_block("right", vec![]);
        let header = b.create_named_block("header", vec![]);
        body = b.create_named_block("body", vec![]);
        let latch = b.create_named_block("latch", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);

        b.position_at_end(entry);
        let c1 = b.emit_const_i64(5);
        let c2 = b.emit_const_i64(3);
        let a = b.emit_iadd(c1, c2);
        cond_br(&mut b, left, right);

        b.position_at_end(left);
        b.terminate(Terminator::Br { target: header, args: vec![] });
        b.position_at_end(right);
        b.terminate(Terminator::Br { target: header, args: vec![] });

        b.position_at_end(header);
        cond_br(&mut b, body, exit);

        b.position_at_end(body);
        invariant = b.emit_iadd(a, a);
        b.terminate(Terminator::Br { target: latch, args: vec![] });

        b.position_at_end(latch);
        b.terminate(Terminator::Br { target: header, args: vec![] });

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: Some(a) });
    }

    assert!(!run_licm(&mut function), "no preheader means no hoisting");
    assert_eq!(def_block_of(&function, invariant), body, "stays in the body");
}

/// A value invariant with respect to both loops is hoisted all the way to the
/// outer preheader in a single run (inner-to-outer cascade).
#[test]
fn cascades_to_outer_preheader() {
    let mut function = Function::new("nested".to_string(), IrType::I64, PhpType::Int);
    let (entry, inner_body, invariant);
    {
        let mut b = Builder::new(&mut function);
        entry = b.create_named_block("entry", vec![]);
        let outer_h = b.create_named_block("outer_h", vec![]);
        let inner_pre = b.create_named_block("inner_pre", vec![]);
        let inner_h = b.create_named_block("inner_h", vec![]);
        inner_body = b.create_named_block("inner_body", vec![]);
        let inner_latch = b.create_named_block("inner_latch", vec![]);
        let outer_latch = b.create_named_block("outer_latch", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);

        b.position_at_end(entry);
        let c1 = b.emit_const_i64(5);
        let c2 = b.emit_const_i64(3);
        let a = b.emit_iadd(c1, c2);
        b.terminate(Terminator::Br { target: outer_h, args: vec![] });

        b.position_at_end(outer_h);
        cond_br(&mut b, inner_pre, exit);
        b.position_at_end(inner_pre);
        b.terminate(Terminator::Br { target: inner_h, args: vec![] });
        b.position_at_end(inner_h);
        cond_br(&mut b, inner_body, outer_latch);
        b.position_at_end(inner_body);
        invariant = b.emit_iadd(a, a);
        b.terminate(Terminator::Br { target: inner_latch, args: vec![] });
        b.position_at_end(inner_latch);
        b.terminate(Terminator::Br { target: inner_h, args: vec![] });
        b.position_at_end(outer_latch);
        b.terminate(Terminator::Br { target: outer_h, args: vec![] });
        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: Some(a) });
    }

    assert_eq!(def_block_of(&function, invariant), inner_body, "starts in the inner body");
    assert!(run_licm(&mut function), "the doubly-invariant add is hoisted");
    assert_eq!(
        def_block_of(&function, invariant),
        entry,
        "cascades past the inner preheader to the outer preheader"
    );
    assert!(validate_function(&function).is_ok());
}
