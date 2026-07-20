//! Purpose:
//! Eval registry entry and implementation for `ob_end_flush`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Flushes the top buffer to the parent sink, then pops the stack.

use super::super::super::*;

eval_builtin! {
    name: "ob_end_flush",
    area: Core,
    params: [],
    direct: Core,
    values: Core,
}

/// Evaluates PHP `ob_end_flush()`.
pub(in crate::interpreter) fn eval_builtin_ob_end_flush(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_ob_end_flush_result(&[], context, values)
}

/// Applies `ob_end_flush()` against the shared runtime output-buffer stack.
pub(in crate::interpreter) fn eval_ob_end_flush_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let ended = values.ob_end(true)?;
    values.bool_value(ended)
}
