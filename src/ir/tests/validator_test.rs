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
    validate_function, Builder, Function, FunctionFlags, Immediate, IrHeapKind, IrType, LocalKind,
    Op, Ownership, RuntimeCallTarget, Terminator, ValidationError,
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
    let validation = validate_function(&function);
    assert!(validation.is_ok(), "{validation:?}");
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

/// Accepts the typed DateTime finalizer only for its raw array plus object receiver contract.
#[test]
fn date_serialize_finalize_signature_accepts_array_and_object() {
    let mut function = Function::new("date_finalize".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let raw_type = PhpType::Array(Box::new(PhpType::Mixed));
        let raw_slot = builder.add_local(None, IrType::Heap(IrHeapKind::Array), raw_type.clone(), LocalKind::HiddenTemp);
        let receiver_type = PhpType::Object("DateTime".to_string());
        let receiver_slot = builder.add_local(None, IrType::Heap(IrHeapKind::Object), receiver_type.clone(), LocalKind::HiddenTemp);
        let raw = builder.emit_load_local(raw_slot, IrType::Heap(IrHeapKind::Array), raw_type);
        let receiver = builder.emit_load_local(receiver_slot, IrType::Heap(IrHeapKind::Object), receiver_type);
        builder.emit(
            Op::RuntimeCall,
            vec![raw, receiver],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::DateSerializeFinalize)),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
        );
        builder.terminate(Terminator::Return { value: None });
    }
    assert!(validate_function(&function).is_ok());
}

/// Rejects a DateTime finalizer whose raw ABI result is not a generic mixed-element array.
#[test]
fn date_serialize_finalize_rejects_non_array_raw_result() {
    let mut function = Function::new("date_finalize_bad".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let raw_type = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        };
        let raw_slot = builder.add_local(None, IrType::Heap(IrHeapKind::Hash), raw_type.clone(), LocalKind::HiddenTemp);
        let receiver_type = PhpType::Object("DateTime".to_string());
        let receiver_slot = builder.add_local(None, IrType::Heap(IrHeapKind::Object), receiver_type.clone(), LocalKind::HiddenTemp);
        let raw = builder.emit_load_local(raw_slot, IrType::Heap(IrHeapKind::Hash), raw_type);
        let receiver = builder.emit_load_local(receiver_slot, IrType::Heap(IrHeapKind::Object), receiver_type);
        builder.emit(
            Op::RuntimeCall,
            vec![raw, receiver],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::DateSerializeFinalize)),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
        );
        builder.terminate(Terminator::Return { value: None });
    }
    let validation = validate_function(&function);
    assert!(matches!(
        validation,
        Err(ValidationError::OperandTypeMismatch { .. })
    ), "{validation:?}");
}

/// Accepts the consuming Mixed-to-array return boundary only inside a DateTime serializer.
#[test]
fn date_serialize_mixed_array_return_requires_serializer_provenance() {
    let mut function = Function::new("date_mixed_return".to_string(), IrType::Void, PhpType::Void);
    function.flags = FunctionFlags {
        is_method: true,
        is_date_serialize_method: true,
        ..FunctionFlags::default()
    };
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let mixed_slot = builder.add_local(None, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, LocalKind::HiddenTemp);
        let mixed = builder.emit_load_local(mixed_slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        builder.emit(
            Op::RuntimeCall,
            vec![mixed],
            Some(Immediate::RuntimeCall(
                RuntimeCallTarget::DateSerializeMixedToArrayReturn,
            )),
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Mixed)),
            Ownership::Owned,
        );
        builder.terminate(Terminator::Return { value: None });
    }
    let validation = validate_function(&function);
    assert!(validation.is_ok(), "{validation:?}");

    function.flags.is_date_serialize_method = false;
    assert!(matches!(
        validate_function(&function),
        Err(ValidationError::DateSerializeMixedToArrayReturnOutsideProvenance(_))
    ));
}

/// Rejects both consuming Mixed-to-array targets when their PHP result annotation is not array<mixed>.
#[test]
fn mixed_to_array_return_rejects_non_mixed_element_annotation() {
    for (name, target, is_date_serializer) in [
        (
            "generic_mixed_return",
            RuntimeCallTarget::MixedToArrayReturn,
            false,
        ),
        (
            "date_mixed_return",
            RuntimeCallTarget::DateSerializeMixedToArrayReturn,
            true,
        ),
    ] {
        let mut function = Function::new(name.to_string(), IrType::Void, PhpType::Void);
        function.flags = FunctionFlags {
            is_method: is_date_serializer,
            is_date_serialize_method: is_date_serializer,
            ..FunctionFlags::default()
        };
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let mixed_slot = builder.add_local(None, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, LocalKind::HiddenTemp);
            let mixed = builder.emit_load_local(mixed_slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
            builder.emit(
                Op::RuntimeCall,
                vec![mixed],
                Some(Immediate::RuntimeCall(target)),
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
                Ownership::Owned,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        assert!(matches!(
            validate_function(&function),
            Err(ValidationError::PhpTypeMismatch(_))
        ));
    }
}
