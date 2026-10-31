//! Purpose:
//! Preserves concat reset elimination across proven typed element stores.
//!
//! Called from:
//! - EIR optimizer regression CI.
//!
//! Key details:
//! - Hand-built stores retain conservative defaults, so the operand proof itself is exercised.

use crate::ir::{Builder, CmpPredicate, DataPool, Function, Immediate, IrHeapKind, IrType,
    LocalKind, Op, Ownership, Terminator, ValueId};
use crate::ir_passes::{concat_reset::ConcatResetElim, driver::IrPass};
use crate::types::PhpType;

/// Allocates a small typed container whose elements cannot own PHP destructors.
fn array(builder: &mut Builder<'_>) -> ValueId {
    builder.emit(Op::ArrayNew, vec![], Some(Immediate::Capacity(4)),
        IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int)), Ownership::Owned).unwrap()
}

/// Emits an element write with the full conservative opcode effect mask.
fn store(builder: &mut Builder<'_>, array: ValueId, key: ValueId) {
    builder.emit(Op::ArraySet, vec![array, key, key], None,
        IrType::Void, PhpType::Void, Ownership::NonHeap);
}

/// Adds a statement scratch reset whose elimination must use the container proof.
fn reset(builder: &mut Builder<'_>) {
    builder.emit(Op::ConcatReset, vec![], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
}

/// Closes the allocation owner after the test's last store.
fn release(builder: &mut Builder<'_>, array: ValueId) {
    builder.emit(Op::Release, vec![array], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
}

/// A typed indexed store does not dirty concat scratch on a loop back edge.
#[test]
fn typed_indexed_write_loop_eliminates_statement_concat_resets() {
    let mut function = Function::new("typed_write_loop".into(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let index = builder.add_local(Some("i".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
        let entry = builder.create_named_block("entry", vec![]);
        let header = builder.create_named_block("loop.header", vec![]);
        let body = builder.create_named_block("loop.body", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry); builder.position_at_end(entry);
        let array = array(&mut builder);
        let zero = builder.emit_const_i64(0);
        builder.emit_store_local(index, zero);
        builder.terminate(Terminator::Br { target: header, args: vec![] });
        builder.position_at_end(header);
        let current = builder.emit_load_local(index, IrType::I64, PhpType::Int);
        let bound = builder.emit_const_i64(2);
        let condition = builder.emit(Op::ICmp, vec![current, bound],
            Some(Immediate::CmpPredicate(CmpPredicate::Slt)), IrType::I64, PhpType::Bool, Ownership::NonHeap).unwrap();
        builder.terminate(Terminator::CondBr { cond: condition, then_target: body, then_args: vec![],
            else_target: exit, else_args: vec![] });
        builder.position_at_end(body);
        reset(&mut builder); store(&mut builder, array, current); reset(&mut builder);
        let one = builder.emit_const_i64(1);
        let next = builder.emit_iadd(current, one);
        builder.emit_store_local(index, next);
        builder.terminate(Terminator::Br { target: header, args: vec![] });
        builder.position_at_end(exit);
        release(&mut builder, array);
        builder.terminate(Terminator::Return { value: Some(zero) });
    }
    assert!(ConcatResetElim.run(&mut function, &mut DataPool::default()));
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::ConcatReset));
    crate::ir::validate_function(&function).unwrap();
}

/// The single-block applicability shortcut cannot discard the precise typed-store proof.
#[test]
fn typed_indexed_write_reaches_concat_analysis_after_a_dirty_prefix() {
    let mut function = Function::new("typed_write_straight".into(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry); builder.position_at_end(entry);
        let array = array(&mut builder);
        let zero = builder.emit_const_i64(0);
        builder.emit(Op::IToStr, vec![zero], None, IrType::Str, PhpType::Str, Ownership::Borrowed);
        reset(&mut builder); store(&mut builder, array, zero); reset(&mut builder);
        release(&mut builder, array);
        builder.terminate(Terminator::Return { value: Some(zero) });
    }
    assert!(ConcatResetElim.is_applicable(&function));
    assert!(ConcatResetElim.run(&mut function, &mut DataPool::default()));
    assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::ConcatReset).count(), 1);
    crate::ir::validate_function(&function).unwrap();
}
