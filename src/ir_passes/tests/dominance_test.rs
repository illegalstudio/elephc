//! Purpose:
//! Tests for the dominator-tree analysis: immediate dominators, dominance and
//! strict-dominance queries, dominator-tree children, nearest common dominator,
//! and the handling of loops and unreachable blocks.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built by hand with `crate::ir::Builder`. CFG shape is what
//!   matters here, so blocks carry only the instructions needed to terminate.

use crate::ir::{Builder, Function, IrType, Terminator};
use crate::ir_passes::compute_dominance;
use crate::types::PhpType;

/// Builds a diamond: `entry` branches to `then`/`els`, both branch to `merge`.
/// Returns the four block ids in creation order.
fn diamond() -> Function {
    let mut function = Function::new("diamond".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let then_block = builder.create_named_block("then", vec![]);
        let else_block = builder.create_named_block("els", vec![]);
        let merge = builder.create_named_block("merge", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let cond = builder.emit_const_i64(1);
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: then_block,
            then_args: vec![],
            else_target: else_block,
            else_args: vec![],
        });

        builder.position_at_end(then_block);
        builder.terminate(Terminator::Br { target: merge, args: vec![] });

        builder.position_at_end(else_block);
        builder.terminate(Terminator::Br { target: merge, args: vec![] });

        builder.position_at_end(merge);
        let result = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    function
}

/// In a straight chain `entry -> a -> b`, each block's immediate dominator is its
/// predecessor and the entry dominates everything.
#[test]
fn linear_chain_idoms_are_predecessors() {
    let mut function = Function::new("chain".to_string(), IrType::I64, PhpType::Int);
    let (entry, a, b);
    {
        let mut builder = Builder::new(&mut function);
        entry = builder.create_named_block("entry", vec![]);
        a = builder.create_named_block("a", vec![]);
        b = builder.create_named_block("b", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Br { target: a, args: vec![] });
        builder.position_at_end(a);
        builder.terminate(Terminator::Br { target: b, args: vec![] });
        builder.position_at_end(b);
        let result = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    let dom = compute_dominance(&function);
    assert_eq!(dom.immediate_dominator(entry), None, "entry has no idom");
    assert_eq!(dom.immediate_dominator(a), Some(entry));
    assert_eq!(dom.immediate_dominator(b), Some(a));
    assert!(dom.dominates(entry, b), "entry dominates everything");
    assert!(dom.strictly_dominates(a, b), "a strictly dominates b");
    assert!(!dom.dominates(b, a), "a successor never dominates its predecessor");
    assert!(dom.dominates(b, b), "dominance is reflexive");
}

/// In a diamond, the merge block is dominated by the entry, not by either arm:
/// both arms reach merge, so neither alone is on every path.
#[test]
fn diamond_merge_is_dominated_by_entry_not_arms() {
    let function = diamond();
    let dom = compute_dominance(&function);
    let entry = function.entry;
    let then_block = function.blocks[1].id;
    let else_block = function.blocks[2].id;
    let merge = function.blocks[3].id;

    assert_eq!(dom.immediate_dominator(then_block), Some(entry));
    assert_eq!(dom.immediate_dominator(else_block), Some(entry));
    assert_eq!(dom.immediate_dominator(merge), Some(entry), "merge idom is entry");
    assert!(dom.dominates(entry, merge));
    assert!(!dom.dominates(then_block, merge), "the then-arm does not dominate merge");
    assert!(!dom.dominates(else_block, merge), "the else-arm does not dominate merge");
}

/// The dominator-tree children of the diamond entry are all three other blocks.
#[test]
fn diamond_dominator_tree_children() {
    let function = diamond();
    let dom = compute_dominance(&function);
    let entry = function.entry;
    let mut children = dom.children(entry).to_vec();
    children.sort();
    let mut expected = vec![function.blocks[1].id, function.blocks[2].id, function.blocks[3].id];
    expected.sort();
    assert_eq!(children, expected, "entry dominates the arms and the merge directly");
    assert!(dom.children(function.blocks[1].id).is_empty(), "an arm dominates nothing");
}

/// The nearest common dominator of the two diamond arms is the entry.
#[test]
fn nearest_common_dominator_of_arms_is_entry() {
    let function = diamond();
    let dom = compute_dominance(&function);
    let entry = function.entry;
    let then_block = function.blocks[1].id;
    let else_block = function.blocks[2].id;
    let merge = function.blocks[3].id;
    assert_eq!(dom.nearest_common_dominator(then_block, else_block), Some(entry));
    assert_eq!(dom.nearest_common_dominator(then_block, merge), Some(entry));
    assert_eq!(
        dom.nearest_common_dominator(then_block, then_block),
        Some(then_block),
        "a block is its own nearest common dominator"
    );
}

/// A loop header dominates its body and the exit, and the back-edge does not give
/// the body dominance over the exit. This exercises fixed-point convergence.
#[test]
fn loop_header_dominates_body_and_exit() {
    let mut function = Function::new("loop".to_string(), IrType::I64, PhpType::Int);
    let (entry, header, body, exit);
    {
        let mut builder = Builder::new(&mut function);
        entry = builder.create_named_block("entry", vec![]);
        header = builder.create_named_block("header", vec![]);
        body = builder.create_named_block("body", vec![]);
        exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        builder.terminate(Terminator::Br { target: header, args: vec![] });

        builder.position_at_end(header);
        let cond = builder.emit_const_i64(1);
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: body,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });

        builder.position_at_end(body);
        builder.terminate(Terminator::Br { target: header, args: vec![] });

        builder.position_at_end(exit);
        let result = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    let dom = compute_dominance(&function);
    assert_eq!(dom.immediate_dominator(header), Some(entry));
    assert_eq!(dom.immediate_dominator(body), Some(header), "body idom is the header");
    assert_eq!(dom.immediate_dominator(exit), Some(header), "exit idom is the header");
    assert!(dom.dominates(header, body));
    assert!(dom.dominates(header, exit));
    assert!(!dom.dominates(body, exit), "the loop body does not dominate the exit");
}

/// An unreachable block is not in the dominator tree: it has no immediate
/// dominator, reports unreachable, and dominance queries about it are false,
/// while the reachable blocks are analyzed normally.
#[test]
fn unreachable_block_is_excluded() {
    let mut function = Function::new("orphan".to_string(), IrType::I64, PhpType::Int);
    let (entry, orphan);
    {
        let mut builder = Builder::new(&mut function);
        entry = builder.create_named_block("entry", vec![]);
        orphan = builder.create_named_block("orphan", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let result = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(result) });

        // `orphan` has no incoming edge from the entry.
        builder.position_at_end(orphan);
        let other = builder.emit_const_i64(1);
        builder.terminate(Terminator::Return { value: Some(other) });
    }
    let dom = compute_dominance(&function);
    assert!(dom.is_reachable(entry));
    assert!(!dom.is_reachable(orphan), "orphan is unreachable from entry");
    assert_eq!(dom.immediate_dominator(orphan), None);
    assert!(!dom.dominates(entry, orphan), "entry does not dominate an unreachable block");
    assert!(!dom.dominates(orphan, entry));
    assert_eq!(dom.nearest_common_dominator(entry, orphan), None);
    assert!(dom.dominates(orphan, orphan), "dominance stays reflexive even when unreachable");
}
