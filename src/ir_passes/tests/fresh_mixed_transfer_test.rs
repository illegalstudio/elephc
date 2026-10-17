//! Purpose:
//! Tests ownership transfer of fresh checked-numeric assignment boxes.
//!
//! Called from:
//! - The library test harness through `ir_passes::tests`.
//!
//! Key details:
//! - Extra observers, uncertain ownership, and nonadjacent cleanup fail closed.
//! - Finalization must not feed checked-integer sink specialization again.

use crate::ir::{
    validate_function, Builder, DataPool, Function, FunctionParam, Immediate, IrHeapKind, IrType, LocalKind,
    Module, Op, Ownership, Terminator, ValueId,
};
use crate::ir_passes::driver::{optimize_module, IrPass};
use crate::ir_passes::fresh_mixed_transfer::FreshMixedTransfer;
use crate::codegen::platform::{Arch, Platform, Target};
use crate::types::PhpType;

/// Emits one boxed instruction with explicit ownership for negative cases.
fn boxed(builder: &mut Builder<'_>, op: Op, operands: Vec<ValueId>, owner: Ownership) -> ValueId {
    builder.emit(op, operands, None, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, owner)
        .expect("boxed result")
}

/// Emits the original owner's cleanup directly after its assignment clone.
fn release(builder: &mut Builder<'_>, value: ValueId) {
    builder.emit(Op::Release, vec![value], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
}

/// Builds a numeric copy with optional observers or an intervening instruction.
fn fixture(op: Op, ownership: Ownership, extra_use: bool, gap: bool) -> Function {
    let mut function = Function::new("transfer".to_string(), IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let lhs = builder.emit_const_i64(7);
    let rhs = builder.emit_const_i64(3);
    let source = boxed(&mut builder, op, vec![lhs, rhs], ownership);
    if extra_use {
        let borrowed = boxed(&mut builder, Op::Acquire, vec![source], Ownership::Owned);
        release(&mut builder, borrowed);
    }
    let copy = boxed(&mut builder, Op::MixedClone, vec![source], Ownership::Owned);
    if gap {
        builder.emit(Op::EchoValue, vec![lhs], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
    }
    release(&mut builder, source);
    builder.terminate(Terminator::Return { value: Some(copy) });
    function
}

/// Runs the finalizer alone so ordinary folding cannot erase the test shape.
fn transfer(function: &mut Function) -> bool {
    FreshMixedTransfer.run(function, &mut DataPool::default())
}

/// All checked arithmetic producers create independent numeric cells, including overflow.
#[test]
fn fresh_mixed_transfer_checked_numeric_results() {
    for op in [Op::ICheckedAdd, Op::ICheckedSub, Op::ICheckedMul] {
        let mut function = fixture(op, Ownership::Owned, false, false);
        assert!(validate_function(&function).is_ok());
        let source = function.instructions[2].result.unwrap();
        assert!(transfer(&mut function));
        assert_eq!(function.instructions[3].op, Op::Nop);
        assert_eq!(function.instructions[4].op, Op::Nop);
        assert!(matches!(function.blocks[0].terminator, Some(Terminator::Return { value: Some(v) }) if v == source));
        assert!(validate_function(&function).is_ok());
        assert!(!transfer(&mut function));
    }
}

/// A retained observer prevents transferring the original owner into its clone.
#[test]
fn fresh_mixed_transfer_rejects_shared_source() {
    let mut function = fixture(Op::ICheckedAdd, Ownership::Owned, true, false);
    assert!(validate_function(&function).is_ok());
    assert!(!transfer(&mut function));
}

/// Neither borrowed nor uncertain values carry an owner that this pass can move.
#[test]
fn fresh_mixed_transfer_requires_owned_source() {
    for ownership in [Ownership::Borrowed, Ownership::MaybeOwned] {
        let mut function = fixture(Op::ICheckedAdd, ownership, false, false);
        assert!(!transfer(&mut function));
    }
}

/// An intervening effect prevents changing the lifetime across that boundary.
#[test]
fn fresh_mixed_transfer_requires_adjacent_release() {
    let mut function = fixture(Op::ICheckedAdd, Ownership::Owned, false, true);
    assert!(validate_function(&function).is_ok());
    assert!(!transfer(&mut function));
}

/// Retaining an existing box never proves that its mutable cell is independent.
#[test]
fn fresh_mixed_transfer_rejects_acquired_source() {
    let mut function = Function::new("acquired".to_string(), IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let number = builder.emit_const_i64(7);
    let original = boxed(&mut builder, Op::MixedBox, vec![number], Ownership::Owned);
    let acquired = boxed(&mut builder, Op::Acquire, vec![original], Ownership::Owned);
    release(&mut builder, original);
    let copy = boxed(&mut builder, Op::MixedClone, vec![acquired], Ownership::Owned);
    release(&mut builder, acquired);
    builder.terminate(Terminator::Return { value: Some(copy) });
    assert!(validate_function(&function).is_ok());
    assert!(!transfer(&mut function));
}

/// Removing a clone must not make an implicit integer sink change overflow handling.
#[test]
fn fresh_mixed_transfer_runs_after_integer_sinks() {
    let mut function = Function::new("transfer".to_string(), IrType::I64, PhpType::Int);
    function.params.push(FunctionParam {
        name: "input".to_string(), ir_type: IrType::I64, php_type: PhpType::Int,
        by_ref: false, variadic: false,
    });
    let slot = function.add_local(Some("input".to_string()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let lhs = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
    let rhs = builder.emit_const_i64(1);
    let source = boxed(&mut builder, Op::ICheckedAdd, vec![lhs, rhs], Ownership::Owned);
    let copy = boxed(&mut builder, Op::MixedClone, vec![source], Ownership::Owned);
    release(&mut builder, source);
    let int = builder.emit(Op::Cast, vec![copy], Some(Immediate::CastTarget(IrType::I64)),
        IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
    release(&mut builder, copy);
    builder.terminate(Terminator::Return { value: Some(int) });
    function.return_type = IrType::I64;
    function.return_php_type = PhpType::Int;
    let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    module.functions.push(function);
    optimize_module(&mut module);
    let function = &module.functions[0];
    assert!(function.instructions.iter().any(|inst| inst.op == Op::ICheckedAdd));
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::MixedClone));
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::ICheckedAddToInt));
}

/// Cleanup in another block is outside the local transfer proof.
#[test]
fn fresh_mixed_transfer_rejects_cross_block_release() {
    let mut function = Function::new("cross_block".to_string(), IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    let exit = builder.create_named_block("exit", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let lhs = builder.emit_const_i64(7);
    let rhs = builder.emit_const_i64(3);
    let source = boxed(&mut builder, Op::ICheckedAdd, vec![lhs, rhs], Ownership::Owned);
    let copy = boxed(&mut builder, Op::MixedClone, vec![source], Ownership::Owned);
    builder.terminate(Terminator::Br { target: exit, args: vec![] });
    builder.position_at_end(exit);
    release(&mut builder, source);
    builder.terminate(Terminator::Return { value: Some(copy) });
    assert!(validate_function(&function).is_ok());
    assert!(!transfer(&mut function));
}
