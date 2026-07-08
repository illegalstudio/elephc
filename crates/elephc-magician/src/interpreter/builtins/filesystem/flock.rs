//! Purpose:
//! Declarative eval registry entry for `flock`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference path.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "flock",
    area: Filesystem,
    params: [stream, operation, would_block: by_ref = EvalBuiltinDefaultValue::Null],
    by_ref: [would_block],
    direct: none,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `flock` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_flock_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("flock", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `flock` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_flock_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("flock", evaluated_args, context, values)
}
