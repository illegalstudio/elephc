//! Purpose:
//! Implements symbolic-link, clearstatcache, and unlink eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::ops` re-exports.
//!
//! Key details:
//! - Link failures map to PHP false or documented sentinel values without raising
//!   host IO errors through Rust panics.

use super::super::super::super::*;
use super::super::*;
use crate::stream_wrappers;

/// Evaluates PHP `readlink($path)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_readlink(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_readlink_result(path, values)
}

/// Reads one symbolic-link target string, or returns PHP false on failure.
pub(in crate::interpreter) fn eval_readlink_result(
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    match std::fs::read_link(path) {
        Ok(target) => values.string(target.to_string_lossy().as_ref()),
        Err(_) => values.bool_value(false),
    }
}

/// Evaluates PHP `linkinfo($path)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_linkinfo(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_linkinfo_result(path, values)
}

/// Returns one symlink metadata device id, or PHP's `-1` failure sentinel.
pub(in crate::interpreter) fn eval_linkinfo_result(
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.int(-1);
    };
    let dev = match std::fs::symlink_metadata(path) {
        Ok(metadata) => i64::try_from(metadata.dev()).map_err(|_| EvalStatus::RuntimeFatal)?,
        Err(_) => -1,
    };
    values.int(dev)
}

/// Evaluates `clearstatcache(...)` as an ordered no-op in eval.
pub(in crate::interpreter) fn eval_builtin_clearstatcache(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    for arg in args {
        eval_expr(arg, context, scope, values)?;
    }
    values.null()
}

/// Evaluates PHP `unlink($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_unlink(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_unlink_result(filename, context, values)
}

/// Deletes one path and returns whether it succeeded.
pub(in crate::interpreter) fn eval_unlink_result(
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if stream_wrappers::is_phar_stream(&path) {
        return values.bool_value(elephc_phar::delete_url_bytes(path.as_bytes()).is_some());
    }
    if let Some(result) = eval_user_wrapper_unlink_result(&path, context, values)? {
        return Ok(result);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    values.bool_value(std::fs::remove_file(path).is_ok())
}
