//! Purpose:
//! Eval registry entry and implementation for `clamp`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Bounds are validated before comparison and NaN bounds are runtime fatals.

use super::super::super::*;

eval_builtin! {
    name: "clamp",
    area: Math,
    params: [value, min, max],
    direct: Clamp,
    values: Clamp,
}

/// Evaluates PHP `clamp()` over three eval expressions.
pub(in crate::interpreter) fn eval_builtin_clamp(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value, min, max] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let min = eval_expr(min, context, scope, values)?;
    let max = eval_expr(max, context, scope, values)?;
    eval_clamp_result(value, min, max, values)
}

/// Selects the inclusive clamp result after validating bound order and NaN bounds.
pub(in crate::interpreter) fn eval_clamp_result(
    value: RuntimeCellHandle,
    min: RuntimeCellHandle,
    max: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_clamp_bound_is_nan(min, values)? || eval_clamp_bound_is_nan(max, values)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let invalid_bounds = values.compare(EvalBinOp::Gt, min, max)?;
    if values.truthy(invalid_bounds)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let above_max = values.compare(EvalBinOp::Gt, value, max)?;
    if values.truthy(above_max)? {
        return Ok(max);
    }
    let below_min = values.compare(EvalBinOp::Lt, value, min)?;
    if values.truthy(below_min)? {
        return Ok(min);
    }
    Ok(value)
}

/// Returns whether a clamp bound is a floating-point NaN value.
fn eval_clamp_bound_is_nan(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_FLOAT {
        return Ok(false);
    }
    Ok(eval_float_value(value, values)?.is_nan())
}
