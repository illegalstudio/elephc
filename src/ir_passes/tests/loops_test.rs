//! Purpose:
//! Tests for the natural-loop forest analysis: back-edge detection, loop body
//! construction, preheader detection (present, absent, conditional), self-loops,
//! loop-free functions, and nested-loop parent/depth nesting.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built by hand with `crate::ir::Builder`; only CFG shape
//!   matters, so blocks carry just enough to terminate. PHP loops lower to
//!   slot-based CFGs with no block parameters, which these fixtures mirror.

use crate::ir::{Builder, Function, IrType, Terminator};
use crate::ir_passes::{compute_dominance, compute_loops};
use crate::types::PhpType;

/// Adds a `cond_br cond -> then/els` terminator to the current block using a
/// fresh constant condition (its value is irrelevant to the structural analysis).
fn cond_br(builder: &mut Builder<'_>, then_block: crate::ir::BlockId, else_block: crate::ir::BlockId) {
    let cond = builder.emit_const_i64(1);
    builder.terminate(Terminator::CondBr {
        cond,
        then_target: then_block,
        then_args: vec![],
        else_target: else_block,
        else_args: vec![],
    });
}

/// Builds the canonical `for`-loop CFG: entry -> header; header -> body/exit;
/// body -> latch; latch -> header. Returns (entry, header, body, latch, exit).
fn simple_loop() -> Function {
    let mut function = Function::new("loop".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let header = builder.create_named_block("header", vec![]);
        let body = builder.create_named_block("body", vec![]);
        let latch = builder.create_named_block("latch", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        builder.terminate(Terminator::Br { target: header, args: vec![] });
        builder.position_at_end(header);
        cond_br(&mut builder, body, exit);
        builder.position_at_end(body);
        builder.terminate(Terminator::Br { target: latch, args: vec![] });
        builder.position_at_end(latch);
        builder.terminate(Terminator::Br { target: header, args: vec![] });
        builder.position_at_end(exit);
        let r = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(r) });
    }
    function
}

/// A simple loop is detected with the right header, latch, body, and preheader.
#[test]
fn simple_loop_is_detected() {
    let function = simple_loop();
    let dom = compute_dominance(&function);
    let info = compute_loops(&function, &dom);
    let entry = function.blocks[0].id;
    let header = function.blocks[1].id;
    let body = function.blocks[2].id;
    let latch = function.blocks[3].id;
    let exit = function.blocks[4].id;

    assert_eq!(info.loops().len(), 1, "exactly one loop");
    let lp = info.header_loop(header).expect("header is a loop header");
    assert_eq!(lp.latches, vec![latch], "the latch is the back-edge source");
    assert_eq!(lp.blocks, vec![header, body, latch], "body is header+body+latch");
    assert_eq!(lp.preheader, Some(entry), "the init block is the preheader");
    assert_eq!(lp.depth, 1);
    assert_eq!(info.back_edges(), vec![(latch, header)]);
    assert!(info.is_loop_header(header));
    assert!(!info.is_loop_header(body));
    assert_eq!(info.loop_depth(body), 1, "body is at depth 1");
    assert_eq!(info.loop_depth(exit), 0, "the exit is outside the loop");
    assert!(!lp.contains(exit));
    assert!(lp.contains(body));
}

/// A self-loop (a block branching to itself) is its own header and latch.
#[test]
fn self_loop_is_detected() {
    let mut function = Function::new("selfloop".to_string(), IrType::I64, PhpType::Int);
    let (entry, header, exit);
    {
        let mut builder = Builder::new(&mut function);
        entry = builder.create_named_block("entry", vec![]);
        header = builder.create_named_block("header", vec![]);
        exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Br { target: header, args: vec![] });
        builder.position_at_end(header);
        cond_br(&mut builder, header, exit);
        builder.position_at_end(exit);
        let r = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(r) });
    }
    let dom = compute_dominance(&function);
    let info = compute_loops(&function, &dom);
    let lp = info.header_loop(header).expect("self-loop header");
    assert_eq!(lp.latches, vec![header], "the header is its own latch");
    assert_eq!(lp.blocks, vec![header], "the body is just the header");
    assert_eq!(lp.preheader, Some(entry), "entry is the preheader");
}

/// A loop-free function has no loops.
#[test]
fn loop_free_function_has_no_loops() {
    let mut function = Function::new("straight".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let next = builder.create_named_block("next", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Br { target: next, args: vec![] });
        builder.position_at_end(next);
        let r = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(r) });
    }
    let dom = compute_dominance(&function);
    let info = compute_loops(&function, &dom);
    assert!(info.is_empty(), "no back edges means no loops");
    assert!(info.back_edges().is_empty());
}

/// Two predecessors entering the header from outside the loop mean no preheader
/// is detected (an optimization would have to insert one).
#[test]
fn no_preheader_when_entry_is_shared() {
    let mut function = Function::new("shared".to_string(), IrType::I64, PhpType::Int);
    let header;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let left = builder.create_named_block("left", vec![]);
        let right = builder.create_named_block("right", vec![]);
        header = builder.create_named_block("header", vec![]);
        let latch = builder.create_named_block("latch", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        cond_br(&mut builder, left, right);
        builder.position_at_end(left);
        builder.terminate(Terminator::Br { target: header, args: vec![] });
        builder.position_at_end(right);
        builder.terminate(Terminator::Br { target: header, args: vec![] });
        builder.position_at_end(header);
        cond_br(&mut builder, latch, exit);
        builder.position_at_end(latch);
        builder.terminate(Terminator::Br { target: header, args: vec![] });
        builder.position_at_end(exit);
        let r = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(r) });
    }
    let dom = compute_dominance(&function);
    let info = compute_loops(&function, &dom);
    let lp = info.header_loop(header).expect("loop exists");
    assert_eq!(lp.preheader, None, "two outside entries means no single preheader");
}

/// An out-of-loop predecessor that also branches elsewhere (conditional entry)
/// is not a preheader.
#[test]
fn no_preheader_when_entry_is_conditional() {
    let mut function = Function::new("cond_entry".to_string(), IrType::I64, PhpType::Int);
    let header;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        header = builder.create_named_block("header", vec![]);
        let latch = builder.create_named_block("latch", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);
        // entry branches to the header OR straight to the exit: not a clean preheader.
        builder.position_at_end(entry);
        cond_br(&mut builder, header, exit);
        builder.position_at_end(header);
        cond_br(&mut builder, latch, exit);
        builder.position_at_end(latch);
        builder.terminate(Terminator::Br { target: header, args: vec![] });
        builder.position_at_end(exit);
        let r = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(r) });
    }
    let dom = compute_dominance(&function);
    let info = compute_loops(&function, &dom);
    let lp = info.header_loop(header).expect("loop exists");
    assert_eq!(lp.preheader, None, "a conditional entry is not a preheader");
}

/// Nested loops: the inner loop is a child of the outer loop, with the right
/// depths, parents, preheaders, and innermost-loop assignment per block.
#[test]
fn nested_loops_nest_correctly() {
    let mut function = Function::new("nested".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let outer_h = builder.create_named_block("outer_h", vec![]);
        let inner_pre = builder.create_named_block("inner_pre", vec![]);
        let inner_h = builder.create_named_block("inner_h", vec![]);
        let inner_latch = builder.create_named_block("inner_latch", vec![]);
        let outer_latch = builder.create_named_block("outer_latch", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        builder.terminate(Terminator::Br { target: outer_h, args: vec![] });
        builder.position_at_end(outer_h);
        cond_br(&mut builder, inner_pre, exit);
        builder.position_at_end(inner_pre);
        builder.terminate(Terminator::Br { target: inner_h, args: vec![] });
        builder.position_at_end(inner_h);
        cond_br(&mut builder, inner_latch, outer_latch);
        builder.position_at_end(inner_latch);
        builder.terminate(Terminator::Br { target: inner_h, args: vec![] });
        builder.position_at_end(outer_latch);
        builder.terminate(Terminator::Br { target: outer_h, args: vec![] });
        builder.position_at_end(exit);
        let r = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(r) });
    }
    let dom = compute_dominance(&function);
    let info = compute_loops(&function, &dom);

    let entry = function.blocks[0].id;
    let outer_h = function.blocks[1].id;
    let inner_pre = function.blocks[2].id;
    let inner_h = function.blocks[3].id;
    let inner_latch = function.blocks[4].id;
    let outer_latch = function.blocks[5].id;

    assert_eq!(info.loops().len(), 2, "two loops");
    let outer = info.header_loop(outer_h).expect("outer loop");
    let inner = info.header_loop(inner_h).expect("inner loop");

    assert_eq!(outer.depth, 1, "outer loop is at depth 1");
    assert_eq!(inner.depth, 2, "inner loop is nested at depth 2");
    assert!(outer.parent.is_none(), "outer has no parent");
    assert!(inner.parent.is_some(), "inner has a parent");

    assert!(outer.contains(inner_h), "outer body contains the inner header");
    assert!(inner.contains(inner_latch));
    assert!(!inner.contains(outer_latch), "the outer latch is outside the inner loop");

    assert_eq!(outer.preheader, Some(entry), "entry is the outer preheader");
    assert_eq!(inner.preheader, Some(inner_pre), "inner_pre is the inner preheader");

    // Innermost-loop assignment: blocks shared between loops resolve to the inner.
    assert_eq!(info.loop_depth(inner_latch), 2, "inner-only block is depth 2");
    assert_eq!(info.loop_depth(inner_h), 2, "the inner header is depth 2");
    assert_eq!(info.loop_depth(outer_latch), 1, "outer-only block is depth 1");
    assert_eq!(info.loop_depth(inner_pre), 1, "inner preheader is in the outer loop only");
}
