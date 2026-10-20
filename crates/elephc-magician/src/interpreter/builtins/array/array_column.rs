//! Purpose:
//! Declarative eval registry entry and implementation for `array_column`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - PHP 8 semantics: optional `$index_key`, a `null` `$column_key` selecting whole rows
//!   (scalar elements included), integer column keys, rows lacking the column skipped,
//!   and rows lacking the index key appended with the result's next integer key.
//! - Object rows expose the properties `get_object_vars()` would return from the current
//!   eval scope; `__get`/`__isset` magic is not consulted.
//! - Array or object keys raise PHP's TypeErrors with `zend_zval_value_name()` spelling.

use super::super::super::*;
use super::super::symbols::eval_get_object_vars_result;

eval_builtin! {
    contract: "array_column",
    area: Array,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_column` array builtin.
pub(in crate::interpreter) fn eval_array_column_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_column(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_column` array builtin.
pub(in crate::interpreter) fn eval_array_column_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array, column_key] => eval_array_column_result(*array, *column_key, None, context, values),
        [array, column_key, index_key] => {
            eval_array_column_result(*array, *column_key, Some(*index_key), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `array_column()` over row-array, column-key and optional index-key expressions.
pub(in crate::interpreter) fn eval_builtin_array_column(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array, column_key] => {
            let array = eval_expr(array, context, scope, values)?;
            let column_key = eval_expr(column_key, context, scope, values)?;
            eval_array_column_result(array, column_key, None, context, values)
        }
        [array, column_key, index_key] => {
            let array = eval_expr(array, context, scope, values)?;
            let column_key = eval_expr(column_key, context, scope, values)?;
            let index_key = eval_expr(index_key, context, scope, values)?;
            eval_array_column_result(array, column_key, Some(index_key), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds `array_column()`: extracts each row's column (or the row itself) and keys it by the
/// row's index value when one is requested and present.
pub(in crate::interpreter) fn eval_array_column_result(
    array: RuntimeCellHandle,
    column_key: RuntimeCellHandle,
    index_key: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        let given = eval_given_type_name(array, context, values)?;
        return eval_throw_type_error(
            &format!("array_column(): Argument #1 ($array) must be of type array, {given} given"),
            context,
            values,
        );
    }
    for (key, position, name) in [(Some(column_key), 2, "column_key"), (index_key, 3, "index_key")] {
        let Some(key) = key else { continue };
        if matches!(values.type_tag(key)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT) {
            let given = eval_given_type_name(key, context, values)?;
            return eval_throw_type_error(
                &format!(
                    "array_column(): Argument #{position} (${name}) must be of type string|int|null, {given} given"
                ),
                context,
                values,
            );
        }
    }
    let whole_rows = values.type_tag(column_key)? == EVAL_TAG_NULL;
    let index_key = match index_key {
        Some(index_key) if values.type_tag(index_key)? != EVAL_TAG_NULL => Some(index_key),
        _ => None,
    };
    let len = values.array_len(array)?;
    let mut result = if index_key.is_some() { values.assoc_new(len)? } else { values.array_new(len)? };
    for position in 0..len {
        let row = values.array_iter_value(array, position)?;
        let added = eval_array_column_add_row(
            result, row, column_key, whole_rows, index_key, context, values,
        );
        let released = values.release(row);
        match added.and_then(|next| released.map(|()| next)) {
            Ok(next) => result = next,
            Err(status) => {
                let _ = values.release(result);
                return Err(status);
            }
        }
    }
    Ok(result)
}

/// Adds one source element to the result, returning the (possibly moved) result cell.
fn eval_array_column_add_row(
    result: RuntimeCellHandle,
    row: RuntimeCellHandle,
    column_key: RuntimeCellHandle,
    whole_rows: bool,
    index_key: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let lookup = match values.type_tag(row)? {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => Some(values.retain(row)?),
        EVAL_TAG_OBJECT => Some(eval_get_object_vars_result(&[row], context, values)?),
        _ if whole_rows => None,
        _ => return Ok(result),
    };
    let added = eval_array_column_add_lookup(
        result, row, lookup, column_key, whole_rows, index_key, context, values,
    );
    if let Some(lookup) = lookup {
        values.release(lookup)?;
    }
    added
}

/// Reads the column and index value from a row's key/value view and stores the pair.
#[allow(clippy::too_many_arguments)]
fn eval_array_column_add_lookup(
    result: RuntimeCellHandle,
    row: RuntimeCellHandle,
    lookup: Option<RuntimeCellHandle>,
    column_key: RuntimeCellHandle,
    whole_rows: bool,
    index_key: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = if whole_rows {
        values.retain(row)?
    } else {
        let Some(lookup) = lookup else { return Ok(result) };
        match eval_array_column_read(lookup, column_key, values)? {
            Some(value) => value,
            None => return Ok(result),
        }
    };
    let key = match (index_key, lookup) {
        (Some(index_key), Some(lookup)) => eval_array_column_read(lookup, index_key, values)?,
        _ => None,
    };
    let stored = eval_array_column_store(result, key, value, context, values);
    let released_value = values.release(value);
    let released_key = match key {
        Some(key) => values.release(key),
        None => Ok(()),
    };
    let stored = stored?;
    released_value?;
    released_key?;
    Ok(stored)
}

/// Returns an owned read of `key` when the row view contains it (a stored `null` counts).
fn eval_array_column_read(
    lookup: RuntimeCellHandle,
    key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let exists = values.array_key_exists(key, lookup)?;
    let present = values.truthy(exists);
    values.release(exists)?;
    if !present? {
        return Ok(None);
    }
    values.array_get(lookup, key).map(Some)
}

/// Stores one value under its index value, or appends it with the next integer key.
fn eval_array_column_store(
    result: RuntimeCellHandle,
    key: Option<RuntimeCellHandle>,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(key) = key {
        if matches!(values.type_tag(key)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT) {
            let given = eval_given_type_name(key, context, values)?;
            return eval_throw_type_error(
                &format!("Cannot access offset of type {given} on array"),
                context,
                values,
            );
        }
        return values.array_set(result, key, value);
    }
    let next = values.array_next_index(result)?.ok_or(EvalStatus::RuntimeFatal)?;
    let target_key = values.int(next)?;
    let stored = values.array_set(result, target_key, value);
    values.release(target_key)?;
    stored
}
