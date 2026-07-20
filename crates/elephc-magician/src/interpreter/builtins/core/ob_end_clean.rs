//! Purpose:
//! Eval registry entry and implementation for `ob_end_clean`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Discards the top buffer and pops the stack; false when none is active.

use super::super::super::*;

eval_builtin! {
    name: "ob_end_clean",
    area: Core,
    params: [],
    direct: Core,
    values: Core,
}

/// Evaluates PHP `ob_end_clean()`.
pub(in crate::interpreter) fn eval_builtin_ob_end_clean(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_ob_end_clean_result(&[], context, values)
}

/// Applies `ob_end_clean()` against the shared runtime output-buffer stack.
pub(in crate::interpreter) fn eval_ob_end_clean_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let ended = values.ob_end(false)?;
    values.bool_value(ended)
}
