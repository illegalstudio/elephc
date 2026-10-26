//! Purpose:
//! Owns native SimpleXML debug snapshots and shared recursion guards during eval.
//!
//! Called from:
//! - The eval print_r and var_dump object renderers.
//!
//! Key details:
//! - Hook exceptions are terminal, while projection diagnostics stay catchable.
//! - Snapshot cells, entry cells, and native guard depths are retired on every exit.

use super::super::super::*;

/// Borrows the receiver while protecting a complete nested rendering operation.
pub(super) fn with_guard<V: RuntimeValueOps>(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut V,
    render: impl FnOnce(&mut ElephcEvalContext, &mut V) -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let depth = values.debug_guard_push(object)?;
    let result = render(context, values);
    let restored = match depth {
        Some(depth) => values.debug_guard_restore(depth),
        None => Ok(()),
    };
    result.and(restored)
}

/// Calls the protected info hook, projects its result, and owns the hash through rendering.
pub(super) fn with_projection<V: RuntimeValueOps>(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut V,
    render: impl FnOnce(RuntimeCellHandle, &mut ElephcEvalContext, &mut V) -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let info = match values.operator_method_call(object, "__elephc_debug_info", vec![]) {
        Ok(info) => info,
        Err(EvalStatus::UncaughtThrowable) => {
            values.fatal(elephc_builtin_contract::NATIVE_DEBUG_INFO_FATAL)?;
            return Err(EvalStatus::RuntimeFatal);
        }
        Err(status) => return Err(status),
    };
    let projection = values.operator_method_call(object, "__elephc_debug_projection", vec![info.borrowed()]);
    let info_released = eval_release_value(context, values, info);
    let hash = projection?;
    if let Err(status) = info_released {
        let _ = eval_release_value(context, values, hash);
        return Err(status);
    }
    let result = render(hash, context, values);
    let released = eval_release_value(context, values, hash);
    result.and(released)
}

/// Owns each acquired snapshot key and value until their nested rendering has finished.
pub(super) fn with_entry<V: RuntimeValueOps>(
    hash: RuntimeCellHandle,
    position: usize,
    context: &mut ElephcEvalContext,
    values: &mut V,
    render: impl FnOnce(RuntimeCellHandle, RuntimeCellHandle, &mut ElephcEvalContext, &mut V) -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let key = values.array_iter_key(hash, position)?;
    let element = match values.array_get(hash, key) {
        Ok(element) => element,
        Err(status) => {
            let _ = eval_release_value(context, values, key);
            return Err(status);
        }
    };
    let result = render(key, element, context, values);
    let element_released = eval_release_value(context, values, element);
    let key_released = eval_release_value(context, values, key);
    result.and(element_released).and(key_released)
}

/// Streams the accumulated var_dump prefix before an observable user hook runs.
pub(super) fn flush(
    output: &mut Vec<u8>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if output.is_empty() { return Ok(()); }
    let text = values.string_bytes_value(output)?;
    let result = values.echo(text);
    let released = eval_release_value(context, values, text);
    output.clear();
    result.and(released)
}
