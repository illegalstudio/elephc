//! Purpose:
//! Eval registry entry and implementation for `json_last_error`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The result reads the current eval JSON error code from `ElephcEvalContext`.

use super::super::super::*;

eval_builtin! {
    name: "json_last_error",
    area: Json,
    params: [],
    direct: JsonLastError,
    values: JsonLastError,
}

/// Evaluates PHP `json_last_error()` with no eval arguments.
pub(in crate::interpreter) fn eval_builtin_json_last_error(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_json_last_error_result(context, values)
}

/// Returns the current JSON error code.
pub(in crate::interpreter) fn eval_json_last_error_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(context.json_last_error())
}
