//! Purpose:
//! Declarative eval registry entry for `fsockopen`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference error-output path.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "fsockopen",
    area: Filesystem,
    params: [
        hostname,
        port,
        error_code: by_ref = EvalBuiltinDefaultValue::Null,
        error_message: by_ref = EvalBuiltinDefaultValue::Null,
        timeout = EvalBuiltinDefaultValue::Null
    ],
    by_ref: [error_code, error_message],
    direct: none,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `fsockopen` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fsockopen_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("fsockopen", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fsockopen` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fsockopen_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("fsockopen", evaluated_args, context, values)
}
