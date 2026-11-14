//! Purpose:
//! Eval registry entry and implementation for `preg_grep`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Mirrors the AOT prelude (`crate::pcre_prelude` in the compiler): the pattern is
//!   compiled once, each entry is matched as its string value and keeps its key, and a
//!   match error stops the scan with the entries collected so far.
//! - Unlike the other eval `preg_*` builtins, which treat a malformed pattern as a runtime
//!   fatal, an invalid pattern returns `false` with `PREG_INTERNAL_ERROR`, like the AOT
//!   prelude and PHP.

use super::super::super::*;
use super::*;

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
    let evaluated = args
        .iter()
        .map(|arg| eval_expr(arg, context, scope, values))
        .collect::<Result<Vec<_>, _>>()?;
    eval_preg_grep_values_result(&evaluated, values)
}

/// Dispatches by-value `preg_grep()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_grep_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (pattern, array, flags) = match evaluated_args {
        [pattern, array] => (*pattern, *array, None),
        [pattern, array, flags] => (*pattern, *array, Some(*flags)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let flags = flags.map_or(Ok(0), |flags| eval_int_value(flags, values))?;
    eval_preg_grep_result(pattern, array, flags & EVAL_PREG_GREP_INVERT != 0, values)
}

/// Returns the entries of `array` whose string value matches (or, inverted, does not).
pub(in crate::interpreter) fn eval_preg_grep_result(
    pattern: RuntimeCellHandle,
    array: RuntimeCellHandle,
    invert: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = match eval_preg_regex(pattern, values) {
        Ok(regex) => regex,
        // An invalid pattern is PHP `false` with `PREG_INTERNAL_ERROR`, not a fatal.
        Err(_) if eval_preg_last_error_code() == EVAL_PREG_INTERNAL_ERROR => {
            return values.bool_value(false);
        }
        Err(status) => return Err(status),
    };
    let len = values.array_len(array)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let subject = values.cast_string(value)?;
        let subject = values.string_bytes(subject)?;
        let matched = regex.is_match(&subject);
        if eval_preg_failed() {
            break;
        }
        if matched != invert {
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}
