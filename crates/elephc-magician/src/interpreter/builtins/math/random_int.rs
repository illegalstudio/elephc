//! Purpose:
//! Eval registry entry and implementation for `random_int`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Eval uses the same process-local pseudo-random source as the existing
//!   interpreter implementation; invalid ranges are runtime fatals.

use super::super::super::*;

eval_builtin! {
    name: "random_int",
    area: Math,
    params: [min, max],
    direct: RandomInt,
    values: RandomInt,
}

/// Evaluates PHP `random_int()` over an inclusive integer range.
pub(in crate::interpreter) fn eval_builtin_random_int(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [min, max] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let min = eval_expr(min, context, scope, values)?;
    let max = eval_expr(max, context, scope, values)?;
    eval_random_int_result(min, max, values)
}

/// Dispatches by-value `random_int()` calls after argument binding.
pub(in crate::interpreter) fn eval_random_int_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [min, max] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_random_int_result(*min, *max, values)
}

/// Returns one eval `random_int()` value in the inclusive range `[min, max]`.
pub(in crate::interpreter) fn eval_random_int_result(
    min: RuntimeCellHandle,
    max: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let min = eval_int_value(min, values)?;
    let max = eval_int_value(max, values)?;
    if min > max {
        return Err(EvalStatus::RuntimeFatal);
    }
    let width = (i128::from(max) - i128::from(min) + 1) as u128;
    let offset = (eval_random_u128() % width) as i128;
    let sampled = i128::from(min) + offset;
    let sampled = i64::try_from(sampled).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(sampled)
}
