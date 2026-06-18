//! Purpose:
//! Tests for the EIR peephole pass: box/unbox cancellation, redundant
//! `Move`/`Borrow` cleanup, scalar load/store forwarding, paired
//! acquire/release cancellation, and string-literal concat folding, plus the
//! invariant that every rewrite leaves the function valid.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built with `crate::ir::Builder`. Each test asserts the
//!   rewrite and re-validates with `validate_function`. Negative cases prove the
//!   conservative guards (heap payloads, barriers, multi-use, non-literals,
//!   ownership mismatches) are respected.

use crate::ir::{
    validate_function, Builder, DataId, DataPool, Function, Immediate, IrHeapKind, IrType,
    LocalKind, Op, Ownership, Terminator, ValueId,
};
use crate::ir_passes::driver::IrPass;
use crate::ir_passes::peephole::Peephole;
use crate::types::PhpType;

/// Runs the peephole pass once over `function` with a throwaway literal pool.
fn run_peephole(function: &mut Function) -> bool {
    Peephole.run(function, &mut DataPool::default())
}

/// Runs the peephole pass once over `function` with a caller-supplied literal
/// pool, used by string-concat tests that read and intern interned literals.
fn run_peephole_with(function: &mut Function, data: &mut DataPool) -> bool {
    Peephole.run(function, data)
}

/// Returns the literal string referenced by a `ConstStr` instruction.
fn const_str_text<'a>(function: &Function, data: &'a DataPool, inst: usize) -> &'a str {
    let inst = &function.instructions[inst];
    assert_eq!(inst.op, Op::ConstStr, "expected a const_str instruction");
    match &inst.immediate {
        Some(Immediate::Data(id)) => &data.strings[id.as_raw() as usize],
        other => panic!("const_str without a data immediate: {other:?}"),
    }
}

/// Returns the value returned by the function's single-return entry path.
fn return_value(function: &Function) -> Option<ValueId> {
    match function.block(function.entry).unwrap().terminator.as_ref() {
        Some(Terminator::Return { value }) => *value,
        _ => None,
    }
}

// --- box/unbox cancellation ---------------------------------------------------

/// `unbox(box(x))` for a scalar folds to `x`: the unbox becomes a `Nop` and the
/// return uses the original scalar value directly.
#[test]
fn unbox_of_box_scalar_folds_to_operand() {
    let mut function = Function::new("box_unbox".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let x = builder.emit_const_i64(42);
        let boxed = builder
            .emit(
                Op::MixedBox,
                vec![x],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .expect("box result");
        let unboxed = builder
            .emit(
                Op::MixedUnbox,
                vec![boxed],
                None,
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("unbox result");
        builder.terminate(Terminator::Return { value: Some(unboxed) });
    }
    assert!(run_peephole(&mut function), "scalar unbox(box(x)) must fold");
    assert_eq!(function.instructions[2].op, Op::Nop, "the unbox is neutralized");
    assert_eq!(return_value(&function), Some(ValueId::from_raw(0)), "returns x");
    assert!(validate_function(&function).is_ok(), "folded IR stays valid");
}

/// A non-scalar payload (`unbox` producing a refcounted `Str`) is never folded:
/// the round-trip would change ownership/refcount semantics, so the guard
/// rejects anything whose unbox result is not `NonHeap`.
#[test]
fn unbox_of_box_string_payload_is_not_folded() {
    let mut function = Function::new("box_unbox_str".to_string(), IrType::Str, PhpType::Str);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let s = builder.emit_const_str(DataId::from_raw(0));
        let boxed = builder
            .emit(
                Op::MixedBox,
                vec![s],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .expect("box result");
        let unboxed = builder
            .emit(
                Op::MixedUnbox,
                vec![boxed],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::MaybeOwned,
            )
            .expect("unbox result");
        builder.terminate(Terminator::Return { value: Some(unboxed) });
    }
    assert!(!run_peephole(&mut function), "string payload round-trip must not fold");
    assert_eq!(function.instructions[2].op, Op::MixedUnbox, "unbox is preserved");
    assert!(validate_function(&function).is_ok());
}

// --- redundant Move / Borrow cleanup -----------------------------------------

/// A scalar `Move(x)` whose result has the same ownership as `x` folds to `x`:
/// the forwarding op is neutralized and uses redirect to the operand.
#[test]
fn move_with_matching_ownership_folds_to_operand() {
    let mut function = Function::new("move_fold".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let x = builder.emit_const_i64(5);
        let moved = builder
            .emit(Op::Move, vec![x], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
            .expect("move result");
        builder.terminate(Terminator::Return { value: Some(moved) });
    }
    assert!(run_peephole(&mut function), "scalar Move must fold");
    assert_eq!(function.instructions[1].op, Op::Nop, "the move is neutralized");
    assert_eq!(return_value(&function), Some(ValueId::from_raw(0)), "returns x");
    assert!(validate_function(&function).is_ok());
}

/// A `Borrow` that changes ownership (`Persistent` operand → `Borrowed` result)
/// is not folded: redirecting uses to the operand would shift cleanup
/// responsibility, so the ownership-mismatch guard rejects it.
#[test]
fn borrow_changing_ownership_is_not_folded() {
    let mut function = Function::new("borrow_keep".to_string(), IrType::Str, PhpType::Str);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let s = builder.emit_const_str(DataId::from_raw(0));
        let borrowed = builder
            .emit(Op::Borrow, vec![s], None, IrType::Str, PhpType::Str, Ownership::Borrowed)
            .expect("borrow result");
        builder.terminate(Terminator::Return { value: Some(borrowed) });
    }
    assert!(!run_peephole(&mut function), "ownership-changing Borrow must not fold");
    assert_eq!(function.instructions[1].op, Op::Borrow, "the borrow is preserved");
    assert!(validate_function(&function).is_ok());
}

// --- paired acquire / release cancellation -----------------------------------

/// `a = Acquire(x); Release(a)` with `a` used only by that release cancels both:
/// the acquire and release are neutralized (refcount-neutral, nothing observes
/// the acquired copy).
#[test]
fn single_use_acquire_release_pair_cancels() {
    let mut function = Function::new("acq_rel".to_string(), IrType::Str, PhpType::Str);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let x = builder.emit_const_str(DataId::from_raw(0));
        let acquired = builder
            .emit(Op::Acquire, vec![x], None, IrType::Str, PhpType::Str, Ownership::Owned)
            .expect("acquire result");
        builder.emit(
            Op::Release,
            vec![acquired],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: Some(x) });
    }
    assert!(run_peephole(&mut function), "the acquire/release pair must cancel");
    assert_eq!(function.instructions[1].op, Op::Nop, "the acquire is neutralized");
    assert_eq!(function.instructions[2].op, Op::Nop, "the release is neutralized");
    assert_eq!(return_value(&function), Some(ValueId::from_raw(0)), "still returns x");
    assert!(validate_function(&function).is_ok());
}

/// An acquired value used elsewhere (here, also returned) is not cancelled: the
/// release is not its only use, so the value genuinely lives past the release.
#[test]
fn multi_use_acquire_is_not_cancelled() {
    let mut function = Function::new("acq_multi".to_string(), IrType::Str, PhpType::Str);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let x = builder.emit_const_str(DataId::from_raw(0));
        let acquired = builder
            .emit(Op::Acquire, vec![x], None, IrType::Str, PhpType::Str, Ownership::Owned)
            .expect("acquire result");
        builder.emit(
            Op::Release,
            vec![acquired],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: Some(acquired) });
    }
    assert!(!run_peephole(&mut function), "multi-use acquire must not cancel");
    assert_eq!(function.instructions[1].op, Op::Acquire, "the acquire is preserved");
    assert_eq!(function.instructions[2].op, Op::Release, "the release is preserved");
    assert!(validate_function(&function).is_ok());
}

// --- redundant load / store --------------------------------------------------

/// Adds a scalar `PhpLocal` slot to the function under construction.
fn add_int_local(builder: &mut Builder<'_>) -> crate::ir::LocalSlotId {
    builder.add_local(Some("n".to_string()), IrType::I64, PhpType::Int, LocalKind::PhpLocal)
}

/// A scalar `LoadLocal` right after a `StoreLocal` to the same slot forwards to
/// the stored value: the load is neutralized and uses redirect to that value.
#[test]
fn load_after_store_forwards_stored_value() {
    let mut function = Function::new("load_fwd".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let slot = add_int_local(&mut builder);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let v = builder.emit_const_i64(9);
        builder.emit_store_local(slot, v);
        let loaded = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(loaded) });
    }
    assert!(run_peephole(&mut function), "load-after-store must forward");
    assert_eq!(function.instructions[2].op, Op::Nop, "the load is neutralized");
    assert_eq!(return_value(&function), Some(ValueId::from_raw(0)), "returns the stored value");
    assert!(validate_function(&function).is_ok());
}

/// Storing back the value just loaded from the same slot is a redundant store
/// and is neutralized; the load is preserved.
#[test]
fn store_of_just_loaded_value_is_removed() {
    let mut function = Function::new("self_store".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let slot = add_int_local(&mut builder);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let loaded = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.emit_store_local(slot, loaded);
        builder.terminate(Terminator::Return { value: Some(loaded) });
    }
    assert!(run_peephole(&mut function), "self-store must be removed");
    assert_eq!(function.instructions[0].op, Op::LoadLocal, "the load is preserved");
    assert_eq!(function.instructions[1].op, Op::Nop, "the redundant store is neutralized");
    assert!(validate_function(&function).is_ok());
}

/// A load after the slot is unset must not forward to the earlier stored value:
/// the `UnsetLocal` barrier invalidates the slot's tracked value.
#[test]
fn load_after_unset_barrier_does_not_forward() {
    let mut function = Function::new("unset_barrier".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let slot = add_int_local(&mut builder);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let v = builder.emit_const_i64(3);
        builder.emit_store_local(slot, v);
        builder.emit(
            Op::UnsetLocal,
            vec![],
            Some(Immediate::LocalSlot(slot)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        let loaded = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(loaded) });
    }
    assert!(!run_peephole(&mut function), "the unset barrier blocks forwarding");
    assert_eq!(function.instructions[3].op, Op::LoadLocal, "the load is preserved");
    assert!(validate_function(&function).is_ok());
}

/// A refcounted (`Str`) slot is never forwarded: only `NonHeap` scalar values
/// qualify, so heap ownership/aliasing is never affected.
#[test]
fn load_after_store_heap_slot_is_not_forwarded() {
    let mut function = Function::new("heap_slot".to_string(), IrType::Str, PhpType::Str);
    {
        let mut builder = Builder::new(&mut function);
        let slot =
            builder.add_local(Some("s".to_string()), IrType::Str, PhpType::Str, LocalKind::PhpLocal);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let v = builder.emit_const_str(DataId::from_raw(0));
        builder.emit_store_local(slot, v);
        let loaded = builder.emit_load_local(slot, IrType::Str, PhpType::Str);
        builder.terminate(Terminator::Return { value: Some(loaded) });
    }
    assert!(!run_peephole(&mut function), "heap slot must not forward");
    assert_eq!(function.instructions[2].op, Op::LoadLocal, "the load is preserved");
    assert!(validate_function(&function).is_ok());
}

/// A load whose result feeds a call (potentially a by-reference argument) must
/// not be forwarded: the slot escapes, so the load stays a real `load_local`
/// the backend can take the address of, and a callee mutation is never crossed.
#[test]
fn load_feeding_call_arg_is_not_forwarded() {
    let mut function = Function::new("escaping".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let slot = add_int_local(&mut builder);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let v = builder.emit_const_i64(7);
        builder.emit_store_local(slot, v);
        let loaded = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        // A call consuming the loaded value — it may pass the local by reference.
        builder.emit(
            Op::Call,
            vec![loaded],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: None });
    }
    assert!(!run_peephole(&mut function), "an escaping slot must not forward");
    assert_eq!(function.instructions[2].op, Op::LoadLocal, "the load is preserved");
    assert!(validate_function(&function).is_ok());
}

// --- string-literal concat folding -------------------------------------------

/// Builds `return concat(const a, const b)` with both operands string literals.
fn literal_concat_function(data: &mut DataPool, a: &str, b: &str) -> Function {
    let id_a = data.intern_string(a);
    let id_b = data.intern_string(b);
    let mut function = Function::new("concat".to_string(), IrType::Str, PhpType::Str);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let lhs = builder.emit_const_str(id_a);
        let rhs = builder.emit_const_str(id_b);
        let result = builder
            .emit(Op::StrConcat, vec![lhs, rhs], None, IrType::Str, PhpType::Str, Ownership::Owned)
            .expect("concat result");
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    function
}

/// `concat("foo", "bar")` folds in place to a single `ConstStr "foobar"` marked
/// `Persistent`, so no concat runs and the literal is never freed at runtime.
#[test]
fn concat_of_two_literals_folds_to_const_str() {
    let mut data = DataPool::default();
    let mut function = literal_concat_function(&mut data, "foo", "bar");
    assert!(run_peephole_with(&mut function, &mut data), "literal concat must fold");
    assert_eq!(const_str_text(&function, &data, 2), "foobar", "operands are joined");
    let result = function.instructions[2].result.expect("const_str result");
    assert_eq!(
        function.value(result).unwrap().ownership,
        Ownership::Persistent,
        "the folded literal is persistent",
    );
    assert!(function.instructions[2].operands.is_empty(), "const_str has no operands");
    assert!(validate_function(&function).is_ok(), "folded IR stays valid");
}

/// A concat with a non-literal operand (a runtime `i_to_str`) is not folded:
/// only two `ConstStr` operands qualify.
#[test]
fn concat_with_non_literal_operand_is_not_folded() {
    let mut data = DataPool::default();
    let id_a = data.intern_string("x");
    let mut function = Function::new("concat_rt".to_string(), IrType::Str, PhpType::Str);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let lhs = builder.emit_const_str(id_a);
        let n = builder.emit_const_i64(7);
        let rhs = builder
            .emit(Op::IToStr, vec![n], None, IrType::Str, PhpType::Str, Ownership::Owned)
            .expect("i_to_str result");
        let result = builder
            .emit(Op::StrConcat, vec![lhs, rhs], None, IrType::Str, PhpType::Str, Ownership::Owned)
            .expect("concat result");
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    assert!(!run_peephole_with(&mut function, &mut data), "runtime concat must not fold");
    assert_eq!(function.instructions[3].op, Op::StrConcat, "the concat is preserved");
    assert!(validate_function(&function).is_ok());
}

/// Nested literal concats `concat(concat("a","b"),"c")` converge to a single
/// `ConstStr "abc"` across repeated sweeps (the driver's fixed point).
#[test]
fn nested_literal_concats_converge_to_single_literal() {
    let mut data = DataPool::default();
    let id_a = data.intern_string("a");
    let id_b = data.intern_string("b");
    let id_c = data.intern_string("c");
    let mut function = Function::new("nested".to_string(), IrType::Str, PhpType::Str);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let a = builder.emit_const_str(id_a);
        let b = builder.emit_const_str(id_b);
        let inner = builder
            .emit(Op::StrConcat, vec![a, b], None, IrType::Str, PhpType::Str, Ownership::Owned)
            .expect("inner concat");
        let c = builder.emit_const_str(id_c);
        let outer = builder
            .emit(Op::StrConcat, vec![inner, c], None, IrType::Str, PhpType::Str, Ownership::Owned)
            .expect("outer concat");
        builder.terminate(Terminator::Return { value: Some(outer) });
    }
    // Iterate to a fixed point, mirroring the driver.
    let mut sweeps = 0;
    while run_peephole_with(&mut function, &mut data) {
        sweeps += 1;
        assert!(sweeps < 8, "peephole must converge");
        assert!(validate_function(&function).is_ok(), "stays valid each sweep");
    }
    let outer = return_value(&function).expect("return value");
    let outer_inst = function.value(outer).unwrap();
    let crate::ir::ValueDef::Instruction { inst, .. } = outer_inst.def else {
        panic!("return value must be instruction-defined");
    };
    assert_eq!(
        const_str_text(&function, &data, inst.as_raw() as usize),
        "abc",
        "nested literals fully fold",
    );
    assert!(validate_function(&function).is_ok());
}
