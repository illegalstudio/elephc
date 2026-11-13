//! Purpose:
//! Adds function and value context to EIR lowering validation errors.
//!
//! Called from:
//! - Complete-module lowering when validation rejects a generated function.
//!
//! Key details:
//! - Validation rules are unchanged; extra inspection runs only after a failure.

use crate::ir::{Module, ValidationError, ValueDef};
use super::LoweringError;

/// Identifies the failing function and includes ownership/type metadata when available.
pub(super) fn describe(module: &Module, error: ValidationError) -> LoweringError {
    for function in module.functions.iter().chain(module.class_methods.iter())
        .chain(module.closures.iter()).chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter()).chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        let Err(failure) = crate::ir::validate_function(function) else { continue; };
        let detail = match &failure {
            ValidationError::OwnershipTypeMismatch(id) | ValidationError::PhpTypeMismatch(id)
            | ValidationError::ResultTypeMismatch(id) => function.value(*id).map(|value| {
                let instruction = match value.def {
                    ValueDef::Instruction { inst, .. } => function.instruction(inst),
                    _ => None,
                };
                format!("; value={value:?}; instruction={instruction:?}")
            }).unwrap_or_default(),
            _ => String::new(),
        };
        return LoweringError::ValidationInFunction { function: function.name.clone(), error: failure, detail };
    }
    LoweringError::Validation(error)
}
