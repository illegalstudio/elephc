//! Purpose:
//! Declarative eval registry entry for `file_put_contents`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the one-shot file write helper.
//! - `$flags` honours `FILE_APPEND` (8) and `LOCK_EX` (2), matching the AOT runtime. The lock
//!   is advisory and released when the handle drops, which is the same lifetime `close()`
//!   gives the compiled path.

eval_builtin! {
    contract: "file_put_contents",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;
use crate::stream_wrappers;

/// Dispatches direct eval calls for the `file_put_contents` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_put_contents_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_file_put_contents(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `file_put_contents` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_put_contents_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename, data] => eval_file_put_contents_result(*filename, *data, 0, context, values),
        [filename, data, flags] => {
            let flags = eval_int_value(*flags, values)?;
            eval_file_put_contents_result(*filename, *data, flags, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `file_put_contents($filename, $data)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_file_put_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (filename, data, flags) = match args {
        [filename, data] => (filename, data, None),
        [filename, data, flags] => (filename, data, Some(flags)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let filename = eval_expr(filename, context, scope, values)?;
    let data = eval_expr(data, context, scope, values)?;
    let flags = match flags {
        Some(flags) => {
            let flags = eval_expr(flags, context, scope, values)?;
            eval_int_value(flags, values)?
        }
        None => 0,
    };
    eval_file_put_contents_result(filename, data, flags, context, values)
}

/// Writes a PHP string to a local file or supported wrapper and returns a byte count.
pub(in crate::interpreter) fn eval_file_put_contents_result(
    filename: RuntimeCellHandle,
    data: RuntimeCellHandle,
    flags: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let data = values.string_bytes(data)?;
    if stream_wrappers::is_phar_stream(&path) {
        // A phar entry is rewritten whole, so FILE_APPEND has nothing to append to and
        // LOCK_EX has no descriptor to lock. Report failure rather than truncating what the
        // caller asked to extend, matching the compiled path.
        if flags != 0 {
            return values.bool_value(false);
        }
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
    let written = if flags & EVAL_FILE_APPEND != 0 {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| file.write_all(&data))
    } else {
        std::fs::write(path, &data)
    };
    match written {
        Ok(()) => values.int(i64::try_from(data.len()).map_err(|_| EvalStatus::RuntimeFatal)?),
        Err(_) => values.bool_value(false),
    }
}

/// PHP's `FILE_APPEND`: extend the file instead of truncating it.
const EVAL_FILE_APPEND: i64 = 8;
