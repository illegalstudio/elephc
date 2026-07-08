//! Purpose:
//! Declarative eval registry entry for `readlink`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the symbolic-link target helper.

eval_builtin! {
    name: "readlink",
    area: Filesystem,
    params: [path],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::stream_wrappers;

/// Dispatches direct eval calls for the `readlink` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_readlink_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_readlink(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `readlink` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_readlink_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [path] => eval_readlink_result(*path, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

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
