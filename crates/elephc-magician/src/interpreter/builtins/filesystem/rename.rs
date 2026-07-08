//! Purpose:
//! Declarative eval registry entry for `rename`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the binary path operation helper.

eval_builtin! {
    name: "rename",
    area: Filesystem,
    params: [from, to],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `rename` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_rename_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::copy::eval_builtin_binary_path_bool("rename", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `rename` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_rename_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [from, to] => super::copy::eval_binary_path_bool_result("rename", *from, *to, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
