//! Purpose:
//! Declarative eval registry entry and implementation for `array_replace`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - php's own signature is variadic (`array_replace(array $array, array ...$replacements):
//!   array`), but the shared contract declares exactly two fixed `Mixed` params with no
//!   `variadic` spec, because the compiled/AOT `check` hook and lowering both hard-code arity 2
//!   (`src/builtins/array/array_replace.rs`, `src/codegen/lower_inst/builtins/arrays/
//!   misc_dispatch.rs`). The INTERPRETER is free of that constraint:
//!   `eval_declared_builtin_direct_call()` calls a direct hook with whatever args were written,
//!   with no min/max gate from the contract, so this hook accepts 1..N arrays directly, matching
//!   php, without touching the contract or the compiled side.
//! - Int keys are MATCHED, never renumbered -- the opposite of `array_merge`'s rule. The result's
//!   tag must follow the FINAL key sequence (`EVAL_TAG_ARRAY` only when the keys are exactly
//!   `0,1,...,n-1` in that order), not any input's representation, or `json_encode` picks the
//!   wrong bracket shape for a case like `array_replace([1,2,3],[4,5])` (php: `[4,5,3]`).

use super::super::super::*;

eval_builtin! {
    contract: "array_replace",
    area: Array,
    direct: Array,
    values: Array,
}

/// Dispatches direct eval calls for the `array_replace` array builtin.
pub(in crate::interpreter) fn eval_array_replace_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return eval_throw_argument_count_error(
            "array_replace() expects at least 1 argument, 0 given",
            context,
            values,
        );
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_array_replace_result(&evaluated, context, values)
}

/// Dispatches evaluated-argument eval calls for the `array_replace` array builtin.
pub(in crate::interpreter) fn eval_array_replace_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.is_empty() {
        return eval_throw_argument_count_error(
            "array_replace() expects at least 1 argument, 0 given",
            context,
            values,
        );
    }
    eval_array_replace_result(evaluated_args, context, values)
}

/// Validates every argument is an array (php's left-to-right, eager `TypeError` order) and folds
/// them left to right, later arrays overwriting matching keys in place and appending new ones.
fn eval_array_replace_result(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut capacity = 0usize;
    for (position, &arg) in args.iter().enumerate() {
        if !matches!(values.type_tag(arg)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            let given = eval_given_type_spelling(arg, values)?;
            // php names the first parameter (`Argument #1 ($array)`); every variadic slot after
            // it is anonymous (`Argument #N`), which is php's own variadic-error asymmetry.
            let message = if position == 0 {
                format!(
                    "array_replace(): Argument #1 ($array) must be of type array, {given} given"
                )
            } else {
                format!(
                    "array_replace(): Argument #{} must be of type array, {given} given",
                    position + 1
                )
            };
            return eval_throw_type_error(&message, context, values);
        }
        capacity = capacity.saturating_add(values.array_len(arg)?);
    }

    let mut result = values.assoc_new(capacity)?;
    for &arg in args {
        let len = values.array_len(arg)?;
        for position in 0..len {
            let key = values.array_iter_key(arg, position)?;
            let value = values.array_get(arg, key)?;
            result = values.array_set(result, key, value)?;
        }
    }

    eval_array_replace_retag_as_list_if_packed(result, values)
}

/// Re-tags a fold result as a packed list when its final key sequence is exactly `0..len`, in
/// that order -- `array_replace` builds through `assoc_new` because it must preserve arbitrary
/// key order, but php's own list-shape (and therefore `json_encode`'s `[...]` vs `{...}`) is
/// decided by the KEYS, not by any builder's internal tag.
fn eval_array_replace_retag_as_list_if_packed(
    assoc: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(assoc)?;
    for position in 0..len {
        let key = values.array_iter_key(assoc, position)?;
        let is_sequential_int = values.type_tag(key)? == EVAL_TAG_INT
            && values.raw_value_word(key)? as i64 == position as i64;
        if !is_sequential_int {
            return Ok(assoc);
        }
    }
    let mut list = values.array_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(assoc, position)?;
        let value = values.array_get(assoc, key)?;
        list = values.array_set(list, key, value)?;
    }
    values.release(assoc)?;
    Ok(list)
}
