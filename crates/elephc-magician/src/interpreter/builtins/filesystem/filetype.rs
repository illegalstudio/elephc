//! Purpose:
//! Declarative eval registry entry for `filetype`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the filetype helper.

eval_builtin! {
    name: "filetype",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::stream_wrappers;
use super::*;

/// Dispatches direct eval calls for the `filetype` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_filetype_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_filetype(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `filetype` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_filetype_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => eval_filetype_result(*filename, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `filetype($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_filetype(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_filetype_result(filename, context, values)
}

/// Returns the PHP filetype string for one path, or false when lstat fails.
pub(in crate::interpreter) fn eval_filetype_result(
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(stat) = eval_user_wrapper_url_stat_result(&path, 0, context, values)? {
        return match eval_user_wrapper_stat_int_field(stat, "mode", values)? {
            Some(mode) => values.string(eval_filetype_label_from_mode(mode)),
            None => values.bool_value(false),
        };
    }
    if stream_wrappers::is_phar_stream(&path) {
        return if elephc_phar::extract_url_bytes(path.as_bytes()).is_some() {
            values.string("file")
        } else {
            values.bool_value(false)
        };
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    let file_type = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata.file_type(),
        Err(_) => return values.bool_value(false),
    };
    let label = if file_type.is_file() {
        "file"
    } else if file_type.is_dir() {
        "dir"
    } else if file_type.is_symlink() {
        "link"
    } else {
        eval_special_filetype(&file_type)
    };
    values.string(label)
}

/// Classifies Unix special file kinds that have no Windows filesystem equivalent.
#[cfg(unix)]
fn eval_special_filetype(file_type: &std::fs::FileType) -> &'static str {
    if file_type.is_char_device() {
        "char"
    } else if file_type.is_block_device() {
        "block"
    } else if file_type.is_fifo() {
        "fifo"
    } else if file_type.is_socket() {
        "socket"
    } else {
        "unknown"
    }
}

/// Classifies Windows special files as unknown outside files, directories, and links.
#[cfg(windows)]
fn eval_special_filetype(_file_type: &std::fs::FileType) -> &'static str {
    "unknown"
}
