//! Purpose:
//! Substring and substring replacement builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings` re-exports.
//!
//! Key details:
//! - Runtime cells remain opaque and string bytes are obtained through `RuntimeValueOps`.

use super::super::super::*;
use super::super::*;

/// Evaluates PHP's `substr(...)` over one eval string, offset, and optional length.
pub(in crate::interpreter) fn eval_builtin_substr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, offset] => {
            let value = eval_expr(value, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            eval_substr_result(value, offset, None, values)
        }
        [value, offset, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_substr_result(value, offset, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Slices a PHP byte string using PHP `substr()` offset and length rules.
pub(in crate::interpreter) fn eval_substr_result(
    value: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let total = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let offset = eval_int_value(offset, values)?;
    let start = if offset < 0 {
        (total + offset).max(0)
    } else {
        offset.min(total)
    };
    let end = match length {
        None => total,
        Some(length) if values.is_null(length)? => total,
        Some(length) => {
            let length = eval_int_value(length, values)?;
            if length < 0 {
                (total + length).max(0)
            } else {
                start.saturating_add(length).min(total)
            }
        }
    };
    let end = end.max(start);
    let start = usize::try_from(start).map_err(|_| EvalStatus::RuntimeFatal)?;
    let end = usize::try_from(end).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string_bytes_value(&bytes[start..end])
}

/// Evaluates PHP's `substr_replace(...)` over eval scalar byte strings.
pub(in crate::interpreter) fn eval_builtin_substr_replace(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, replace, offset] => {
            let value = eval_expr(value, context, scope, values)?;
            let replace = eval_expr(replace, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            eval_substr_replace_result(value, replace, offset, None, values)
        }
        [value, replace, offset, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let replace = eval_expr(replace, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_substr_replace_result(value, replace, offset, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Replaces the byte range selected by PHP `substr_replace()` scalar rules.
pub(in crate::interpreter) fn eval_substr_replace_result(
    value: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let replacement = values.string_bytes(replace)?;
    let total = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let offset = eval_int_value(offset, values)?;
    let start = if offset < 0 {
        (total + offset).max(0)
    } else {
        offset.min(total)
    };
    let end = match length {
        None => total,
        Some(length) if values.is_null(length)? => total,
        Some(length) => {
            let length = eval_int_value(length, values)?;
            if length < 0 {
                (total + length).max(start)
            } else {
                start.saturating_add(length).min(total)
            }
        }
    };
    let start = usize::try_from(start).map_err(|_| EvalStatus::RuntimeFatal)?;
    let end = usize::try_from(end).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut output = Vec::with_capacity(bytes.len() + replacement.len());
    output.extend_from_slice(&bytes[..start]);
    output.extend_from_slice(&replacement);
    output.extend_from_slice(&bytes[end..]);
    values.string_bytes_value(&output)
}
