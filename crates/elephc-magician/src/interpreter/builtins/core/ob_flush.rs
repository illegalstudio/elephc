//! Purpose:
//! Eval registry entry and implementation for `ob_flush`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Flushes the top buffer to the parent sink without popping it.

use super::super::super::*;

eval_builtin! {
    name: "ob_flush",
    area: Core,
    params: [],
    direct: Core,
    values: Core,
}

/// Evaluates PHP `ob_flush()`.
pub(in crate::interpreter) fn eval_builtin_ob_flush(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_ob_flush_result(&[], context, values)
}

/// Applies `ob_flush()` against the shared runtime output-buffer stack.
pub(in crate::interpreter) fn eval_ob_flush_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let flushed = values.ob_flush()?;
    values.bool_value(flushed)
}
