//! Purpose:
//! Eval registry entry and implementation for `ob_get_status`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Simple mode returns the top buffer's status (empty array when no buffer);
//! -   full mode returns an int-keyed list with one status entry per level.
//! - Entries reflect shared handler state; every constructed key, value, and nested array has an owner.

use super::super::super::*;

eval_builtin! {
    contract: "ob_get_status",
    area: Core,
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
            let full_status = eval_owned_expr(full_status, context, scope, values)?;
            let result = eval_ob_get_status_result(&[full_status], context, values);
            super::call_user_func::release_callback_result(full_status, result, context, values)
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
    let mut owners = Vec::new();
    let built = (|| {
        for index in 0..level {
            let entry = eval_ob_status_entry(index, values)?;
            owners.push(entry);
            let key = values.int(index)?;
            owners.push(key);
            result = values.array_set(result, key, entry)?;
        }
        Ok(())
    })();
    finish_status_array(result, built, owners, values)
}

/// Builds the PHP status entry for one buffer level from its stored metadata.
fn eval_ob_status_entry(
    index: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((buffer_used, buffer_size)) = values.ob_stats(index)? else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let Some((chunk_size, stored_flags, is_user, _started)) = values.ob_slot_meta(index)? else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name_bytes = values.ob_slot_name(index)?.unwrap_or_default();
    let name = String::from_utf8_lossy(&name_bytes).into_owned();
    let mut flags = stored_flags;
    if is_user {
        flags |= 1;
    }
    let mut entry = values.assoc_new(8)?;
    let mut owners = Vec::new();
    let built = (|| {
        let key = values.string("name")?;
        owners.push(key);
        let name = values.string(&name)?;
        owners.push(name);
        entry = values.array_set(entry, key, name)?;
        for (key, value) in [
            ("type", i64::from(is_user)), ("flags", flags), ("level", index),
            ("chunk_size", chunk_size), ("buffer_size", buffer_size), ("buffer_used", buffer_used),
        ] {
            let key = values.string(key)?;
            owners.push(key);
            let value = values.int(value)?;
            owners.push(value);
            entry = values.array_set(entry, key, value)?;
        }
        Ok(())
    })();
    finish_status_array(entry, built, owners, values)
}

/// Releases temporary scalar or nested-array owners and transfers only a fully built result.
fn finish_status_array(
    array: RuntimeCellHandle,
    mut result: Result<(), EvalStatus>,
    owners: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    for owner in owners {
        if let Err(status) = values.release(owner) { result = Err(status); }
    }
    match result {
        Ok(()) => Ok(array),
        Err(status) => { let _ = values.release(array); Err(status) },
    }
}
