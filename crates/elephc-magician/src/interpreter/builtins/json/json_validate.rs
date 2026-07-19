//! Purpose:
//! Eval registry entry and dispatch wrappers for `json_validate`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - This file owns the validation implementation, direct wrapper, and by-value
//!   dispatch shape.
//! - JSON parse-error recording is reused from `json_decode` instead of a
//!   separate area-level helper module.

use super::json_decode::eval_record_json_parse_error;
use super::super::super::*;
use super::super::spec::EvalBuiltinDefaultValue;
use crate::json_validate;

eval_builtin! {
    name: "json_validate",
    area: Json,
    params: [
        json,
        depth = EvalBuiltinDefaultValue::Int(512),
        flags = EvalBuiltinDefaultValue::Int(0),
    ],
    direct: JsonValidate,
    values: JsonValidate,
}

/// Evaluates PHP `json_validate()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_json_validate(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [json] => {
            let json = eval_expr(json, context, scope, values)?;
            eval_json_validate_result(json, None, None, context, values)
        }
        [json, depth] => {
            let json = eval_expr(json, context, scope, values)?;
            let depth = eval_expr(depth, context, scope, values)?;
            eval_json_validate_result(json, Some(depth), None, context, values)
        }
        [json, depth, flags] => {
            let json = eval_expr(json, context, scope, values)?;
            let depth = eval_expr(depth, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_json_validate_result(json, Some(depth), Some(flags), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches by-value `json_validate()` calls after argument binding.
pub(in crate::interpreter) fn eval_json_validate_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [json] => eval_json_validate_result(*json, None, None, context, values),
        [json, depth] => eval_json_validate_result(*json, Some(*depth), None, context, values),
        [json, depth, flags] => eval_json_validate_result(*json, Some(*depth), Some(*flags), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Validates JSON text with eval's current zero-flag JSON subset and records JSON state.
fn eval_json_validate_result(
    json: RuntimeCellHandle,
    depth: Option<RuntimeCellHandle>,
    flags: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let flags = flags
        .map(|flags| eval_int_value(flags, values))
        .transpose()?
        .unwrap_or(0);
    if flags & !EVAL_JSON_INVALID_UTF8_IGNORE != 0 {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let depth = depth
        .map(|depth| eval_int_value(depth, values))
        .transpose()?
        .unwrap_or(512);
    if depth <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }

    let bytes = values.string_bytes(json)?;
    let result = if flags & EVAL_JSON_INVALID_UTF8_IGNORE != 0 {
        json_validate::decode_result_ignoring_invalid_utf8(&bytes, depth as usize)
    } else {
        json_validate::decode_result(&bytes, depth as usize)
    };
    match result {
        Ok(_) => {
            context.clear_json_error();
            values.bool_value(true)
        }
        Err(error) => {
            eval_record_json_parse_error(context, error, &bytes);
            values.bool_value(false)
        }
    }
}
