//! Purpose:
//! Tests for linear-program-order live-interval construction over EIR.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built by hand with `crate::ir::Builder`. Tests assert
//!   relational properties (overlap, definition order) rather than absolute
//!   position numbers, so they survive numbering-scheme changes.

use crate::ir::{Builder, Function, IrType, Terminator, ValueId};
use crate::ir_passes::{build_intervals, compute_liveness, LiveInterval};
use crate::types::PhpType;

/// Returns the interval describing `value`, panicking if none exists.
fn interval_for(intervals: &[LiveInterval], value: ValueId) -> &LiveInterval {
    intervals
        .iter()
        .find(|iv| iv.value == value)
        .expect("missing interval for value")
}

/// Two intervals overlap when each starts strictly before the other ends. A
/// value whose last use coincides with another value's definition does not
/// overlap, so the register can be reused.
fn overlap(a: &LiveInterval, b: &LiveInterval) -> bool {
    a.start < b.end && b.start < a.end
}

/// In `v2 = v0 + v1`, the two operands are live simultaneously (they overlap),
/// the result begins where the operands die (no overlap with either, so it can
/// reuse a register), and definition order follows program order.
#[test]
fn straight_line_add_intervals_overlap_and_order() {
    let mut function = Function::new("add".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let v0 = builder.emit_const_i64(1);
        let v1 = builder.emit_const_i64(2);
        let v2 = builder.emit_iadd(v0, v1);
        builder.terminate(Terminator::Return { value: Some(v2) });
    }

    let v0 = ValueId::from_raw(0);
    let v1 = ValueId::from_raw(1);
    let v2 = ValueId::from_raw(2);
    let liveness = compute_liveness(&function);
    let intervals = build_intervals(&function, &liveness);

    assert_eq!(intervals.len(), 3, "one interval per defined value");

    let iv0 = interval_for(&intervals, v0);
    let iv1 = interval_for(&intervals, v1);
    let iv2 = interval_for(&intervals, v2);

    assert!(iv0.start < iv0.end, "v0 spans a real range");
    assert!(overlap(iv0, iv1), "both operands are live at the add");
    assert!(
        !overlap(iv0, iv2),
        "v2 is born where v0 dies; it can reuse v0's register"
    );
    assert!(
        !overlap(iv1, iv2),
        "v2 is born where v1 dies; it can reuse v1's register"
    );
    assert!(iv0.start < iv1.start, "definition order follows program order");
}

/// A value defined before a loop and used inside it must have an interval that
/// reaches past the loop body. Its end must be at least the position of its
/// in-loop use, which (because the body is numbered after the entry) is greater
/// than its definition point.
#[test]
fn loop_invariant_interval_spans_the_loop_body() {
    let mut function = Function::new("loop_iv".to_string(), IrType::I64, PhpType::Int);
    {
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
    }

    let invariant = ValueId::from_raw(0);
    let step = ValueId::from_raw(1);
    let liveness = compute_liveness(&function);
    let intervals = build_intervals(&function, &liveness);

    let iv_invariant = interval_for(&intervals, invariant);
    let iv_step = interval_for(&intervals, step);

    assert!(
        iv_invariant.end > iv_step.start,
        "invariant outlives the loop-body temporary that uses it"
    );
    assert!(
        overlap(iv_invariant, iv_step),
        "invariant is still live where the loop body computes with it"
    );
}
