//! Purpose:
//! Eval registry entries and implementations for php 8.4's predicate walks: `array_find`,
//! `array_find_key`, `array_any` and `array_all`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array` dispatch tables.
//!
//! Key details:
//! - php calls the predicate with the value AND the key, over any array, and stops at the first
//!   element that decides the answer.
//! - The compiled program gets these from the compiler (`array_find_prelude` for the walks,
//!   `array_find_key` as a prelude-provided function), which interpreted code cannot reach: on an
//!   8.4+ profile `symfony/polyfill-php84` declares nothing, and Laravel's interpreted `Arr` and
//!   `Collection` call them directly.

use super::super::super::*;

eval_builtin! {
    contract: "array_find",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_find_key",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_any",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_all",
    area: Array,
    direct: Array,
    values: Array,
}

/// Which answer one predicate walk produces.
#[derive(Clone, Copy)]
pub(in crate::interpreter) enum ArrayPredicateWalk {
    /// `array_find()`: the first accepted value, or null.
    FindValue,
    /// `array_find_key()`: the first accepted key, or null.
    FindKey,
    /// `array_any()`: whether any element is accepted.
    Any,
    /// `array_all()`: whether every element is accepted.
    All,
}

/// Dispatches direct eval calls for one predicate walk.
pub(in crate::interpreter) fn eval_array_predicate_walk_declared_call(
    walk: ArrayPredicateWalk,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, callback] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let callback = eval_expr(callback, context, scope, values)?;
    eval_array_predicate_walk_result(walk, array, callback, Some(scope), context, values)
}

/// Dispatches evaluated-argument eval calls for one predicate walk.
pub(in crate::interpreter) fn eval_array_predicate_walk_declared_values_result(
    walk: ArrayPredicateWalk,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, callback] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_array_predicate_walk_result(walk, *array, *callback, None, context, values)
}

/// Walks the array in iteration order, calling the predicate with `(value, key)` until the
/// answer is decided.
fn eval_array_predicate_walk_result(
    walk: ArrayPredicateWalk,
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // A callable array's receiver read (`[$object, 'method']`) is an owner the dispatch
    // borrows for the whole walk; it is released once, after the last callback.
    let (callback, callback_receiver) =
        eval_callable_for_probe(callback, context, lexical_scope, values)?;
    let result = (|| -> Result<RuntimeCellHandle, EvalStatus> {
        let len = values.array_len(array)?;
        for position in 0..len {
            let key = values.array_iter_key(array, position)?;
            let value = values.array_get(array, key)?;
            let verdict =
                eval_evaluated_callable_with_values(&callback, vec![value, key], context, values)?;
            let accepted = values.truthy(verdict);
            values.release(verdict)?;
            let accepted = accepted?;
            // Exactly one of this iteration's key and value reads may leave as the answer; every
            // other one is released here, where it used to leak two cells per visited element.
            match (walk, accepted) {
                (ArrayPredicateWalk::FindValue, true) => {
                    values.release(key)?;
                    return Ok(value);
                }
                (ArrayPredicateWalk::FindKey, true) => {
                    values.release(value)?;
                    return Ok(key);
                }
                _ => {}
            }
            values.release(key)?;
            values.release(value)?;
            match (walk, accepted) {
                (ArrayPredicateWalk::Any, true) => return values.bool_value(true),
                (ArrayPredicateWalk::All, false) => return values.bool_value(false),
                _ => {}
            }
        }
        match walk {
            ArrayPredicateWalk::FindValue | ArrayPredicateWalk::FindKey => values.null(),
            ArrayPredicateWalk::Any => values.bool_value(false),
            ArrayPredicateWalk::All => values.bool_value(true),
        }
    })();
    eval_release_callable_receiver(result, callback_receiver, values)
}
