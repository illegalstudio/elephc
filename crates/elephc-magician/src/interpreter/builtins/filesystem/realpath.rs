//! Purpose:
//! Declarative eval registry entry for `realpath`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the canonical path helper.

eval_builtin! {
    name: "realpath",
    area: Filesystem,
    params: [path],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `realpath` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_realpath_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_realpath(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `realpath` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_realpath_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [path] => eval_realpath_result(*path, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `realpath($path)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_realpath(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_realpath_result(path, values)
}

/// Canonicalizes one path or returns PHP false when the path cannot be resolved.
pub(in crate::interpreter) fn eval_realpath_result(
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let path = String::from_utf8_lossy(&path);
    let Ok(canonical) = std::fs::canonicalize(path.as_ref()) else {
        return values.bool_value(false);
    };
    let canonical = canonical.to_string_lossy();
    values.string(canonical.as_ref())
}
