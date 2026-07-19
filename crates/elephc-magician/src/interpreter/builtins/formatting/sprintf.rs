//! Purpose:
//! Eval registry entry and implementation for `sprintf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - This file owns the string-returning printf-family implementation.
//! - Echoing and vector-argument variants reuse the byte formatter here because
//!   they are behavior variants of `sprintf`.

use super::super::super::*;
use super::*;

eval_builtin! {
    name: "sprintf",
    area: Formatting,
    params: [format],
    variadic: values,
    direct: Sprintf,
    values: Sprintf,
}

/// Evaluates direct positional `sprintf()` calls in source order.
pub(in crate::interpreter) fn eval_builtin_sprintf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_sprintf_result(&evaluated_args, values)
}

/// Formats `sprintf()` arguments and returns the resulting PHP string.
pub(in crate::interpreter) fn eval_sprintf_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((format, format_args)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    let output = eval_sprintf_bytes(&format, format_args, values)?;
    values.string_bytes_value(&output)
}

/// Formats one printf-style byte string through eval runtime value coercions.
pub(in crate::interpreter) fn eval_sprintf_bytes(
    format: &[u8],
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let mut output = Vec::new();
    let mut index = 0;
    let mut arg_index = 0;
    while index < format.len() {
        if format[index] != b'%' {
            output.push(format[index]);
            index += 1;
            continue;
        }
        index += 1;
        if index >= format.len() {
            break;
        }
        if format[index] == b'%' {
            output.push(b'%');
            index += 1;
            continue;
        }

        let (spec, next_index) = eval_parse_sprintf_spec(format, index)?;
        index = next_index;
        let Some(arg) = args.get(arg_index).copied() else {
            return Err(EvalStatus::RuntimeFatal);
        };
        arg_index += 1;
        let bytes = eval_format_sprintf_arg(spec, arg, values)?;
        output.extend_from_slice(&bytes);
    }
    Ok(output)
}
