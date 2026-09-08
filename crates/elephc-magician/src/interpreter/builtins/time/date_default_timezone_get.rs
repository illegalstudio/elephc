//! Purpose:
//! Eval registry entry and implementation for `date_default_timezone_get`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - Native request state is authoritative when a runtime getter is installed.

use super::super::super::*;

eval_builtin! {
    contract: "date_default_timezone_get",
    area: Time,
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

/// Returns the native request timezone, falling back to standalone eval state.
pub(in crate::interpreter) fn eval_date_default_timezone_get_result(
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(value) = values.runtime_builtin_call(
        elephc_builtin_contract::RuntimeBuiltinId::DateDefaultTimezoneGet, &[],
    )? {
        return Ok(value);
    }
    values.string(context.default_timezone())
}

/// Queries timezone state on every operation so native callbacks cannot leave eval stale.
pub(in crate::interpreter) fn eval_request_timezone(
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let Some(value) = values.runtime_builtin_call(
        elephc_builtin_contract::RuntimeBuiltinId::DateDefaultTimezoneGet, &[],
    )? else {
        return Ok(context.default_timezone().to_owned());
    };
    let bytes = values.string_bytes(value);
    let released = values.release(value);
    let bytes = bytes?;
    released?;
    String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
}
