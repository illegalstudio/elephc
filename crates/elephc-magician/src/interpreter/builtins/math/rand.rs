//! Purpose:
//! Eval registry entry and implementation for `rand`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - `rand()` accepts either no arguments or an inclusive min/max range.

use super::super::super::*;

eval_builtin! {
    name: "rand",
    area: Math,
    params: [min, max],
    direct: Rand,
    values: Rand,
}

/// Evaluates PHP `rand()` over zero args or an inclusive range.
pub(in crate::interpreter) fn eval_builtin_rand(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_rand_result(None, None, values),
        [min, max] => {
            let min = eval_expr(min, context, scope, values)?;
            let max = eval_expr(max, context, scope, values)?;
            eval_rand_result(Some(min), Some(max), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches by-value `rand()` calls after argument binding.
pub(in crate::interpreter) fn eval_rand_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => eval_rand_result(None, None, values),
        [min, max] => eval_rand_result(Some(*min), Some(*max), values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns one non-cryptographic random integer using PHP's inclusive range rules.
pub(in crate::interpreter) fn eval_rand_result(
    min: Option<RuntimeCellHandle>,
    max: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (min, max) = match (min, max) {
        (None, None) => (0, i64::from(i32::MAX)),
        (Some(min), Some(max)) => (eval_int_value(min, values)?, eval_int_value(max, values)?),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let low = min.min(max);
    let high = min.max(max);
    let width = (i128::from(high) - i128::from(low) + 1) as u128;
    let offset = (eval_random_u128() % width) as i128;
    let sampled = i128::from(low) + offset;
    let sampled = i64::try_from(sampled).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(sampled)
}
