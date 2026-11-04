//! Purpose:
//! Regression tests for scalar register homes across block and exception edges.
//!
//! Called from:
//! - `cargo test --lib ir_passes::tests::regalloc_edges_test`.
//!
//! Key details:
//! - Dead parameters are still written by edge copies and must not alias live homes.
//! - Implicit exception edges require stack homes until liveness models them.
//! - Allocation safety is checked for every supported target and scalar register class.

use crate::codegen::platform::{AppleVariant, Arch, Platform, Target};
use crate::ir::{Builder, Function, Immediate, IrType, Op, Ownership, Terminator};
use crate::ir_passes::allocate_registers;
use crate::types::PhpType;

/// Returns the complete supported target matrix for target-independent allocation guards.
fn targets() -> [Target; 5] {
    [
        Target::new(Platform::Linux, Arch::X86_64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new_apple(Arch::AArch64, AppleVariant::IOS),
        Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
    ]
}

/// A dead earlier parameter must not overwrite a later live parameter during reverse copies.
#[test]
fn dead_block_parameter_cannot_share_a_live_destination_home() {
    for (ir_type, php_type) in [(IrType::I64, PhpType::Int), (IrType::F64, PhpType::Float)] {
        let mut function = Function::new("dead_param".into(), ir_type, php_type.clone());
        let (dead, live) = {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            let body = builder.create_named_block("body", vec![
                (ir_type, php_type.clone()), (ir_type, php_type.clone()),
            ]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let mut constant = |value: i64| {
                let (op, immediate) = match ir_type {
                    IrType::I64 => (Op::ConstI64, Immediate::I64(value)),
                    _ => (Op::ConstF64, Immediate::F64(value as f64)),
                };
                builder.emit(op, vec![], Some(immediate), ir_type, php_type.clone(), Ownership::NonHeap)
                    .expect("scalar constant produces a value")
            };
            let first = constant(7);
            let second = constant(42);
            builder.terminate(Terminator::Br { target: body, args: vec![first, second] });
            builder.position_at_end(body);
            let dead = builder.block_param(body, 0);
            let live = builder.block_param(body, 1);
            builder.terminate(Terminator::Return { value: Some(live) });
            (dead, live)
        };
        crate::ir::validate_function(&function).expect("valid scalar edge fixture");

        for target in targets() {
            let allocation = allocate_registers(&function, target);
            let dead_home = allocation.register_of(dead);
            let live_home = allocation.register_of(live).expect("a live scalar remains register-eligible");
            assert_ne!(dead_home, Some(live_home), "{target:?}: dead {ir_type:?} parameter overwrites live home");
        }
    }
}

/// A scalar return reachable only through a handler cannot use a truncated live interval.
#[test]
fn exception_handler_functions_keep_scalar_values_on_stack() {
    let mut function = Function::new("exception_param".into(), IrType::I64, PhpType::Int);
    let (argument, param) = {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let body = builder.create_named_block("body", vec![(IrType::I64, PhpType::Int)]);
        let handler = builder.create_named_block("handler", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let argument = builder.emit_const_i64(42);
        builder.terminate(Terminator::Br { target: body, args: vec![argument] });
        builder.position_at_end(body);
        let param = builder.block_param(body, 0);
        builder.emit(
            Op::TryPushHandler, vec![], Some(Immediate::I64(handler.as_raw() as i64)),
            IrType::Void, PhpType::Void, Ownership::NonHeap,
        );
        builder.terminate(Terminator::Unreachable);
        builder.position_at_end(handler);
        builder.terminate(Terminator::Return { value: Some(param) });
        (argument, param)
    };
    crate::ir::validate_function(&function).expect("valid implicit exception edge fixture");

    for target in targets() {
        let allocation = allocate_registers(&function, target);
        for value in [argument, param] {
            assert_eq!(allocation.register_of(value), None, "{target:?}: exception live value needs a stack home");
        }
        assert!(allocation.used_callee_saved().is_empty(), "{target:?}: stack allocation needs no saved registers");
    }
}

/// A loop parameter used after its backedge value is defined must not share that
/// value's register: the later use still observes the old iteration.
#[test]
fn loop_parameter_used_after_backedge_value_is_not_coalesced() {
    for target in targets() {
        let mut function = Function::new("loop_use_after_def".into(), IrType::I64, PhpType::Int);
        let (param, updated);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            let header = builder.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
            let body = builder.create_named_block("body", vec![]);
            let latch = builder.create_named_block("latch", vec![]);
            let exit = builder.create_named_block("exit", vec![]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let one = builder.emit_const_i64(1);
            builder.terminate(Terminator::Br { target: header, args: vec![one] });
            builder.position_at_end(header);
            param = builder.block_param(header, 0);
            let condition = builder.emit_const_i64(1);
            builder.terminate(Terminator::CondBr {
                cond: condition,
                then_target: body,
                then_args: vec![],
                else_target: exit,
                else_args: vec![],
            });
            builder.position_at_end(body);
            let one = builder.emit_const_i64(1);
            updated = builder
                .emit(
                    Op::IAdd,
                    vec![param, one],
                    None,
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .expect("backedge value");
            let _later = builder
                .emit(
                    Op::IAdd,
                    vec![param, updated],
                    None,
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .expect("later use of the old parameter");
            builder.terminate(Terminator::Br { target: latch, args: vec![] });
            builder.position_at_end(latch);
            builder.terminate(Terminator::Br { target: header, args: vec![updated] });
            builder.position_at_end(exit);
            builder.terminate(Terminator::Return { value: Some(param) });
        }
        crate::ir::validate_function(&function).expect("valid overlapping loop fixture");
        let allocation = allocate_registers(&function, target);
        let param_home = allocation.register_of(param).expect("loop parameter stays register-resident");
        let updated_home = allocation.register_of(updated).expect("backedge value stays register-resident");
        assert_ne!(
            param_home, updated_home,
            "{target:?}: the old parameter is still live after the backedge value is defined"
        );
    }
}
