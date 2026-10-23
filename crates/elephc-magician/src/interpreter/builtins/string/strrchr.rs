//! Purpose:
//! Declarative eval registry entry and implementation for `strrchr`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Only the needle's first byte is searched, as in php-src; an empty needle searches NUL.
//! - `$before_needle` (PHP 8.3) returns the part before the last occurrence instead.

eval_builtin! {
    contract: "strrchr",
    area: String,
    direct: Strrchr,
    values: Strrchr,
}

use super::super::super::*;

/// Evaluates PHP `strrchr(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_strrchr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_strrchr_values_result(&evaluated, values)
}

/// Dispatches already evaluated `strrchr()` arguments.
pub(in crate::interpreter) fn eval_strrchr_values_result(
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (haystack, needle, before_needle) = match args {
        [haystack, needle] => (*haystack, *needle, false),
        [haystack, needle, before_needle] => (*haystack, *needle, values.truthy(*before_needle)?),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let byte = needle.first().copied().unwrap_or(0);
    let Some(position) = haystack.iter().rposition(|candidate| *candidate == byte) else {
        return values.bool_value(false);
    };
    let selected = if before_needle { &haystack[..position] } else { &haystack[position..] };
    values.string_bytes_value(selected)
}
