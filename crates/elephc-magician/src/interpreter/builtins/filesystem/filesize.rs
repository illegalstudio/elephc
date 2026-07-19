//! Purpose:
//! Declarative eval registry entry for `filesize`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the filesize helper.

eval_builtin! {
    name: "filesize",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::stream_wrappers;
use super::*;

/// Dispatches direct eval calls for the `filesize` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_filesize_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_filesize(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `filesize` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_filesize_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => eval_filesize_result(*filename, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `filesize($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_filesize(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_filesize_result(filename, context, values)
}

/// Returns one local file or supported wrapper size in bytes, or zero on failure.
pub(in crate::interpreter) fn eval_filesize_result(
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(stat) = eval_user_wrapper_url_stat_result(&path, 0, context, values)? {
        let size = eval_user_wrapper_stat_int_field(stat, "size", values)?.unwrap_or(0);
        return values.int(size);
    }
    if let Ok(bytes) = super::file_get_contents::eval_read_path_or_wrapper_bytes(&path) {
        return values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.int(0);
    };
    let len = std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    values.int(i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?)
}
