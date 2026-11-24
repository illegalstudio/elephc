//! Purpose:
//! Eval registry entry and implementation for `preg_grep`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Mirrors the AOT prelude (`crate::pcre_prelude` in the compiler): the pattern is
//!   compiled once, each entry is matched as its string value and keeps its key, and a
//!   match error stops the scan. Before PHP 8.6 the call returns the entries collected so
//!   far; from 8.6 (php-src GH-11936) any error left in `preg_last_error()` makes it `false`.
//! - Each match runs against a cleared error slot, so a nested `preg_*` error raised by an
//!   entry's `__toString()` neither stops the scan nor counts as this call's own error; it
//!   is put back afterwards and stays visible through `preg_last_error()`, like php-src.
//! - An entry's string value is PHP's string conversion: an object runs `__toString()` (or
//!   throws PHP's conversion `Error`), and an array warns and matches as `"Array"`.
//! - Entry keys and values are leased temporaries, and a throwing `__toString()` releases
//!   the partial result before the exception propagates.
//! - Unlike the other eval `preg_*` builtins, which treat a malformed pattern as a runtime
//!   fatal, an invalid pattern returns `false` with `PREG_INTERNAL_ERROR`, like the AOT
//!   prelude and PHP. A valid modifier eval does not implement (`A`, `D`, `S`, `X`, `J`,
//!   `n`) stays the explicit runtime error the other eval `preg_*` builtins raise.

use super::super::super::*;
use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// `PREG_GREP_INVERT`: keep the entries that do not match.
const EVAL_PREG_GREP_INVERT: i64 = 1;

eval_builtin! {
    contract: "preg_grep",
    area: Regex,
    direct: PregGrep,
    values: PregGrep,
}

/// Evaluates PHP `preg_grep()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_grep(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let operands = args.iter().collect::<Vec<_>>();
    with_eval_operands(&operands, context, scope, values, |operands, context, _, values| {
        eval_preg_grep_values_result(operands, context, values)
    })
}

/// Dispatches by-value `preg_grep()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_grep_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (pattern, array, flags) = match evaluated_args {
        [pattern, array] => (*pattern, *array, None),
        [pattern, array, flags] => (*pattern, *array, Some(*flags)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let flags = flags.map_or(Ok(0), |flags| eval_int_value(flags, values))?;
    eval_preg_grep_result(pattern, array, flags & EVAL_PREG_GREP_INVERT != 0, context, values)
}

/// Returns the entries of `array` whose string value matches (or, inverted, does not).
pub(in crate::interpreter) fn eval_preg_grep_result(
    pattern: RuntimeCellHandle,
    array: RuntimeCellHandle,
    invert: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = match eval_preg_callback_pattern(pattern, values)? {
        EvalPregCallbackPattern::Compiled(regex) => regex,
        // An invalid pattern is PHP `false` with `PREG_INTERNAL_ERROR`, not a fatal.
        EvalPregCallbackPattern::Invalid => return values.bool_value(false),
        EvalPregCallbackPattern::UnsupportedModifier => return Err(EvalStatus::RuntimeFatal),
    };
    let len = values.array_len(array)?;
    let result = RuntimeOwnerSlot::new(values.assoc_new(len)?);
    with_runtime_owners(&result, || {
        let filled = eval_preg_grep_fill(&regex, array, len, invert, &result, context, values);
        // PHP 8.6 (php-src GH-11936) returns false when the scan leaves an error.
        if filled.is_ok() && eval_preg_last_error_code() != 0
            && crate::eval_php_profile::eval_php_version_id() >= 80600
        {
            eval_release_value(context, values, result.take())?;
            return values.bool_value(false);
        }
        result.finish(filled, |cell| eval_release_value(context, values, cell))
    })
}

/// Copies the matching entries of `array` into `result`, stopping at a match error.
///
/// Each key and value read is a leased temporary released on every exit; `array_set`
/// retains what it stores.
fn eval_preg_grep_fill(
    regex: &Regex,
    array: RuntimeCellHandle,
    len: usize,
    invert: bool,
    result: &RuntimeOwnerSlot,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let mut failed = false;
        with_eval_value_lease(key, context, values, |key, context, values| {
            let value = values.array_get(array, key)?;
            with_eval_value_lease(value, context, values, |value, context, values| {
                let subject = eval_preg_grep_subject(value.borrowed(), context, values)?;
                // `__toString()` may have run a failing `preg_*` call: match against a
                // cleared slot and put that nested error back unless this match fails.
                let nested_error = eval_preg_last_error_code();
                eval_preg_set_last_error(0);
                let matched = regex.is_match(&subject);
                failed = eval_preg_failed();
                if !failed {
                    eval_preg_set_last_error(nested_error);
                }
                if !failed && matched != invert {
                    result.update_receiver(values.array_set(result.borrowed(), key, value)?);
                }
                Ok(())
            })
        })?;
        if failed {
            break;
        }
    }
    Ok(())
}

/// Converts one entry to the bytes PHP matches: `__toString()` for objects (PHP's `Error`
/// without one), `"Array"` with PHP's warning for arrays, the scalar string otherwise.
fn eval_preg_grep_subject(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let tag = values.type_tag(value)?;
    if matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        values.warning("Warning: Array to string conversion\n")?;
        return Ok(b"Array".to_vec());
    }
    if tag == EVAL_TAG_CALLABLE {
        return eval_throw_error(
            "Object of class Closure could not be converted to string",
            context,
            values,
        );
    }
    if tag == EVAL_TAG_STRING {
        return values.string_bytes(value);
    }
    // Both the `__toString()` result and the scalar cast are temporaries this call owns.
    let converted = if tag == EVAL_TAG_OBJECT {
        eval_string_context_value(value, context, values)?
    } else {
        values.cast_string(value)?
    };
    let bytes = values.string_bytes(converted);
    values.release(converted)?;
    bytes
}
