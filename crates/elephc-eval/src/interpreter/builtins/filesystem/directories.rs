//! Purpose:
//! Implements eval-local directory resource builtins backed by host directory listings.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - Directory resources share the eval resource id namespace with file streams.
//! - Entry lists are snapshotted when `opendir()` runs and can be rewound.

use super::super::super::*;
use super::*;

/// Evaluates PHP `opendir($directory)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_opendir(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    eval_opendir_result(directory, context, values)
}

/// Opens a local directory and returns a resource cell or PHP false.
pub(in crate::interpreter) fn eval_opendir_result(
    directory: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let directory = eval_path_string(directory, values)?;
    match context.stream_resources_mut().open_directory(&directory) {
        Some(id) => values.resource(id),
        None => values.bool_value(false),
    }
}

/// Evaluates PHP directory handle builtins over one eval expression.
pub(in crate::interpreter) fn eval_builtin_unary_directory(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [dir_handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let dir_handle = eval_expr(dir_handle, context, scope, values)?;
    eval_unary_directory_result(name, dir_handle, context, values)
}

/// Evaluates a materialized directory handle builtin argument.
pub(in crate::interpreter) fn eval_unary_directory_result(
    name: &str,
    dir_handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_directory_resource_id(dir_handle, values)?;
    match name {
        "closedir" => {
            context.stream_resources_mut().close_directory(id);
            values.null()
        }
        "readdir" => match context.stream_resources_mut().read_directory(id) {
            Some(name) => values.string(&name),
            None => values.bool_value(false),
        },
        "rewinddir" => {
            context.stream_resources_mut().rewind_directory(id);
            values.null()
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Converts a runtime resource cell into eval's zero-based directory id.
fn eval_directory_resource_id(
    dir_handle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(dir_handle)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(dir_handle, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}
