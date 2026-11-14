//! Purpose:
//! Applies the shared checked numeric cast contract to borrowed eval values.
//!
//! Called from:
//! - Shared walker and bytecode cast evaluation after source operands are evaluated.
//!
//! Key details:
//! - Scalar parsing is pure Rust, so no native value failure crosses an interpreter frame.
//! - Reviewed object adapters retain their native contract and transfer one result owner.

use super::*;
use elephc_semantics::{numeric_cast::{checked_numeric_cast, NumericCastFailure,
    NumericCastInput, NumericCastTarget}, numeric_unary::NumericValue};

/// Computes one owned numeric result without coercing an unsupported original PHP kind.
pub(super) fn numeric(target: &EvalCastType, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let target = match target { EvalCastType::Int => NumericCastTarget::Int,
        EvalCastType::Float => NumericCastTarget::Float, _ => return Err(EvalStatus::UnsupportedConstruct) };
    if values.type_tag(value)? == EVAL_TAG_OBJECT {
        let cast = if target == NumericCastTarget::Int { elephc_builtin_contract::ObjectCast::Int }
            else { elephc_builtin_contract::ObjectCast::Float };
        let result = runtime_ops::object_operators::numeric_cast(values, value, cast);
        if let Some(result) = native_results::finish(result, context, values)? {
            return adapter_result(target, result, context, values);
        }
        let adapter = if target == NumericCastTarget::Int { "__elephc_to_int" } else { "__elephc_to_float" };
        let result = eval_object_scalar_adapter(value, adapter, context, values);
        if let Some(result) = native_results::finish(result, context, values)? {
            return adapter_result(target, result, context, values);
        }
    }
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

/// Requires the reviewed adapter's promised exact result kind and retires a rejected owner.
fn adapter_result(target: NumericCastTarget, result: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let expected = if target == NumericCastTarget::Int { EVAL_TAG_INT } else { EVAL_TAG_FLOAT };
    let checked = match values.type_tag(result) {
        Ok(tag) if tag == expected => return Ok(result),
        Ok(_) => throw_failure(NumericCastFailure::UnsupportedKind, context, values),
        Err(status) => Err(status),
    };
    finish_synthesized_method_arguments(checked, [result], context, values)
}

/// Publishes a PHP Throwable using the same failure category and message as native lowering.
fn throw_failure(failure: NumericCastFailure, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let class = if failure == NumericCastFailure::UnsupportedKind { "TypeError" } else { "ValueError" };
    throwables::eval_throw_builtin_exception(class, failure.message(), context, values)
}
