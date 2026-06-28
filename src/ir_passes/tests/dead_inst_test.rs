//! Purpose:
//! Tests for CFG-aware dead instruction elimination over EIR functions.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are hand-built with `crate::ir::Builder` so tests can isolate
//!   liveness, effect preservation, and fixed-point cleanup across blocks.

use crate::ir::{
    validate_function, Builder, DataPool, Function, IrType, LocalKind, Op, Ownership, Terminator,
    ValueDef,
};
use crate::ir_passes::dead_inst::DeadInst;
use crate::ir_passes::driver::{run_function_passes, IrPass};
use crate::types::PhpType;

/// Runs one dead-instruction pass over a function with a throwaway literal pool.
fn run_once(function: &mut Function) -> bool {
    DeadInst.run(function, &mut DataPool::default())
}

/// Runs the fixed-point driver with only the dead-instruction pass registered.
fn run_to_fixed_point(function: &mut Function) {
    let passes: Vec<Box<dyn IrPass>> = vec![Box::new(DeadInst)];
    run_function_passes(function, &passes, &mut DataPool::default());
}

/// Unused pure instructions in one block collapse in a single backward walk.
#[test]
fn dead_pure_chain_in_one_block_becomes_nops() {
    let mut function = Function::new("dead_chain".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let lhs = builder.emit_const_i64(1);
        let rhs = builder.emit_const_i64(2);
        let _unused = builder.emit_iadd(lhs, rhs);
        let live = builder.emit_const_i64(9);
        builder.terminate(Terminator::Return { value: Some(live) });
    }

    assert!(
        run_once(&mut function),
        "the dead add and operands should be removed"
    );
    assert_eq!(
        function.instructions[0].op,
        Op::Nop,
        "dead lhs constant removed"
    );
    assert_eq!(
        function.instructions[1].op,
        Op::Nop,
        "dead rhs constant removed"
    );
    assert_eq!(function.instructions[2].op, Op::Nop, "dead add removed");
    assert_eq!(
        function.instructions[3].op,
        Op::ConstI64,
        "returned value stays live"
    );
    assert!(
        validate_function(&function).is_ok(),
        "DIE keeps the function valid"
    );
}

/// A value used only by a successor block remains live through CFG live-out.
#[test]
fn successor_block_use_keeps_predecessor_definition_live() {
    let mut function = Function::new("successor_use".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let live_across_edge = builder.emit_const_i64(4);
        let _dead_in_entry = builder.emit_const_i64(0);
        builder.terminate(Terminator::Br {
            target: exit,
            args: Vec::new(),
        });

        builder.position_at_end(exit);
        builder.terminate(Terminator::Return {
            value: Some(live_across_edge),
        });
    }

    assert!(
        run_once(&mut function),
        "only the unused entry constant should die"
    );
    assert_eq!(
        function.instructions[0].op,
        Op::ConstI64,
        "successor use keeps value live"
    );
    assert_eq!(
        function.instructions[1].op,
        Op::Nop,
        "unused predecessor value dies"
    );
    assert!(
        validate_function(&function).is_ok(),
        "DIE keeps cross-block IR valid"
    );
}

/// Cross-block deadness converges through the fixed-point driver after a
/// successor's dead use is removed.
#[test]
fn fixed_point_driver_removes_cross_block_dead_chain() {
    let mut function = Function::new("cross_block_chain".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let seed = builder.emit_const_i64(5);
        builder.terminate(Terminator::Br {
            target: exit,
            args: Vec::new(),
        });

        builder.position_at_end(exit);
        let one = builder.emit_const_i64(1);
        let _unused = builder.emit_iadd(seed, one);
        let live = builder.emit_const_i64(7);
        builder.terminate(Terminator::Return { value: Some(live) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instructions[0].op,
        Op::Nop,
        "predecessor seed dies after recompute"
    );
    assert_eq!(
        function.instructions[1].op,
        Op::Nop,
        "successor operand dies"
    );
    assert_eq!(function.instructions[2].op, Op::Nop, "successor add dies");
    assert_eq!(
        function.instructions[3].op,
        Op::ConstI64,
        "returned value stays live"
    );
    assert!(
        validate_function(&function).is_ok(),
        "fixed-point DIE stays valid"
    );
}

/// Constants used only by an already-neutralized instruction become dead too.
#[test]
fn dead_operand_of_existing_nop_is_removed() {
    let mut function = Function::new(
        "dead_operand_after_fold".to_string(),
        IrType::Void,
        PhpType::Void,
    );
    {
        let mut builder = Builder::new(&mut function);
        let slot = builder.add_local(
            Some("argc".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let live = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        let dead_zero = builder.emit_const_i64(0);
        let folded = builder.emit_iadd(live, dead_zero);
        let echo_result = builder.emit(
            Op::EchoValue,
            vec![live],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        assert!(echo_result.is_none(), "echo_value has no result");
        builder.terminate(Terminator::Return { value: None });

        let folded_inst = match function.value(folded).expect("folded value exists").def {
            ValueDef::Instruction { inst, .. } => inst,
            _ => panic!("folded value should be instruction-defined"),
        };
        let inst = function
            .instruction_mut(folded_inst)
            .expect("folded add still exists");
        inst.op = Op::Nop;
        inst.operands.clear();
        inst.immediate = None;
        inst.effects = Op::Nop.default_effects();
    }

    assert!(
        run_once(&mut function),
        "the zero operand should die after its only user was neutralized"
    );
    assert_eq!(
        function.instructions[0].op,
        Op::LoadLocal,
        "live echo operand remains"
    );
    assert_eq!(
        function.instructions[1].op,
        Op::Nop,
        "dead identity operand is removed"
    );
    assert_eq!(
        function.instructions[2].op,
        Op::Nop,
        "pre-neutralized instruction stays nop"
    );
    assert!(
        validate_function(&function).is_ok(),
        "DIE keeps folded IR valid"
    );
}

/// Output-producing instructions are retained even when their result is unused.
#[test]
fn observable_instruction_with_unused_result_is_preserved() {
    let mut function = Function::new(
        "observable_unused_result".to_string(),
        IrType::I64,
        PhpType::Int,
    );
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let printed = builder.emit_const_i64(3);
        let _print_result = builder
            .emit(
                Op::PrintValue,
                vec![printed],
                None,
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("print_value returns the PHP print result");
        let live = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(live) });
    }

    assert!(
        !run_once(&mut function),
        "observable print and its operand are live"
    );
    assert_eq!(
        function.instructions[0].op,
        Op::ConstI64,
        "print operand stays live"
    );
    assert_eq!(
        function.instructions[1].op,
        Op::PrintValue,
        "print stays observable"
    );
    assert_eq!(
        function.instructions[2].op,
        Op::ConstI64,
        "return value stays live"
    );
    assert!(
        validate_function(&function).is_ok(),
        "observable IR stays valid"
    );
}

/// Read-only instructions are preserved until the IR memory model can prove
/// that eliminating unused reads is safe across lowering and codegen metadata.
#[test]
fn read_only_instruction_with_unused_result_is_preserved() {
    let mut function = Function::new("read_unused_result".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let slot = builder.add_local(
            Some("n".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let _unused_read = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        let live = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(live) });
    }

    assert!(
        !run_once(&mut function),
        "unused read-only instructions are not dead-inst candidates"
    );
    assert_eq!(
        function.instructions[0].op,
        Op::LoadLocal,
        "local read stays in place"
    );
    assert_eq!(
        function.instructions[1].op,
        Op::ConstI64,
        "return value stays live"
    );
    assert!(
        validate_function(&function).is_ok(),
        "preserving read-only IR keeps the function valid"
    );
}
