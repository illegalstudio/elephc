//! Purpose:
//! Declarative eval registry entry and implementation for `array_count_values`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Only `int` and `string` elements are countable; PHP warns and skips anything
//!   else, and eval skips silently because it emits no diagnostics.
//! - Counting through `array_set` shares the selected profile's key identity with
//!   the compiled runtime. The result builder retires all temporary handles.

use super::super::super::*;

eval_builtin! {
    contract: "array_count_values",
    area: Array,
    direct: Array,
    values: Array,
}

/// Dispatches direct eval calls for the `array_count_values` array builtin.
pub(in crate::interpreter) fn eval_array_count_values_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_count_values(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_count_values` array builtin.
pub(in crate::interpreter) fn eval_array_count_values_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_array_count_values_result(*array, values)
}

/// Evaluates PHP `array_count_values()` over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_count_values(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_count_values_result(array, values)
}

/// Builds the value-to-occurrence-count map PHP's `array_count_values()` returns.
pub(in crate::interpreter) fn eval_array_count_values_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = super::super::collection_builder::EvalArrayBuilder::assoc(values, len)?;
    for position in 0..len {
        let Some(key) = super::key_inputs::countable_key_at(array, position, result.values())? else { continue; };
        let current = result.borrowed_cell();
        result.entry_key_first(|_| Ok(key), |values, key| next_count(current, key, values))?;
    }
    Ok(result.finish())
}

/// Materializes one updated count while retiring lookup and arithmetic operands.
fn next_count(array: RuntimeCellHandle, key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = values.array_key_exists(key, array)?;
    let present = values.truthy(exists);
    let released = values.release(exists);
    let present = present?; released?;
    if !present { return values.int(1); }
    let existing = values.array_get(array, key).and_then(|value| promote_borrowed_result(value, values))?;
    let next = (|| {
        let one = values.int(1)?;
        let added = values.add(existing, one).and_then(|value| promote_borrowed_result(value, values));
        let released = values.release(one);
        match (added, released) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(status), _) => Err(status),
            (Ok(value), Err(status)) => { let _ = values.release(value); Err(status) }
        }
    })();
    let released = values.release(existing);
    match (next, released) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => { let _ = values.release(value); Err(status) }
    }
}
