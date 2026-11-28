//! Purpose:
//! Applies the shared checked numeric cast contract to borrowed eval values.
//!
//! Called from:
//! - Shared walker and bytecode cast evaluation after source operands are evaluated.
//!
//! Key details:
//! - Scalar parsing is pure Rust, so no native value failure crosses an interpreter frame.
//! - Reviewed object projections expose original text before PHP's numeric adapters can narrow it.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};
use elephc_semantics::{numeric_cast::{checked_numeric_cast, NumericCastFailure,
    NumericCastInput, NumericCastTarget}, numeric_unary::NumericValue};

/// Computes one owned numeric result without coercing an unsupported original PHP kind.
pub(super) fn numeric(target: &EvalCastType, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let target = match target { EvalCastType::Int => NumericCastTarget::Int,
        EvalCastType::Float => NumericCastTarget::Float, _ => return Err(EvalStatus::UnsupportedConstruct) };
    if values.type_tag(value)? == EVAL_TAG_OBJECT {
        let cast = if target == NumericCastTarget::Int { elephc_builtin_contract::ObjectCast::Int } else { elephc_builtin_contract::ObjectCast::Float };
        let projection = runtime_ops::object_operators::checked_numeric_projection(values, value, cast)?;
        let Some(projection) = projection else { return throw_failure(NumericCastFailure::UnsupportedKind, context, values); };
        let projected = values.operator_method_call(value, projection.source_method, vec![]);
        let projected = native_results::finish(projected, context, values)?;
        let owner = RuntimeOwnerSlot::new(projected);
        return with_runtime_owners(&owner, || {
            let result = match values.type_tag(projected) {
                Ok(EVAL_TAG_STRING) => scalar(target, projected.borrowed(), context, values).and_then(|proof| {
                    let Some(method) = projection.result_method else { return Ok(proof); };
                    let proof = RuntimeOwnerSlot::new(proof);
                    with_runtime_owners(&proof, || {
                        let result = float_adapter(value, method, context, values);
                        finish_synthesized_method_arguments(result, [proof.take()], context, values)
                    })
                }),
                Ok(_) => throw_failure(NumericCastFailure::UnsupportedKind, context, values),
                Err(status) => Err(status),
            };
            finish_synthesized_method_arguments(result, [owner.take()], context, values)
        });
    }
    scalar(target, value, context, values)
}

/// Preserves a reviewed floating adapter's result only after original-domain proof succeeds.
fn float_adapter(value: RuntimeCellHandle, method: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = values.operator_method_call(value, method, vec![]);
    let result = native_results::finish(result, context, values)?;
    let owner = RuntimeOwnerSlot::new(result);
    with_runtime_owners(&owner, || {
        let invalid = match values.type_tag(result) {
            Ok(EVAL_TAG_FLOAT) => return Ok(owner.take()),
            Ok(_) => throw_failure(NumericCastFailure::UnsupportedKind, context, values),
            Err(status) => Err(status),
        };
        finish_synthesized_method_arguments(invalid, [owner.take()], context, values)
    })
}

/// Converts an original scalar or reviewed numeric projection before publishing a typed result.
fn scalar(target: NumericCastTarget, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    let result = if tag == EVAL_TAG_STRING {
        let bytes = values.string_bytes(value)?;
        match std::str::from_utf8(&bytes) {
            Ok(text) => checked_numeric_cast(target, NumericCastInput::String(text)),
            Err(_) => Err(NumericCastFailure::InvalidString),
        }
    } else {
        let input = match tag {
            EVAL_TAG_INT => NumericCastInput::Int(values.raw_value_word(value)? as i64),
            EVAL_TAG_FLOAT => NumericCastInput::Float(f64::from_bits(values.raw_value_word(value)?)),
            EVAL_TAG_BOOL => NumericCastInput::Bool(values.raw_value_word(value)? != 0),
            EVAL_TAG_NULL => NumericCastInput::Null, _ => NumericCastInput::Unsupported,
        };
        checked_numeric_cast(target, input)
    };
    match result {
        Ok(NumericValue::Int(value)) => values.int(value),
        Ok(NumericValue::Float(value)) => values.float(value),
        Err(failure) => throw_failure(failure, context, values),
    }
}

/// Publishes a PHP Throwable using the same failure category and message as native lowering.
fn throw_failure(failure: NumericCastFailure, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let class = if failure == NumericCastFailure::UnsupportedKind { "TypeError" } else { "ValueError" };
    throwables::eval_throw_builtin_exception(class, failure.message(), context, values)
}
