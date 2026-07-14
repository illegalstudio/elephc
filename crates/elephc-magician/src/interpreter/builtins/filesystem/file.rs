//! Purpose:
//! Declarative eval registry entry for `file`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the file-lines helper.

eval_builtin! {
    name: "file",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `file` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_file(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `file` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => eval_file_result(*filename, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `file($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_file(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_result(filename, context, values)
}

/// Reads one local file or supported wrapper and returns indexed line byte strings.
pub(in crate::interpreter) fn eval_file_result(
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(result) = eval_user_wrapper_file_get_contents_result(&path, context, values)? {
        if values.type_tag(result)? == EVAL_TAG_STRING {
            let bytes = values.string_bytes(result)?;
            return eval_file_lines_array(&bytes, values);
        }
        values.warning("Warning: file_get_contents(): Failed to open stream\n")?;
        return values.array_new(0);
    }
    let bytes = match super::file_get_contents::eval_read_path_or_wrapper_bytes(&path) {
        Ok(bytes) => bytes,
        Err(_) => {
            values.warning("Warning: file_get_contents(): Failed to open stream\n")?;
            return values.array_new(0);
        }
    };
    eval_file_lines_array(&bytes, values)
}

/// Splits file payload bytes into runtime array entries, preserving trailing newlines.
fn eval_file_lines_array(
    bytes: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(0)?;
    let mut line_start = 0;
    let mut line_index = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        result =
            super::scandir::eval_array_set_indexed_bytes(result, line_index, &bytes[line_start..=index], values)?;
        line_start = index + 1;
        line_index += 1;
    }
    if line_start < bytes.len() {
        result = super::scandir::eval_array_set_indexed_bytes(result, line_index, &bytes[line_start..], values)?;
    }
    Ok(result)
}
