//! Purpose:
//! Dispatches path mutation builtins to eval userspace stream wrappers.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::ops` for `unlink()`, `rename()`,
//!   `mkdir()`, and `rmdir()` when the source path scheme is registered.
//!
//! Key details:
//! - Path methods use a throwaway wrapper instance, matching the generated
//!   runtime's path-op helpers instead of reusing open stream resources.

use super::super::super::*;
use super::user_wrapper_streams::eval_user_wrapper_method;

/// Dispatches `unlink($path)` to a wrapper object's `unlink()` method.
pub(in crate::interpreter) fn eval_user_wrapper_unlink_result(
    path: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    eval_user_wrapper_path_method_result(path, "unlink", context, values, |_| Ok(Vec::new()))
}

/// Dispatches `mkdir($path)` or `rmdir($path)` to the registered wrapper.
pub(in crate::interpreter) fn eval_user_wrapper_single_path_op_result(
    name: &str,
    path: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    match name {
        "mkdir" => eval_user_wrapper_path_method_result(path, name, context, values, |values| {
            Ok(vec![values.int(0)?, values.int(0)?])
        }),
        "rmdir" => eval_user_wrapper_path_method_result(path, name, context, values, |values| {
            Ok(vec![values.int(0)?])
        }),
        _ => Ok(None),
    }
}

/// Dispatches `rename($from, $to)` using the source path's wrapper scheme.
pub(in crate::interpreter) fn eval_user_wrapper_rename_result(
    from: &str,
    to: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    eval_user_wrapper_path_method_result(from, "rename", context, values, |values| {
        Ok(vec![values.string(to)?])
    })
}

/// Instantiates the wrapper for one path and invokes a boolean path method.
fn eval_user_wrapper_path_method_result<V>(
    path: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut V,
    extra_args: impl FnOnce(&mut V) -> Result<Vec<RuntimeCellHandle>, EvalStatus>,
) -> Result<Option<RuntimeCellHandle>, EvalStatus>
where
    V: RuntimeValueOps,
{
    let Some(class_name) = context
        .stream_resources()
        .user_stream_wrapper_class_for_path(path)
    else {
        return Ok(None);
    };
    let Some(class) = context.class(&class_name).cloned() else {
        return values.bool_value(false).map(Some);
    };
    let Some((declaring_class, method)) =
        eval_user_wrapper_method(class.name(), method_name, context)
    else {
        return values.bool_value(false).map(Some);
    };
    let mut scope = ElephcEvalScope::new();
    let object = eval_dynamic_class_new_object(&class, Vec::new(), context, &mut scope, values)?;
    let path = values.string(path)?;
    let extra_args = match extra_args(values) {
        Ok(args) => args,
        Err(status) => {
            values.release(object)?;
            values.release(path)?;
            return Err(status);
        }
    };
    let mut args = Vec::with_capacity(extra_args.len() + 1);
    args.push(path);
    args.extend(extra_args);
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        class.name(),
        &method,
        object,
        positional_args(args),
        context,
        values,
    )?;
    values.release(object)?;
    let ok = values.truthy(result)?;
    values.release(result)?;
    values.bool_value(ok).map(Some)
}
