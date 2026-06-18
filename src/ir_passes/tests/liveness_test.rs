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
