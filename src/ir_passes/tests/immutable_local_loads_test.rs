//! Purpose:
//! Regression tests for conservative immutable integer-local load classification.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Covers read-only `$argc`, one dominating entry store, multiple writes,
//!   non-entry stores, by-reference parameters, and alias metadata that reject purity.
//! - Also covers the slot a CALLEE writes through a by-reference argument alias: it has
//!   exactly one entry store in this function and would otherwise pass the immutability
//!   proof, after which LICM hoists the loop condition reading it out of the loop.

use crate::ir::{
    validate_function, Builder, DataPool, Function, FunctionParam, Immediate, IrType, LocalKind,
    Op, Ownership, Terminator,
};
use crate::ir_passes::driver::IrPass;
use crate::ir_passes::immutable_local_loads::ImmutableLocalLoads;
use crate::types::PhpType;

/// Runs immutable-local classification once with an unused literal pool.
fn classify(function: &mut Function) -> bool {
    ImmutableLocalLoads.run(function, &mut DataPool::default())
}

/// A read-only top-level `$argc` load is pure and validates with the refined effect.
#[test]
fn classifies_main_argc_as_immutable() {
    let mut function = Function::new("main".to_string(), IrType::I64, PhpType::Int);
    function.flags.is_main = true;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let argc = builder.add_local(
            Some("argc".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let value = builder.emit_load_local(argc, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(value) });
    }

    assert!(classify(&mut function));
    assert!(function.instructions[0].effects.is_pure());
    assert!(validate_function(&function).is_ok());
    assert!(!classify(&mut function), "effect refinement is idempotent");
}

/// A by-reference integer parameter may change through its alias and is never read-only.
#[test]
fn rejects_by_reference_integer_parameter() {
    let mut function = Function::new("by_ref_param".to_string(), IrType::I64, PhpType::Int);
    function.params.push(FunctionParam {
        name: "value".to_string(),
        ir_type: IrType::I64,
        php_type: PhpType::Int,
        by_ref: true,
        variadic: false,
    });
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("value".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let value = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(value) });
    }

    assert!(!classify(&mut function));
    assert_eq!(function.instructions[0].effects, Op::LoadLocal.default_effects());
}

/// One entry-block store before every load makes a concrete integer local immutable.
#[test]
fn classifies_single_dominating_entry_store() {
    let mut function = Function::new("single_store".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("value".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let seed = builder.emit_const_i64(7);
        builder.emit_store_local(slot, seed);
        let value = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(value) });
    }

    assert!(classify(&mut function));
    assert!(function.instructions[2].effects.is_pure());
    assert!(validate_function(&function).is_ok());
}

/// Two stores make the slot mutable even when both occur in the entry block.
#[test]
fn rejects_multiple_stores() {
    let mut function = Function::new("multiple_stores".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("value".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let first = builder.emit_const_i64(1);
        let second = builder.emit_const_i64(2);
        builder.emit_store_local(slot, first);
        builder.emit_store_local(slot, second);
        let value = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(value) });
    }

    assert!(!classify(&mut function));
    assert_eq!(function.instructions[4].effects, Op::LoadLocal.default_effects());
}

/// A sole store outside the entry may execute repeatedly and is conservatively rejected.
#[test]
fn rejects_non_entry_store() {
    let mut function = Function::new("late_store".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let body = builder.create_named_block("body", vec![]);
        builder.set_entry(entry);
        let slot = builder.add_local(
            Some("value".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        builder.position_at_end(entry);
        builder.terminate(Terminator::Br { target: body, args: vec![] });
        builder.position_at_end(body);
        let seed = builder.emit_const_i64(7);
        builder.emit_store_local(slot, seed);
        let value = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(value) });
    }

    assert!(!classify(&mut function));
    assert_eq!(function.instructions[2].effects, Op::LoadLocal.default_effects());
}

/// Any local-slot pair metadata represents aliasing and invalidates an otherwise immutable slot.
#[test]
fn rejects_reference_alias_metadata() {
    let mut function = Function::new("aliased".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("value".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let cell = builder.add_local(
            Some("value#ref".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::RefCell,
        );
        let seed = builder.emit_const_i64(7);
        builder.emit_store_local(slot, seed);
        let _ = builder.emit(
            Op::AliasLocalRefCell,
            vec![],
            Some(Immediate::LocalSlotPair { first: slot, second: cell }),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        let value = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(value) });
    }

    assert!(!classify(&mut function));
    assert_eq!(function.instructions[3].effects, Op::LoadLocal.default_effects());
}

/// A LOCAL PASSED BY REFERENCE IS NEVER IMMUTABLE, even though this function stores it
/// exactly once, in the entry, before every load: the callee writes the caller's slot
/// through the argument alias, and no `store_local` here records that.
///
/// This is the shape PHP's own `curl_multi_exec()` loop is written in —
/// `$n = 0; do { f($h, $n); … } while ($n > 0);` — and calling the load pure let LICM
/// hoist the whole `$n > 0` compare into the preheader, so the loop ran exactly once.
#[test]
fn rejects_a_slot_written_through_a_by_reference_argument() {
    let mut data = DataPool::default();
    let callee = data.intern_function_name("step");
    let mut function = Function::new("by_ref_arg".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("running".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let seed = builder.emit_const_i64(0);
        builder.emit_store_local(slot, seed);
        // The by-reference argument: an ordinary load whose RESULT the call consumes.
        // Codegen turns that value back into the slot's address.
        let alias = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder
            .emit(
                Op::Call,
                vec![alias],
                Some(Immediate::Data(callee)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        // The loop condition's read of the same slot, which must stay effectful.
        let observed = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(observed) });
    }

    assert!(!classify(&mut function), "a by-ref-aliased slot is not immutable");
    for inst in &function.instructions {
        if inst.op == Op::LoadLocal {
            assert_eq!(
                inst.effects,
                Op::LoadLocal.default_effects(),
                "loads of a by-ref-aliased slot must stay effectful"
            );
        }
    }
    assert!(validate_function(&function).is_ok());
}
