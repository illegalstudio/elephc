//! Purpose:
//! Unit tests for IntegerRange's refinement of `php_rel_cmp` against checked `+`/`-` bounds.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture compares an unknown integer `L` with a boxed `X - c` or `X + c` and asks
//!   whether `L + 1` (or `L - 1`) on one branch is proven, so only the one-sided overflow
//!   direction that bounds `L` may unlock the rewrite.

use crate::ir::{
    validate_function, Builder, CmpPredicate, Function, Immediate, InstId, IrHeapKind, IrType,
    Op, Ownership, Terminator, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::integer_range_test::{emit_scalar_binop, emit_unknown_int, specialize};

/// Which branch of the comparison holds the checked step under test.
#[derive(Clone, Copy)]
enum Branch {
    Taken,
    NotTaken,
}

/// Shape of one fixture: the bound producer, its constant, the comparison and the step.
struct Fixture {
    bound_op: Op,
    constant: i64,
    predicate: CmpPredicate,
    bound_on_left: bool,
    branch: Branch,
    step_op: Op,
}

/// Builds `if (php_rel_cmp(L, X op c)) { step(L) } else { step(L) }` with the step on one branch.
fn build(fixture: &Fixture) -> (Function, InstId) {
    let mut function = Function::new("bound".to_string(), IrType::I64, PhpType::Int);
    let step_value;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let taken = builder.create_named_block("taken", vec![]);
        let not_taken = builder.create_named_block("not_taken", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let counter = emit_unknown_int(&mut builder, "counter");
        let base = emit_unknown_int(&mut builder, "base");
        let constant = builder.emit_const_i64(fixture.constant);
        let bound = builder
            .emit(
                fixture.bound_op,
                vec![base, constant],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .expect("boxed bound");
        let operands = if fixture.bound_on_left {
            vec![bound, counter]
        } else {
            vec![counter, bound]
        };
        let condition = builder
            .emit(
                Op::PhpRelCmp,
                operands,
                Some(Immediate::CmpPredicate(fixture.predicate)),
                IrType::I64,
                PhpType::Bool,
                Ownership::NonHeap,
            )
            .expect("comparison");
        let _ = builder.emit(
            Op::Release,
            vec![bound],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::CondBr {
            cond: condition,
            then_target: taken,
            then_args: vec![],
            else_target: not_taken,
            else_args: vec![],
        });
        let (step_block, other_block) = match fixture.branch {
            Branch::Taken => (taken, not_taken),
            Branch::NotTaken => (not_taken, taken),
        };
        builder.position_at_end(step_block);
        let one = builder.emit_const_i64(1);
        step_value = emit_scalar_binop(&mut builder, fixture.step_op, counter, one);
        builder.terminate(Terminator::Return { value: Some(step_value) });
        builder.position_at_end(other_block);
        builder.terminate(Terminator::Return { value: Some(counter) });
    }
    validate_function(&function).expect("fixture is valid EIR");
    let step = defining(&function, step_value);
    (function, step)
}

/// Returns the instruction defining `value`.
fn defining(function: &Function, value: ValueId) -> InstId {
    match function.value(value).expect("value").def {
        ValueDef::Instruction { inst, .. } => inst,
        ValueDef::BlockParam { .. } => unreachable!("fixture values are instruction results"),
    }
}

/// Runs IntegerRange and returns the step's final opcode, checking the result stays valid.
fn step_after_range(fixture: &Fixture) -> Op {
    let (mut function, step) = build(fixture);
    specialize(&mut function);
    validate_function(&function).expect("rewritten function is valid EIR");
    function.instruction(step).expect("step").op
}

/// `L < X - 1` and `L <= X - 1` taken leave `L <= PHP_INT_MAX - 1`, so `L + 1` is unchecked.
#[test]
fn downward_bound_below_outcome_proves_increment() {
    for predicate in [CmpPredicate::Slt, CmpPredicate::Sle] {
        let fixture = Fixture {
            bound_op: Op::ICheckedSub,
            constant: 1,
            predicate,
            bound_on_left: false,
            branch: Branch::Taken,
            step_op: Op::ICheckedAddToInt,
        };
        assert_eq!(step_after_range(&fixture), Op::IAdd, "{predicate:?}");
    }
}

/// `L > X - 1` or `L >= X - 1` NOT taken also puts `L` below the bound.
#[test]
fn downward_bound_failed_greater_outcome_proves_increment() {
    for predicate in [CmpPredicate::Sgt, CmpPredicate::Sge] {
        let fixture = Fixture {
            bound_op: Op::ICheckedSub,
            constant: 1,
            predicate,
            bound_on_left: false,
            branch: Branch::NotTaken,
            step_op: Op::ICheckedAddToInt,
        };
        assert_eq!(step_after_range(&fixture), Op::IAdd, "{predicate:?}");
    }
}

/// The bound on the left (`X - 1 > L`) is the same test with the predicate swapped.
#[test]
fn downward_bound_on_the_left_proves_increment() {
    let fixture = Fixture {
        bound_op: Op::ICheckedSub,
        constant: 1,
        predicate: CmpPredicate::Sgt,
        bound_on_left: true,
        branch: Branch::Taken,
        step_op: Op::ICheckedAddToInt,
    };
    assert_eq!(step_after_range(&fixture), Op::IAdd);
}

/// `X + (-1)` overflows downward too, exactly like `X - 1`.
#[test]
fn negative_addend_is_a_downward_bound() {
    let fixture = Fixture {
        bound_op: Op::ICheckedAdd,
        constant: -1,
        predicate: CmpPredicate::Slt,
        bound_on_left: false,
        branch: Branch::Taken,
        step_op: Op::ICheckedAddToInt,
    };
    assert_eq!(step_after_range(&fixture), Op::IAdd);
}

/// `L > X + 1` taken leaves `L >= PHP_INT_MIN + 1`, so `L - 1` is unchecked.
#[test]
fn upward_bound_above_outcome_proves_decrement() {
    for predicate in [CmpPredicate::Sgt, CmpPredicate::Sge] {
        let fixture = Fixture {
            bound_op: Op::ICheckedAdd,
            constant: 1,
            predicate,
            bound_on_left: false,
            branch: Branch::Taken,
            step_op: Op::ICheckedSubToInt,
        };
        assert_eq!(step_after_range(&fixture), Op::ISub, "{predicate:?}");
    }
}

/// An outcome the overflow direction does not decide keeps the step checked.
///
/// `L < X + 1` holds for every `L` up to PHP_INT_MAX - 512 when `X + 1` overflows to 2^63, and
/// `L > X - 1` holds for `L == PHP_INT_MAX`, so neither bounds the step's overflow.
#[test]
fn undecided_outcomes_keep_the_step_checked() {
    let fixtures = [
        Fixture {
            bound_op: Op::ICheckedAdd,
            constant: 1,
            predicate: CmpPredicate::Slt,
            bound_on_left: false,
            branch: Branch::Taken,
            step_op: Op::ICheckedAddToInt,
        },
        Fixture {
            bound_op: Op::ICheckedSub,
            constant: 1,
            predicate: CmpPredicate::Sgt,
            bound_on_left: false,
            branch: Branch::Taken,
            step_op: Op::ICheckedAddToInt,
        },
        Fixture {
            bound_op: Op::ICheckedSub,
            constant: 1,
            predicate: CmpPredicate::Slt,
            bound_on_left: false,
            branch: Branch::NotTaken,
            step_op: Op::ICheckedAddToInt,
        },
    ];
    for fixture in &fixtures {
        assert_eq!(step_after_range(fixture), Op::ICheckedAddToInt);
    }
}

/// A multiplied bound, or a subtrahend of unknown sign, overflows both ways and proves nothing.
#[test]
fn two_sided_bounds_keep_the_step_checked() {
    for (bound_op, constant) in [(Op::ICheckedMul, 2), (Op::ICheckedSub, 0)] {
        let fixture = Fixture {
            bound_op,
            constant,
            predicate: CmpPredicate::Slt,
            bound_on_left: false,
            branch: Branch::Taken,
            step_op: Op::ICheckedAddToInt,
        };
        assert_eq!(step_after_range(&fixture), Op::ICheckedAddToInt, "{bound_op:?}");
    }
}
