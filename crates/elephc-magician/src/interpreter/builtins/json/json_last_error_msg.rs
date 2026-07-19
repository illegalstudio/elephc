//! Purpose:
//! Eval registry entry and implementation for `json_last_error_msg`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The result reads the current eval JSON error message from `ElephcEvalContext`.

use super::super::super::*;

eval_builtin! {
    name: "json_last_error_msg",
    area: Json,
    params: [],
    direct: JsonLastErrorMsg,
    values: JsonLastErrorMsg,
}

/// Evaluates PHP `json_last_error_msg()` with no eval arguments.
pub(in crate::interpreter) fn eval_builtin_json_last_error_msg(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_json_last_error_msg_result(context, values)
}

/// Returns the current JSON error message.
pub(in crate::interpreter) fn eval_json_last_error_msg_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string(context.json_last_error_msg())
}
