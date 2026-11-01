//! Purpose:
//! Eval registry entry and implementation for `substr_compare`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - The return value is php's, not a normalized sign. At the first differing byte php hands
//!   back `memcmp()`'s RAW unsigned-byte difference (`substr_compare("a","z",0) === -25`), and
//!   only the equal-prefix tiebreak is `ZEND_THREEWAY_COMPARE`'s `-1`/`0`/`1`. Callers testing
//!   `=== 0` and callers testing `< 0` both exist in the wild, so neither half may be
//!   approximated by the other. (php 8.5.10, measured.)
//! - Validation order is fixed and OBSERVABLE: every argument is COERCED first (so a
//!   `TypeError` on `$offset` beats a `ValueError` on `$length`), then `$length` is refused if
//!   negative, then `$offset` is bounds-checked. `substr_compare("abc","a",100,-1)` reports the
//!   `$length` error, never the `$offset` one.
//! - A negative `$offset` counts back from the haystack end and CLAMPS to zero when it
//!   underflows -- `substr_compare("abcdef","def",-100) === -3`, not an error. This is the
//!   opposite of `substr_count()`, which raises there, so the two must not share a guard.
//! - A negative `$length` is a `ValueError` in php 8; it is NOT measured back from the end the
//!   way `substr_count()`'s is.
//! - `$length`'s contract type is `Mixed` (not `?int`) specifically so a RUN-TIME `null` cell
//!   can be told apart from an omitted argument by inspecting its tag: a null `$length` means
//!   "compare to the end of the LONGER operand", never "compare zero bytes".
//! - Case folding is ASCII-only (`zend_tolower_ascii`), and that is a DELIBERATE, MEASURED
//!   divergence from php's default on this host. php routes the `$case_insensitive` path
//!   through `zend_binary_strncasecmp_l`, whose fold is the C library's LOCALE-dependent
//!   `tolower()`: with `setlocale(LC_CTYPE, "C")` php answers
//!   `substr_compare("\xE9","\xC9",0,1,true) === 32` (no fold, what this does), and under any
//!   real locale -- and under macOS's own startup rune table, which is what a php process that
//!   never calls `setlocale` gets here -- it answers `0`, folding all of Latin-1 `0xC0`-`0xDE`
//!   except `0xD7`. `strcasecmp()`/`strncasecmp()` are ASCII-only in php 8 and are NOT affected,
//!   which is why they must not share a fold table with this. A compiled binary has no locale
//!   to consult, so elephc pins the deterministic ASCII answer; the only inputs that can tell
//!   the two apart are RAW Latin-1 high bytes, never UTF-8 (a UTF-8 lead byte folds identically
//!   on both sides of the comparison and cancels out).

use super::super::super::*;

eval_builtin! {
    contract: "substr_compare",
    area: String,
    direct: SubstrCompare,
    values: SubstrCompare,
}

/// Evaluates PHP `substr_compare(...)` from unevaluated call-site expressions.
pub(in crate::interpreter) fn eval_builtin_substr_compare(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 3 {
        return eval_throw_argument_count_error(
            &format!(
                "substr_compare() expects at least 3 arguments, {} given",
                args.len()
            ),
            context,
            values,
        );
    }
    if args.len() > 5 {
        return eval_throw_argument_count_error(
            &format!(
                "substr_compare() expects at most 5 arguments, {} given",
                args.len()
            ),
            context,
            values,
        );
    }
    let haystack = eval_expr(&args[0], context, scope, values)?;
    let needle = eval_expr(&args[1], context, scope, values)?;
    let offset = eval_expr(&args[2], context, scope, values)?;
    let length = match args.get(3) {
        Some(expr) => Some(eval_expr(expr, context, scope, values)?),
        None => None,
    };
    let case_insensitive = match args.get(4) {
        Some(expr) => Some(eval_expr(expr, context, scope, values)?),
        None => None,
    };
    eval_substr_compare_result(
        haystack,
        needle,
        offset,
        length,
        case_insensitive,
        context,
        values,
    )
}

/// Evaluates PHP `substr_compare(...)` from already-evaluated argument cells (the named/spread
/// and dynamic-call route).
pub(in crate::interpreter) fn eval_substr_compare_values(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [haystack, needle, offset] => eval_substr_compare_result(
            *haystack, *needle, *offset, None, None, context, values,
        ),
        [haystack, needle, offset, length] => eval_substr_compare_result(
            *haystack,
            *needle,
            *offset,
            Some(*length),
            None,
            context,
            values,
        ),
        [haystack, needle, offset, length, case_insensitive] => eval_substr_compare_result(
            *haystack,
            *needle,
            *offset,
            Some(*length),
            Some(*case_insensitive),
            context,
            values,
        ),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `substr_compare()` over already-evaluated arguments, applying php's exact
/// coercion order, validation order, and offset/length normalization rules.
fn eval_substr_compare_result(
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    case_insensitive: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // php's parameter parsing coerces EVERY argument (raising TypeError) before the function
    // body validates any of them, so the coercions all happen first and in declaration order.
    let haystack_bytes =
        eval_require_string_arg(haystack, "substr_compare", 1, "haystack", context, values)?;
    let needle_bytes =
        eval_require_string_arg(needle, "substr_compare", 2, "needle", context, values)?;
    let raw_offset =
        eval_require_int_arg(offset, "substr_compare", 3, "offset", context, values)?;
    // A run-time NULL-tagged `$length` cell means "omitted", exactly like a genuinely absent
    // argument: php declares the parameter `?int $length = null` and reads null as "to the end
    // of the longer operand".
    let length_given = match length {
        Some(length) if values.type_tag(length)? != EVAL_TAG_NULL => Some(length),
        _ => None,
    };
    let length_value = match length_given {
        Some(length) => Some(eval_substr_compare_length_int(length, context, values)?),
        None => None,
    };
    let fold_case = match case_insensitive {
        Some(flag) => eval_substr_compare_case_flag(flag, context, values)?,
        None => false,
    };

    // php refuses a negative `$length` BEFORE it looks at `$offset`.
    if matches!(length_value, Some(value) if value < 0) {
        return eval_throw_builtin_value_error(
            "substr_compare(): Argument #4 ($length) must be greater than or equal to 0",
            context,
            values,
        );
    }

    let haystack_len = haystack_bytes.len() as i64;
    let mut offset_value = raw_offset;
    if offset_value < 0 {
        // `saturating_add` only differs from php's wrapping C addition for offsets so extreme
        // that both stay negative, and both land in the same clamp-to-zero arm.
        offset_value = offset_value.saturating_add(haystack_len);
        if offset_value < 0 {
            offset_value = 0;
        }
    }
    if offset_value > haystack_len {
        return eval_throw_builtin_value_error(
            "substr_compare(): Argument #3 ($offset) must be contained in argument #1 ($haystack)",
            context,
            values,
        );
    }

    let remaining = haystack_len - offset_value;
    let compare_len = match length_value {
        Some(value) => value as u64,
        // With no `$length`, php compares the LONGER of the two operands, which is what makes a
        // prefix answer -1/1 instead of 0.
        None => remaining.max(needle_bytes.len() as i64) as u64,
    };
    let window = &haystack_bytes[offset_value as usize..];
    let result = eval_substr_compare_bytes(window, &needle_bytes, compare_len, fold_case);
    values.int(result)
}

/// Coerces `substr_compare()`'s `$length` to an int, wording a refusal with php's own `?int`
/// spelling rather than the plain `int` a non-nullable parameter reports.
fn eval_substr_compare_length_int(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_BOOL | EVAL_TAG_NULL => {
            eval_int_value(value, values)
        }
        EVAL_TAG_STRING if eval_is_numeric_string(&values.string_bytes(value)?) => {
            eval_int_value(value, values)
        }
        _ => {
            let given = eval_given_type_spelling(value, values)?;
            eval_throw_type_error(
                &format!(
                    "substr_compare(): Argument #4 ($length) must be of type ?int, {given} given"
                ),
                context,
                values,
            )
        }
    }
}

/// Coerces `substr_compare()`'s `$case_insensitive` to a bool the way a declared `bool`
/// parameter does: every scalar folds through PHP truthiness, and an array is a `TypeError`.
fn eval_substr_compare_case_flag(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        let given = eval_given_type_spelling(value, values)?;
        return eval_throw_type_error(
            &format!(
                "substr_compare(): Argument #5 ($case_insensitive) must be of type bool, {given} given"
            ),
            context,
            values,
        );
    }
    values.truthy(value)
}

/// Compares two byte runs the way php-src's `zend_binary_strncmp` / `zend_binary_strncasecmp_l`
/// pair does.
///
/// Both operands are first truncated to `compare_len`. The shared prefix is compared byte by
/// byte and the FIRST mismatch returns the raw unsigned-byte difference, exactly what `memcmp`
/// hands php. Only when the shared prefix is equal does the truncated-length comparison decide,
/// and that one is normalized to `-1`/`0`/`1` (`ZEND_THREEWAY_COMPARE`) rather than being a
/// length difference.
fn eval_substr_compare_bytes(
    window: &[u8],
    needle: &[u8],
    compare_len: u64,
    fold_case: bool,
) -> i64 {
    let window_len = (window.len() as u64).min(compare_len);
    let needle_len = (needle.len() as u64).min(compare_len);
    let prefix = window_len.min(needle_len) as usize;
    for index in 0..prefix {
        let mut left = window[index];
        let mut right = needle[index];
        if fold_case {
            // `zend_tolower_ascii`: `A`-`Z` only, never locale-aware and never multi-byte.
            left = left.to_ascii_lowercase();
            right = right.to_ascii_lowercase();
        }
        if left != right {
            return i64::from(left) - i64::from(right);
        }
    }
    match window_len.cmp(&needle_len) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}
