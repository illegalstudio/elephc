//! Purpose:
//! Dispatches eval directory builtins to userspace stream-wrapper directory methods.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::directories` for `opendir()`,
//!   `readdir()`, `rewinddir()`, and `closedir()`.
//!
//! Key details:
//! - `dir_opendir()` owns the wrapper object for the directory resource lifetime;
//!   `dir_closedir()` releases it, while `readdir()`/`rewinddir()` reuse it.

use super::super::super::*;
use super::user_wrapper_streams::eval_user_wrapper_method;

/// Dispatches `opendir($path)` to a wrapper object's `dir_opendir()` method.
pub(in crate::interpreter) fn eval_user_wrapper_opendir_result(
    path: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(class_name) = context
        .stream_resources()
        .user_stream_wrapper_class_for_path(path)
    else {
        return Ok(None);
    };
    let Some(class) = context.class(&class_name).cloned() else {
        return values.bool_value(false).map(Some);
    };
    let Some((declaring_class, dir_opendir)) =
        eval_user_wrapper_method(class.name(), "dir_opendir", context)
    else {
        return values.bool_value(false).map(Some);
    };
    let mut scope = ElephcEvalScope::new();
    let object = eval_dynamic_class_new_object(&class, Vec::new(), context, &mut scope, values)?;
    let path_arg = values.string(path)?;
    let options = values.int(0)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        class.name(),
        &dir_opendir,
        object,
        positional_args(vec![path_arg, options]),
        context,
        values,
    )?;
    let opened = values.truthy(result)?;
    values.release(result)?;
    if !opened {
        values.release(object)?;
        return values.bool_value(false).map(Some);
    }
    let id = context
        .stream_resources_mut()
        .open_user_wrapper_directory(object, class.name());
    values.resource(id).map(Some)
}

/// Dispatches `closedir($handle)` to a wrapper object's `dir_closedir()` method.
pub(in crate::interpreter) fn eval_user_wrapper_closedir_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_directory_info(id) else {
        return Ok(None);
    };
    if let Some((declaring_class, dir_closedir)) =
        eval_user_wrapper_method(&info.class_name, "dir_closedir", context)
    {
        let result = eval_dynamic_method_with_values(
            &declaring_class,
            &info.class_name,
            &dir_closedir,
            info.object,
            Vec::new(),
            context,
            values,
        )?;
        values.release(result)?;
    }
    if let Some(info) = context
        .stream_resources_mut()
        .close_user_wrapper_directory(id)
    {
        values.release(info.object)?;
    }
    values.null().map(Some)
}

/// Dispatches `readdir($handle)` to a wrapper object's `dir_readdir()` method.
pub(in crate::interpreter) fn eval_user_wrapper_readdir_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_directory_info(id) else {
        return Ok(None);
    };
    let Some((declaring_class, dir_readdir)) =
        eval_user_wrapper_method(&info.class_name, "dir_readdir", context)
    else {
        return values.bool_value(false).map(Some);
    };
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &info.class_name,
        &dir_readdir,
        info.object,
        Vec::new(),
        context,
        values,
    )?;
    if values.type_tag(result)? == EVAL_TAG_STRING && values.string_bytes(result)?.is_empty() {
        values.release(result)?;
        return values.bool_value(false).map(Some);
    }
    Ok(Some(result))
}

/// Dispatches `rewinddir($handle)` to a wrapper object's `dir_rewinddir()` method.
pub(in crate::interpreter) fn eval_user_wrapper_rewinddir_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_directory_info(id) else {
        return Ok(None);
    };
    if let Some((declaring_class, dir_rewinddir)) =
        eval_user_wrapper_method(&info.class_name, "dir_rewinddir", context)
    {
        let result = eval_dynamic_method_with_values(
            &declaring_class,
            &info.class_name,
            &dir_rewinddir,
            info.object,
            Vec::new(),
            context,
            values,
        )?;
        values.release(result)?;
    }
    values.null().map(Some)
}
