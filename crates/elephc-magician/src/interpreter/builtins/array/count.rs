//! Purpose:
//! Declarative eval registry entry and implementation for `count`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Recursive counting tracks visited arrays to avoid cycles.
//! - Top-level objects dispatch through `Countable::count()` when applicable.
//! - Direct calls consume their temporary operands; value hooks borrow cells the
//!   caller already owns, and recursive counting retires every key and element it reads.

use super::super::super::*;

eval_builtin! {
    contract: "count",
    area: Array,
    direct: Count,
    values: Count,
}
/// Dispatches direct eval calls for the `count` array builtin.
pub(in crate::interpreter) fn eval_count_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_count(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `count` array builtin.
pub(in crate::interpreter) fn eval_count_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [value] => eval_count_result(*value, None, context, values),
        [value, mode] => eval_count_result(*value, Some(*mode), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates the builtin `count(...)` for arrays and `Countable` objects.
///
/// Direct dispatch hands this hook unevaluated expressions and releases nothing on its
/// behalf, so the operands are taken through `with_eval_operands`: a borrowed storage read
/// acquires a lease, an owned temporary such as `count($table["items"])` keeps its single
/// owner, and both are retired on the success and failure paths.
pub(in crate::interpreter) fn eval_builtin_count(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => with_eval_operands(&[value], context, scope, values, |args, context, _, values| {
            eval_count_result(args[0], None, context, values)
        }),
        [value, mode] => {
            with_eval_operands(&[value, mode], context, scope, values, |args, context, _, values| {
                eval_count_result(args[0], Some(args[1]), context, values)
            })
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
///
/// The boxed key and the fetched element are both owned results of the runtime bridge, so
/// each is released before the next position is read, including when the nested count or a
/// cleanup call fails.
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
        let element = values.array_get(value, key);
        let key_released = values.release(key);
        let element = match (element, key_released) {
            (Ok(element), Ok(())) => element,
            (Ok(element), Err(status)) => {
                let _ = values.release(element);
                return Err(status);
            }
            (Err(status), _) => return Err(status),
        };
        let nested = (|| {
            if values.is_array_like(element)? {
                eval_count_recursive_len(element, values, arrays_seen)
            } else {
                Ok(0)
            }
        })();
        let element_released = values.release(element);
        total = total
            .checked_add(nested?)
            .ok_or(EvalStatus::RuntimeFatal)?;
        element_released?;
    }

    arrays_seen.pop();
    Ok(total)
}
