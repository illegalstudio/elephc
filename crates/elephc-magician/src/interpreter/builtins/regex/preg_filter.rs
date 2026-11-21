//! Purpose:
//! Eval registry entry and implementation for `preg_filter`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Mirrors the AOT prelude (`crate::pcre_prelude` in the compiler): `preg_replace()` over the
//!   same string-only subset, answering `null` when nothing matched or the pattern failed.

use super::super::super::*;
use super::*;

eval_builtin! {
    contract: "preg_filter",
    area: Regex,
    direct: PregFilter,
    values: PregFilter,
}

/// Evaluates PHP `preg_filter()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_filter(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated = args
        .iter()
        .map(|arg| eval_expr(arg, context, scope, values))
        .collect::<Result<Vec<_>, _>>()?;
    eval_preg_filter_values_result(&evaluated, values)
}

/// Dispatches by-value `preg_filter()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_filter_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, replacement, subject] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let regex = eval_preg_regex(*pattern, values)?;
    let subject_bytes = values.string_bytes(*subject)?;
    let matched = regex.captures_iter(&subject_bytes).next().is_some();
    if eval_preg_failed() || !matched {
        return values.null();
    }
    eval_preg_replace_result(*pattern, *replacement, *subject, values)
}
