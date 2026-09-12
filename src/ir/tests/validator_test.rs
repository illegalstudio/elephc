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

/// A nominally matching integer is still not a captured reference-cell pointer.
#[test]
fn reference_return_rejects_a_payload_shaped_scalar() {
    let mut function = Function::new("bad_reference".to_string(), IrType::I64, PhpType::Int);
    function.flags.by_ref_return = true;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(7);
        builder.terminate(Terminator::Return { value: Some(value) });
    }
    assert!(matches!(
        validate_function(&function),
        Err(ValidationError::ReturnTypeMismatch { .. })
    ));
}

/// Acquiring a returned cell must publish its pointer snapshot, not only a cleanup side effect.
#[test]
fn reference_acquisition_requires_a_pointer_result() {
    use crate::ir::{Immediate, LocalKind, Op, Ownership};
    let mut function = Function::new("missing_snapshot".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("lease".to_string()), IrType::I64, PhpType::Pointer(None), LocalKind::ReturnRefCell,
        );
        let value = builder.emit_const_i64(0);
        builder.emit(
            Op::AcquireRefCell, vec![value], Some(Immediate::LocalSlot(slot)),
            IrType::Void, PhpType::Void, Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: None });
    }
    assert!(matches!(
        validate_function(&function),
        Err(ValidationError::InstructionResultMissing(_))
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
