//! Purpose:
//! Declarative eval registry entry for `realpath_cache_size`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through elephc's empty realpath-cache helper.

eval_builtin! {
    name: "realpath_cache_size",
    area: Filesystem,
    params: [],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `realpath_cache_size` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_realpath_cache_size_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("realpath_cache_size", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `realpath_cache_size` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_realpath_cache_size_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("realpath_cache_size", evaluated_args, context, values)
}
