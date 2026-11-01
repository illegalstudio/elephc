//! Purpose:
//! Eval registry entries and implementations for `array_replace_recursive()` and
//! `array_merge_recursive()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array` dispatch tables.
//!
//! Key details:
//! - `array_replace_recursive()` descends only where BOTH sides hold an array under the key;
//!   anything else is replaced, as `array_replace()` would.
//! - `array_merge_recursive()` appends integer keys; a string key present on both sides becomes
//!   an array of both values (a scalar is wrapped first, `null` as `[null]`), merged recursively
//!   when the new value is itself an array.
//! - The compiled program has its own implementations; interpreted code reached neither, and
//!   Carbon's `Carbon::setLocale()` merges its translation messages with
//!   `array_replace_recursive()`.

use super::super::super::*;

eval_builtin! {
    contract: "array_replace_recursive",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_merge_recursive",
    area: Array,
    direct: Array,
    values: Array,
}

/// Which recursive combination one call performs.
#[derive(Clone, Copy)]
pub(in crate::interpreter) enum ArrayRecursiveCombine {
    /// `array_replace_recursive()`.
    Replace,
    /// `array_merge_recursive()`.
    Merge,
}

impl ArrayRecursiveCombine {
    const fn name(self) -> &'static str {
        match self {
            Self::Replace => "array_replace_recursive",
            Self::Merge => "array_merge_recursive",
        }
    }
}

/// Dispatches direct eval calls for one recursive combination.
pub(in crate::interpreter) fn eval_array_recursive_declared_call(
    combine: ArrayRecursiveCombine,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_array_recursive_result(combine, &evaluated, context, values)
}

/// Dispatches evaluated-argument eval calls for one recursive combination.
pub(in crate::interpreter) fn eval_array_recursive_declared_values_result(
    combine: ArrayRecursiveCombine,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_array_recursive_result(combine, evaluated_args, context, values)
}

/// Validates the operands and folds them left to right.
fn eval_array_recursive_result(
    combine: ArrayRecursiveCombine,
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return match combine {
            // `array_merge_recursive()` with no arguments answers an empty array.
            ArrayRecursiveCombine::Merge => values.array_new(0),
            ArrayRecursiveCombine::Replace => eval_throw_argument_count_error(
                "array_replace_recursive() expects at least 1 argument, 0 given",
                context,
                values,
            ),
        };
    }
    for (position, &arg) in args.iter().enumerate() {
        if !eval_array_recursive_is_array(arg, values)? {
            let given = eval_given_type_spelling(arg, values)?;
            let message = if position == 0 {
                format!(
                    "{}(): Argument #1 ($array) must be of type array, {given} given",
                    combine.name()
                )
            } else {
                format!(
                    "{}(): Argument #{} must be of type array, {given} given",
                    combine.name(),
                    position + 1
                )
            };
            return eval_throw_type_error(&message, context, values);
        }
    }
    let capacity = values.array_len(args[0])?;
    let mut result = values.assoc_new(capacity)?;
    match combine {
        ArrayRecursiveCombine::Replace => {
            result = eval_array_replace_into(result, args[0], values)?;
            for &arg in &args[1..] {
                result = eval_array_replace_recursive_into(result, arg, values, 0)?;
            }
        }
        ArrayRecursiveCombine::Merge => {
            for &arg in args {
                result = eval_array_merge_recursive_into(result, arg, values, 0)?;
            }
        }
    }
    Ok(result)
}

/// Whether a value is a PHP array.
fn eval_array_recursive_is_array(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC))
}

/// Copies every entry of `source` into `target`, key for key.
fn eval_array_replace_into(
    mut target: RuntimeCellHandle,
    source: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(source)?;
    for position in 0..len {
        let key = values.array_iter_key(source, position)?;
        let value = values.array_get(source, key)?;
        target = values.array_set(target, key, value)?;
    }
    Ok(target)
}

/// Replaces `target`'s entries with `source`'s, descending where both hold an array.
fn eval_array_replace_recursive_into(
    mut target: RuntimeCellHandle,
    source: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if depth > 256 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(source)?;
    for position in 0..len {
        let key = values.array_iter_key(source, position)?;
        let value = values.array_get(source, key)?;
        let existing = if eval_array_recursive_has_key(target, key, values)? {
            Some(values.array_get(target, key)?)
        } else {
            None
        };
        let replacement = match existing {
            Some(existing)
                if eval_array_recursive_is_array(existing, values)?
                    && eval_array_recursive_is_array(value, values)? =>
            {
                let capacity = values.array_len(existing)?;
                let copy = values.assoc_new(capacity)?;
                let copy = eval_array_replace_into(copy, existing, values)?;
                eval_array_replace_recursive_into(copy, value, values, depth + 1)?
            }
            _ => value,
        };
        target = values.array_set(target, key, replacement)?;
    }
    Ok(target)
}

/// Merges `source` into `target`: integer keys append, a shared string key combines both values.
fn eval_array_merge_recursive_into(
    mut target: RuntimeCellHandle,
    source: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if depth > 256 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut next_key = eval_array_next_append_key(target, values)?;
    let len = values.array_len(source)?;
    for position in 0..len {
        let key = values.array_iter_key(source, position)?;
        let value = values.array_get(source, key)?;
        if values.type_tag(key)? != EVAL_TAG_STRING {
            let append_key = values.int(next_key)?;
            next_key = next_key.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
            target = values.array_set(target, append_key, value)?;
            continue;
        }
        if !eval_array_recursive_has_key(target, key, values)? {
            target = values.array_set(target, key, value)?;
            continue;
        }
        let existing = values.array_get(target, key)?;
        // php turns the existing value into an array first: an array stays one (copied, so the
        // source operand is never written through), `null` becomes `[null]`, anything else
        // `[value]`.
        let mut combined = values.assoc_new(1)?;
        if eval_array_recursive_is_array(existing, values)? {
            combined = eval_array_replace_into(combined, existing, values)?;
        } else {
            let zero = values.int(0)?;
            combined = values.array_set(combined, zero, existing)?;
        }
        combined = if eval_array_recursive_is_array(value, values)? {
            eval_array_merge_recursive_into(combined, value, values, depth + 1)?
        } else {
            let next = eval_array_next_append_key(combined, values)?;
            let append_key = values.int(next)?;
            values.array_set(combined, append_key, value)?
        };
        target = values.array_set(target, key, combined)?;
    }
    Ok(target)
}

/// Whether `array` holds `key`, as `array_key_exists()` answers it.
fn eval_array_recursive_has_key(
    array: RuntimeCellHandle,
    key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let exists = values.array_key_exists(key, array)?;
    values.truthy(exists)
}

/// Returns the key php's `$array[] = ...` would use: one past the largest integer key, or 0.
fn eval_array_next_append_key(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let mut next = 0i64;
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? == EVAL_TAG_INT {
            let key = eval_int_value(key, values)?;
            if key >= next {
                next = key.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
            }
        }
    }
    Ok(next)
}
