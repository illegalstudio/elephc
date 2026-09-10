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
    contract: "clamp",
    area: Math,
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
    with_eval_operands(
        &[value, min, max],
        context,
        scope,
        values,
        |args, _, _, values| eval_clamp_result(args[0], args[1], args[2], values),
    )
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
    if eval_comparison_condition(EvalBinOp::Gt, min, max, values)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    if eval_comparison_condition(EvalBinOp::Gt, value, max, values)? {
        return Ok(max);
    }
    if eval_comparison_condition(EvalBinOp::Lt, value, min, values)? {
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
