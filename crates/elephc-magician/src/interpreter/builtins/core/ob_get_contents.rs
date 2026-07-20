//! Purpose:
//! Eval registry entry and implementation for `ob_get_contents`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Returns the top buffer's bytes, or false when no buffer is active.

use super::super::super::*;

eval_builtin! {
    name: "ob_get_contents",
    area: Core,
    params: [],
    direct: Core,
    values: Core,
}

/// Evaluates PHP `ob_get_contents()`.
pub(in crate::interpreter) fn eval_builtin_ob_get_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_ob_get_contents_result(&[], context, values)
}

/// Applies `ob_get_contents()` against the shared runtime output-buffer stack.
pub(in crate::interpreter) fn eval_ob_get_contents_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    match values.ob_contents()? {
        Some(bytes) => values.string_bytes_value(&bytes),
        None => values.bool_value(false),
    }
}
