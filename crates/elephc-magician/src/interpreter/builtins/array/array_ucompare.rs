//! Purpose:
//! Eval registry entries and one shared implementation for php's comparing set functions:
//! `array_diff_assoc`, `array_intersect_assoc` and the callback family (`array_udiff`,
//! `array_udiff_assoc`, `array_udiff_uassoc`, `array_diff_ukey`, `array_diff_uassoc`,
//! `array_uintersect`, `array_uintersect_assoc`, `array_uintersect_uassoc`,
//! `array_intersect_ukey`, `array_intersect_uassoc`).
//!
//! Called from:
//! - `crate::interpreter::builtins::array` dispatch tables.
//!
//! Key details:
//! - Every variant is the same question asked with different comparisons: keep an entry of the
//!   first array when it has a match in every other array (intersect) or in none (diff). A
//!   match compares the VALUE and/or the KEY, each either php's built-in string comparison or a
//!   user callback that answers `0` for equal.
//! - Callback variants take the comparator(s) as the trailing argument(s): the value
//!   comparator first, then the key comparator, as in php.
//! - Keys and values of the first array are preserved, as php does.
//! - Symfony's `UrlGenerator::doGenerate()` calls `array_udiff_assoc()` on every generated URL;
//!   interpreted, it was an unknown function and every Twig page died on its first `path()`.

use super::super::super::*;

eval_builtin! {
    contract: "array_diff_assoc",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_intersect_assoc",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_udiff",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_udiff_assoc",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_udiff_uassoc",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_diff_ukey",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_diff_uassoc",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_uintersect",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_uintersect_assoc",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_uintersect_uassoc",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_intersect_ukey",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_intersect_uassoc",
    area: Array,
    direct: Array,
    values: Array,
}

/// How one side of an entry (its value or its key) takes part in a match.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Compare {
    /// This side is not compared.
    Ignored,
    /// php's built-in comparison: equal when the string forms are identical.
    Builtin,
    /// A user comparator that answers `0` for equal.
    User,
}

/// The shape of one function of the family.
#[derive(Clone, Copy)]
struct SetOperation {
    /// Keep entries matched in every other array (`true`) or in none (`false`).
    intersect: bool,
    value: Compare,
    key: Compare,
}

/// Returns the shape of a family member, or `None` for another name.
fn set_operation(name: &str) -> Option<SetOperation> {
    use Compare::{Builtin, Ignored, User};
    let (intersect, value, key) = match name {
        "array_diff_assoc" => (false, Builtin, Builtin),
        "array_intersect_assoc" => (true, Builtin, Builtin),
        "array_udiff" => (false, User, Ignored),
        "array_udiff_assoc" => (false, User, Builtin),
        "array_udiff_uassoc" => (false, User, User),
        "array_diff_ukey" => (false, Ignored, User),
        "array_diff_uassoc" => (false, Builtin, User),
        "array_uintersect" => (true, User, Ignored),
        "array_uintersect_assoc" => (true, User, Builtin),
        "array_uintersect_uassoc" => (true, User, User),
        "array_intersect_ukey" => (true, Ignored, User),
        "array_intersect_uassoc" => (true, Builtin, User),
        _ => return None,
    };
    Some(SetOperation {
        intersect,
        value,
        key,
    })
}

/// Returns true when `name` belongs to this family.
pub(in crate::interpreter) fn is_array_ucompare_builtin(name: &str) -> bool {
    set_operation(name).is_some()
}

/// Dispatches direct eval calls for the family.
pub(in crate::interpreter) fn eval_array_ucompare_declared_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_array_ucompare_declared_values_result(name, &evaluated, context, values)
}

/// Dispatches evaluated-argument eval calls for the family.
pub(in crate::interpreter) fn eval_array_ucompare_declared_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let operation = set_operation(name).ok_or(EvalStatus::UnsupportedConstruct)?;
    let callbacks = usize::from(operation.value == Compare::User)
        + usize::from(operation.key == Compare::User);
    // At least two arrays, then the comparators.
    if evaluated_args.len() < 2 + callbacks {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (arrays, comparators) = evaluated_args.split_at(evaluated_args.len() - callbacks);
    let mut comparators = comparators.iter();
    let value_callback = match operation.value {
        Compare::User => Some(eval_callable(
            *comparators.next().ok_or(EvalStatus::RuntimeFatal)?,
            context,
            values,
        )?),
        _ => None,
    };
    let key_callback = match operation.key {
        Compare::User => {
            let resolved = match comparators.next() {
                Some(comparator) => eval_callable(*comparator, context, values),
                None => Err(EvalStatus::RuntimeFatal),
            };
            match resolved {
                Ok(callback) => Some(callback),
                Err(status) => {
                    if let Some(value_callback) = value_callback {
                        let _ = value_callback.release(context, values);
                    }
                    return Err(status);
                }
            }
        }
        _ => None,
    };
    // Both callbacks stay leased across the whole walk, and their array receivers are retired
    // once afterwards on every status; an earlier failure keeps precedence.
    let outcome = eval_array_ucompare_walk(
        operation,
        arrays,
        value_callback.as_ref().map(ResolvedEvalCallable::callable),
        key_callback.as_ref().map(ResolvedEvalCallable::callable),
        context,
        values,
    );
    let mut cleanup = Ok(());
    for callback in [value_callback, key_callback].into_iter().flatten() {
        let released = callback.release(context, values);
        if cleanup.is_ok() {
            cleanup = released;
        }
    }
    let result = outcome?;
    cleanup?;
    Ok(result)
}

/// Walks the first array, keeping the entries the set operation retains.
fn eval_array_ucompare_walk(
    operation: SetOperation,
    arrays: &[RuntimeCellHandle],
    value_callback: Option<&EvaluatedCallable>,
    key_callback: Option<&EvaluatedCallable>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    for array in arrays {
        if !matches!(values.type_tag(*array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    let first = arrays[0];
    let others = &arrays[1..];
    let len = values.array_len(first)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(first, position)?;
        let value = values.array_get(first, key)?;
        let mut matched_everywhere = true;
        let mut matched_somewhere = false;
        for other in others {
            let matched = entry_matches(
                operation,
                key,
                value,
                *other,
                value_callback,
                key_callback,
                context,
                values,
            )?;
            matched_everywhere &= matched;
            matched_somewhere |= matched;
        }
        let keep = if operation.intersect {
            matched_everywhere
        } else {
            !matched_somewhere
        };
        if keep {
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// Returns whether `other` holds an entry matching `(key, value)` under `operation`.
#[allow(clippy::too_many_arguments)]
fn entry_matches(
    operation: SetOperation,
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
    other: RuntimeCellHandle,
    value_callback: Option<&EvaluatedCallable>,
    key_callback: Option<&EvaluatedCallable>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    // A built-in key comparison is a direct lookup, as in php.
    if operation.key == Compare::Builtin {
        let exists = values.array_key_exists(key, other)?;
        if !values.truthy(exists)? {
            return Ok(false);
        }
        let candidate = values.array_get(other, key)?;
        return values_match(operation.value, value, candidate, value_callback, context, values);
    }
    let len = values.array_len(other)?;
    for position in 0..len {
        let candidate_key = values.array_iter_key(other, position)?;
        if operation.key == Compare::User {
            let callback = key_callback.ok_or(EvalStatus::RuntimeFatal)?;
            if !user_says_equal(callback, key, candidate_key, context, values)? {
                continue;
            }
        }
        let candidate = values.array_get(other, candidate_key)?;
        if values_match(operation.value, value, candidate, value_callback, context, values)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Compares two values the way `mode` says.
fn values_match(
    mode: Compare,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    callback: Option<&EvaluatedCallable>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match mode {
        Compare::Ignored => Ok(true),
        Compare::Builtin => Ok(values.string_bytes(left)? == values.string_bytes(right)?),
        Compare::User => {
            let callback = callback.ok_or(EvalStatus::RuntimeFatal)?;
            user_says_equal(callback, left, right, context, values)
        }
    }
}

/// Calls a user comparator and reads `0` as equal.
fn user_says_equal(
    callback: &EvaluatedCallable,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let result = eval_evaluated_callable_with_values(callback, vec![left, right], context, values)?;
    Ok(eval_int_value(result, values)? == 0)
}
