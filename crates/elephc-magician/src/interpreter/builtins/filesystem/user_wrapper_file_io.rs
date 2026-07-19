//! Purpose:
//! Dispatches one-shot file I/O builtins through eval userspace stream wrappers.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::file`, `file_get_contents`,
//!   `readfile`, and `file_put_contents` for one-shot wrapper I/O.
//!
//! Key details:
//! - These helpers open a temporary wrapper stream, perform the requested read or
//!   write through stream methods, and close the wrapper resource before returning.

use super::super::super::*;

/// Reads a full userspace-wrapper path into a PHP string cell.
pub(in crate::interpreter) fn eval_user_wrapper_file_get_contents_result(
    path: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((id, opened)) = eval_user_wrapper_open_file_path(path, "r", context, values)? else {
        return Ok(None);
    };
    if values.type_tag(opened)? != EVAL_TAG_RESOURCE {
        return Ok(Some(opened));
    }
    let result = match eval_user_wrapper_stream_get_contents_result(id, None, None, context, values)?
    {
        Some(result) => result,
        None => values.bool_value(false)?,
    };
    eval_user_wrapper_close_one_shot(id, context, values)?;
    Ok(Some(result))
}

/// Streams one userspace-wrapper path to eval output and returns the byte count.
pub(in crate::interpreter) fn eval_user_wrapper_readfile_result(
    path: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((id, opened)) = eval_user_wrapper_open_file_path(path, "r", context, values)? else {
        return Ok(None);
    };
    if values.type_tag(opened)? != EVAL_TAG_RESOURCE {
        return Ok(Some(opened));
    }
    let result = match eval_user_wrapper_stream_get_contents_result(id, None, None, context, values)?
    {
        Some(result) => result,
        None => values.bool_value(false)?,
    };
    if values.type_tag(result)? != EVAL_TAG_STRING {
        eval_user_wrapper_close_one_shot(id, context, values)?;
        return Ok(Some(result));
    }
    let bytes = values.string_bytes(result)?;
    values.echo(result)?;
    eval_user_wrapper_close_one_shot(id, context, values)?;
    values
        .int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?)
        .map(Some)
}

/// Writes bytes to one userspace-wrapper path and returns the wrapper byte count.
pub(in crate::interpreter) fn eval_user_wrapper_file_put_contents_result(
    path: &str,
    data: &[u8],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((id, opened)) = eval_user_wrapper_open_file_path(path, "w", context, values)? else {
        return Ok(None);
    };
    if values.type_tag(opened)? != EVAL_TAG_RESOURCE {
        return Ok(Some(opened));
    }
    let result = match eval_user_wrapper_fwrite_result(id, data, context, values)? {
        Some(result) => result,
        None => values.bool_value(false)?,
    };
    eval_user_wrapper_close_one_shot(id, context, values)?;
    Ok(Some(result))
}

/// Opens one userspace-wrapper file path and returns its resource id and cell.
fn eval_user_wrapper_open_file_path(
    path: &str,
    mode: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(i64, RuntimeCellHandle)>, EvalStatus> {
    let mut scope = ElephcEvalScope::new();
    let Some(opened) = eval_user_wrapper_fopen_result(path, mode, context, &mut scope, values)?
    else {
        return Ok(None);
    };
    if values.type_tag(opened)? != EVAL_TAG_RESOURCE {
        return Ok(Some((-1, opened)));
    }
    let id = eval_user_wrapper_file_resource_id(opened, values)?;
    Ok(Some((id, opened)))
}

/// Closes a one-shot wrapper stream and releases the close result cell.
fn eval_user_wrapper_close_one_shot(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if id < 0 {
        return Ok(());
    }
    if let Some(closed) = eval_user_wrapper_fclose_result(id, context, values)? {
        values.release(closed)?;
    }
    Ok(())
}

/// Converts a PHP resource cell into eval's zero-based stream resource id.
fn eval_user_wrapper_file_resource_id(
    stream: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(stream)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(stream, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}
