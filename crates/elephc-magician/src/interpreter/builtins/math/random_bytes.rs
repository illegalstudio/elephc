//! Purpose:
//! Eval registry entry and implementation for `random_bytes`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Bytes come directly from the operating system CSPRNG and remain binary-safe.

use super::super::super::*;

eval_builtin! {
    name: "random_bytes",
    area: Math,
    params: [length],
    direct: RandomBytes,
    values: RandomBytes,
}

/// Evaluates PHP `random_bytes()` with one strictly positive byte length.
pub(in crate::interpreter) fn eval_builtin_random_bytes(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [length] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let length = eval_expr(length, context, scope, values)?;
    eval_random_bytes_result(length, values)
}

/// Dispatches a by-value `random_bytes()` call after argument binding.
pub(in crate::interpreter) fn eval_random_bytes_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [length] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_random_bytes_result(*length, values)
}

/// Returns a binary-safe PHP string filled by the operating system CSPRNG.
pub(in crate::interpreter) fn eval_random_bytes_result(
    length: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let length = eval_int_value(length, values)?;
    let length = usize::try_from(length)
        .ok()
        .filter(|length| *length > 0)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut bytes = vec![0_u8; length];
    getrandom::getrandom(&mut bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string_bytes_value(&bytes)
}
