//! Purpose:
//! Dispatches already evaluated JSON builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;

/// Attempts to dispatch evaluated JSON builtins.
pub(in crate::interpreter) fn eval_json_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "json_decode" => match evaluated_args {
            [json] => eval_json_decode_result(*json, None, None, None, context, values)?,
            [json, associative] => {
                eval_json_decode_result(*json, Some(*associative), None, None, context, values)?
            }
            [json, associative, depth] => eval_json_decode_result(
                *json,
                Some(*associative),
                Some(*depth),
                None,
                context,
                values,
            )?,
            [json, associative, depth, flags] => eval_json_decode_result(
                *json,
                Some(*associative),
                Some(*depth),
                Some(*flags),
                context,
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "json_encode" => match evaluated_args {
            [value] => eval_json_encode_result(*value, None, None, context, values)?,
            [value, flags] => eval_json_encode_result(*value, Some(*flags), None, context, values)?,
            [value, flags, depth] => {
                eval_json_encode_result(*value, Some(*flags), Some(*depth), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "json_last_error" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.int(context.json_last_error())?
        }
        "json_last_error_msg" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.string(context.json_last_error_msg())?
        }
        "json_validate" => match evaluated_args {
            [json] => eval_json_validate_result(*json, None, None, context, values)?,
            [json, depth] => eval_json_validate_result(*json, Some(*depth), None, context, values)?,
            [json, depth, flags] => {
                eval_json_validate_result(*json, Some(*depth), Some(*flags), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        _ => return Ok(None),
    };
    Ok(Some(result))
}
