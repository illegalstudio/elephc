//! Purpose:
//! Eval registry entry and implementation for `ob_get_status`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Simple mode returns the top buffer's status (empty array when no buffer);
//! -   full mode returns an int-keyed list with one status entry per level.
//! - Every entry reports the default output handler (user handlers unsupported).

use super::super::super::*;

use super::super::spec::EvalBuiltinDefaultValue;
use super::super::time::{eval_array_set_string_int, eval_array_set_string_str};

eval_builtin! {
    name: "ob_get_status",
    area: Core,
    params: [full_status = EvalBuiltinDefaultValue::Bool(false)],
    direct: Core,
    values: Core,
}

/// Evaluates PHP `ob_get_status($full_status = false)`.
pub(in crate::interpreter) fn eval_builtin_ob_get_status(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_ob_get_status_result(&[], context, values),
        [full_status] => {
            let full_status = eval_expr(full_status, context, scope, values)?;
            eval_ob_get_status_result(&[full_status], context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds the `ob_get_status()` array from the shared runtime buffer stack.
pub(in crate::interpreter) fn eval_ob_get_status_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let full_status = match evaluated_args {
        [] => false,
        [full_status] => values.truthy(*full_status)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let level = values.ob_level()?;
    if !full_status {
        if level == 0 {
            return values.assoc_new(0);
        }
        return eval_ob_status_entry(level - 1, values);
    }
    let capacity = usize::try_from(level).unwrap_or(0).max(1);
    let mut result = values.assoc_new(capacity)?;
    for index in 0..level {
        let entry = eval_ob_status_entry(index, values)?;
        let key = values.int(index)?;
        result = values.array_set(result, key, entry)?;
    }
    Ok(result)
}

/// Builds the PHP status entry (default-handler shape) for one buffer level.
fn eval_ob_status_entry(
    index: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((buffer_used, buffer_size)) = values.ob_stats(index)? else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let mut entry = values.assoc_new(8)?;
    entry = eval_array_set_string_str(entry, "name", "default output handler", values)?;
    entry = eval_array_set_string_int(entry, "type", 0, values)?;
    entry = eval_array_set_string_int(entry, "flags", 112, values)?;
    entry = eval_array_set_string_int(entry, "level", index, values)?;
    entry = eval_array_set_string_int(entry, "chunk_size", 0, values)?;
    entry = eval_array_set_string_int(entry, "buffer_size", buffer_size, values)?;
    eval_array_set_string_int(entry, "buffer_used", buffer_used, values)
}
