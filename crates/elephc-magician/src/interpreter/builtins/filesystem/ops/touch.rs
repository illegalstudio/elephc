//! Purpose:
//! Implements PHP `touch()` eval support and timestamp conversion helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::ops` re-exports.
//!
//! Key details:
//! - Existing files are preserved, missing files are created, and timestamp
//!   arguments are resolved to `SystemTime` values.

use super::super::super::super::*;
use super::super::super::*;
use super::super::*;
use crate::stream_wrappers;

/// Evaluates PHP `touch($filename, $mtime = null, $atime = null)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_touch(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [filename] => {
            let filename = eval_expr(filename, context, scope, values)?;
            eval_touch_result(filename, None, None, context, values)
        }
        [filename, mtime] => {
            let filename = eval_expr(filename, context, scope, values)?;
            let mtime = eval_expr(mtime, context, scope, values)?;
            eval_touch_result(filename, Some(mtime), None, context, values)
        }
        [filename, mtime, atime] => {
            let filename = eval_expr(filename, context, scope, values)?;
            let mtime = eval_expr(mtime, context, scope, values)?;
            let atime = eval_expr(atime, context, scope, values)?;
            eval_touch_result(filename, Some(mtime), Some(atime), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Creates or stamps one local file and returns whether the operation succeeded.
pub(in crate::interpreter) fn eval_touch_result(
    filename: RuntimeCellHandle,
    mtime: Option<RuntimeCellHandle>,
    atime: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let (mtime, atime) = eval_touch_times(mtime, atime, values)?;
    let metadata_value = eval_touch_metadata_value(mtime, atime, values)?;
    if let Some(result) = eval_user_wrapper_stream_metadata_result(
        &path,
        EVAL_STREAM_META_TOUCH,
        metadata_value,
        context,
        values,
    )? {
        return Ok(result);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    let file = match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
    {
        Ok(file) => file,
        Err(_) => return values.bool_value(false),
    };
    let times = std::fs::FileTimes::new()
        .set_modified(mtime)
        .set_accessed(atime);
    values.bool_value(file.set_times(times).is_ok())
}

/// Builds the `[mtime, atime]` array passed to wrapper `stream_metadata()`.
fn eval_touch_metadata_value(
    mtime: std::time::SystemTime,
    atime: std::time::SystemTime,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(2)?;
    let key = values.int(0)?;
    let value = values.int(eval_system_time_to_unix(mtime).ok_or(EvalStatus::RuntimeFatal)?)?;
    result = values.array_set(result, key, value)?;
    let key = values.int(1)?;
    let value = values.int(eval_system_time_to_unix(atime).ok_or(EvalStatus::RuntimeFatal)?)?;
    values.array_set(result, key, value)
}

/// Resolves PHP touch timestamp defaults into concrete system times.
pub(in crate::interpreter) fn eval_touch_times(
    mtime: Option<RuntimeCellHandle>,
    atime: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(std::time::SystemTime, std::time::SystemTime), EvalStatus> {
    let now = std::time::SystemTime::now();
    let Some(mtime) = mtime else {
        return Ok((now, now));
    };
    if values.is_null(mtime)? {
        if let Some(atime) = atime {
            if !values.is_null(atime)? {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        return Ok((now, now));
    }
    let mtime = eval_system_time_from_unix(eval_int_value(mtime, values)?)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let Some(atime) = atime else {
        return Ok((mtime, mtime));
    };
    if values.is_null(atime)? {
        return Ok((mtime, mtime));
    }
    let atime = eval_system_time_from_unix(eval_int_value(atime, values)?)
        .ok_or(EvalStatus::RuntimeFatal)?;
    Ok((mtime, atime))
}

/// Converts a Unix timestamp in seconds into a `SystemTime`.
pub(in crate::interpreter) fn eval_system_time_from_unix(
    seconds: i64,
) -> Option<std::time::SystemTime> {
    if seconds >= 0 {
        std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(seconds as u64))
    } else {
        std::time::UNIX_EPOCH.checked_sub(std::time::Duration::from_secs(seconds.unsigned_abs()))
    }
}

/// Converts a `SystemTime` back to whole Unix seconds for wrapper metadata.
fn eval_system_time_to_unix(time: std::time::SystemTime) -> Option<i64> {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).ok(),
        Err(error) => i64::try_from(error.duration().as_secs())
            .ok()
            .map(|seconds| -seconds),
    }
}
