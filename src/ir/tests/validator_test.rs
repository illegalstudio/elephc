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
    validate_function, Builder, Function, IrType, Op, Ownership, Terminator, ValidationError,
    ValueDef,
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

/// An operand in an unreachable continuation block has no executable dominance requirement.
#[test]
fn entry_value_in_unreachable_block_is_valid() {
    let mut function = Function::new("dead_use".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let dead = builder.create_named_block("dead", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(1);
        builder.terminate(Terminator::Return { value: None });
        builder.position_at_end(dead);
        let _ = builder.emit_with_effects(
            Op::EchoValue,
            vec![value],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::EchoValue.default_effects(),
            None,
        );
        builder.terminate(Terminator::Unreachable);
    }
    assert_eq!(validate_function(&function), Ok(()));
}

/// An unreachable block cannot import a value from an unrelated unreachable block.
#[test]
fn sibling_unreachable_value_use_is_invalid() {
    let mut function = Function::new("dead_siblings".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let producer = builder.create_named_block("producer", vec![]);
        let consumer = builder.create_named_block("consumer", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Return { value: None });
        builder.position_at_end(producer);
        let value = builder.emit_const_i64(1);
        builder.terminate(Terminator::Unreachable);
        builder.position_at_end(consumer);
        let _ = builder.emit_with_effects(
            Op::EchoValue,
            vec![value],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::EchoValue.default_effects(),
            None,
        );
        builder.terminate(Terminator::Unreachable);
    }
    assert!(matches!(
        validate_function(&function),
        Err(ValidationError::UseNotDominated { .. })
    ));
}

/// Dead code still cannot use a same-block value before its definition.
#[test]
fn unreachable_same_block_use_before_definition_is_invalid() {
    let mut function = Function::new("dead_order".to_string(), IrType::Void, PhpType::Void);
    let dead;
    let value;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        dead = builder.create_named_block("dead", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Return { value: None });
        builder.position_at_end(dead);
        value = builder.emit_const_i64(1);
        let _ = builder.emit_with_effects(
            Op::EchoValue,
            vec![value],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::EchoValue.default_effects(),
            None,
        );
        builder.terminate(Terminator::Unreachable);
    }
    function.blocks[dead.as_raw() as usize].instructions.swap(0, 1);
    let ValueDef::Instruction { index, .. } = &mut function
        .value_mut(value)
        .expect("constant value")
        .def
    else {
        panic!("constant must remain instruction-defined");
    };
    *index = 1;
    assert!(matches!(
        validate_function(&function),
        Err(ValidationError::UseNotDominated { .. })
    ));
}
