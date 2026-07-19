//! Purpose:
//! Declarative eval registry entry for `str_repeat`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing repeat hook.

eval_builtin! {
    name: "str_repeat",
    area: String,
    params: [string, times],
    direct: StrRepeat,
    values: StrRepeat,
}

use super::super::super::*;

/// Evaluates PHP's `str_repeat(...)` over one eval expression pair.
pub(in crate::interpreter) fn eval_builtin_str_repeat(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value, times] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let times = eval_expr(times, context, scope, values)?;
    eval_str_repeat_result(value, times, values)
}

/// Repeats one PHP string byte sequence according to a PHP-cast integer count.
pub(in crate::interpreter) fn eval_str_repeat_result(
    value: RuntimeCellHandle,
    times: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let times = eval_int_value(times, values)?;
    if times < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let times = usize::try_from(times).map_err(|_| EvalStatus::RuntimeFatal)?;
    let capacity = bytes
        .len()
        .checked_mul(times)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut output = Vec::with_capacity(capacity);
    for _ in 0..times {
        output.extend_from_slice(&bytes);
    }
    values.string_bytes_value(&output)
}
