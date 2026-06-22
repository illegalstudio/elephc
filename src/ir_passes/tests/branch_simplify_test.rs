//! Purpose:
//! Tests for EIR branch simplification: constant-condition folding, empty-block
//! jump threading, and unreachable-block neutralization.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are hand-built with `crate::ir::Builder`. Tests assert both the
//!   rewritten terminators/blocks and that the function still validates, since
//!   the pass deliberately leaves unreachable blocks in place in neutral form.

use crate::ir::{
    validate_function, Builder, DataPool, Function, IrType, Op, SwitchCase, Terminator,
};
use crate::ir_passes::branch_simplify::BranchSimplify;
use crate::ir_passes::driver::{run_function_passes, IrPass};
use crate::types::PhpType;

/// Runs one branch-simplification pass over a function with a throwaway pool.
fn run_once(function: &mut Function) -> bool {
    BranchSimplify.run(function, &mut DataPool::default())
}

/// Runs the fixed-point driver with only branch simplification registered.
fn run_to_fixed_point(function: &mut Function) {
    let passes: Vec<Box<dyn IrPass>> = vec![Box::new(BranchSimplify)];
    run_function_passes(function, &passes, &mut DataPool::default());
}

/// Returns true when every instruction in the block at `index` is a `nop`.
fn all_nop(function: &Function, index: usize) -> bool {
    function.blocks[index]
        .instructions
        .iter()
        .all(|id| function.instruction(*id).unwrap().op == Op::Nop)
}

/// A `CondBr` on a constant-true condition folds to a `Br` to the then-target;
/// the else block becomes unreachable and is neutralized.
#[test]
fn constant_true_condbr_folds_to_then_branch() {
    let mut function = Function::new("cond_true".to_string(), IrType::I64, PhpType::Int);
    let (then_block, else_block);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        then_block = builder.create_named_block("then", vec![]);
        else_block = builder.create_named_block("else", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let cond = builder.emit_const_bool(true);
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: then_block,
            then_args: Vec::new(),
            else_target: else_block,
            else_args: Vec::new(),
        });

        builder.position_at_end(then_block);
        let taken = builder.emit_const_i64(1);
        builder.terminate(Terminator::Return { value: Some(taken) });

        builder.position_at_end(else_block);
        let untaken = builder.emit_const_i64(2);
        builder.terminate(Terminator::Return { value: Some(untaken) });
    }

    assert!(run_once(&mut function), "the constant branch should fold");
    assert_eq!(
        function.blocks[0].terminator,
        Some(Terminator::Br {
            target: then_block,
            args: Vec::new()
        }),
        "entry should branch unconditionally to the taken target"
    );
    assert_eq!(
        function.blocks[else_block.as_raw() as usize].terminator,
        Some(Terminator::Unreachable),
        "the untaken block becomes unreachable"
    );
    assert!(
        all_nop(&function, else_block.as_raw() as usize),
        "the unreachable block's instructions are neutralized"
    );
    assert!(validate_function(&function).is_ok(), "folded IR stays valid");
}

/// A `CondBr` on a constant-false condition folds to the else-target.
#[test]
fn constant_false_condbr_folds_to_else_branch() {
    let mut function = Function::new("cond_false".to_string(), IrType::I64, PhpType::Int);
    let (then_block, else_block);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        then_block = builder.create_named_block("then", vec![]);
        else_block = builder.create_named_block("else", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let cond = builder.emit_const_bool(false);
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: then_block,
            then_args: Vec::new(),
            else_target: else_block,
            else_args: Vec::new(),
        });

        builder.position_at_end(then_block);
        let taken = builder.emit_const_i64(1);
        builder.terminate(Terminator::Return { value: Some(taken) });

        builder.position_at_end(else_block);
        let untaken = builder.emit_const_i64(2);
        builder.terminate(Terminator::Return { value: Some(untaken) });
    }

    assert!(run_once(&mut function), "the constant branch should fold");
    assert_eq!(
        function.blocks[0].terminator,
        Some(Terminator::Br {
            target: else_block,
            args: Vec::new()
        }),
        "entry should branch to the else target"
    );
    assert_eq!(
        function.blocks[then_block.as_raw() as usize].terminator,
        Some(Terminator::Unreachable),
        "the untaken then block becomes unreachable"
    );
    assert!(validate_function(&function).is_ok());
}

/// A `CondBr` on a runtime value is left unchanged and both arms stay reachable.
#[test]
fn runtime_condbr_is_preserved() {
    let mut function = Function::new("cond_runtime".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let slot = builder.add_local(
            Some("c".to_string()),
            IrType::I64,
            PhpType::Bool,
            crate::ir::LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        let then_block = builder.create_named_block("then", vec![]);
        let else_block = builder.create_named_block("else", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let cond = builder.emit_load_local(slot, IrType::I64, PhpType::Bool);
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: then_block,
            then_args: Vec::new(),
            else_target: else_block,
            else_args: Vec::new(),
        });

        builder.position_at_end(then_block);
        let a = builder.emit_const_i64(1);
        builder.terminate(Terminator::Return { value: Some(a) });

        builder.position_at_end(else_block);
        let b = builder.emit_const_i64(2);
        builder.terminate(Terminator::Return { value: Some(b) });
    }

    assert!(
        !run_once(&mut function),
        "a runtime condition is not foldable"
    );
    assert!(
        matches!(function.blocks[0].terminator, Some(Terminator::CondBr { .. })),
        "the conditional branch stays"
    );
    assert!(validate_function(&function).is_ok());
}

/// A `Switch` on a constant scrutinee folds to the matching case target.
#[test]
fn constant_switch_folds_to_matching_case() {
    let mut function = Function::new("switch_const".to_string(), IrType::I64, PhpType::Int);
    let (case_block, default_block);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        case_block = builder.create_named_block("case7", vec![]);
        default_block = builder.create_named_block("default", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let scrutinee = builder.emit_const_i64(7);
        builder.terminate(Terminator::Switch {
            scrutinee,
            cases: vec![SwitchCase {
                value: 7,
                target: case_block,
                args: Vec::new(),
            }],
            default: default_block,
            default_args: Vec::new(),
        });

        builder.position_at_end(case_block);
        let a = builder.emit_const_i64(70);
        builder.terminate(Terminator::Return { value: Some(a) });

        builder.position_at_end(default_block);
        let b = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(b) });
    }

    assert!(run_once(&mut function), "the constant switch should fold");
    assert_eq!(
        function.blocks[0].terminator,
        Some(Terminator::Br {
            target: case_block,
            args: Vec::new()
        }),
        "entry should branch to the matching case"
    );
    assert_eq!(
        function.blocks[default_block.as_raw() as usize].terminator,
        Some(Terminator::Unreachable),
        "the unmatched default becomes unreachable"
    );
    assert!(validate_function(&function).is_ok());
}

/// An empty forwarding block is threaded out: predecessors jump straight to its
/// target and the empty block becomes unreachable.
#[test]
fn empty_forwarding_block_is_threaded() {
    let mut function = Function::new("thread".to_string(), IrType::I64, PhpType::Int);
    let (mid_block, exit_block);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        mid_block = builder.create_named_block("mid", vec![]);
        exit_block = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        builder.terminate(Terminator::Br {
            target: mid_block,
            args: Vec::new(),
        });

        builder.position_at_end(mid_block);
        builder.terminate(Terminator::Br {
            target: exit_block,
            args: Vec::new(),
        });

        builder.position_at_end(exit_block);
        let result = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(result) });
    }

    assert!(run_once(&mut function), "the empty block should be threaded");
    assert_eq!(
        function.blocks[0].terminator,
        Some(Terminator::Br {
            target: exit_block,
            args: Vec::new()
        }),
        "entry should jump straight to the exit block"
    );
    assert_eq!(
        function.blocks[mid_block.as_raw() as usize].terminator,
        Some(Terminator::Unreachable),
        "the threaded-through block becomes unreachable"
    );
    assert!(validate_function(&function).is_ok());
}

/// Functions that use exception-handling opcodes are skipped wholesale, since
/// their handler blocks are reachable through implicit edges this pass ignores.
#[test]
fn function_with_exception_handler_is_skipped() {
    let mut function = Function::new("with_finally".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let then_block = builder.create_named_block("then", vec![]);
        let else_block = builder.create_named_block("else", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        builder.emit(
            Op::FinallyEnter,
            Vec::new(),
            None,
            IrType::Void,
            PhpType::Void,
            crate::ir::Ownership::NonHeap,
        );
        let cond = builder.emit_const_bool(true);
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: then_block,
            then_args: Vec::new(),
            else_target: else_block,
            else_args: Vec::new(),
        });

        builder.position_at_end(then_block);
        let a = builder.emit_const_i64(1);
        builder.terminate(Terminator::Return { value: Some(a) });

        builder.position_at_end(else_block);
        let b = builder.emit_const_i64(2);
        builder.terminate(Terminator::Return { value: Some(b) });
    }

    assert!(
        !run_once(&mut function),
        "exception-handling functions are not simplified"
    );
    assert!(
        matches!(function.blocks[0].terminator, Some(Terminator::CondBr { .. })),
        "the constant branch is left intact when handlers are present"
    );
    assert!(validate_function(&function).is_ok());
}

/// The fixed-point driver converges (no repeated change reports) on a function
/// already simplified, and keeps the result valid.
#[test]
fn fixed_point_converges_and_stays_valid() {
    let mut function = Function::new("converge".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let then_block = builder.create_named_block("then", vec![]);
        let else_block = builder.create_named_block("else", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let cond = builder.emit_const_bool(true);
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: then_block,
            then_args: Vec::new(),
            else_target: else_block,
            else_args: Vec::new(),
        });

        builder.position_at_end(then_block);
        let a = builder.emit_const_i64(1);
        builder.terminate(Terminator::Return { value: Some(a) });

        builder.position_at_end(else_block);
        let b = builder.emit_const_i64(2);
        builder.terminate(Terminator::Return { value: Some(b) });
    }

    run_to_fixed_point(&mut function);
    assert!(
        !run_once(&mut function),
        "a second run reports no change once simplified"
    );
    assert!(validate_function(&function).is_ok());
}
