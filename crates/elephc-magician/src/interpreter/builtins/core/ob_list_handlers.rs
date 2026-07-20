//! Purpose:
//! Eval registry entry and implementation for `ob_list_handlers`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Returns one "default output handler" name per active buffer level
//! -   (user handlers are unsupported, so every level reports the default).

use super::super::super::*;

eval_builtin! {
    name: "ob_list_handlers",
    area: Core,
    params: [],
    direct: Core,
    values: Core,
}

/// Evaluates PHP `ob_list_handlers()`.
pub(in crate::interpreter) fn eval_builtin_ob_list_handlers(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_ob_list_handlers_result(&[], context, values)
}

/// Applies `ob_list_handlers()` against the shared runtime output-buffer stack.
pub(in crate::interpreter) fn eval_ob_list_handlers_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let level = values.ob_level()?;
    let capacity = usize::try_from(level).unwrap_or(0).max(1);
    let mut handlers = values.string_array_new(capacity)?;
    for index in 0..level {
        let name_bytes = values.ob_slot_name(index)?.unwrap_or_default();
        let name = String::from_utf8_lossy(&name_bytes).into_owned();
        handlers = values.string_array_push(handlers, &name)?;
    }
    Ok(handlers)
}
