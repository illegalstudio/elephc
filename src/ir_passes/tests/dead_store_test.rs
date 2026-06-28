//! Purpose:
//! Tests for CFG-aware dead store elimination over scalar PHP local slots.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are hand-built with `crate::ir::Builder` so the tests isolate slot
//!   liveness, the overwrite-before-read condition across blocks, the refcounted
//!   slot exclusion, and the aliasing exclusion for non-load/store slot ops.

use crate::ir::{
    validate_function, Builder, DataPool, Function, Immediate, IrType, LocalKind, Op, Ownership,
    Terminator,
};
use crate::ir_passes::dead_store::DeadStore;
use crate::ir_passes::driver::{run_function_passes, IrPass};
use crate::types::PhpType;

/// Runs one dead-store pass over a function with a throwaway literal pool.
fn run_once(function: &mut Function) -> bool {
    DeadStore.run(function, &mut DataPool::default())
}

/// Runs the fixed-point driver with only the dead-store pass registered.
fn run_to_fixed_point(function: &mut Function) {
    let passes: Vec<Box<dyn IrPass>> = vec![Box::new(DeadStore)];
    run_function_passes(function, &passes, &mut DataPool::default());
}

/// Declares one scalar `PhpLocal` integer slot on the function under test.
fn add_int_local(builder: &mut Builder<'_>, name: &str) -> crate::ir::LocalSlotId {
    builder.add_local(
        Some(name.to_string()),
        IrType::I64,
        PhpType::Int,
        LocalKind::PhpLocal,
    )
}

/// A store overwritten by a later store with no intervening read is dead and the
/// surviving store stays in place.
#[test]
fn store_overwritten_before_read_is_removed() {
    let mut function = Function::new("overwrite".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let slot = add_int_local(&mut builder, "x");
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let dead = builder.emit_const_i64(1);
        builder.emit_store_local(slot, dead); // dead: overwritten before any read
        let live = builder.emit_const_i64(2);
        builder.emit_store_local(slot, live);
        let loaded = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(loaded) });
    }

    assert!(run_once(&mut function), "the overwritten store should die");
    assert_eq!(function.instructions[1].op, Op::Nop, "dead store removed");
    assert_eq!(
        function.instructions[3].op,
        Op::StoreLocal,
        "the surviving store that feeds the read stays"
    );
    assert!(
        validate_function(&function).is_ok(),
        "DSE keeps the function valid"
    );
}

/// A store whose value is read before the next store is live and preserved.
#[test]
fn store_read_before_overwrite_is_preserved() {
    let mut function = Function::new("read_first".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let slot = add_int_local(&mut builder, "x");
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let first = builder.emit_const_i64(1);
        builder.emit_store_local(slot, first);
        let mid = builder.emit_load_local(slot, IrType::I64, PhpType::Int); // read keeps the store live
        let second = builder.emit_const_i64(2);
        builder.emit_store_local(slot, second);
        let last = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        let sum = builder.emit_iadd(mid, last);
        builder.terminate(Terminator::Return { value: Some(sum) });
    }

    assert!(
        !run_once(&mut function),
        "both stores are observed before the next write"
    );
    assert_eq!(function.instructions[1].op, Op::StoreLocal, "first store stays");
    assert_eq!(function.instructions[4].op, Op::StoreLocal, "second store stays");
    assert!(validate_function(&function).is_ok());
}

/// A store to a slot that is never read is dead even with no later overwrite.
#[test]
fn store_to_never_read_slot_is_removed() {
    let mut function = Function::new("write_only".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let slot = add_int_local(&mut builder, "unused");
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(7);
        builder.emit_store_local(slot, value); // slot never loaded anywhere
        let result = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(result) });
    }

    assert!(run_once(&mut function), "a write-only slot store is dead");
    assert_eq!(function.instructions[1].op, Op::Nop, "write-only store removed");
    assert!(validate_function(&function).is_ok());
}

/// A store overwritten in a successor block before any read is dead through
/// cross-block slot liveness.
#[test]
fn cross_block_overwrite_kills_predecessor_store() {
    let mut function = Function::new("cross_block".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let slot = add_int_local(&mut builder, "x");
        let entry = builder.create_named_block("entry", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let dead = builder.emit_const_i64(1);
        builder.emit_store_local(slot, dead); // overwritten in `exit` before any read
        builder.terminate(Terminator::Br {
            target: exit,
            args: Vec::new(),
        });

        builder.position_at_end(exit);
        let live = builder.emit_const_i64(2);
        builder.emit_store_local(slot, live);
        let loaded = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(loaded) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instructions[1].op,
        Op::Nop,
        "predecessor store dies once the successor overwrite is seen"
    );
    assert_eq!(
        function.instructions[3].op,
        Op::StoreLocal,
        "the successor store feeding the read stays"
    );
    assert!(validate_function(&function).is_ok());
}

/// A store stays live when the slot is read on at least one successor path, even
/// if another path overwrites it first.
#[test]
fn conditional_read_on_one_path_keeps_store_live() {
    let mut function = Function::new("cond_read".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let slot = add_int_local(&mut builder, "x");
        let entry = builder.create_named_block("entry", vec![]);
        let read_block = builder.create_named_block("read", vec![]);
        let write_block = builder.create_named_block("write", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let stored = builder.emit_const_i64(1);
        builder.emit_store_local(slot, stored); // read on the `read` path → live
        let cond = builder.emit_const_bool(true);
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: read_block,
            then_args: Vec::new(),
            else_target: write_block,
            else_args: Vec::new(),
        });

        builder.position_at_end(read_block);
        let loaded = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(loaded) });

        builder.position_at_end(write_block);
        let overwrite = builder.emit_const_i64(2);
        builder.emit_store_local(slot, overwrite);
        // Read back the overwrite so this block holds no dead store of its own,
        // isolating the entry store as the only elimination candidate.
        let reloaded = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(reloaded) });
    }

    assert!(
        !run_once(&mut function),
        "a read on one path keeps the entry store live"
    );
    assert_eq!(function.instructions[1].op, Op::StoreLocal, "entry store stays");
    assert!(validate_function(&function).is_ok());
}

/// A store to a refcounted slot is preserved because dropping it in isolation
/// would unbalance the acquire/release ownership ops lowering emits around it.
#[test]
fn refcounted_slot_store_is_preserved() {
    let mut function = Function::new("refcounted".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        // A string-typed slot needs lifetime tracking, so it is ineligible. The
        // stored value's IR type is irrelevant to eligibility, which keys off the
        // slot's refcounted storage type.
        let slot = builder.add_local(
            Some("s".to_string()),
            IrType::I64,
            PhpType::Str,
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(0);
        builder.emit_store_local(slot, value); // never read, but slot is refcounted
        builder.terminate(Terminator::Return { value: None });
    }

    assert!(
        !run_once(&mut function),
        "refcounted slot stores are out of scope for this pass"
    );
    assert_eq!(
        function.instructions[1].op,
        Op::StoreLocal,
        "refcounted store stays in place"
    );
    assert!(validate_function(&function).is_ok());
}

/// A store stays live when the slot is also named by a non-load/store op, since
/// that op could read or alias the slot in a way the pass does not model.
#[test]
fn slot_aliased_by_other_op_is_ineligible() {
    let mut function = Function::new("aliased".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let slot = add_int_local(&mut builder, "x");
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(1);
        builder.emit_store_local(slot, value); // would look dead by load/store alone
        // An `unset_local` names the slot, marking it ineligible.
        builder.emit(
            Op::UnsetLocal,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: None });
    }

    assert!(
        !run_once(&mut function),
        "a slot named by unset_local is ineligible for DSE"
    );
    assert_eq!(
        function.instructions[1].op,
        Op::StoreLocal,
        "store to an aliased slot stays"
    );
    assert!(validate_function(&function).is_ok());
}
