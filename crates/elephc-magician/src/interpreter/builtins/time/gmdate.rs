//! Purpose:
//! Eval registry entry and implementation wrapper for `gmdate`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - UTC formatting delegates to the shared formatter owned by `date`.

use super::*;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "gmdate",
    area: Time,
    params: [format, timestamp = EvalBuiltinDefaultValue::Null],
    direct: Time,
    values: Time,
}

/// Evaluates PHP `gmdate($format, $timestamp = time())` for the eval subset.
pub(in crate::interpreter) fn eval_builtin_gmdate(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_date_like("gmdate", args, context, scope, values)
}

/// Formats one UTC timestamp through the shared `date` formatter.
pub(in crate::interpreter) fn eval_gmdate_result(
    format: RuntimeCellHandle,
    timestamp: Option<RuntimeCellHandle>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_date_result("gmdate", format, timestamp, context, values)
}
