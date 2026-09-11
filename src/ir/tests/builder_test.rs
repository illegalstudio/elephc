//! Purpose:
//! Verifies the checked construction API for blocks, values, instructions, and terminators.
//!
//! Called from:
//! - `crate::ir::tests`.
//!
//! Key details:
//! - The builder must preserve table ID relationships that the validator later checks.

use crate::ir::{
    Builder, Function, Immediate, IrHeapKind, IrType, LocalKind, Op, Ownership,
    RuntimeCallTarget, Terminator, ValueDef,
};
use crate::types::PhpType;

/// Builds a minimal function that returns a constant.
#[test]
fn build_function_with_return() {
    let mut function = Function::new("ret_42".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(42);
        builder.terminate(Terminator::Return { value: Some(value) });
    }
    assert_eq!(function.blocks.len(), 1);
    assert_eq!(function.values.len(), 1);
    assert_eq!(function.instructions.len(), 1);
}

/// Builds a branch into a block with a block parameter.
#[test]
fn build_function_with_block_param_and_iadd() {
    let mut function = Function::new("add_one".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let body = builder.create_named_block("body", vec![(IrType::I64, PhpType::Int)]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let one = builder.emit_const_i64(1);
        builder.terminate(Terminator::Br {
            target: body,
            args: vec![one],
        });

        let arg = builder.block_param(body, 0);
        builder.position_at_end(body);
        let one_again = builder.emit_const_i64(1);
        let sum = builder.emit_iadd(arg, one_again);
        builder.terminate(Terminator::Return { value: Some(sum) });
    }
    assert_eq!(function.blocks.len(), 2);
    assert_eq!(function.blocks[1].params.len(), 1);
    assert_eq!(function.blocks[1].instructions.len(), 2);
}

/// Keeps a deferred local release after the slot widens from scalar to Mixed storage.
#[test]
fn deferred_local_release_survives_refcounted_widening() {
    let mut function = Function::new("widened_release".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("value".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        builder.emit(
            Op::ReleaseLocalSlot,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            IrType::Void,
            PhpType::Int,
            Ownership::NonHeap,
        );
        builder.widen_local_storage_type(slot, PhpType::Mixed);
        builder.prune_untracked_release_local_slot_ops();
        builder.terminate(Terminator::Return { value: None });
    }

    assert_eq!(function.locals[0].ir_type, IrType::Heap(IrHeapKind::Mixed));
    assert_eq!(function.instructions[0].op, Op::ReleaseLocalSlot);
    assert_eq!(
        function.instructions[0].immediate,
        Some(Immediate::LocalSlot(function.locals[0].id))
    );
}

/// Rewrites a deferred local release to `Nop` when the slot remains scalar.
#[test]
fn deferred_local_release_is_pruned_for_scalar_storage() {
    let mut function = Function::new("scalar_release".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("value".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        builder.emit(
            Op::ReleaseLocalSlot,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            IrType::Void,
            PhpType::Int,
            Ownership::NonHeap,
        );
        builder.prune_untracked_release_local_slot_ops();
        builder.terminate(Terminator::Return { value: None });
    }

    assert_eq!(function.instructions[0].op, Op::Nop);
    assert_eq!(function.instructions[0].immediate, None);
    assert_eq!(function.instructions[0].effects, Op::Nop.default_effects());
}

/// Keeps a provisional concrete-load release when the source slot widens to Mixed.
#[test]
fn deferred_local_load_release_survives_mixed_storage_widening() {
    let mut function = Function::new("mixed_load_release".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let array_ty = PhpType::Array(Box::new(PhpType::Int));
        let slot = builder.add_local(
            Some("value".to_string()),
            IrType::Heap(IrHeapKind::Array),
            array_ty.clone(),
            LocalKind::PhpLocal,
        );
        let load = builder
            .emit(
                Op::LoadLocal,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                IrType::Heap(IrHeapKind::Array),
                array_ty.clone(),
                Ownership::MaybeOwned,
            )
            .expect("load_local should produce a value");
        builder.emit(
            Op::Release,
            vec![load],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.widen_local_storage_type(
            slot,
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Int),
            },
        );
        builder.prune_borrowed_local_load_release_ops();
        builder.terminate(Terminator::Return { value: None });
    }

    assert_eq!(function.locals[0].php_type, PhpType::Mixed);
    assert_eq!(function.instructions[1].op, Op::Release);
}

/// Prunes a provisional concrete-load release when the source slot stays concrete.
#[test]
fn deferred_local_load_release_is_pruned_for_concrete_storage() {
    let mut function = Function::new("concrete_load_release".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let array_ty = PhpType::Array(Box::new(PhpType::Int));
        let slot = builder.add_local(
            Some("value".to_string()),
            IrType::Heap(IrHeapKind::Array),
            array_ty.clone(),
            LocalKind::PhpLocal,
        );
        let load = builder
            .emit(
                Op::LoadLocal,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                IrType::Heap(IrHeapKind::Array),
                array_ty,
                Ownership::MaybeOwned,
            )
            .expect("load_local should produce a value");
        builder.emit(
            Op::Release,
            vec![load],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.prune_borrowed_local_load_release_ops();
        builder.terminate(Terminator::Return { value: None });
    }

    assert_eq!(function.instructions[1].op, Op::Nop);
    assert!(function.instructions[1].operands.is_empty());
}

/// Preserves an explicit owned-slot cleanup even when its storage remains concrete.
#[test]
fn owned_local_load_release_is_not_pruned_for_concrete_storage() {
    let mut function = Function::new("owned_load_release".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let array_ty = PhpType::Array(Box::new(PhpType::Int));
        let slot = builder.add_local(
            Some("value".to_string()),
            IrType::Heap(IrHeapKind::Array),
            array_ty.clone(),
            LocalKind::PhpLocal,
        );
        let load = builder
            .emit(
                Op::LoadLocal,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                IrType::Heap(IrHeapKind::Array),
                array_ty,
                Ownership::Owned,
            )
            .expect("load_local should produce a value");
        builder.emit(
            Op::Release,
            vec![load],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.prune_borrowed_local_load_release_ops();
        builder.terminate(Terminator::Return { value: None });
    }

    assert_eq!(function.instructions[1].op, Op::Release);
}

/// Prunes borrowed local guards transitively and retains a true temporary guard.
#[test]
fn deferred_local_guard_pruning_preserves_true_temporary_ownership() {
    let mut function = Function::new("guard_pruning".to_string(), IrType::Void, PhpType::Void);
    let anchor;
    let borrowed_guard;
    let second_borrowed_guard;
    let owned_guard;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let array_ty = PhpType::Array(Box::new(PhpType::Int));
        let slot = builder.add_local(
            Some("arguments".to_string()),
            IrType::Heap(IrHeapKind::Array),
            array_ty.clone(),
            LocalKind::PhpLocal,
        );
        let borrowed = builder
            .emit(
                Op::LoadLocal,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                IrType::Heap(IrHeapKind::Array),
                array_ty.clone(),
                Ownership::MaybeOwned,
            )
            .expect("load_local produces a value");
        anchor = builder.emit_const_i64(0);
        borrowed_guard = builder
            .emit(
                Op::RuntimeCall,
                vec![borrowed, anchor],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("guard produces a token");
        let second_slot = builder.add_local(
            Some("more_arguments".to_string()),
            IrType::Heap(IrHeapKind::Array),
            array_ty.clone(),
            LocalKind::PhpLocal,
        );
        let second_borrowed = builder
            .emit(
                Op::LoadLocal,
                Vec::new(),
                Some(Immediate::LocalSlot(second_slot)),
                IrType::Heap(IrHeapKind::Array),
                array_ty.clone(),
                Ownership::MaybeOwned,
            )
            .expect("load_local produces a value");
        second_borrowed_guard = builder
            .emit(
                Op::RuntimeCall,
                vec![second_borrowed, borrowed_guard],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("guard produces a token");
        let owned = builder
            .emit(
                Op::ArrayNew,
                Vec::new(),
                Some(Immediate::Capacity(0)),
                IrType::Heap(IrHeapKind::Array),
                array_ty,
                Ownership::Owned,
            )
            .expect("array_new produces a value");
        owned_guard = builder
            .emit(
                Op::RuntimeCall,
                vec![owned, second_borrowed_guard],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("guard produces a token");
        for token in [owned_guard, second_borrowed_guard, borrowed_guard] {
            builder.emit(
                Op::RuntimeCall,
                vec![token],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionUnguardOwned)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
        }
        builder.prune_borrowed_local_load_guard_ops();
        builder.terminate(Terminator::Return { value: None });
    }

    let ValueDef::Instruction { inst: borrowed_guard_inst, .. } =
        function.value(borrowed_guard).unwrap().def
    else {
        panic!("guard token must be instruction-defined");
    };
    let ValueDef::Instruction { inst: owned_guard_inst, .. } =
        function.value(owned_guard).unwrap().def
    else {
        panic!("guard token must be instruction-defined");
    };
    let ValueDef::Instruction { inst: second_borrowed_guard_inst, .. } =
        function.value(second_borrowed_guard).unwrap().def
    else {
        panic!("guard token must be instruction-defined");
    };
    assert_eq!(function.instruction(borrowed_guard_inst).unwrap().op, Op::Nop);
    assert_eq!(
        function.instruction(second_borrowed_guard_inst).unwrap().op,
        Op::Nop
    );
    let owned_guard = function.instruction(owned_guard_inst).unwrap();
    assert_eq!(
        owned_guard.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned))
    );
    assert_eq!(owned_guard.operands.get(1), Some(&anchor));
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|inst| {
                inst.immediate
                    == Some(Immediate::RuntimeCall(
                        RuntimeCallTarget::ExceptionUnguardOwned,
                    ))
            })
            .count(),
        1
    );
}
