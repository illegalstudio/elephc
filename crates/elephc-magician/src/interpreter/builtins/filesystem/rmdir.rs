//! Purpose:
//! Declarative eval registry entry for `rmdir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary path operation helper.

eval_builtin! {
    contract: "rmdir",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `rmdir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_rmdir_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // php's signature is `rmdir($directory, $context = null)`; the trailing context is accepted and
    // ignored, matching the compiled backend.
    let (path, rest) = match args {
        [path] => (path, &args[1..1]),
        [path, rest @ ..] if rest.len() == 1 => (path, rest),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    for arg in rest {
        eval_expr(arg, context, scope, values)?;
    }
    let path = eval_expr(path, context, scope, values)?;
    super::chdir::eval_unary_path_bool_result("rmdir", path, context, values)
}

/// Dispatches evaluated-argument calls for the `rmdir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_rmdir_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [path] | [path, _] => {
            super::chdir::eval_unary_path_bool_result("rmdir", *path, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
