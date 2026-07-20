//! Purpose:
//! Declarative eval registry entry for `closedir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the directory resource close helper.

eval_builtin! {
    name: "closedir",
    area: Filesystem,
    params: [dir_handle],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `closedir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_closedir_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_unary_directory("closedir", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `closedir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_closedir_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [dir_handle] => eval_unary_directory_result("closedir", *dir_handle, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
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
            if let Some(result) = eval_user_wrapper_closedir_result(id, context, values)? {
                return Ok(result);
            }
            context.stream_resources_mut().close_directory(id);
            values.null()
        }
        "readdir" => {
            if let Some(result) = eval_user_wrapper_readdir_result(id, context, values)? {
                return Ok(result);
            }
            match context.stream_resources_mut().read_directory(id) {
                Some(name) => values.string(&name),
                None => values.bool_value(false),
            }
        }
        "rewinddir" => {
            if let Some(result) = eval_user_wrapper_rewinddir_result(id, context, values)? {
                return Ok(result);
            }
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
