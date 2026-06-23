//! Purpose:
//! Implements small core builtins that are tightly coupled to interpreter expression helpers.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//!
//! Key details:
//! - These helpers are kept out of large domain builtin files because they are short and rely on core eval traversal.

use super::*;

/// Evaluates the builtin `strlen(...)` for one PHP-coerced string argument.
pub(in crate::interpreter) fn eval_builtin_strlen(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let bytes = values.string_bytes(value)?;
    let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(len)
}

/// Evaluates the builtin `ord(...)` for the first byte of one coerced string.
pub(in crate::interpreter) fn eval_builtin_ord(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_ord_result(value, values)
}

/// Returns the first byte of one converted string, or zero for an empty string.
pub(in crate::interpreter) fn eval_ord_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    values.int(i64::from(bytes.first().copied().unwrap_or(0)))
}

/// Evaluates the builtin `count(...)` for arrays and `Countable` objects.
pub(in crate::interpreter) fn eval_builtin_count(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_count_result(value, None, context, values)
        }
        [value, mode] => {
            let value = eval_expr(value, context, scope, values)?;
            let mode = eval_expr(mode, context, scope, values)?;
            eval_count_result(value, Some(mode), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Counts an eval array or dispatches top-level `Countable` objects.
pub(in crate::interpreter) fn eval_count_result(
    value: RuntimeCellHandle,
    mode: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mode = match mode {
        Some(mode) => eval_int_value(mode, values)?,
        None => EVAL_COUNT_NORMAL,
    };
    if !matches!(mode, EVAL_COUNT_NORMAL | EVAL_COUNT_RECURSIVE) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if values.type_tag(value)? == EVAL_TAG_OBJECT
        && eval_countable_object_matches(value, context, values)?
    {
        return eval_method_call_result(value, "count", Vec::new(), context, values);
    }
    let len = match mode {
        EVAL_COUNT_NORMAL => values.array_len(value)?,
        EVAL_COUNT_RECURSIVE => eval_count_recursive_len(value, values, &mut Vec::new())?,
        _ => unreachable!("count mode was validated before dispatch"),
    };
    let len = i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(len)
}

/// Returns whether an object value satisfies PHP's `Countable` interface.
fn eval_countable_object_matches(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    dynamic_object_is_a(value, "Countable", false, context, values)?
        .map_or_else(|| values.object_is_a(value, "Countable", false), Ok)
}

/// Recursively counts nested eval arrays for `count($value, COUNT_RECURSIVE)`.
pub(in crate::interpreter) fn eval_count_recursive_len(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    arrays_seen: &mut Vec<usize>,
) -> Result<usize, EvalStatus> {
    let address = value.as_ptr() as usize;
    if arrays_seen.contains(&address) {
        return Ok(0);
    }
    arrays_seen.push(address);

    let len = values.array_len(value)?;
    let mut total = len;
    for position in 0..len {
        let key = values.array_iter_key(value, position)?;
        let element = values.array_get(value, key)?;
        if values.is_array_like(element)? {
            total = total
                .checked_add(eval_count_recursive_len(element, values, arrays_seen)?)
                .ok_or(EvalStatus::RuntimeFatal)?;
        }
    }

    arrays_seen.pop();
    Ok(total)
}
