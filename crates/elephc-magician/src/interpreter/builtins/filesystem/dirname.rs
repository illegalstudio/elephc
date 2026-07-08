//! Purpose:
//! Declarative eval registry entry for `dirname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the path helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "dirname",
    area: Filesystem,
    params: [path, levels = EvalBuiltinDefaultValue::Int(1)],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `dirname` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_dirname_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("dirname", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `dirname` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_dirname_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("dirname", evaluated_args, context, values)
}
