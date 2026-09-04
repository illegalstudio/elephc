//! Purpose:
//! Declarative eval registry entry for `filesize`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the filesize helper.

eval_builtin! {
    contract: "filesize",
    area: Filesystem,
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

/// Returns one local file or supported wrapper size in bytes, or `false` on failure.
///
/// Every failure here used to answer `int(0)` — and said so in this very doc comment. `0` is a
/// legitimate size for an empty file, so a caller could not tell a missing path from an empty
/// one, and `=== false` never matched. PHP returns `false`, as do the seven scalar stat getters
/// this function sits beside.
///
/// The distinction that matters: a file that genuinely reads as zero bytes still answers
/// `int(0)`. Only the paths that could not be measured at all answer `false`.
pub(in crate::interpreter) fn eval_filesize_result(
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(stat) = eval_user_wrapper_url_stat_result(&path, eval_url_stat_flags("filesize"), context, values)? {
        // A matched wrapper that reports no readable `size` field has failed to stat, which is
        // not the same as reporting a size of zero.
        return match eval_user_wrapper_stat_int_field(stat, "size", values)? {
            Some(size) => values.int(size),
            None => values.bool_value(false),
        };
    }
    if let Ok(bytes) = super::file_get_contents::eval_read_path_or_wrapper_bytes(&path) {
        return values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    let Ok(metadata) = std::fs::metadata(path) else {
        return values.bool_value(false);
    };
    values.int(i64::try_from(metadata.len()).map_err(|_| EvalStatus::RuntimeFatal)?)
}
