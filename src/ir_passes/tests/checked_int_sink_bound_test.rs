//! Purpose:
//! Regression tests for the checked-int sink's strict bound comparisons (`L < (X - c)`).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The rewrite is exact only for a strict comparison and a literal `1 <= c <= 1024`, where an
//!   overflowing difference is exactly -2^63 and casts to PHP_INT_MIN; every other shape, and a
//!   bound with any other observer, must keep its boxed form and its PHP comparison.

use crate::ir::{
    validate_function, Builder, CmpPredicate, DataPool, Function, Immediate, InstId, IrHeapKind,
    IrType, LocalKind, Op, Ownership, Terminator, ValueDef, ValueId,
};
use crate::ir_passes::checked_int_sink::CheckedIntSink;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// One fixture: `php_rel_cmp(counter, base - constant)` with an optional extra observer.
struct Fixture {
    constant: i64,
    predicate: CmpPredicate,
    bound_on_left: bool,
    echo_bound: bool,
}

/// The rewritable instructions of a built fixture.
struct Built {
    function: Function,
    bound: InstId,
    compare: InstId,
}

/// Adds and loads one unconstrained scalar integer local.
fn unknown_int(builder: &mut Builder<'_>, name: &str) -> ValueId {
    let slot = builder.add_local(Some(name.to_string()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    builder.emit_load_local(slot, IrType::I64, PhpType::Int)
}

/// Returns the instruction defining `value`.
fn defining(function: &Function, value: ValueId) -> InstId {
    match function.value(value).expect("value").def {
        ValueDef::Instruction { inst, .. } => inst,
        ValueDef::BlockParam { .. } => unreachable!("fixture values are instruction results"),
    }
}

/// Builds `return php_rel_cmp(counter, base - constant)` with the bound's release.
fn build(fixture: &Fixture) -> Built {
    let mut function = Function::new("bound_compare".to_string(), IrType::I64, PhpType::Bool);
    let (bound, compare);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let counter = unknown_int(&mut builder, "counter");
        let base = unknown_int(&mut builder, "base");
        let constant = builder.emit_const_i64(fixture.constant);
        bound = builder
            .emit(
                Op::ICheckedSub,
                vec![base, constant],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .expect("boxed bound");
        let operands = if fixture.bound_on_left { vec![bound, counter] } else { vec![counter, bound] };
        compare = builder
            .emit(
                Op::PhpRelCmp,
                operands,
                Some(Immediate::CmpPredicate(fixture.predicate)),
                IrType::I64,
                PhpType::Bool,
                Ownership::NonHeap,
            )
            .expect("comparison");
        if fixture.echo_bound {
            let _ = builder.emit(
                Op::EchoValue,
                vec![bound],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
        }
        let _ = builder.emit(
            Op::Release,
            vec![bound],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: Some(compare) });
    }
    validate_function(&function).expect("fixture is valid EIR");
    Built { bound: defining(&function, bound), compare: defining(&function, compare), function }
}

/// Runs the sink once and reports whether it rewrote the fixture, checking it stays valid.
fn sink(built: &mut Built) -> bool {
    let changed = CheckedIntSink.run(&mut built.function, &mut DataPool::default());
    validate_function(&built.function).expect("sunk function is valid EIR");
    changed
}

/// `L < (X - c)` and `(X - c) > L` become an integer test of a cast-mode difference.
#[test]
fn strict_bound_comparisons_become_integer_tests() {
    for (constant, predicate, bound_on_left) in [
        (1, CmpPredicate::Slt, false),
        (1024, CmpPredicate::Slt, false),
        (5, CmpPredicate::Sgt, true),
    ] {
        let mut built = build(&Fixture { constant, predicate, bound_on_left, echo_bound: false });
        assert!(sink(&mut built), "c={constant} {predicate:?}");
        let bound = built.function.instruction(built.bound).expect("bound");
        assert_eq!(bound.op, Op::ICheckedSubToInt);
        assert_eq!(bound.immediate, Some(Immediate::Bool(true)), "an overflow casts, never fatals");
        assert_eq!(bound.result_type, IrType::I64);
        let compare = built.function.instruction(built.compare).expect("comparison");
        assert_eq!(compare.op, Op::ICmp);
        assert_eq!(compare.immediate, Some(Immediate::CmpPredicate(predicate)));
        assert!(
            !built.function.instructions.iter().any(|inst| inst.op == Op::Release),
            "the unboxed bound needs no release"
        );
        assert!(!sink(&mut built), "the rewrite is idempotent");
    }
}

/// Non-strict tests, out-of-range constants and a second observer keep the boxed bound.
///
/// `L <= (X - 1)` is true in PHP for every `L` within 512 of PHP_INT_MIN once `X - 1`
/// overflows, which no integer comparison against PHP_INT_MIN reproduces; a subtrahend above
/// 1024 can round the promoted difference below -2^63.
#[test]
fn other_bound_shapes_keep_the_php_comparison() {
    let fixtures = [
        Fixture { constant: 1, predicate: CmpPredicate::Sle, bound_on_left: false, echo_bound: false },
        Fixture { constant: 1, predicate: CmpPredicate::Sgt, bound_on_left: false, echo_bound: false },
        Fixture { constant: 1025, predicate: CmpPredicate::Slt, bound_on_left: false, echo_bound: false },
        Fixture { constant: 0, predicate: CmpPredicate::Slt, bound_on_left: false, echo_bound: false },
        Fixture { constant: -1, predicate: CmpPredicate::Slt, bound_on_left: false, echo_bound: false },
        Fixture { constant: 1, predicate: CmpPredicate::Slt, bound_on_left: false, echo_bound: true },
    ];
    for fixture in &fixtures {
        let mut built = build(fixture);
        let before = crate::ir::print_function(&built.function);
        assert!(!sink(&mut built), "c={} {:?}", fixture.constant, fixture.predicate);
        assert_eq!(crate::ir::print_function(&built.function), before);
    }
}
