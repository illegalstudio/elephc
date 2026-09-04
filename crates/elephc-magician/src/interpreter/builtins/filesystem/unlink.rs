//! Purpose:
//! Declarative eval registry entry for `unlink`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unlink helper.

eval_builtin! {
    contract: "unlink",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::stream_wrappers;
use super::*;

/// Dispatches direct eval calls for the `unlink` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_unlink_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_unlink(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `unlink` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_unlink_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] | [filename, _] => eval_unlink_result(*filename, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `unlink($filename, $context)` over eval expressions.
///
/// php's signature is `unlink($filename, $context = null)`; the trailing context is accepted and
/// ignored, matching the compiled backend.
pub(in crate::interpreter) fn eval_builtin_unlink(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (filename, rest) = match args {
        [filename] => (filename, &args[1..1]),
        [filename, rest @ ..] if rest.len() == 1 => (filename, rest),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    for arg in rest {
        eval_expr(arg, context, scope, values)?;
    }
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
