//! Purpose:
//! Declarative eval registry entry for `range`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the integer range hook.
//! - PHP's optional `$step` is accepted; its sign never chooses the traversal direction
//!   (`$start` vs `$end` does), matching php-src and the AOT `__rt_range` helper.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

eval_builtin! {
    contract: "range",
    area: Array,
    direct: Range,
    values: Range,
}
/// Dispatches direct eval calls for the `range` array builtin.
pub(in crate::interpreter) fn eval_range_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_range(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `range` array builtin.
pub(in crate::interpreter) fn eval_range_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ([start, end], step) = match evaluated_args {
        [start, end] => ([*start, *end], None),
        [start, end, step] => ([*start, *end], Some(*step)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_range_result(start, end, step, values)
}

/// Evaluates PHP `range()` over integer-compatible start, end, and step expressions.
pub(in crate::interpreter) fn eval_builtin_range(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (start, end, step) = match args {
        [start, end] => (start, end, None),
        [start, end, step] => (start, end, Some(step)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let start = eval_expr(start, context, scope, values)?;
    let end = eval_expr(end, context, scope, values)?;
    let step = match step {
        Some(step) => Some(eval_expr(step, context, scope, values)?),
        None => None,
    };
    eval_range_result(start, end, step, values)
}

/// Builds an inclusive ascending or descending integer `range()` result.
///
/// `step` is optional and defaults to 1. Its sign is ignored: the direction comes from the
/// endpoints, exactly like php-src. PHP's three `$step` `ValueError`s (zero step, negative step
/// on an increasing range, step wider than the spanned interval) surface as `RuntimeFatal`.
pub(in crate::interpreter) fn eval_range_result(
    start: RuntimeCellHandle,
    end: RuntimeCellHandle,
    step: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let start = eval_int_value(start, values)?;
    let end = eval_int_value(end, values)?;
    let requested_step = match step {
        Some(step) => eval_int_value(step, values)?,
        None => 1,
    };
    if requested_step == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    if start < end && requested_step < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let magnitude = requested_step.checked_abs().ok_or(EvalStatus::RuntimeFatal)?;
    let distance = if start <= end {
        end.checked_sub(start).ok_or(EvalStatus::RuntimeFatal)?
    } else {
        start.checked_sub(end).ok_or(EvalStatus::RuntimeFatal)?
    };
    if start != end && magnitude > distance {
        return Err(EvalStatus::RuntimeFatal);
    }
    let count = (distance / magnitude)
        .checked_add(1)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let count = usize::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?;
    let step = if start <= end { magnitude } else { -magnitude };
    let mut current = start;
    let mut result = EvalArrayBuilder::indexed(values, count)?;

    for index in 0..count {
        result.index(index, |values| values.int(current))?;
        if index + 1 < count {
            current = current.checked_add(step).ok_or(EvalStatus::RuntimeFatal)?;
        }
    }
    Ok(result.finish())
}
