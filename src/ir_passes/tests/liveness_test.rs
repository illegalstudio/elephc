//! Purpose:
//! Tests for backward-dataflow liveness analysis over EIR functions.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built by hand with `crate::ir::Builder`. Values are
//!   referenced by `ValueId::from_raw` using their definition order.

use crate::ir::{Builder, Function, IrType, Terminator, ValueId};
use crate::ir_passes::compute_liveness;
use crate::types::PhpType;

/// Two constants defined in the entry block are used in a successor block, so
/// they must be live-out of entry and live-in of the body. Nothing escapes the
/// body. This exercises the core cross-block propagation of the dataflow.
#[test]
fn values_used_in_successor_are_live_across_the_edge() {
    let mut function = Function::new("cross_block".to_string(), IrType::I64, PhpType::Int);
    let (entry, body) = {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let body = builder.create_named_block("body", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let _v0 = builder.emit_const_i64(1);
        let _v1 = builder.emit_const_i64(2);
        builder.terminate(Terminator::Br {
            target: body,
            args: vec![],
        });

        builder.position_at_end(body);
        let v0 = ValueId::from_raw(0);
        let v1 = ValueId::from_raw(1);
        let sum = builder.emit_iadd(v0, v1);
        builder.terminate(Terminator::Return { value: Some(sum) });
        (entry, body)
    };

    let v0 = ValueId::from_raw(0);
    let v1 = ValueId::from_raw(1);
    let liveness = compute_liveness(&function);

    let entry_out = liveness.live_out_of(entry);
    assert!(entry_out.contains(&v0), "v0 must be live-out of entry");
    assert!(entry_out.contains(&v1), "v1 must be live-out of entry");

    let body_in = liveness.live_in_of(body);
    assert!(body_in.contains(&v0), "v0 must be live-in of body");
    assert!(body_in.contains(&v1), "v1 must be live-in of body");

    assert!(
        liveness.live_in_of(entry).is_empty(),
        "nothing is live entering the entry block"
    );
    assert!(
        liveness.live_out_of(body).is_empty(),
        "nothing escapes the body block"
    );
}

/// A value defined before a loop and used on every iteration must stay live
/// across the loop's back-edge. This only holds if the dataflow iterates to a
/// fixed point: the value is live-out of the loop header because the header
/// branches back to itself and still needs the value.
#[test]
fn loop_invariant_value_stays_live_across_the_back_edge() {
    let mut function = Function::new("loop_live".to_string(), IrType::I64, PhpType::Int);
    let (entry, header, exit) = {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let header = builder.create_named_block("header", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let invariant = builder.emit_const_i64(10);
        builder.terminate(Terminator::Br {
            target: header,
            args: vec![],
        });

        builder.position_at_end(header);
        let step = builder.emit_const_i64(1);
        let _acc = builder.emit_iadd(invariant, step);
        let cond = builder.emit_const_i64(0);
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: header,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });

        builder.position_at_end(exit);
        builder.terminate(Terminator::Return {
            value: Some(invariant),
        });
        (entry, header, exit)
    };

    let invariant = ValueId::from_raw(0);
    let liveness = compute_liveness(&function);

    assert!(
        liveness.live_out_of(entry).contains(&invariant),
        "invariant live leaving entry"
    );
    assert!(
        liveness.live_in_of(header).contains(&invariant),
        "invariant live entering the loop header"
    );
    assert!(
        liveness.live_out_of(header).contains(&invariant),
        "invariant must survive the back-edge: live-out of the header"
    );
    assert!(
        liveness.live_in_of(exit).contains(&invariant),
        "invariant live entering exit where it is returned"
    );
}

/// A block parameter is a definition at block entry, not a value that flows in
/// from predecessors. The argument passed across the edge is consumed at the
/// predecessor's terminator, so the parameter's value never appears in the
/// successor's live-in set, and the argument does not propagate as itself.
#[test]
fn block_parameter_is_a_definition_not_a_live_in() {
    let mut function = Function::new("param_def".to_string(), IrType::I64, PhpType::Int);
    let (entry, body, param) = {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let body = builder.create_named_block("body", vec![(IrType::I64, PhpType::Int)]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let arg = builder.emit_const_i64(7);
        builder.terminate(Terminator::Br {
            target: body,
            args: vec![arg],
        });

        let param = builder.block_param(body, 0);
        builder.position_at_end(body);
        builder.terminate(Terminator::Return { value: Some(param) });
        (entry, body, param)
    };

    let liveness = compute_liveness(&function);

    assert!(
        !liveness.live_in_of(body).contains(&param),
        "a block parameter is defined at entry, never live-in"
    );
    assert!(
        liveness.live_in_of(body).is_empty(),
        "the argument is consumed at the edge; nothing flows into body"
    );
    assert!(
        liveness.live_out_of(entry).is_empty(),
        "the branch argument does not propagate across the edge as itself"
    );
}

/// Hoisted values propagate through a large loop whose layout opposes execution order.
#[test]
fn hoisted_values_survive_reversed_loop_layout_and_parameter_kills() {
    let mut function = Function::new("reversed_loop".into(), IrType::I64, PhpType::Int);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    let body: Vec<_> = (0..512).map(|_| builder.create_block_with_params(vec![])).collect();
    let header = builder.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
    let exit = builder.create_named_block("exit", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let invariants: Vec<_> = (0..128).map(|value| builder.emit_const_i64(value)).collect();
    let condition = builder.emit_const_bool(true);
    builder.terminate(Terminator::Br { target: header, args: vec![invariants[0]] });
    let counter = builder.block_param(header, 0);
    builder.position_at_end(header);
    builder.terminate(Terminator::Br { target: *body.last().unwrap(), args: vec![] });
    for (index, &block) in body.iter().enumerate() {
        builder.position_at_end(block);
        builder.emit_iadd(counter, invariants[index % invariants.len()]);
        if index == 0 {
            let next = builder.emit_iadd(counter, invariants[1]);
            builder.terminate(Terminator::CondBr {
                cond: condition, then_target: header, then_args: vec![next],
                else_target: exit, else_args: vec![],
            });
        } else {
            builder.terminate(Terminator::Br { target: body[index - 1], args: vec![] });
        }
    }
    builder.position_at_end(exit);
    builder.terminate(Terminator::Return { value: Some(invariants[0]) });
    crate::ir::validate_function(&function).unwrap();

    let liveness = compute_liveness(&function);
    let mut live: std::collections::HashSet<_> = invariants.into_iter().collect();
    live.insert(condition);
    assert!(liveness.live_in_of(entry).is_empty());
    assert_eq!(liveness.live_out_of(entry), &live);
    assert_eq!(liveness.live_in_of(header), &live, "header kills its parameter");
    assert_eq!(liveness.live_out_of(body[0]), &live, "back edge uses the new argument");
    live.insert(counter);
    assert_eq!(liveness.live_out_of(header), &live);
    for (index, &block) in body.iter().enumerate() {
        assert_eq!(liveness.live_in_of(block), &live);
        if index != 0 { assert_eq!(liveness.live_out_of(block), &live); }
    }
    assert!(liveness.live_out_of(exit).is_empty());
}

/// A join unions successors, parallel edges do not duplicate work, and dead CFGs still have facts.
#[test]
fn branching_cycles_and_unreachable_uses_keep_complete_live_sets() {
    let mut function = Function::new("branching_liveness".into(), IrType::I64, PhpType::Int);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    let left = builder.create_named_block("left", vec![]);
    let right = builder.create_named_block("right", vec![]);
    let join = builder.create_named_block("join", vec![]);
    let dead = builder.create_named_block("dead", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let x = builder.emit_const_i64(1);
    let y = builder.emit_const_i64(2);
    let cond = builder.emit_const_bool(true);
    builder.terminate(Terminator::CondBr {
        cond, then_target: left, then_args: vec![], else_target: right, else_args: vec![],
    });
    builder.position_at_end(left);
    builder.emit_iadd(x, x);
    builder.terminate(Terminator::Br { target: join, args: vec![] });
    builder.position_at_end(right);
    builder.emit_iadd(y, y);
    builder.terminate(Terminator::Br { target: join, args: vec![] });
    builder.position_at_end(join);
    builder.terminate(Terminator::CondBr {
        cond, then_target: left, then_args: vec![], else_target: left, else_args: vec![],
    });
    builder.position_at_end(dead);
    builder.terminate(Terminator::Return { value: Some(y) });
    crate::ir::validate_function(&function).unwrap();

    let liveness = compute_liveness(&function);
    let cycle = std::collections::HashSet::from([x, cond]);
    assert_eq!(liveness.live_in_of(left), &cycle);
    assert_eq!(liveness.live_in_of(join), &cycle);
    assert_eq!(liveness.live_out_of(right), &cycle);
    assert_eq!(liveness.live_in_of(right), &std::collections::HashSet::from([x, y, cond]));
    assert_eq!(liveness.live_out_of(entry), &std::collections::HashSet::from([x, y, cond]));
    assert!(liveness.live_in_of(entry).is_empty());
    assert_eq!(liveness.live_in_of(dead), &std::collections::HashSet::from([y]));
    assert!(liveness.live_out_of(dead).is_empty());
}
