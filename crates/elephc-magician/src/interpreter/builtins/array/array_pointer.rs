//! Purpose:
//! Shared PHP internal array pointer state and moves behind `key`, `current`,
//! `next`, `prev`, `reset`, and `end`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array` internal-pointer builtin owners.
//! - `crate::interpreter::builtins::array::mutating_dispatch`.
//! - `crate::interpreter::builtins::registry::dynamic_mutation`.
//!
//! Key details:
//! - The pointer is a cursor over the array's iteration order, stored per runtime
//!   array cell in the eval context because runtime cells carry no `zend_array`
//!   internal position of their own.
//! - PHP has exactly one invalid state: once the cursor runs off either end it
//!   stays invalid until `reset()` or `end()` recovers it.
//! - The by-reference movers persist the moved cursor; by-value callable dispatch
//!   computes the same move over PHP's temporary copy and leaves the source alone.

use super::super::super::*;

/// Returns the addressable internal pointer position for one array cell, if any.
pub(in crate::interpreter) fn eval_array_pointer_position(
    array: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<usize>, EvalStatus> {
    let len = values.array_len(array)?;
    Ok(match context.array_cursor(array) {
        EvalArrayCursor::Position(position) if position < len => Some(position),
        _ => None,
    })
}

/// Returns the moved cursor and PHP-visible result for one internal pointer mover.
pub(in crate::interpreter) fn eval_array_pointer_move(
    name: &str,
    array: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(EvalArrayCursor, RuntimeCellHandle), EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(array)?;
    let position = eval_array_pointer_position(array, context, values)?;
    let moved = match name {
        "reset" => (len > 0).then_some(0),
        "end" => len.checked_sub(1),
        "next" => position
            .and_then(|position| position.checked_add(1))
            .filter(|moved| *moved < len),
        "prev" => position.and_then(|position| position.checked_sub(1)),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let Some(moved) = moved else {
        let result = values.bool_value(false)?;
        return Ok((EvalArrayCursor::Invalid, result));
    };
    let value = eval_array_pointer_value(array, moved, values)?;
    Ok((EvalArrayCursor::Position(moved), value))
}

/// Reads the array value stored at one internal pointer position.
fn eval_array_pointer_value(
    array: RuntimeCellHandle,
    position: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.array_iter_key(array, position)?;
    values.array_get(array, key)
}

/// Evaluates direct by-reference internal pointer calls and stores the moved cursor.
pub(in crate::interpreter) fn eval_array_pointer_declared_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let (array, _target) =
        super::mutation::eval_array_mutation_lvalue_arg(name, arg, context, scope, values)?;
    let (cursor, result) = eval_array_pointer_move(name, array, context, values)?;
    context.set_array_cursor(array, cursor);
    Ok(result)
}

/// Evaluates by-value callable internal pointer calls without moving the source cursor.
pub(in crate::interpreter) fn eval_array_pointer_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    super::array_pop::eval_warn_array_by_value(name, values)?;
    let (_cursor, result) = eval_array_pointer_move(name, *array, context, values)?;
    Ok(result)
}
