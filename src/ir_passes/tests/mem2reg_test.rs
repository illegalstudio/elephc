//! Purpose:
//! Unit tests for scalar local promotion into block-parameter SSA.
//!
//! Called from:
//! - The compiler library test harness.
//!
//! Key details:
//! - Hand-built CFGs pin join and loop back-edge values independently of AST folding.
//! - Exclusion cases prove that observable or not definitely initialized slots stay in memory.

use crate::codegen::platform::{AppleVariant, Arch, Platform, Target};
use crate::ir::{
    validate_function, Builder, DataPool, Function, Immediate, IrType, LocalKind, LocalSlotId,
    Op, Ownership, SwitchCase, Terminator,
};
use crate::ir_passes::driver::IrPass;
use crate::ir_passes::mem2reg::Mem2Reg;
use crate::ir_passes::allocate_registers;
use crate::types::PhpType;

/// Runs the promotion pass directly on one hand-built function.
fn promote(function: &mut Function) -> bool {
    Mem2Reg.run(function, &mut DataPool::default())
}

/// Counts still-active memory accesses to one local slot.
fn traffic(function: &Function, slot: LocalSlotId) -> usize {
    function.instructions.iter().filter(|inst| {
        matches!(inst.op, Op::LoadLocal | Op::StoreLocal)
            && inst.immediate == Some(Immediate::LocalSlot(slot))
    }).count()
}

/// A diamond merges two different scalar definitions through one block parameter.
#[test]
fn promotes_diamond_join() {
    let mut function = Function::new("diamond".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(Some("x".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    let (left, right, join, left_value, right_value) = {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let left = b.create_named_block("left", vec![]);
        let right = b.create_named_block("right", vec![]);
        let join = b.create_named_block("join", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let cond = b.emit_const_bool(true);
        b.terminate(Terminator::CondBr {
            cond, then_target: left, then_args: vec![], else_target: right, else_args: vec![],
        });
        b.position_at_end(left);
        let left_value = b.emit_const_i64(11);
        b.emit_store_local(slot, left_value);
        b.terminate(Terminator::Br { target: join, args: vec![] });
        b.position_at_end(right);
        let right_value = b.emit_const_i64(22);
        b.emit_store_local(slot, right_value);
        b.terminate(Terminator::Br { target: join, args: vec![] });
        b.position_at_end(join);
        let result = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        b.terminate(Terminator::Return { value: Some(result) });
        (left, right, join, left_value, right_value)
    };
    validate_function(&function).expect("input EIR is valid");
    assert!(promote(&mut function));
    validate_function(&function).expect("promoted EIR is valid");
    assert_eq!(traffic(&function, slot), 0);
    let param = function.block(join).unwrap().params[0];
    assert_eq!(function.block(left).unwrap().terminator,
        Some(Terminator::Br { target: join, args: vec![left_value] }));
    assert_eq!(function.block(right).unwrap().terminator,
        Some(Terminator::Br { target: join, args: vec![right_value] }));
    assert_eq!(function.block(join).unwrap().terminator,
        Some(Terminator::Return { value: Some(param) }));
    assert!(!promote(&mut function), "promotion converges after one pass");
}

/// A counter and accumulator cross the loop back edge through register-eligible parameters.
#[test]
fn promotes_loop_carried_counter_into_register_eligible_parameter() {
    let mut function = Function::new("loop".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(Some("counter".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    let sum_slot = function.add_local(Some("sum".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    let (entry, header, body, initial, updated, sum_initial, sum_updated) = {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![]);
        let body = b.create_named_block("body", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let initial = b.emit_const_i64(3);
        b.emit_store_local(slot, initial);
        let sum_initial = b.emit_const_i64(0);
        b.emit_store_local(sum_slot, sum_initial);
        b.terminate(Terminator::Br { target: header, args: vec![] });
        b.position_at_end(header);
        let condition = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        b.terminate(Terminator::CondBr {
            cond: condition, then_target: body, then_args: vec![], else_target: exit, else_args: vec![],
        });
        b.position_at_end(body);
        b.emit(Op::ConcatReset, vec![], None, IrType::Void,
            PhpType::Void, Ownership::NonHeap);
        let old = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        let old_sum = b.emit_load_local(sum_slot, IrType::I64, PhpType::Int);
        let sum_updated = b.emit(Op::IBitXor, vec![old_sum, old], None, IrType::I64,
            PhpType::Int, Ownership::NonHeap).expect("xor produces a value");
        b.emit_store_local(sum_slot, sum_updated);
        let one = b.emit_const_i64(1);
        let updated = b.emit(Op::ISub, vec![old, one], None, IrType::I64,
            PhpType::Int, Ownership::NonHeap).expect("sub produces a value");
        b.emit_store_local(slot, updated);
        b.terminate(Terminator::Br { target: header, args: vec![] });
        b.position_at_end(exit);
        let result = b.emit_load_local(sum_slot, IrType::I64, PhpType::Int);
        b.terminate(Terminator::Return { value: Some(result) });
        (entry, header, body, initial, updated, sum_initial, sum_updated)
    };
    validate_function(&function).expect("input EIR is valid");
    assert!(promote(&mut function));
    validate_function(&function).expect("promoted EIR is valid");
    assert_eq!(traffic(&function, slot), 0);
    assert_eq!(traffic(&function, sum_slot), 0);
    let param = function.block(header).unwrap().params[0];
    let sum_param = function.block(header).unwrap().params[1];
    assert_eq!(function.block(entry).unwrap().terminator,
        Some(Terminator::Br { target: header, args: vec![initial, sum_initial] }));
    assert_eq!(function.block(body).unwrap().terminator,
        Some(Terminator::Br { target: header, args: vec![updated, sum_updated] }));
    for target in [
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new_apple(Arch::AArch64, AppleVariant::IOS),
        Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
    ] {
        let allocation = allocate_registers(&function, target);
        assert!(allocation.register_of(param).is_some(), "{target:?}: header value gets a register");
        assert!(allocation.register_of(sum_param).is_some(), "{target:?}: accumulator gets a register");
    }
}

/// A join load with an unstored predecessor must retain its memory initialization.
#[test]
fn keeps_possibly_uninitialized_join_in_memory() {
    let mut function = Function::new("partial".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(Some("x".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let written = b.create_named_block("written", vec![]);
        let skipped = b.create_named_block("skipped", vec![]);
        let join = b.create_named_block("join", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let cond = b.emit_const_bool(true);
        b.terminate(Terminator::CondBr {
            cond, then_target: written, then_args: vec![], else_target: skipped, else_args: vec![],
        });
        b.position_at_end(written);
        let value = b.emit_const_i64(7);
        b.emit_store_local(slot, value);
        b.terminate(Terminator::Br { target: join, args: vec![] });
        b.position_at_end(skipped);
        b.terminate(Terminator::Br { target: join, args: vec![] });
        b.position_at_end(join);
        let result = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        b.terminate(Terminator::Return { value: Some(result) });
    }
    assert!(!promote(&mut function));
    assert_eq!(traffic(&function, slot), 2);
}

/// A direct call operand may be reinterpreted as the local's address for by-reference passing.
#[test]
fn keeps_address_escaping_local_in_memory() {
    let mut function = Function::new("alias".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(Some("x".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let value = b.emit_const_i64(1);
        b.emit_store_local(slot, value);
        let argument = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        b.emit(Op::Call, vec![argument], Some(Immediate::I64(0)), IrType::Void,
            PhpType::Void, Ownership::NonHeap);
        let result = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        b.terminate(Terminator::Return { value: Some(result) });
    }
    assert!(!promote(&mut function));
    assert_eq!(traffic(&function, slot), 3);
}

/// A non-load/store operation naming the slot can observe or change its storage.
#[test]
fn keeps_unset_local_in_memory() {
    let mut function = Function::new("unset".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(Some("x".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let value = b.emit_const_i64(1);
        b.emit_store_local(slot, value);
        b.emit(Op::UnsetLocal, vec![], Some(Immediate::LocalSlot(slot)), IrType::Void,
            PhpType::Void, Ownership::NonHeap);
        let result = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        b.terminate(Terminator::Return { value: Some(result) });
    }
    assert!(!promote(&mut function));
    assert_eq!(traffic(&function, slot), 2);
}

/// A float local uses the same scalar forwarding path without changing its value bits.
#[test]
fn promotes_float_local() {
    let mut function = Function::new("float".into(), IrType::F64, PhpType::Float);
    let slot = function.add_local(Some("value".into()), IrType::F64, PhpType::Float, LocalKind::PhpLocal);
    let constant = {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let constant = b.emit_const_f64(-0.0);
        b.emit_store_local(slot, constant);
        let loaded = b.emit_load_local(slot, IrType::F64, PhpType::Float);
        b.terminate(Terminator::Return { value: Some(loaded) });
        constant
    };
    assert!(promote(&mut function));
    validate_function(&function).expect("promoted float IR is valid");
    assert_eq!(traffic(&function, slot), 0);
    assert_eq!(function.blocks[0].terminator,
        Some(Terminator::Return { value: Some(constant) }));
}

/// Boolean PHP locals use integer storage but keep their boolean value type.
#[test]
fn promotes_boolean_local() {
    let mut function = Function::new("bool".into(), IrType::I64, PhpType::Bool);
    let slot = function.add_local(Some("flag".into()), IrType::I64, PhpType::Bool, LocalKind::PhpLocal);
    let constant = {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let constant = b.emit_const_bool(true);
        b.emit_store_local(slot, constant);
        let loaded = b.emit_load_local(slot, IrType::I64, PhpType::Bool);
        b.terminate(Terminator::Return { value: Some(loaded) });
        constant
    };
    assert!(promote(&mut function));
    validate_function(&function).expect("promoted boolean IR is valid");
    assert_eq!(traffic(&function, slot), 0);
    assert_eq!(function.blocks[0].terminator,
        Some(Terminator::Return { value: Some(constant) }));
}

/// Literal false values widen into boolean SSA joins without retaining local traffic.
#[test]
fn promotes_false_values_into_boolean_storage() {
    for loaded_type in [PhpType::False, PhpType::Bool] {
        let mut function = Function::new("false_flag".into(), IrType::I64, PhpType::Bool);
        let slot = function.add_local(
            Some("flag".into()), IrType::I64, PhpType::Bool, LocalKind::PhpLocal,
        );
        let join = {
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", vec![]);
            let left = b.create_named_block("left", vec![]);
            let right = b.create_named_block("right", vec![]);
            let join = b.create_named_block("join", vec![]);
            b.set_entry(entry);
            b.position_at_end(entry);
            let initial = b.emit(Op::ConstBool, vec![], Some(Immediate::Bool(false)),
                IrType::I64, PhpType::False, Ownership::NonHeap).expect("false has a value");
            b.emit_store_local(slot, initial);
            let condition = b.emit_const_bool(true);
            b.terminate(Terminator::CondBr {
                cond: condition, then_target: left, then_args: vec![],
                else_target: right, else_args: vec![],
            });
            b.position_at_end(left);
            let updated = b.emit_const_bool(true);
            b.emit_store_local(slot, updated);
            b.terminate(Terminator::Br { target: join, args: vec![] });
            b.position_at_end(right);
            b.terminate(Terminator::Br { target: join, args: vec![] });
            b.position_at_end(join);
            let loaded = b.emit_load_local(slot, IrType::I64, loaded_type);
            b.terminate(Terminator::Return { value: Some(loaded) });
            join
        };
        validate_function(&function).expect("boolean storage EIR is valid");
        assert!(promote(&mut function), "false values remain eligible for boolean storage");
        validate_function(&function).expect("promoted boolean EIR is valid");
        assert_eq!(traffic(&function, slot), 0);
        let param = function.block(join).unwrap().params[0];
        assert_eq!(function.value(param).unwrap().php_type, PhpType::Bool);
    }
}

/// A boolean value cannot acquire the narrower false type through SSA forwarding.
#[test]
fn keeps_boolean_values_out_of_false_storage() {
    let mut function = Function::new("narrow_false".into(), IrType::I64, PhpType::False);
    let slot = function.add_local(
        Some("flag".into()), IrType::I64, PhpType::Bool, LocalKind::PhpLocal,
    );
    function.locals[slot.as_raw() as usize].php_type = PhpType::False;
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let stored = b.emit_const_bool(true);
        b.emit_store_local(slot, stored);
        let loaded = b.emit_load_local(slot, IrType::I64, PhpType::False);
        b.terminate(Terminator::Return { value: Some(loaded) });
    }
    validate_function(&function).expect("narrow false storage EIR is valid");
    assert!(!promote(&mut function), "boolean widening must remain directional");
    assert_eq!(traffic(&function, slot), 2);
}

/// Integer slot storage must not erase a resource's logical type during SSA forwarding.
#[test]
fn keeps_resource_type_mismatches_in_memory() {
    let resource = PhpType::Resource(Some("stream".into()));
    for (stored_type, loaded_type) in [
        (resource.clone(), resource.clone()),
        (resource.clone(), PhpType::Int),
        (PhpType::Int, resource),
    ] {
        let mut function = Function::new("resource_storage".into(), IrType::Void, PhpType::Void);
        let slot = function.add_local(
            Some("context".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", vec![]);
            let left = b.create_named_block("left", vec![]);
            let right = b.create_named_block("right", vec![]);
            let join = b.create_named_block("join", vec![]);
            b.set_entry(entry);
            b.position_at_end(entry);
            let stored = b.emit(Op::ConstI64, vec![], Some(Immediate::I64(4)), IrType::I64,
                stored_type, Ownership::NonHeap).expect("resource payload has a value");
            b.emit_store_local(slot, stored);
            let condition = b.emit_const_bool(true);
            b.terminate(Terminator::CondBr {
                cond: condition, then_target: left, then_args: vec![],
                else_target: right, else_args: vec![],
            });
            for block in [left, right] {
                b.position_at_end(block);
                b.terminate(Terminator::Br { target: join, args: vec![] });
            }
            b.position_at_end(join);
            let loaded = b.emit_load_local(slot, IrType::I64, loaded_type);
            b.emit(Op::TypePredicate, vec![loaded],
                Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Resource)),
                IrType::I64, PhpType::Bool, Ownership::NonHeap);
            b.terminate(Terminator::Return { value: None });
        }
        validate_function(&function).expect("resource storage EIR is valid");
        assert!(!promote(&mut function), "promotion must preserve logical PHP types");
        assert_eq!(traffic(&function, slot), 2);
    }
}

/// Global, static, and ref-cell slots do not represent ordinary PHP scalar locals.
#[test]
fn keeps_nonlocal_storage_kinds_in_memory() {
    for kind in [LocalKind::GlobalAlias, LocalKind::StaticLocal, LocalKind::RefCell] {
        let mut function = Function::new("other_storage".into(), IrType::I64, PhpType::Int);
        let slot = function.add_local(Some("value".into()), IrType::I64, PhpType::Int, kind);
        {
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", vec![]);
            b.set_entry(entry);
            b.position_at_end(entry);
            let constant = b.emit_const_i64(9);
            b.emit_store_local(slot, constant);
            let loaded = b.emit_load_local(slot, IrType::I64, PhpType::Int);
            b.terminate(Terminator::Return { value: Some(loaded) });
        }
        assert!(!promote(&mut function), "{kind:?} is not promotable");
        assert_eq!(traffic(&function, slot), 2);
    }
}

/// A refcounted string has lifetime obligations that scalar SSA cannot erase.
#[test]
fn keeps_refcounted_local_in_memory() {
    let mut function = Function::new("string".into(), IrType::Str, PhpType::Str);
    let slot = function.add_local(Some("value".into()), IrType::Str, PhpType::Str, LocalKind::PhpLocal);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let constant = b.emit_const_str(crate::ir::DataId::from_raw(0));
        b.emit_store_local(slot, constant);
        let loaded = b.emit_load_local(slot, IrType::Str, PhpType::Str);
        b.terminate(Terminator::Return { value: Some(loaded) });
    }
    assert!(!promote(&mut function));
    assert_eq!(traffic(&function, slot), 2);
}

/// An existing branch parameter can carry a local load onward to a by-reference use.
#[test]
fn keeps_local_load_passed_through_branch_parameter() {
    let mut function = Function::new("forwarded".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(Some("value".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let next = b.create_named_block("next", vec![(IrType::I64, PhpType::Int)]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let constant = b.emit_const_i64(4);
        b.emit_store_local(slot, constant);
        let loaded = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        b.terminate(Terminator::Br { target: next, args: vec![loaded] });
        b.position_at_end(next);
        let param = b.block_param(next, 0);
        b.terminate(Terminator::Return { value: Some(param) });
    }
    assert!(!promote(&mut function));
    assert_eq!(traffic(&function, slot), 2);
}

/// An eval-capable function conservatively retains its local frame storage.
#[test]
fn keeps_eval_visible_local_in_memory() {
    let mut function = Function::new("eval_scope".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(Some("value".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let constant = b.emit_const_i64(9);
        b.emit_store_local(slot, constant);
        b.emit(Op::EvalFunctionExists, vec![], Some(Immediate::Data(crate::ir::DataId::from_raw(0))), IrType::I64,
            PhpType::Bool, Ownership::NonHeap);
        let loaded = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        b.terminate(Terminator::Return { value: Some(loaded) });
    }
    validate_function(&function).expect("input EIR is valid");
    assert!(!promote(&mut function));
    assert_eq!(traffic(&function, slot), 2);
}

/// Dynamic eval uses a profiled language-construct call and can read later-declared locals.
#[test]
fn keeps_dynamic_eval_visible_local_in_memory() {
    let mut function = Function::new("dynamic_eval_scope".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(Some("value".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        b.emit(Op::LanguageConstructCall, vec![],
            Some(Immediate::ProfiledData { data: crate::ir::DataId::from_raw(0), strict_php: false }),
            IrType::Void, PhpType::Void, Ownership::NonHeap);
        let value = b.emit_const_i64(9);
        b.emit_store_local(slot, value);
        let result = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        b.terminate(Terminator::Return { value: Some(result) });
    }
    validate_function(&function).expect("input EIR is valid");
    assert!(!promote(&mut function));
    assert_eq!(traffic(&function, slot), 2);
}

/// A definition before a loop must stay definite through a read-only back edge.
#[test]
fn promotes_read_only_loop_value() {
    let mut function = Function::new("loop_invariant".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(Some("bound".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![]);
        let body = b.create_named_block("body", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let initial = b.emit_const_i64(7);
        b.emit_store_local(slot, initial);
        b.terminate(Terminator::Br { target: header, args: vec![] });
        b.position_at_end(header);
        let bound = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        b.terminate(Terminator::CondBr {
            cond: bound, then_target: body, then_args: vec![], else_target: exit, else_args: vec![],
        });
        b.position_at_end(body);
        b.terminate(Terminator::Br { target: header, args: vec![] });
        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: Some(bound) });
    }
    validate_function(&function).expect("input EIR is valid");
    assert!(promote(&mut function));
    validate_function(&function).expect("promoted EIR is valid");
    assert_eq!(traffic(&function, slot), 0);
}

/// Every switch edge supplies the promoted local to a shared target parameter.
#[test]
fn promotes_value_carried_by_switch_edges() {
    let mut function = Function::new("switch_values".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(Some("value".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    let (entry, join, initial) = {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let join = b.create_named_block("join", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let initial = b.emit_const_i64(7);
        b.emit_store_local(slot, initial);
        let scrutinee = b.emit_const_i64(1);
        b.terminate(Terminator::Switch {
            scrutinee,
            cases: vec![
                SwitchCase { value: 1, target: join, args: vec![] },
                SwitchCase { value: 2, target: join, args: vec![] },
            ],
            default: join,
            default_args: vec![],
        });
        b.position_at_end(join);
        let result = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        b.terminate(Terminator::Return { value: Some(result) });
        (entry, join, initial)
    };
    validate_function(&function).expect("input EIR is valid");
    assert!(promote(&mut function));
    validate_function(&function).expect("promoted EIR is valid");
    assert_eq!(traffic(&function, slot), 0);
    let Some(Terminator::Switch { cases, default_args, .. }) =
        function.block(entry).unwrap().terminator.as_ref() else { panic!("switch survives promotion") };
    assert!(cases.iter().all(|case| case.args == vec![initial]));
    assert_eq!(default_args, &vec![initial]);
    assert_eq!(function.block(join).unwrap().params.len(), 1);
}

/// Implicit catch edges keep try/catch functions outside scalar promotion.
#[test]
fn keeps_exception_handler_function_in_memory() {
    let mut function = Function::new("with_handler".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(Some("value".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        b.emit(Op::TryPushHandler, vec![], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
        let initial = b.emit_const_i64(7);
        b.emit_store_local(slot, initial);
        let result = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        b.terminate(Terminator::Return { value: Some(result) });
    }
    assert!(!promote(&mut function));
    assert_eq!(traffic(&function, slot), 2);
}
