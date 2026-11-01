//! Purpose:
//! Eval registry entry and implementation for `array_change_key_case`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - php-src's loop: integer keys are kept, string keys are ASCII-lowercased for `CASE_LOWER`
//!   (0) and ASCII-uppercased for any other `$case`, and a later key that folds onto an earlier
//!   one overwrites the earlier entry's value in place.
//! - The compiled side is the AST prelude `src/array_change_key_case_prelude.rs`; interpreted
//!   code cannot reach a compiler prelude, and Symfony's `RedirectResponse` calls it from
//!   interpreted code for every redirect it builds.

use super::super::super::*;

eval_builtin! {
    contract: "array_change_key_case",
    area: Array,
    direct: ArrayChangeKeyCase,
    values: ArrayChangeKeyCase,
}

/// Evaluates PHP `array_change_key_case(...)` from unevaluated call-site expressions.
pub(in crate::interpreter) fn eval_builtin_array_change_key_case(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_array_change_key_case_values(&evaluated, context, values)
}

/// Evaluates PHP `array_change_key_case(...)` from already-evaluated argument cells.
pub(in crate::interpreter) fn eval_array_change_key_case_values(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array, upper) = match evaluated_args {
        [array] => (*array, false),
        [array, case] => (*array, eval_int_value(*case, values)? != 0),
        _ => {
            let bound = if evaluated_args.is_empty() { "at least 1 argument" } else { "at most 2 arguments" };
            return eval_throw_argument_count_error(
                &format!(
                    "array_change_key_case() expects {bound}, {} given",
                    evaluated_args.len()
                ),
                context,
                values,
            );
        }
    };
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        let given = eval_given_type_spelling(array, values)?;
        return eval_throw_type_error(
            &format!(
                "array_change_key_case(): Argument #1 ($array) must be of type array, {given} given"
            ),
            context,
            values,
        );
    }
    let len = values.array_len(array)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let key = if values.type_tag(key)? == EVAL_TAG_STRING {
            let mut bytes = values.string_bytes(key)?;
            values.release(key)?;
            if upper {
                bytes.make_ascii_uppercase();
            } else {
                bytes.make_ascii_lowercase();
            }
            values.string_bytes_value(&bytes)?
        } else {
            key
        };
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}
