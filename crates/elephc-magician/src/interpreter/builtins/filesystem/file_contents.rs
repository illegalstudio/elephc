//! Purpose:
//! Implements one-shot file content builtins for eval.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem` re-exports.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - Local paths, built-in URL wrappers, PHAR URLs, and userspace stream wrappers
//!   share these entry points for content reads and writes.

use super::super::super::*;
use super::*;
use crate::stream_wrappers;

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
    let bytes = match eval_read_path_or_wrapper_bytes(&path) {
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
            eval_array_set_indexed_bytes(result, line_index, &bytes[line_start..=index], values)?;
        line_start = index + 1;
        line_index += 1;
    }
    if line_start < bytes.len() {
        result = eval_array_set_indexed_bytes(result, line_index, &bytes[line_start..], values)?;
    }
    Ok(result)
}

/// Evaluates PHP `readfile($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_readfile(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_readfile_result(filename, context, values)
}

/// Streams one local file or supported wrapper to eval output.
pub(in crate::interpreter) fn eval_readfile_result(
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(result) = eval_user_wrapper_readfile_result(&path, context, values)? {
        return Ok(result);
    }
    if let Some(local_path) = stream_wrappers::local_filesystem_path(&path) {
        let path = std::path::Path::new(&local_path);
        if path.is_dir() {
            return values.int(-1);
        }
    }
    let bytes = match eval_read_path_or_wrapper_bytes(&path) {
        Ok(bytes) => bytes,
        Err(_) => return values.bool_value(false),
    };
    let output = values.string_bytes_value(&bytes)?;
    values.echo(output)?;
    values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Evaluates PHP `file_put_contents($filename, $data)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_file_put_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename, data] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    let data = eval_expr(data, context, scope, values)?;
    eval_file_put_contents_result(filename, data, context, values)
}

/// Writes a PHP string to a local file or supported wrapper and returns a byte count.
pub(in crate::interpreter) fn eval_file_put_contents_result(
    filename: RuntimeCellHandle,
    data: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let data = values.string_bytes(data)?;
    if stream_wrappers::is_phar_stream(&path) {
        return match elephc_phar::put_url_bytes(path.as_bytes(), &data) {
            Some(len) => values.int(i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?),
            None => values.bool_value(false),
        };
    }
    if let Some(result) =
        eval_user_wrapper_file_put_contents_result(&path, &data, context, values)?
    {
        return Ok(result);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    match std::fs::write(path, &data) {
        Ok(()) => values.int(i64::try_from(data.len()).map_err(|_| EvalStatus::RuntimeFatal)?),
        Err(_) => values.bool_value(false),
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
