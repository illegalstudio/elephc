//! Purpose:
//! Verifies structural, type, branch, and dominance checks in the EIR validator.
//!
//! Called from:
//! - `crate::ir::tests`.
//!
//! Key details:
//! - Negative cases prove validator failures are based on current function state,
//!   not on assumptions from the builder.

use crate::ir::{
    validate_function, Builder, Function, IrType, Terminator, ValidationError,
};
use crate::types::PhpType;

/// An empty function has no valid entry block and fails validation.
#[test]
fn empty_function_fails_validation() {
    let function = Function::new("empty".to_string(), IrType::Void, PhpType::Void);
    assert_eq!(validate_function(&function), Err(ValidationError::NoBlocks));
}

/// A one-block return function passes validation.
#[test]
fn well_formed_function_passes() {
    let mut function = Function::new("ok".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(7);
        builder.terminate(Terminator::Return { value: Some(value) });
    }
    assert!(validate_function(&function).is_ok());
}

/// A returned value must match the function's EIR return type.
#[test]
fn return_type_mismatch_fails() {
    let mut function = Function::new("bad".to_string(), IrType::F64, PhpType::Float);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(1);
        builder.terminate(Terminator::Return { value: Some(value) });
    }
    assert!(matches!(
        validate_function(&function),
        Err(ValidationError::ReturnTypeMismatch { .. })
    ));
}

/// Branch argument types must match destination block parameter types.
#[test]
fn branch_argument_type_mismatch_fails() {
    let mut function = Function::new("branch_bad".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let target = builder.create_named_block("target", vec![(IrType::F64, PhpType::Float)]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(1);
        builder.terminate(Terminator::Br {
            target,
            args: vec![value],
        });
        builder.position_at_end(target);
        builder.terminate(Terminator::Return { value: None });
    }
    assert!(matches!(
        validate_function(&function),
        Err(ValidationError::BranchArgTypeMismatch { .. })
    ));
}

/// A value defined in a non-dominating block cannot be returned elsewhere.
#[test]
fn use_not_dominated_fails() {
    let mut function = Function::new("dom_bad".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let other = builder.create_named_block("other", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Br {
            target: exit,
            args: vec![],
        });
        builder.position_at_end(other);
        let hidden = builder.emit_const_i64(9);
        builder.terminate(Terminator::Unreachable);
        builder.position_at_end(exit);
        builder.terminate(Terminator::Return {
            value: Some(hidden),
        });
    }
    assert!(matches!(
        validate_function(&function),
        Err(ValidationError::UseNotDominated { .. })
    ));
}

/// Rejects non-owning scalar inputs and malformed result storage for exception capture guards.
#[test]
fn exception_owned_guard_validates_heap_and_token_shapes() {
    use crate::ir::{Effects, Immediate, Op, Ownership, RuntimeCallTarget};
    for (heap, result_type, result_php_type, valid) in [
        (true, IrType::I64, PhpType::Int, true),
        (false, IrType::I64, PhpType::Int, false),
        (true, IrType::F64, PhpType::Float, false),
        (true, IrType::I64, PhpType::Bool, false),
    ] {
        let mut function = Function::new("guard".into(), IrType::Void, PhpType::Void);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let scalar = builder.emit_const_i64(7);
        let value = if heap {
            builder.emit(Op::MixedBox, vec![scalar], None, IrType::Heap(crate::ir::IrHeapKind::Mixed),
                PhpType::Mixed, Ownership::Owned).unwrap()
        } else { scalar };
        let anchor = builder.emit_const_i64(0);
        builder.emit_with_effects(Op::RuntimeCall, vec![value, anchor],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned)),
            result_type, result_php_type, Ownership::NonHeap, Effects::WRITES_GLOBAL, None);
        builder.terminate(Terminator::Return { value: None });
        assert_eq!(validate_function(&function).is_ok(), valid, "{function:?}");
    }
}

/// Rejects unboxed packed arrays and runtime targets without dynamic-argument support.
#[test]
fn packed_runtime_calls_validate_argument_storage() {
    use crate::ir::{Effects, Immediate, IrHeapKind, Op, Ownership, RuntimeArgumentLayout, RuntimeCallTarget, RuntimeFnId};
    for (element, target, valid) in [
        (PhpType::Mixed, RuntimeFnId::MbStrlen, true),
        (PhpType::Int, RuntimeFnId::MbStrlen, false),
        (PhpType::Mixed, RuntimeFnId::Count, false),
    ] {
        let mut function = Function::new("packed".into(), IrType::Void, PhpType::Void);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let array = builder.emit(Op::ArrayNew, vec![], Some(Immediate::Capacity(0)),
            IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(element)), Ownership::Owned).unwrap();
        builder.emit_with_effects(Op::RuntimeCall, vec![array],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
                target, arguments: RuntimeArgumentLayout::IndexedArray, strict_php: false, strict_types: Some(false),
            })), IrType::I64, PhpType::Int, Ownership::NonHeap, Effects::all(), None);
        builder.terminate(Terminator::Return { value: None });
        assert_eq!(validate_function(&function).is_ok(), valid, "{function:?}");
    }
}
