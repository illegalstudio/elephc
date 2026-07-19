//! Purpose:
//! Eval registry entry and implementation for `ob_clean`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Truncates the top buffer without popping it; false when none is active.

use super::super::super::*;

eval_builtin! {
    name: "ob_clean",
    area: Core,
    params: [],
    direct: Core,
    values: Core,
}

/// Evaluates PHP `ob_clean()`.
pub(in crate::interpreter) fn eval_builtin_ob_clean(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_ob_clean_result(&[], context, values)
}

/// Applies `ob_clean()` against the shared runtime output-buffer stack.
pub(in crate::interpreter) fn eval_ob_clean_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let cleaned = values.ob_clean()?;
    values.bool_value(cleaned)
}
