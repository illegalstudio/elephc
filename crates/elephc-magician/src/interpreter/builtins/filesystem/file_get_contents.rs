//! Purpose:
//! Declarative eval registry entry for `file_get_contents`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the one-shot file read helper.

eval_builtin! {
    name: "file_get_contents",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;
use crate::stream_wrappers;

/// Dispatches direct eval calls for the `file_get_contents` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_get_contents_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_file_get_contents(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `file_get_contents` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_get_contents_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => eval_file_get_contents_result(*filename, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `file_get_contents($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_file_get_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_get_contents_result(filename, context, values)
}

/// Reads a local file or supported wrapper into a PHP string, or returns false on failure.
pub(in crate::interpreter) fn eval_file_get_contents_result(
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(result) = eval_user_wrapper_file_get_contents_result(&path, context, values)? {
        return Ok(result);
    }
    match eval_read_path_or_wrapper_bytes(&path) {
        Ok(bytes) => values.string_bytes_value(&bytes),
        Err(_) => {
            values.warning("Warning: file_get_contents(): Failed to open stream\n")?;
            values.bool_value(false)
        }
    }
}

/// Reads bytes from supported direct path or stream-wrapper URLs.
pub(in crate::interpreter) fn eval_read_path_or_wrapper_bytes(
    path: &str,
) -> Result<Vec<u8>, ()> {
    if stream_wrappers::is_data_stream(path) {
        return stream_wrappers::decode_data_uri(path).ok_or(());
    }
    if stream_wrappers::is_phar_stream(path) {
        return elephc_phar::extract_url_bytes(path.as_bytes()).ok_or(());
    }
    if stream_wrappers::is_http_stream(path) {
        return stream_wrappers::read_http_url(path).ok_or(());
    }
    let Some(path) = stream_wrappers::local_filesystem_path(path) else {
        return Err(());
    };
    std::fs::read(path).map_err(|_| ())
}
