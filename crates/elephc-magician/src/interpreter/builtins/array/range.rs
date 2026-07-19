//! Purpose:
//! Declarative eval registry entry for `range`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the integer range hook.

use super::super::super::*;

eval_builtin! {
    name: "range",
    area: Array,
    params: [start, end],
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
    let [start, end] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_range_result(*start, *end, values)
}

/// Evaluates PHP `range()` over integer-compatible start and end expressions.
pub(in crate::interpreter) fn eval_builtin_range(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [start, end] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let start = eval_expr(start, context, scope, values)?;
    let end = eval_expr(end, context, scope, values)?;
    eval_range_result(start, end, values)
}

/// Builds an inclusive ascending or descending integer `range()` result.
pub(in crate::interpreter) fn eval_range_result(
    start: RuntimeCellHandle,
    end: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let start = eval_int_value(start, values)?;
    let end = eval_int_value(end, values)?;
    let distance = if start <= end {
        end.checked_sub(start).ok_or(EvalStatus::RuntimeFatal)?
    } else {
        start.checked_sub(end).ok_or(EvalStatus::RuntimeFatal)?
    };
    let count = distance.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
    let count = usize::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?;
    let step = if start <= end { 1_i64 } else { -1_i64 };
    let mut current = start;
    let mut result = values.array_new(count)?;

    for index in 0..count {
        let key = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(key)?;
        let value = values.int(current)?;
        result = values.array_set(result, key, value)?;
        if index + 1 < count {
            current = current.checked_add(step).ok_or(EvalStatus::RuntimeFatal)?;
        }
    }
    Ok(result)
}
