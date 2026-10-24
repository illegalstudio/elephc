//! Purpose:
//! Tests counter proofs whose bound is a checked `+`/`-` (`$i < $n - 1`) or whose seed is
//! checked arithmetic on another counter (`$j = $i + 1`).
//!
//! Called from:
//! - The focused EIR pass library test harness.
//!
//! Key details:
//! - Fixtures mirror lowering's boxed counter scaffolding: Mixed loads, `str_inc_dec` updates,
//!   acquire/clone/release owners and `release_local_slot` before every store.
//! - A bound comparison must stay `php_rel_cmp`; a seed must be proven unchecked like an update,
//!   and a seed reading another counter is only provable together with that counter.

use std::collections::HashSet;

use crate::ir::{
    validate_function, Builder, CmpPredicate, DataPool, Function, FunctionParam, Immediate,
    InstId, IrHeapKind, IrType, LocalKind, LocalSlotId, MixedNumericOp, Op, Ownership,
    Terminator, ValueDef, ValueId,
};
use crate::ir_passes::bounded_mixed_counter::BoundedMixedCounter;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Boxed Mixed metadata shared by every counter value in these fixtures.
const MIXED: IrType = IrType::Heap(IrHeapKind::Mixed);

/// Creates a function with one `int $n` parameter and its frame slot.
fn function_with_limit(name: &str) -> (Function, LocalSlotId) {
    let mut function = Function::new(name.into(), IrType::I64, PhpType::Int);
    function.params.push(FunctionParam {
        name: "n".into(),
        ir_type: IrType::I64,
        php_type: PhpType::Int,
        by_ref: false,
        variadic: false,
    });
    let limit = function.add_local(Some("n".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    (function, limit)
}

/// Adds one boxed counter local.
fn counter_slot(function: &mut Function, name: &str) -> LocalSlotId {
    function.add_local(Some(name.into()), MIXED, PhpType::Mixed, LocalKind::PhpLocal)
}

/// Emits a void bookkeeping instruction.
fn emit_void(builder: &mut Builder<'_>, op: Op, operands: Vec<ValueId>, immediate: Option<Immediate>) {
    let _ = builder.emit(op, operands, immediate, IrType::Void, PhpType::Void, Ownership::NonHeap);
}

/// Stores an owned boxed value the way lowering does: acquire, retire the slot, store, release.
fn emit_owned_store(builder: &mut Builder<'_>, slot: LocalSlotId, value: ValueId) {
    let acquired = builder
        .emit(Op::Acquire, vec![value], None, MIXED, PhpType::Mixed, Ownership::Owned)
        .expect("acquired owner");
    emit_void(builder, Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(slot)));
    builder.emit_store_local(slot, acquired);
    emit_void(builder, Op::Release, vec![value], None);
}

/// Emits `$slot++` on boxed storage.
fn emit_increment(builder: &mut Builder<'_>, slot: LocalSlotId) {
    let old = builder.emit_load_local(slot, MIXED, PhpType::Mixed);
    let update = builder
        .emit(Op::StrIncDec, vec![old], Some(Immediate::I64(1)), MIXED, PhpType::Mixed, Ownership::Owned)
        .expect("boxed update");
    emit_owned_store(builder, slot, update);
}

/// Emits `php_rel_cmp($slot, $n)` with `predicate`.
fn emit_limit_compare(
    builder: &mut Builder<'_>,
    slot: LocalSlotId,
    limit: LocalSlotId,
    predicate: CmpPredicate,
) -> ValueId {
    let value = builder.emit_load_local(slot, MIXED, PhpType::Mixed);
    let bound = builder.emit_load_local(limit, IrType::I64, PhpType::Int);
    builder
        .emit(
            Op::PhpRelCmp,
            vec![value, bound],
            Some(Immediate::CmpPredicate(predicate)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
        )
        .expect("comparison")
}

/// Builds `for ($i = 0; $i PRED $n OP 1; $i++) {}` with a boxed checked bound.
fn checked_bound_loop(bound_op: Op, predicate: CmpPredicate) -> Function {
    let (mut function, limit) = function_with_limit("checked_bound");
    let counter = counter_slot(&mut function, "i");
    let mut b = Builder::new(&mut function);
    let entry = b.create_named_block("entry", vec![]);
    let cond = b.create_named_block("cond", vec![]);
    let body = b.create_named_block("body", vec![]);
    let exit = b.create_named_block("exit", vec![]);
    b.set_entry(entry);
    b.position_at_end(entry);
    let zero = b.emit_const_i64(0);
    b.emit_store_local(counter, zero);
    b.terminate(Terminator::Br { target: cond, args: vec![] });
    b.position_at_end(cond);
    let value = b.emit_load_local(counter, MIXED, PhpType::Mixed);
    let base = b.emit_load_local(limit, IrType::I64, PhpType::Int);
    let one = b.emit_const_i64(1);
    let bound = b
        .emit(bound_op, vec![base, one], None, MIXED, PhpType::Mixed, Ownership::Owned)
        .expect("boxed bound");
    let test = b
        .emit(
            Op::PhpRelCmp,
            vec![value, bound],
            Some(Immediate::CmpPredicate(predicate)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
        )
        .expect("comparison");
    emit_void(&mut b, Op::Release, vec![bound], None);
    b.terminate(Terminator::CondBr {
        cond: test,
        then_target: body,
        then_args: vec![],
        else_target: exit,
        else_args: vec![],
    });
    b.position_at_end(body);
    emit_increment(&mut b, counter);
    b.terminate(Terminator::Br { target: cond, args: vec![] });
    b.position_at_end(exit);
    let zero = b.emit_const_i64(0);
    b.terminate(Terminator::Return { value: Some(zero) });
    validate_function(&function).expect("fixture is valid EIR");
    function
}

/// The counter slots of the nested fixture.
struct Nested {
    function: Function,
    outer: LocalSlotId,
    inner: LocalSlotId,
    unprovable: Option<LocalSlotId>,
}

/// Builds `for ($i = 0; $i < $n; $i++) { for ($j = $i OP c; $j < $n; $j++) {} }`, optionally
/// followed by `for ($k = 0; $k <= $n; $k++) {}`, whose update no range can prove.
fn nested_loops(seed: MixedNumericOp, constant: i64, with_unprovable: bool) -> Nested {
    let (mut function, limit) = function_with_limit("nested");
    let outer = counter_slot(&mut function, "i");
    let inner = counter_slot(&mut function, "j");
    let unprovable = with_unprovable.then(|| counter_slot(&mut function, "k"));
    let mut b = Builder::new(&mut function);
    let entry = b.create_named_block("entry", vec![]);
    let outer_cond = b.create_named_block("outer.cond", vec![]);
    let outer_body = b.create_named_block("outer.body", vec![]);
    let inner_cond = b.create_named_block("inner.cond", vec![]);
    let inner_body = b.create_named_block("inner.body", vec![]);
    let outer_update = b.create_named_block("outer.update", vec![]);
    let after = b.create_named_block("after", vec![]);
    b.set_entry(entry);
    b.position_at_end(entry);
    let zero = b.emit_const_i64(0);
    b.emit_store_local(outer, zero);
    if let Some(slot) = unprovable {
        let zero = b.emit_const_i64(0);
        b.emit_store_local(slot, zero);
    }
    b.terminate(Terminator::Br { target: outer_cond, args: vec![] });

    b.position_at_end(outer_cond);
    let test = emit_limit_compare(&mut b, outer, limit, CmpPredicate::Slt);
    b.terminate(Terminator::CondBr {
        cond: test,
        then_target: outer_body,
        then_args: vec![],
        else_target: after,
        else_args: vec![],
    });

    b.position_at_end(outer_body);
    let value = b.emit_load_local(outer, MIXED, PhpType::Mixed);
    let offset = b.emit_const_i64(constant);
    let seeded = b
        .emit(
            Op::MixedNumericBinop,
            vec![value, offset],
            Some(Immediate::MixedNumericOp(seed)),
            MIXED,
            PhpType::Mixed,
            Ownership::Owned,
        )
        .expect("checked seed");
    let copy = b
        .emit(Op::MixedClone, vec![seeded], None, MIXED, PhpType::Mixed, Ownership::Owned)
        .expect("assignment copy");
    emit_void(&mut b, Op::Release, vec![seeded], None);
    emit_owned_store(&mut b, inner, copy);
    b.terminate(Terminator::Br { target: inner_cond, args: vec![] });

    b.position_at_end(inner_cond);
    let test = emit_limit_compare(&mut b, inner, limit, CmpPredicate::Slt);
    b.terminate(Terminator::CondBr {
        cond: test,
        then_target: inner_body,
        then_args: vec![],
        else_target: outer_update,
        else_args: vec![],
    });

    b.position_at_end(inner_body);
    emit_increment(&mut b, inner);
    b.terminate(Terminator::Br { target: inner_cond, args: vec![] });

    b.position_at_end(outer_update);
    emit_increment(&mut b, outer);
    b.terminate(Terminator::Br { target: outer_cond, args: vec![] });

    b.position_at_end(after);
    if let Some(slot) = unprovable {
        let cond = b.create_named_block("last.cond", vec![]);
        let body = b.create_named_block("last.body", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.terminate(Terminator::Br { target: cond, args: vec![] });
        b.position_at_end(cond);
        let test = emit_limit_compare(&mut b, slot, limit, CmpPredicate::Sle);
        b.terminate(Terminator::CondBr {
            cond: test,
            then_target: body,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });
        b.position_at_end(body);
        emit_increment(&mut b, slot);
        b.terminate(Terminator::Br { target: cond, args: vec![] });
        b.position_at_end(exit);
    }
    let zero = b.emit_const_i64(0);
    b.terminate(Terminator::Return { value: Some(zero) });
    validate_function(&function).expect("fixture is valid EIR");
    Nested { function, outer, inner, unprovable }
}

/// Slots the storage proof retained, and the subset whose proof needed a checked seed.
struct Proven {
    slots: HashSet<LocalSlotId>,
    seeded: HashSet<LocalSlotId>,
}

/// Runs the storage proof with no extra by-value call positions.
fn prove(function: &Function) -> Proven {
    let proven = crate::ir_passes::prove_integer_slots(
        function,
        &mut DataPool::default(),
        &std::collections::HashMap::new(),
    );
    Proven { slots: proven.slots, seeded: proven.seeded }
}

/// `$i < $n - 1` and `$i <= $n - 1` bound the counter: it becomes an integer while the test
/// keeps PHP's comparison, which still sees the float an overflowing `$n - 1` produces.
#[test]
fn bounded_mixed_counter_promotes_counters_bounded_by_a_checked_difference() {
    for predicate in [CmpPredicate::Slt, CmpPredicate::Sle] {
        let mut function = checked_bound_loop(Op::ICheckedSub, predicate);
        assert!(
            BoundedMixedCounter::default().run(&mut function, &mut DataPool::default()),
            "{predicate:?}"
        );
        validate_function(&function).expect("promoted function is valid EIR");
        assert_eq!(function.locals[1].ir_type, IrType::I64);
        assert!(!function.instructions.iter().any(|inst| inst.op == Op::StrIncDec));
        assert!(
            function.instructions.iter().any(|inst| inst.op == Op::PhpRelCmp),
            "the bound comparison keeps its PHP semantics"
        );
    }
}

/// `$i < $n + 1` can let the counter reach PHP_INT_MAX - 512, so nothing may be committed.
#[test]
fn bounded_mixed_counter_rejects_counters_below_an_upward_bound() {
    let mut function = checked_bound_loop(Op::ICheckedAdd, CmpPredicate::Slt);
    let before = crate::ir::print_function(&function);
    assert!(!BoundedMixedCounter::default().run(&mut function, &mut DataPool::default()));
    assert_eq!(crate::ir::print_function(&function), before);
}

/// `$j = $i + 1` is proven together with `$i`, and reported as a seeded slot.
#[test]
fn storage_proof_accepts_a_seed_read_from_another_counter() {
    let nested = nested_loops(MixedNumericOp::Add, 1, false);
    let original = crate::ir::print_function(&nested.function);
    let proven = prove(&nested.function);
    assert!(proven.slots.contains(&nested.outer));
    assert!(proven.slots.contains(&nested.inner));
    assert!(proven.seeded.contains(&nested.inner));
    assert!(!proven.seeded.contains(&nested.outer));
    assert_eq!(crate::ir::print_function(&nested.function), original, "the trial is discarded");
}

/// An unprovable sibling counter defeats the joint trial of every counter; the seeded counter
/// is then retried together with the counter its seed reads, and the sibling stays boxed.
#[test]
fn storage_proof_retries_a_seeded_counter_with_its_source() {
    let nested = nested_loops(MixedNumericOp::Add, 1, true);
    let proven = prove(&nested.function);
    assert!(proven.slots.contains(&nested.outer));
    assert!(proven.slots.contains(&nested.inner));
    assert!(proven.seeded.contains(&nested.inner));
    assert!(!proven.slots.contains(&nested.unprovable.expect("sibling counter")));
}

/// `$j = $i * 2` can overflow even below `$n`, so only the outer counter is proven.
#[test]
fn storage_proof_rejects_a_seed_that_can_overflow() {
    for with_unprovable in [false, true] {
        let nested = nested_loops(MixedNumericOp::Mul, 2, with_unprovable);
        let proven = prove(&nested.function);
        assert!(proven.slots.contains(&nested.outer), "{with_unprovable}");
        assert!(!proven.slots.contains(&nested.inner), "{with_unprovable}");
        assert!(proven.seeded.is_empty(), "{with_unprovable}");
    }
}

/// Builds `$k = $n & 255; for ($j = $k + 1; $j < $n; $j++) {}`, or with `$k = $n` when
/// `masked` is false, and returns the checked seed producer.
fn seeded_loop(masked: bool) -> (Function, LocalSlotId, InstId) {
    let (mut function, limit) = function_with_limit("seeded");
    let counter = counter_slot(&mut function, "j");
    let seed;
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let cond = b.create_named_block("cond", vec![]);
        let body = b.create_named_block("body", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let input = b.emit_load_local(limit, IrType::I64, PhpType::Int);
        let base = if masked {
            let mask = b.emit_const_i64(255);
            b.emit(Op::IBitAnd, vec![input, mask], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .expect("masked base")
        } else {
            input
        };
        let one = b.emit_const_i64(1);
        seed = b
            .emit(Op::ICheckedAdd, vec![base, one], None, MIXED, PhpType::Mixed, Ownership::Owned)
            .expect("checked seed");
        let copy = b
            .emit(Op::MixedClone, vec![seed], None, MIXED, PhpType::Mixed, Ownership::Owned)
            .expect("assignment copy");
        emit_void(&mut b, Op::Release, vec![seed], None);
        emit_owned_store(&mut b, counter, copy);
        b.terminate(Terminator::Br { target: cond, args: vec![] });
        b.position_at_end(cond);
        let test = emit_limit_compare(&mut b, counter, limit, CmpPredicate::Slt);
        b.terminate(Terminator::CondBr {
            cond: test,
            then_target: body,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });
        b.position_at_end(body);
        emit_increment(&mut b, counter);
        b.terminate(Terminator::Br { target: cond, args: vec![] });
        b.position_at_end(exit);
        let zero = b.emit_const_i64(0);
        b.terminate(Terminator::Return { value: Some(zero) });
    }
    validate_function(&function).expect("fixture is valid EIR");
    let ValueDef::Instruction { inst, .. } = function.value(seed).expect("seed value").def else {
        unreachable!("the seed is an instruction result");
    };
    (function, counter, inst)
}

/// The optimizer pass promotes a counter whose checked seed range analysis bounds, turning the
/// seed into unchecked arithmetic, and leaves an unbounded `$n + 1` seed and its boxed counter.
#[test]
fn bounded_mixed_counter_promotes_a_counter_with_a_bounded_checked_seed() {
    let (mut function, counter, seed) = seeded_loop(true);
    assert!(BoundedMixedCounter::default().run(&mut function, &mut DataPool::default()));
    validate_function(&function).expect("promoted function is valid EIR");
    assert_eq!(function.locals[counter.as_raw() as usize].ir_type, IrType::I64);
    assert_eq!(function.instruction(seed).expect("seed").op, Op::IAdd);
    assert!(!function.instructions.iter().any(|inst| matches!(
        inst.op,
        Op::StrIncDec | Op::ICheckedAddToInt | Op::MixedClone
    )));

    let (mut function, _, _) = seeded_loop(false);
    let before = crate::ir::print_function(&function);
    assert!(!BoundedMixedCounter::default().run(&mut function, &mut DataPool::default()));
    assert_eq!(crate::ir::print_function(&function), before);
}
