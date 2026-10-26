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

/// Dispatches the PHP hook through eval, projects its result, and owns the rendering hash.
pub(super) fn with_projection<V: RuntimeValueOps>(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut V,
    render: impl FnOnce(RuntimeCellHandle, &mut ElephcEvalContext, &mut V) -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let info = match values.operator_method_call(object, "__debugInfo", vec![]) {
        Ok(info) => info,
        Err(EvalStatus::UncaughtThrowable) => {
            return hook_failed(context, values);
        }
        Err(status) => return Err(status),
    };
    let projection = project_info(object, info, context, values);
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

/// Shares native warning formatting while retaining the original error until the fatal boundary.
fn hook_failed(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let error = context.take_pending_throw();
    if let Some(error) = error {
        if let Ok(result) = values.operator_static_call("SimpleXMLElement", "__elephc_debug_warning", vec![error.borrowed()]) {
            let _ = eval_release_value(context, values, result);
        }
    }
    let fatal = values.fatal(elephc_builtin_contract::NATIVE_DEBUG_INFO_FATAL);
    // Native fatal exits here. Embedders may return an error and still need to retire the owner.
    let released = match error {
        Some(error) => eval_release_value(context, values, error),
        None => Ok(()),
    };
    fatal.and(released)?;
    Err(EvalStatus::RuntimeFatal)
}

/// Supplies the interpreter-visible class name for nullable hook diagnostics.
fn project_info(
    object: RuntimeCellHandle,
    info: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let identity = super::print_r::eval_debug_object_identity(object, values);
    let class_name = super::print_r::eval_debug_object_class_name(object, identity, context, values)?;
    let class = values.string(&class_name)?;
    let projection = values.operator_static_call("SimpleXMLElement", "__elephc_debug_project", vec![info.borrowed(), class.borrowed()]);
    let class_released = eval_release_value(context, values, class);
    let hash = projection?;
    if let Err(status) = class_released {
        let _ = eval_release_value(context, values, hash);
        return Err(status);
    }
    Ok(hash)
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
