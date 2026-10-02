//! Purpose:
//! Regression tests for conservative range joins and complete CFG edge handling.
//!
//! Called from:
//! - The EIR pass unit-test harness.
//!
//! Key details:
//! - Every fixture is validated before and after specialization.
//! - Checked operations must survive when any reachable path can overflow.

use crate::ir::{
    validate_function, Builder, CmpPredicate, Function, Immediate, IrHeapKind, IrType, LocalKind,
    Op, Ownership, SwitchCase, Terminator, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::integer_range_test::{emit_icmp, emit_scalar_binop, emit_unknown_int, specialize};

/// Specializes valid EIR and checks the selected operation after validation.
fn assert_specialized_op(function: &mut Function, value: ValueId, expected: Op) {
    assert!(validate_function(function).is_ok(), "invalid fixture");
    specialize(function);
    assert!(validate_function(function).is_ok(), "invalid rewrite");
    let ValueDef::Instruction { inst, .. } = function.value(value).expect("result").def else {
        panic!("expected an instruction result");
    };
    assert_eq!(function.instruction(inst).expect("instruction").op, expected);
}

/// A shift loses its exact range as a loop parameter grows and cannot retain its first value.
#[test]
fn shift_widening_invalidates_successor_fact() {
    let mut function = Function::new("shift_widening".to_string(), IrType::I64, PhpType::Int);
    let product;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let header = builder.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let body = builder.create_named_block("body", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let zero = builder.emit_const_i64(0);
        let one = builder.emit_const_i64(1);
        let four = builder.emit_const_i64(4);
        let limit = builder.emit_const_i64(63);
        builder.terminate(Terminator::Br { target: header, args: vec![zero] });

        builder.position_at_end(header);
        let counter = builder.block_param(header, 0);
        let shifted = emit_scalar_binop(&mut builder, Op::IShl, one, counter);
        let condition = emit_icmp(&mut builder, counter, limit, CmpPredicate::Slt);
        builder.terminate(Terminator::CondBr {
            cond: condition,
            then_target: body,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });

        builder.position_at_end(body);
        product = emit_scalar_binop(&mut builder, Op::ICheckedMulToInt, shifted, four);
        let next = emit_scalar_binop(&mut builder, Op::IAdd, counter, one);
        let masked = emit_scalar_binop(&mut builder, Op::IBitAnd, next, limit);
        builder.terminate(Terminator::Br { target: header, args: vec![masked] });

        builder.position_at_end(exit);
        builder.terminate(Terminator::Return { value: Some(counter) });
    }
    assert_specialized_op(&mut function, product, Op::ICheckedMulToInt);
}

/// Unsupported integer comparison predicates keep both reachable paths at a join.
#[test]
fn unsupported_comparison_edges_are_preserved() {
    for predicate in [CmpPredicate::Olt, CmpPredicate::Ole, CmpPredicate::Ogt, CmpPredicate::Oge] {
        for bounded_first in [false, true] {
            let mut function = Function::new("ordered_edge".to_string(), IrType::I64, PhpType::Int);
            let sum;
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", vec![]);
                let bounded = builder.create_named_block("bounded", vec![]);
                let ordered = builder.create_named_block("ordered", vec![]);
                let join = builder.create_named_block("join", vec![(IrType::I64, PhpType::Int)]);
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let input = emit_unknown_int(&mut builder, "input");
                let unknown = emit_unknown_int(&mut builder, "branch");
                let zero = builder.emit_const_i64(0);
                let one = builder.emit_const_i64(1);
                let branch = emit_icmp(&mut builder, unknown, zero, CmpPredicate::Eq);
                builder.terminate(Terminator::CondBr {
                    cond: branch,
                    then_target: if bounded_first { bounded } else { ordered },
                    then_args: vec![],
                    else_target: if bounded_first { ordered } else { bounded },
                    else_args: vec![],
                });

                builder.position_at_end(bounded);
                builder.terminate(Terminator::Br { target: join, args: vec![zero] });

                builder.position_at_end(ordered);
                let compare = emit_icmp(&mut builder, input, zero, predicate);
                builder.terminate(Terminator::CondBr {
                    cond: compare,
                    then_target: join,
                    then_args: vec![input],
                    else_target: join,
                    else_args: vec![input],
                });

                builder.position_at_end(join);
                let joined = builder.block_param(join, 0);
                sum = emit_scalar_binop(&mut builder, Op::ICheckedAddToInt, joined, one);
                builder.terminate(Terminator::Return { value: Some(sum) });
            }
            assert_specialized_op(&mut function, sum, Op::ICheckedAddToInt);
        }
    }
}

/// Opposite updates from the same latch cannot share an increasing induction summary.
#[test]
fn parallel_backedges_require_identical_recurrences() {
    for switch in [false, true] {
        let (mut function, difference, _) = parallel_latch_fixture(switch, false);
        assert_specialized_op(&mut function, difference, Op::ICheckedSubToInt);
    }
}

/// Identical conditional and switch backedges still permit bounded induction proofs.
#[test]
fn identical_parallel_backedges_allow_induction() {
    for switch in [false, true] {
        let (mut function, _, increment) = parallel_latch_fixture(switch, true);
        assert_specialized_op(&mut function, increment, Op::IAdd);
    }
}

/// Builds a latch with either agreeing or opposite updates on its parallel edges.
fn parallel_latch_fixture(switch: bool, identical: bool) -> (Function, ValueId, ValueId) {
    let mut function = Function::new("parallel_latch".to_string(), IrType::I64, PhpType::Int);
    let difference;
    let increment;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let header = builder.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let latch = builder.create_named_block("latch", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let zero = builder.emit_const_i64(0);
        let one = builder.emit_const_i64(1);
        let ten = builder.emit_const_i64(10);
        let input = emit_unknown_int(&mut builder, "branch");
        let choose = emit_icmp(&mut builder, input, zero, CmpPredicate::Eq);
        builder.terminate(Terminator::Br { target: header, args: vec![zero] });

        builder.position_at_end(header);
        let counter = builder.block_param(header, 0);
        let condition = emit_icmp(&mut builder, counter, ten, CmpPredicate::Slt);
        builder.terminate(Terminator::CondBr {
            cond: condition,
            then_target: latch,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });

        builder.position_at_end(latch);
        difference = emit_scalar_binop(&mut builder, Op::ICheckedSubToInt, counter, one);
        increment = emit_scalar_binop(&mut builder, Op::ICheckedAddToInt, counter, one);
        let alternative = if identical { increment } else { difference };
        builder.terminate(if switch {
            Terminator::Switch {
                scrutinee: input,
                cases: vec![SwitchCase { value: 0, target: header, args: vec![increment] }],
                default: header,
                default_args: vec![alternative],
            }
        } else { Terminator::CondBr {
            cond: choose,
            then_target: header,
            then_args: vec![increment],
            else_target: header,
            else_args: vec![alternative],
        } });

        builder.position_at_end(exit);
        builder.terminate(Terminator::Return { value: Some(counter) });
    }
    (function, difference, increment)
}

/// Many independent boxed proofs narrow together without invalidating scalar consumers.
#[test]
fn boxed_candidates_narrow_as_one_valid_batch() {
    let mut function = Function::new("boxed_batch".to_string(), IrType::Void, PhpType::Void);
    let mut results = Vec::new();
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let input = emit_unknown_int(&mut builder, "input");
        let mask = builder.emit_const_i64(31);
        let bounded = emit_scalar_binop(&mut builder, Op::IBitAnd, input, mask);
        let one = builder.emit_const_i64(1);
        for _ in 0..64 {
            let value = builder.emit(
                Op::ICheckedAdd, vec![bounded, one], None,
                IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned,
            ).expect("boxed result");
            let _ = builder.emit(
                Op::EchoValue, vec![value], None,
                IrType::Void, PhpType::Void, Ownership::NonHeap,
            );
            let _ = builder.emit(
                Op::Release, vec![value], None,
                IrType::Void, PhpType::Void, Ownership::NonHeap,
            );
            results.push(value);
        }
        builder.terminate(Terminator::Return { value: None });
    }
    assert!(validate_function(&function).is_ok());
    assert!(specialize(&mut function));
    assert!(validate_function(&function).is_ok());
    for value in results {
        assert_eq!(function.value(value).expect("result").ir_type, IrType::I64);
    }
    assert!(!specialize(&mut function), "batch rewrite is idempotent");
}

/// Casting a nonzero integer to bool produces one, not the source integer interval.
#[test]
fn boolean_cast_does_not_inherit_integer_magnitude() {
    for source in [-16, 16] {
        let mut function = Function::new("bool_cast".to_string(), IrType::I64, PhpType::Int);
        let result;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let input = builder.emit_const_i64(source);
            let boolean = builder.emit(
                Op::Cast, vec![input], Some(Immediate::CastTarget(IrType::I64)),
                IrType::I64, PhpType::Bool, Ownership::NonHeap,
            ).expect("boolean cast");
            let (base, delta) = if source > 0 {
                let two = builder.emit_const_i64(2);
                let delta = emit_scalar_binop(&mut builder, Op::ISub, boolean, two);
                (builder.emit_const_i64(i64::MIN), delta)
            } else {
                (builder.emit_const_i64(i64::MAX), boolean)
            };
            result = emit_scalar_binop(&mut builder, Op::ICheckedAddToInt, base, delta);
            builder.terminate(Terminator::Return { value: Some(result) });
        }
        assert_specialized_op(&mut function, result, Op::ICheckedAddToInt);
    }
}

/// Static Mixed assignments require the boxed pointer rather than an unboxed integer.
#[test]
fn boxed_static_store_keeps_its_representation() {
    let mut function = Function::new("static_store".to_string(), IrType::Void, PhpType::Void);
    let sum;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("value".to_string()), IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed, LocalKind::StaticLocal,
        );
        let one = builder.emit_const_i64(1);
        sum = builder.emit(
            Op::ICheckedAdd, vec![one, one], None,
            IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned,
        ).expect("checked sum");
        builder.emit(
            Op::StoreStaticLocal, vec![sum], Some(Immediate::LocalSlot(slot)),
            IrType::Void, PhpType::Void, Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: None });
    }
    assert_specialized_op(&mut function, sum, Op::ICheckedAdd);
}
