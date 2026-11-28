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
use crate::runtime_hooks::owner_frames::{
    RuntimeOwnerSlot, StagedRuntimeOwners, with_runtime_owners,
};

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
    let info = RuntimeOwnerSlot::new(info);
    let hash = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&info, &hash), || {
        let projection = project_info(object, info.borrowed(), context, values);
        if let Ok(value) = projection.as_ref() {
            hash.replace(Some(*value));
        }
        let info_released = eval_release_value(context, values, info.take());
        match (projection, info_released) {
            (Err(status), _) => Err(status),
            (Ok(_), Err(status)) => {
                let _ = eval_release_value(context, values, hash.take());
                Err(status)
            }
            (Ok(_), Ok(())) => {
                let result = render(hash.borrowed(), context, values);
                let released = eval_release_value(context, values, hash.take());
                result.and(released)
            }
        }
    })
}

/// Shares native warning formatting while retaining the original error until the fatal boundary.
fn hook_failed(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let error = RuntimeOwnerSlot::empty();
    error.replace(context.take_pending_throw());
    with_runtime_owners(&error, || {
        if !error.is_empty() {
            let _ = warn_hook_failure(error.borrowed(), context, values);
        }
        let fatal = values.fatal(elephc_builtin_contract::NATIVE_DEBUG_INFO_FATAL);
        // Native fatal exits here. Embedders may return an error and still need to retire the owner.
        let released = error
            .take_optional()
            .map_or(Ok(()), |error| eval_release_value(context, values, error));
        fatal.and(released)?;
        Err(EvalStatus::RuntimeFatal)
    })
}

/// Formats eval-defined exception names without consulting the native parent class id.
fn warn_hook_failure(
    error: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let identity = super::print_r::eval_debug_object_identity(error, values);
    let name = super::print_r::eval_debug_object_class_name(error, identity, context, values)?;
    let class = RuntimeOwnerSlot::new(values.string(&name)?);
    let warning = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&class, &warning), || {
        let operation = values.operator_static_call(
            "SimpleXMLElement",
            "__elephc_debug_warning",
            vec![error.borrowed(), class.borrowed()],
        );
        if let Ok(result) = operation.as_ref() {
            warning.replace(Some(*result));
        }
        let class_released = eval_release_value(context, values, class.take());
        match (operation, class_released) {
            (Err(status), _) => Err(status),
            (Ok(_), Err(status)) => {
                let _ = eval_release_value(context, values, warning.take());
                Err(status)
            }
            (Ok(_), Ok(())) => eval_release_value(context, values, warning.take()),
        }
    })
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
    let class = RuntimeOwnerSlot::new(values.string(&class_name)?);
    let hash = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&class, &hash), || {
        let projection = values.operator_static_call(
            "SimpleXMLElement",
            "__elephc_debug_project",
            vec![info.borrowed(), class.borrowed()],
        );
        if let Ok(result) = projection.as_ref() {
            hash.replace(Some(*result));
        }
        let class_released = eval_release_value(context, values, class.take());
        match (projection, class_released) {
            (Err(status), _) => Err(status),
            (Ok(_), Err(status)) => {
                let _ = eval_release_value(context, values, hash.take());
                Err(status)
            }
            (Ok(_), Ok(())) => Ok(hash.take()),
        }
    })
}

/// Owns each acquired snapshot key and value until their nested rendering has finished.
pub(super) fn with_entry<V: RuntimeValueOps>(
    hash: RuntimeCellHandle,
    position: usize,
    context: &mut ElephcEvalContext,
    values: &mut V,
    render: impl FnOnce(RuntimeCellHandle, RuntimeCellHandle, &mut ElephcEvalContext, &mut V) -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let owners = StagedRuntimeOwners::default();
    with_runtime_owners(&owners, || {
        let result = (|| {
            let key = owners.stage(values.array_iter_key(hash, position)?);
            let element = owners.stage(values.array_get(hash, key.borrowed())?);
            render(
                key.borrowed(),
                element.borrowed(),
                context,
                values,
            )
        })();
        owners.release(result, |cell| eval_release_value(context, values, cell))
    })
}

/// Streams the accumulated var_dump prefix before an observable user hook runs.
pub(super) fn flush(
    output: &mut Vec<u8>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if output.is_empty() { return Ok(()); }
    let text = RuntimeOwnerSlot::new(values.string_bytes_value(output)?);
    with_runtime_owners(&text, || {
        let result = values.echo(text.borrowed());
        let released = eval_release_value(context, values, text.take());
        output.clear();
        result.and(released)
    })
}
