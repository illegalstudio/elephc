//! Purpose:
//! Declarative eval registry entry for `closedir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the directory resource close helper.

eval_builtin! {
    contract: "closedir",
    area: Filesystem,
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
    eval_unary_directory_values_result("closedir", evaluated_args, context, values)
}

/// php's refusal when a handle-less directory call has no stream to work on.
///
/// The wording carries NO function prefix — MEASURED on `php -n` 8.5.6:
/// `Uncaught TypeError: No resource supplied`.
pub(in crate::interpreter) const NO_DIRECTORY_RESOURCE_SUPPLIED: &str = "No resource supplied";

/// Evaluates the shared `readdir`/`rewinddir`/`closedir` shape over evaluated arguments.
///
/// `$dir_handle` is `= null`, so an ABSENT argument and a written `null` are the same call and
/// both take php's last-opened directory slot.
pub(in crate::interpreter) fn eval_unary_directory_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => eval_last_opened_directory_result(name, context, values),
        [dir_handle] if values.type_tag(*dir_handle)? == EVAL_TAG_NULL => {
            eval_last_opened_directory_result(name, context, values)
        }
        [dir_handle] => eval_unary_directory_result(name, *dir_handle, context, values),
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
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_unary_directory_values_result(name, &evaluated, context, values)
}

/// Runs a directory builtin against php's last opened directory stream.
///
/// The deprecation prints BEFORE the slot is consulted, so a program with nothing open gets the
/// notice AND the refusal — MEASURED on `php -n` 8.5.6.
fn eval_last_opened_directory_result(
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.warning(&format!(
        "Deprecated: {}(): Passing null is deprecated, instead the last opened directory \
         stream should be provided\n",
        name
    ))?;
    let Some(id) = context.stream_resources_mut().last_open_directory() else {
        return eval_stream_type_error(NO_DIRECTORY_RESOURCE_SUPPLIED, context, values);
    };
    eval_directory_operation_result(name, id, context, values)
}

/// Evaluates a materialized directory handle builtin argument.
pub(in crate::interpreter) fn eval_unary_directory_result(
    name: &str,
    dir_handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_directory_resource_id(dir_handle, values)?;
    eval_directory_operation_result(name, id, context, values)
}

/// Runs one directory builtin against an eval directory id, wherever that id came from.
fn eval_directory_operation_result(
    name: &str,
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
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
    eval_resource_payload(dir_handle, values)
}
