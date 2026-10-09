//! Purpose:
//! Structural regression tests for canonical induction and hot-loop CFG simplification.
//!
//! Called from:
//! - The library test harness through `ir_passes::tests`.
//!
//! Key details:
//! - Fixtures validate SSA after transformations and check fixed-point idempotence.

use crate::ir::{
    validate_function, BlockId, Builder, CmpPredicate, DataPool, Function, Immediate,
    IrType, Op, Ownership, Terminator, ValueDef, ValueId,
};
use crate::ir_passes::branch_simplify::BranchSimplify;
use crate::ir_passes::driver::IrPass;
use crate::ir_passes::licm::Licm;
use crate::ir_passes::loop_optimize::LoopOptimize;
use crate::ir_passes::rewrite::defining_instruction;
use crate::types::PhpType;
use super::integer_range_test::{emit_icmp, emit_scalar_binop, emit_unknown_int};

/// Builds two loop counters with distinct literal materializations and a separate update block.
fn counters(second_initial: i64, second_step: i64, update_op: Op) -> Function {
    let mut function = Function::new("counters".into(), IrType::I64, PhpType::Int);
    let mut b = Builder::new(&mut function);
    let entry = b.create_named_block("entry", vec![]);
    let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int); 2]);
    let body = b.create_named_block("body", vec![]);
    let update = b.create_named_block("update", vec![]);
    let exit = b.create_named_block("exit", vec![]);
    b.set_entry(entry);
    b.position_at_end(entry);
    let zero = b.emit_const_i64(0);
    let other = b.emit_const_i64(second_initial);
    b.terminate(Terminator::Br { target: header, args: vec![zero, other] });
    b.position_at_end(header);
    let first = b.block_param(header, 0);
    let second = b.block_param(header, 1);
    let bound = b.emit_const_i64(10);
    let condition = emit_icmp(&mut b, bound, first, CmpPredicate::Sgt);
    b.terminate(Terminator::CondBr { cond: condition, then_target: body, then_args: vec![], else_target: exit, else_args: vec![] });
    b.position_at_end(body);
    emit_scalar_binop(&mut b, Op::IBitXor, first, second);
    b.terminate(Terminator::Br { target: update, args: vec![] });
    b.position_at_end(update);
    let one = b.emit_const_i64(1);
    let step = b.emit_const_i64(second_step);
    let next = emit_scalar_binop(&mut b, update_op, one, first);
    let other_next = emit_scalar_binop(&mut b, update_op, second, step);
    b.terminate(Terminator::Br { target: header, args: vec![next, other_next] });
    b.position_at_end(exit);
    let sum = b.emit_iadd(first, second);
    b.terminate(Terminator::Return { value: Some(sum) });
    function
}

/// Runs one pass and verifies the complete SSA graph before and after it.
fn run(pass: &dyn IrPass, function: &mut Function) -> bool {
    validate_function(function).unwrap();
    let changed = pass.run(function, &mut DataPool::default());
    validate_function(function).unwrap();
    changed
}

/// Equal initial values and unchecked steps need only one header parameter and update.
#[test]
fn equivalent_inductions_share_one_counter() {
    let mut f = counters(0, 1, Op::IAdd);
    assert!(run(&LoopOptimize, &mut f));
    assert_eq!(f.blocks[1].params.len(), 1);
    assert!(!run(&LoopOptimize, &mut f));
    let sum = f.instructions.iter().find(|inst| inst.op == Op::IAdd && inst.operands[0] == inst.operands[1]).unwrap();
    assert_eq!(sum.operands[0], f.blocks[1].params[0]);
    let mut module = crate::ir::Module::new(crate::codegen::platform::Target::detect_host());
    module.functions.push(f);
    super::super::driver::optimize_module(&mut module);
    let optimized = &module.functions[0];
    let header = optimized.blocks[1].id;
    assert_eq!(optimized.blocks[1].params.len(), 1);
    assert_eq!(optimized.instructions.iter().filter(|inst| inst.op == Op::IAdd).count(), 2);
    let loops = crate::ir_passes::compute_loops(optimized, &crate::ir_passes::compute_dominance(optimized));
    assert_eq!(loops.header_loop(header).unwrap().blocks.len(), 2);

}

/// Different initial values and strides remain distinct.
#[test]
fn unequal_inductions_are_not_coalesced() {
    for (initial, step, op) in [(1, 1, Op::IAdd), (0, 2, Op::IAdd)] {
        let mut f = counters(initial, step, op);
        run(&LoopOptimize, &mut f);
        assert_eq!(f.blocks[1].params.len(), 2);
    }
}

/// Pure bounds and steps leave the loop and reversed comparisons become canonical.
#[test]
fn materializations_and_loop_test_are_canonical() {
    let mut f = counters(1, 2, Op::IAdd);
    assert!(run(&Licm, &mut f));
    assert!(run(&LoopOptimize, &mut f));
    let cmp = f.instructions.iter().find(|inst| inst.op == Op::ICmp).unwrap();
    assert_eq!(cmp.operands[0], f.blocks[1].params[0]);
    assert_eq!(cmp.immediate, Some(Immediate::CmpPredicate(CmpPredicate::Slt)));
    for block in &f.blocks[1..] {
        assert!(block.instructions.iter().all(|id| f.instruction(*id).unwrap().op != Op::ConstI64));
    }
    assert!(!run(&Licm, &mut f));
    assert!(!run(&LoopOptimize, &mut f));
}

/// The body jumps directly to its header after the sole update block is merged.
#[test]
fn merges_single_predecessor_update_into_body() {
    let mut f = counters(0, 1, Op::IAdd);
    assert!(run(&BranchSimplify, &mut f));
    assert!(matches!(f.blocks[2].terminator, Some(Terminator::Br { target, .. }) if target == BlockId::from_raw(1)));
    assert_eq!(f.blocks[3].terminator, Some(Terminator::Unreachable));
    assert!(f.blocks[3].instructions.is_empty());
    assert!(!run(&BranchSimplify, &mut f));
}

/// A loop update reached by two continue paths must not be duplicated into one path.
#[test]
fn shared_update_block_is_preserved() {
    let mut f = counters(0, 1, Op::IAdd);
    let condition = f.instructions.iter().find(|inst| inst.op == Op::ICmp).unwrap().result.unwrap();
    f.blocks[2].terminator = Some(Terminator::CondBr {
        cond: condition, then_target: BlockId::from_raw(3), then_args: vec![],
        else_target: BlockId::from_raw(3), else_args: vec![],
    });
    run(&BranchSimplify, &mut f);
    assert!(!matches!(f.blocks[3].terminator, Some(Terminator::Unreachable)));
}

/// Exception-handler functions keep both induction state and loop blocks unchanged.
#[test]
fn exception_handlers_disable_loop_rewrites() {
    let mut f = counters(0, 1, Op::IAdd);
    let mut b = Builder::new(&mut f);
    let handler = b.create_named_block("handler", vec![]);
    b.position_at_end(handler);
    b.emit(Op::TryPopHandler, vec![], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
    b.terminate(Terminator::Unreachable);
    assert!(!run(&LoopOptimize, &mut f));
    assert!(!run(&BranchSimplify, &mut f));
    assert!(!run(&Licm, &mut f));
}

/// Builds a two-argument forwarding block whose outgoing edge swaps its parameters.
fn forwarding(escape: bool) -> (Function, ValueId, ValueId) {
    let mut f = Function::new("forwarding".into(), IrType::I64, PhpType::Int);
    let mut b = Builder::new(&mut f);
    let entry = b.create_named_block("entry", vec![]);
    let forward = b.create_named_block("forward", vec![(IrType::I64, PhpType::Int); 2]);
    let exit = b.create_named_block("exit", vec![(IrType::I64, PhpType::Int); 2]);
    b.set_entry(entry);
    b.position_at_end(entry);
    let condition = emit_unknown_int(&mut b, "condition");
    let a = b.emit_const_i64(2);
    let z = b.emit_const_i64(7);
    b.terminate(Terminator::CondBr { cond: condition, then_target: forward, then_args: vec![a, z], else_target: forward, else_args: vec![z, a] });
    b.position_at_end(forward);
    let x = b.block_param(forward, 0);
    let y = b.block_param(forward, 1);
    b.terminate(Terminator::Br { target: exit, args: vec![y, x] });
    b.position_at_end(exit);
    let returned = if escape { x } else { b.block_param(exit, 0) };
    b.terminate(Terminator::Return { value: Some(returned) });
    (f, a, z)
}

/// Parallel conditional edges compose argument permutations independently.
#[test]
fn threads_parallel_ssa_edges_with_permuted_arguments() {
    let (mut f, a, z) = forwarding(false);
    assert!(run(&BranchSimplify, &mut f));
    let Some(Terminator::CondBr { then_target, then_args, else_target, else_args, .. }) = &f.blocks[0].terminator else { panic!("conditional edge"); };
    assert_eq!((*then_target, *else_target), (BlockId::from_raw(2), BlockId::from_raw(2)));
    assert_eq!(then_args, &[z, a]);
    assert_eq!(else_args, &[a, z]);
    assert!(!run(&BranchSimplify, &mut f));
}

/// A forwarding parameter read directly by a successor cannot lose its definition.
#[test]
fn preserves_escaping_forwarding_parameter() {
    let (mut f, _, _) = forwarding(true);
    assert!(!run(&BranchSimplify, &mut f));
}

/// LICM follows dependencies rather than the original instruction table order.
#[test]
fn licm_orders_relocated_dependencies_before_uses() {
    let mut f = counters(1, 2, Op::IAdd);
    let operand = f.blocks[0].instructions[0];
    let value = f.instruction(operand).unwrap().result.unwrap();
    let first = f.blocks[2].instructions[0];
    let later = f.blocks[3].instructions[0];
    let dependency = f.instruction(later).unwrap().result.unwrap();
    f.instruction_mut(first).unwrap().operands = vec![value, dependency];
    // Move the later-created definition into the dominating header.
    f.blocks[3].instructions.remove(0);
    f.blocks[1].instructions.insert(0, later);
    for block in &f.blocks {
        for (index, &inst) in block.instructions.iter().enumerate() {
            if let Some(result) = f.instructions[inst.as_raw() as usize].result {
                f.values[result.as_raw() as usize].def = ValueDef::Instruction { block: block.id, index: index as u32, inst };
            }
        }
    }
    assert!(run(&Licm, &mut f));
    let result = f.instruction(first).unwrap().result.unwrap();
    assert!(matches!(f.value(result).unwrap().def, ValueDef::Instruction { block, .. } if block == f.entry));
    assert_eq!(defining_instruction(&f, dependency).unwrap().op, Op::ConstI64);
}

/// Merging a parameterized update preserves downstream uses of its argument value.
#[test]
fn merges_parameterized_update_and_rewrites_escaped_uses() {
    let mut f = counters(1, 2, Op::IAdd);
    let parameter = f.blocks[1].params[0];
    let added = ValueId::from_raw(f.values.len() as u32);
    f.values.push(crate::ir::Value {
        ir_type: IrType::I64, php_type: PhpType::Int, ownership: Ownership::NonHeap,
        def: ValueDef::BlockParam { block: BlockId::from_raw(3), index: 0 },
    });
    f.blocks[3].params.push(added);
    f.blocks[2].terminator = Some(Terminator::Br { target: BlockId::from_raw(3), args: vec![parameter] });
    let update = f.blocks[3].instructions[2];
    f.instruction_mut(update).unwrap().operands[1] = added;
    assert!(run(&BranchSimplify, &mut f));
    assert!(f.blocks[3].params.is_empty());
    assert_eq!(f.instruction(update).unwrap().operands[1], parameter);
}

/// A continue edge carrying the old counter invalidates constant-step recognition.
#[test]
fn different_latch_updates_prevent_counter_coalescing() {
    let mut f = counters(0, 1, Op::IAdd);
    let params = f.blocks[1].params.clone();
    let condition = f.instructions.iter().find(|inst| inst.op == Op::ICmp).unwrap().result.unwrap();
    f.blocks[2].terminator = Some(Terminator::CondBr {
        cond: condition, then_target: BlockId::from_raw(3), then_args: vec![],
        else_target: BlockId::from_raw(1), else_args: vec![params[0], params[1]],
    });
    assert!(!run(&LoopOptimize, &mut f));
    assert_eq!(f.blocks[1].params.len(), 2);
}

/// Equal descending recurrences can share one parameter without negating integer endpoints.
#[test]
fn descending_counters_coalesce() {
    let mut f = counters(0, 1, Op::ISub);
    let update = f.blocks[3].instructions[2];
    f.instruction_mut(update).unwrap().operands.swap(0, 1);
    assert!(run(&LoopOptimize, &mut f));
    assert_eq!(f.blocks[1].params.len(), 1);
}

/// MIN subtraction cannot be represented as a signed constant step and stays conservative.
#[test]
fn minimum_subtraction_step_is_not_recognized() {
    let mut f = counters(0, i64::MIN, Op::ISub);
    let first = f.blocks[3].instructions[2];
    f.instruction_mut(first).unwrap().operands.swap(0, 1);
    run(&LoopOptimize, &mut f);
    assert_eq!(f.blocks[1].params.len(), 2);
}

/// Parallel switch cases carry their own substituted arguments through forwarding blocks.
#[test]
fn threads_switch_arguments_independently() {
    let (mut f, a, z) = forwarding(false);
    let condition = match f.blocks[0].terminator { Some(Terminator::CondBr { cond, .. }) => cond, _ => unreachable!() };
    f.blocks[0].terminator = Some(Terminator::Switch {
        scrutinee: condition,
        cases: vec![crate::ir::SwitchCase { value: 1, target: BlockId::from_raw(1), args: vec![a, z] }],
        default: BlockId::from_raw(1), default_args: vec![z, a],
    });
    assert!(run(&BranchSimplify, &mut f));
    let Some(Terminator::Switch { cases, default_args, .. }) = &f.blocks[0].terminator else { unreachable!() };
    assert_eq!(cases[0].args, vec![z, a]);
    assert_eq!(*default_args, vec![a, z]);
}

/// Argument permutations on a forwarding cycle cannot trigger endless driver rewrites.
#[test]
fn forwarding_cycles_are_stable() {
    let (mut f, _, _) = forwarding(false);
    f.blocks[2].terminator = Some(Terminator::Br {
        target: BlockId::from_raw(1), args: f.blocks[2].params.clone(),
    });
    // Straight-line loop merging may collapse the cycle once, but threading must converge.
    run(&BranchSimplify, &mut f);
    assert!(!run(&BranchSimplify, &mut f));
}

/// Self-comparisons exposed by equivalent counters become constants for branch cleanup.
#[test]
fn equivalent_counter_tests_fold() {
    let mut f = counters(0, 1, Op::IAdd);
    let compare = f.blocks[2].instructions[0];
    let inst = f.instruction_mut(compare).unwrap();
    inst.op = Op::ICmp;
    inst.immediate = Some(Immediate::CmpPredicate(CmpPredicate::Ne));
    inst.result_php_type = PhpType::Bool;
    let result = inst.result.unwrap();
    f.values[result.as_raw() as usize].php_type = PhpType::Bool;
    assert!(run(&LoopOptimize, &mut f));
    assert_eq!(f.instruction(compare).unwrap().op, Op::ConstBool);
    assert_eq!(f.instruction(compare).unwrap().immediate, Some(Immediate::Bool(false)));
}

/// Equal checked recurrences share state while retaining the first overflow check.
#[test]
fn checked_inductions_keep_their_overflow_semantics() {
    let mut f = counters(0, 1, Op::ICheckedAddToInt);
    assert!(run(&LoopOptimize, &mut f));
    assert_eq!(f.blocks[1].params.len(), 1);
    assert!(f.instructions.iter().any(|inst| inst.op == Op::ICheckedAddToInt));
}

/// Distinct overflow modes and an earlier duplicate check cannot be coalesced.
#[test]
fn checked_inductions_require_equal_modes_and_check_order() {
    for different_mode in [true, false] {
        let mut f = counters(0, 1, Op::ICheckedAddToInt);
        let update = f.blocks[3].instructions[3];
        if different_mode {
            f.instruction_mut(update).unwrap().immediate = Some(Immediate::Bool(true));
        } else {
            f.blocks[3].instructions.swap(2, 3);
            for (index, &inst) in f.blocks[3].instructions.iter().enumerate() {
                let result = f.instructions[inst.as_raw() as usize].result.unwrap();
                f.values[result.as_raw() as usize].def = ValueDef::Instruction {
                    block: BlockId::from_raw(3), index: index as u32, inst,
                };
            }
        }
        run(&LoopOptimize, &mut f);
        assert_eq!(f.blocks[1].params.len(), 2);
    }
}

/// A self-carried scalar bound is replaced by its dominating entry value.
#[test]
fn invariant_header_arguments_leave_the_loop() {
    let mut f = counters(1, 2, Op::IAdd);
    let initial = f.instruction(f.blocks[0].instructions[1]).unwrap().result.unwrap();
    let parameter = f.blocks[1].params[1];
    if let Some(Terminator::Br { args, .. }) = &mut f.blocks[3].terminator { args[1] = parameter; }
    assert!(run(&LoopOptimize, &mut f));
    assert_eq!(f.blocks[1].params.len(), 1);
    let sum = f.instruction(f.blocks[4].instructions[0]).unwrap();
    assert_eq!(sum.operands[1], initial);
    assert!(!run(&LoopOptimize, &mut f));
}

/// Exposing an invariant header value must not speculate a raw-slot overflow fatal.
#[test]
fn invariant_parameter_folding_preserves_checked_fatal_execution() {
    for mode in [None, Some(Immediate::Bool(false)), Some(Immediate::Bool(true))] {
        let mut f = counters(1, 2, Op::IAdd);
        let parameter = f.blocks[1].params[1];
        if let Some(Terminator::Br { args, .. }) = &mut f.blocks[3].terminator { args[1] = parameter; }
        let term = f.blocks[2].terminator.take().unwrap();
        let result;
        {
            let mut b = Builder::new(&mut f);
            b.position_at_end(BlockId::from_raw(2));
            let max = b.emit_const_i64(i64::MAX);
            result = b.emit(Op::ICheckedAddToInt, vec![parameter, max], mode.clone(),
                IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            b.emit(Op::EchoValue, vec![result], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            b.terminate(term);
        }
        assert!(run(&LoopOptimize, &mut f));
        run(&Licm, &mut f);
        let ValueDef::Instruction { block, .. } = f.value(result).unwrap().def else { unreachable!() };
        let expected = if mode == Some(Immediate::Bool(true)) { f.entry } else { BlockId::from_raw(2) };
        assert_eq!(block, expected);
    }
}
