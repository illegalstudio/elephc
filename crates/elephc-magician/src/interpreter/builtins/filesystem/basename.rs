//! Purpose:
//! Declarative eval registry entry for `basename`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the path helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "basename",
    area: Filesystem,
    params: [path, suffix = EvalBuiltinDefaultValue::String("")],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `basename` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_basename_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_basename(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `basename` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_basename_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [path] => eval_basename_result(*path, None, values),
        [path, suffix] => eval_basename_result(*path, Some(*suffix), values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `basename($path, $suffix = "")` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_basename(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [path] => {
            let path = eval_expr(path, context, scope, values)?;
            eval_basename_result(path, None, values)
        }
        [path, suffix] => {
            let path = eval_expr(path, context, scope, values)?;
            let suffix = eval_expr(suffix, context, scope, values)?;
            eval_basename_result(path, Some(suffix), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `basename()` bytes and returns them as a runtime string.
pub(in crate::interpreter) fn eval_basename_result(
    path: RuntimeCellHandle,
    suffix: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let suffix = suffix
        .map(|suffix| values.string_bytes(suffix))
        .transpose()?;
    let result = eval_basename_bytes(&path, suffix.as_deref());
    values.string_bytes_value(&result)
}

/// Extracts a PHP basename from one path byte string.
pub(in crate::interpreter) fn eval_basename_bytes(path: &[u8], suffix: Option<&[u8]>) -> Vec<u8> {
    let mut end = path.len();
    while end > 0 && eval_basename_separator(path[end - 1]) {
        end -= 1;
    }
    if end == 0 {
        return Vec::new();
    }
    let mut start = end;
    while start > 0 && !eval_basename_separator(path[start - 1]) {
        start -= 1;
    }
    let mut result = path[start..end].to_vec();
    if let Some(suffix) = suffix {
        if !suffix.is_empty() && suffix.len() < result.len() && result.ends_with(suffix) {
            result.truncate(result.len() - suffix.len());
        }
    }
    result
}

/// Returns whether one byte separates path components on the current host platform.
fn eval_basename_separator(byte: u8) -> bool {
    byte == b'/' || cfg!(windows) && byte == b'\\'
}
