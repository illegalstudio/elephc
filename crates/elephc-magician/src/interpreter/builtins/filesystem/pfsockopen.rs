//! Purpose:
//! Declarative eval registry entry for `pfsockopen`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference error-output path.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "pfsockopen",
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

/// Dispatches direct eval calls for the `pfsockopen` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_pfsockopen_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("pfsockopen", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `pfsockopen` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_pfsockopen_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("pfsockopen", evaluated_args, context, values)
}
