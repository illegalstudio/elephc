//! Purpose:
//! Declarative eval registry entry for `pathinfo`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the pathinfo helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "pathinfo",
    area: Filesystem,
    params: [path, flags = EvalBuiltinDefaultValue::Int(15)],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `pathinfo` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_pathinfo_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("pathinfo", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `pathinfo` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_pathinfo_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("pathinfo", evaluated_args, context, values)
}
