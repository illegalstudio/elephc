//! Purpose:
//! Eval registry entry and implementation for `preg_filter`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks` and the named-call router in
//!   `crate::interpreter::expressions::calls`.
//!
//! Key details:
//! - `preg_filter()` is `preg_replace()` that keeps only the subjects a pattern matched, so it
//!   is [`eval_preg_replace_full`] with `filter` set. The compiled side builds it in
//!   `src/preg_array_prelude.rs`.

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
    let args = args.iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
    eval_builtin_preg_filter_call(&args, context, scope, values)
}

/// Evaluates a metadata-rich `preg_filter()` call, writing its `$count` through the caller.
pub(in crate::interpreter) fn eval_builtin_preg_filter_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_bound_preg_replace_call(args, true, "preg_filter", context, scope, values)
}

/// Dispatches by-value `preg_filter()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_filter_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_preg_replace_by_value("preg_filter", true, evaluated_args, context, values)
}
