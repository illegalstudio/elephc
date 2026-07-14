//! Purpose:
//! Eval registry entry and implementation for `date_default_timezone_get`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - The result reads the eval-local default timezone from the context.

use super::super::super::*;

eval_builtin! {
    name: "date_default_timezone_get",
    area: Time,
    params: [],
    direct: Time,
    values: Time,
}

/// Evaluates PHP `date_default_timezone_get()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_date_default_timezone_get(
    args: &[EvalExpr],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_date_default_timezone_get_result(context, values)
}

/// Returns the eval-local default timezone identifier.
pub(in crate::interpreter) fn eval_date_default_timezone_get_result(
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string(context.default_timezone())
}
