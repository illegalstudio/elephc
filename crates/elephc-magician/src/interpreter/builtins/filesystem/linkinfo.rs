//! Purpose:
//! Declarative eval registry entry for `linkinfo`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the symbolic-link metadata helper.

eval_builtin! {
    name: "linkinfo",
    area: Filesystem,
    params: [path],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::stream_wrappers;

/// Dispatches direct eval calls for the `linkinfo` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_linkinfo_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_linkinfo(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `linkinfo` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_linkinfo_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [path] => eval_linkinfo_result(*path, values),
        _ => Err(EvalStatus::RuntimeFatal),
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
    #[cfg(unix)]
    let dev = match std::fs::symlink_metadata(path) {
        Ok(metadata) => i64::try_from(metadata.dev()).map_err(|_| EvalStatus::RuntimeFatal)?,
        Err(_) => -1,
    };
    #[cfg(windows)]
    let dev = if std::fs::symlink_metadata(path).is_ok() { 0 } else { -1 };
    values.int(dev)
}
