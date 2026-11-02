//! Purpose:
//! Declarative eval registry entry and PHP-compatible byte-string implementation for `strrchr`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` through direct and evaluated-value dispatch.
//!
//! Key details:
//! - PHP searches for the final occurrence of the first needle byte and returns that suffix.
//! - An empty needle or an absent byte returns `false` through the shared boxed-cell ABI.

eval_builtin! {
    contract: "strrchr",
    area: String,
    direct: Strrchr,
    values: Strrchr,
}

use super::super::super::*;

/// Evaluates `strrchr()` after evaluating its two PHP expressions in source order.
pub(in crate::interpreter) fn eval_builtin_strrchr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [haystack, needle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let haystack = eval_expr(haystack, context, scope, values)?;
    let needle = eval_expr(needle, context, scope, values)?;
    eval_strrchr_result(haystack, needle, values)
}

/// Returns the suffix beginning with PHP's final matching needle byte, or `false` on a miss.
pub(in crate::interpreter) fn eval_strrchr_result(
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let Some(needle_byte) = needle.first() else {
        return values.bool_value(false);
    };
    let Some(position) = haystack.iter().rposition(|byte| byte == needle_byte) else {
        return values.bool_value(false);
    };
    values.string_bytes_value(&haystack[position..])
}
