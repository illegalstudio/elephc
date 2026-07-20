//! Purpose:
//! Eval registry entry and implementation for `ob_get_level`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Returns the output-buffer nesting depth (0 = no buffering).

use super::super::super::*;

eval_builtin! {
    name: "ob_get_level",
    area: Core,
    params: [],
    direct: Core,
    values: Core,
}

/// Evaluates PHP `ob_get_level()`.
pub(in crate::interpreter) fn eval_builtin_ob_get_level(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_ob_get_level_result(&[], context, values)
}

/// Applies `ob_get_level()` against the shared runtime output-buffer stack.
pub(in crate::interpreter) fn eval_ob_get_level_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let level = values.ob_level()?;
    values.int(level)
}
