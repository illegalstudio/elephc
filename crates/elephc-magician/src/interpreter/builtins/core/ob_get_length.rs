//! Purpose:
//! Eval registry entry and implementation for `ob_get_length`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Returns the top buffer's byte count, or false when no buffer is active.

use super::super::super::*;

eval_builtin! {
    name: "ob_get_length",
    area: Core,
    params: [],
    direct: Core,
    values: Core,
}

/// Evaluates PHP `ob_get_length()`.
pub(in crate::interpreter) fn eval_builtin_ob_get_length(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_ob_get_length_result(&[], context, values)
}

/// Applies `ob_get_length()` against the shared runtime output-buffer stack.
pub(in crate::interpreter) fn eval_ob_get_length_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    match values.ob_length()? {
        Some(length) => values.int(length),
        None => values.bool_value(false),
    }
}
