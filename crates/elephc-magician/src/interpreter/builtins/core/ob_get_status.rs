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
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};
use elephc_memoir::runtime_owners::{CleanupErrorPolicy, StagedRuntimeOwners};

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
        [full_status] => with_eval_operands(&[full_status], context, scope, values,
            |operands, context, _, values| eval_ob_get_status_result(operands, context, values)),
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
    build_status_array(capacity, values, |result, owners, values| {
        for index in 0..level {
            let entry = owners.stage(eval_ob_status_entry(index, values)?);
            let key = owners.stage(values.int(index)?);
            result.update_receiver(values.array_set(result.borrowed(), key, entry)?);
        }
        Ok(())
    })
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
    build_status_array(8, values, |entry, owners, values| {
        let key = owners.stage(values.string("name")?);
        let name = owners.stage(values.string(&name)?);
        entry.update_receiver(values.array_set(entry.borrowed(), key, name)?);
        for (key, value) in [
            ("type", i64::from(is_user)), ("flags", flags), ("level", index),
            ("chunk_size", chunk_size), ("buffer_size", buffer_size), ("buffer_used", buffer_used),
        ] {
            let key = owners.stage(values.string(key)?);
            let value = owners.stage(values.int(value)?);
            entry.update_receiver(values.array_set(entry.borrowed(), key, value)?);
        }
        Ok(())
    })
}

/// Builds a status array and gives the last temporary-release failure precedence.
fn build_status_array<V: RuntimeValueOps>(
    capacity: usize,
    values: &mut V,
    build: impl FnOnce(&RuntimeOwnerSlot, &StagedRuntimeOwners, &mut V) -> Result<(), EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let array = RuntimeOwnerSlot::new(values.assoc_new(capacity)?);
    let owners = StagedRuntimeOwners::default();
    with_runtime_owners(&(&array, &owners), || {
        let built = build(&array, &owners, values);
        let cleanup = owners.release_with_policy(CleanupErrorPolicy::Last, |cell| values.release(cell));
        array.finish(cleanup.and(built), |cell| values.release(cell))
    })
}
