//! Purpose:
//! Eval registry entry and implementation for `substr_count`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Non-overlapping byte scan: after a hit the cursor advances by `strlen($needle)`, matching
//!   php-src exactly (`substr_count("aaaa","aa") === 2`, never 3).
//! - Validation order is fixed and OBSERVABLE in php: `$needle` empty, THEN `$offset` bounds,
//!   THEN `$length` bounds -- each its own catchable `ValueError`. A call with two bad arguments
//!   reports only the first (`php -n` 8.5.6, measured).
//! - `$length`'s contract type is `Mixed` (not `?int`) specifically so a RUN-TIME `null` cell can
//!   be told apart from an omitted argument by inspecting its tag rather than folding both to a
//!   static "was a length given?" test the way the compiled backend's
//!   `substr_count_has_length()` does -- that static test is the one place the compiled backend
//!   is WRONG (`src/codegen/lower_inst/builtins/strings/search.rs:548`): a `?int $length` that is
//!   `null` at run time reaches `load_as_int()` and becomes `0` instead of "to the end". This
//!   implementation must not copy that.

use super::super::super::*;

eval_builtin! {
    contract: "substr_count",
    area: String,
    direct: SubstrCount,
    values: SubstrCount,
}

/// Evaluates PHP `substr_count(...)` from unevaluated call-site expressions.
pub(in crate::interpreter) fn eval_builtin_substr_count(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return eval_throw_argument_count_error(
            &format!(
                "substr_count() expects at least 2 arguments, {} given",
                args.len()
            ),
            context,
            values,
        );
    }
    if args.len() > 4 {
        return eval_throw_argument_count_error(
            &format!(
                "substr_count() expects at most 4 arguments, {} given",
                args.len()
            ),
            context,
            values,
        );
    }
    let haystack = eval_expr(&args[0], context, scope, values)?;
    let needle = eval_expr(&args[1], context, scope, values)?;
    let offset = match args.get(2) {
        Some(expr) => Some(eval_expr(expr, context, scope, values)?),
        None => None,
    };
    let length = match args.get(3) {
        Some(expr) => Some(eval_expr(expr, context, scope, values)?),
        None => None,
    };
    eval_substr_count_result(haystack, needle, offset, length, context, values)
}

/// Evaluates PHP `substr_count(...)` from already-evaluated argument cells (the named/spread and
/// dynamic-call route).
pub(in crate::interpreter) fn eval_substr_count_values(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [haystack, needle] => {
            eval_substr_count_result(*haystack, *needle, None, None, context, values)
        }
        [haystack, needle, offset] => {
            eval_substr_count_result(*haystack, *needle, Some(*offset), None, context, values)
        }
        [haystack, needle, offset, length] => eval_substr_count_result(
            *haystack,
            *needle,
            Some(*offset),
            Some(*length),
            context,
            values,
        ),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `substr_count()` over already-evaluated arguments, applying php's exact
/// validation order and offset/length normalization rules.
fn eval_substr_count_result(
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    offset: Option<RuntimeCellHandle>,
    length: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack_bytes =
        eval_require_string_arg(haystack, "substr_count", 1, "haystack", context, values)?;
    let needle_bytes =
        eval_require_string_arg(needle, "substr_count", 2, "needle", context, values)?;
    if needle_bytes.is_empty() {
        return eval_throw_builtin_value_error(
            "substr_count(): Argument #2 ($needle) must not be empty",
            context,
            values,
        );
    }

    let haystack_len = haystack_bytes.len() as i64;

    let mut offset_value = match offset {
        Some(offset) => eval_require_int_arg(offset, "substr_count", 3, "offset", context, values)?,
        None => 0,
    };
    if offset_value < 0 {
        offset_value += haystack_len;
    }
    if offset_value < 0 || offset_value > haystack_len {
        return eval_throw_builtin_value_error(
            "substr_count(): Argument #3 ($offset) must be contained in argument #1 ($haystack)",
            context,
            values,
        );
    }

    // A run-time NULL-tagged `$length` cell means "omitted", exactly like a genuinely absent
    // argument -- unlike the compiled backend's static-only test, this reads the cell itself.
    let length_given = match length {
        Some(length) if values.type_tag(length)? != EVAL_TAG_NULL => Some(length),
        _ => None,
    };
    let mut length_value = match length_given {
        Some(length) => eval_require_int_arg(length, "substr_count", 4, "length", context, values)?,
        None => haystack_len - offset_value,
    };
    if length_value < 0 {
        // Measured from the SUBJECT'S END, not from the offset.
        length_value += haystack_len - offset_value;
    }
    if length_value < 0 || offset_value + length_value > haystack_len {
        return eval_throw_builtin_value_error(
            "substr_count(): Argument #4 ($length) must be contained in argument #1 ($haystack)",
            context,
            values,
        );
    }

    let window_start = offset_value as usize;
    let window_end = (offset_value + length_value) as usize;
    let window = &haystack_bytes[window_start..window_end];
    let count = eval_substr_count_non_overlapping(window, &needle_bytes);
    values.int(count)
}

/// Counts non-overlapping occurrences of `needle` in `haystack`, advancing past each hit by the
/// needle's own length so `"aaaa"` contains `"aa"` twice, never three times.
fn eval_substr_count_non_overlapping(haystack: &[u8], needle: &[u8]) -> i64 {
    let mut count = 0_i64;
    let mut cursor = 0usize;
    while cursor + needle.len() <= haystack.len() {
        if &haystack[cursor..cursor + needle.len()] == needle {
            count += 1;
            cursor += needle.len();
        } else {
            cursor += 1;
        }
    }
    count
}
