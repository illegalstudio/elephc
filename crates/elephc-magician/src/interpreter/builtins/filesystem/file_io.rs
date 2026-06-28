//! Purpose:
//! getcwd, file reads/writes, filesize, filetype, stat, and disk-space builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem` re-exports.
//!
//! Key details:
//! - Helpers return PHP-compatible false/null/string/int cells via `RuntimeValueOps`.

use super::super::super::*;
use super::*;
use crate::stream_wrappers;

/// Evaluates PHP `getcwd()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_getcwd(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_getcwd_result(values)
}

/// Returns the process current working directory as a boxed PHP string.
pub(in crate::interpreter) fn eval_getcwd_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let cwd = std::env::current_dir().map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(cwd.to_string_lossy().as_ref())
}

/// Evaluates one PHP filesystem predicate over an eval expression.
pub(in crate::interpreter) fn eval_builtin_file_probe(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_probe_result(name, filename, context, values)
}

/// Computes one local filesystem predicate and returns a PHP boolean.
pub(in crate::interpreter) fn eval_file_probe_result(
    name: &str,
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(stat) = eval_user_wrapper_url_stat_result(&path, 0, context, values)? {
        return eval_user_wrapper_file_probe_from_stat(name, stat, values);
    }
    if stream_wrappers::is_phar_stream(&path) {
        let exists = elephc_phar::extract_url_bytes(path.as_bytes()).is_some();
        let supported = matches!(name, "file_exists" | "is_file" | "is_readable");
        return values.bool_value(supported && exists);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    let path = std::path::Path::new(&path);
    let result = match name {
        "file_exists" => path.exists(),
        "is_dir" => path.is_dir(),
        "is_executable" => eval_path_is_executable(path),
        "is_file" => path.is_file(),
        "is_link" => std::fs::symlink_metadata(path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false),
        "is_readable" => eval_path_is_readable(path),
        "is_writable" | "is_writeable" => eval_path_is_writable(path),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(result)
}

/// Evaluates one scalar PHP stat metadata builtin over an eval expression.
pub(in crate::interpreter) fn eval_builtin_file_stat_scalar(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_stat_scalar_result(name, filename, context, values)
}

/// Returns scalar stat metadata, using PHP false for failure where native elephc does.
pub(in crate::interpreter) fn eval_file_stat_scalar_result(
    name: &str,
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(stat) = eval_user_wrapper_url_stat_result(&path, 0, context, values)? {
        return eval_user_wrapper_file_stat_scalar_from_stat(name, stat, values);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return match name {
            "filemtime" => values.int(0),
            _ => values.bool_value(false),
        };
    };
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) if name == "filemtime" => return values.int(0),
        Err(_) => return values.bool_value(false),
    };
    match name {
        "fileatime" => values.int(metadata.atime()),
        "filectime" => values.int(metadata.ctime()),
        "filegroup" => values.int(i64::from(metadata.gid())),
        "fileinode" => {
            values.int(i64::try_from(metadata.ino()).map_err(|_| EvalStatus::RuntimeFatal)?)
        }
        "filemtime" => values.int(metadata.mtime()),
        "fileowner" => values.int(i64::from(metadata.uid())),
        "fileperms" => values.int(i64::from(metadata.mode())),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `filesize($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_filesize(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_filesize_result(filename, context, values)
}

/// Returns one local file or supported wrapper size in bytes, or zero on failure.
pub(in crate::interpreter) fn eval_filesize_result(
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(stat) = eval_user_wrapper_url_stat_result(&path, 0, context, values)? {
        let size = eval_user_wrapper_stat_int_field(stat, "size", values)?.unwrap_or(0);
        return values.int(size);
    }
    if let Ok(bytes) = eval_read_path_or_wrapper_bytes(&path) {
        return values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.int(0);
    };
    let len = std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    values.int(i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?)
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
    } else if file_type.is_char_device() {
        "char"
    } else if file_type.is_block_device() {
        "block"
    } else if file_type.is_fifo() {
        "fifo"
    } else if file_type.is_socket() {
        "socket"
    } else {
        "unknown"
    };
    values.string(label)
}

/// Evaluates PHP `stat($filename)` or `lstat($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_stat_array(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_stat_array_result(name, filename, context, values)
}

/// Builds PHP's stat array for one local path, or returns false on stat failure.
pub(in crate::interpreter) fn eval_stat_array_result(
    name: &str,
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(stat) = eval_user_wrapper_url_stat_result(&path, 0, context, values)? {
        return Ok(stat);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    let metadata = match name {
        "stat" => std::fs::metadata(path),
        "lstat" => std::fs::symlink_metadata(path),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let metadata = match metadata {
        Ok(metadata) => metadata,
        Err(_) => return values.bool_value(false),
    };
    eval_stat_metadata_array(&metadata, values)
}

/// Converts filesystem metadata into PHP's numeric-and-string keyed stat array.
pub(in crate::interpreter) fn eval_stat_metadata_array(
    metadata: &std::fs::Metadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let fields = [
        ("dev", eval_u64_to_i64(metadata.dev())?),
        ("ino", eval_u64_to_i64(metadata.ino())?),
        ("mode", i64::from(metadata.mode())),
        ("nlink", eval_u64_to_i64(metadata.nlink())?),
        ("uid", i64::from(metadata.uid())),
        ("gid", i64::from(metadata.gid())),
        ("rdev", eval_u64_to_i64(metadata.rdev())?),
        ("size", eval_u64_to_i64(metadata.size())?),
        ("atime", metadata.atime()),
        ("mtime", metadata.mtime()),
        ("ctime", metadata.ctime()),
        ("blksize", eval_u64_to_i64(metadata.blksize())?),
        ("blocks", eval_u64_to_i64(metadata.blocks())?),
    ];
    let mut result = values.assoc_new(fields.len() * 2)?;
    for (index, (name, value)) in fields.iter().enumerate() {
        result = eval_stat_array_set_int_key(result, index, *value, values)?;
        result = eval_stat_array_set_string_key(result, name, *value, values)?;
    }
    Ok(result)
}

/// Inserts one integer stat field under a numeric PHP array key.
pub(in crate::interpreter) fn eval_stat_array_set_int_key(
    array: RuntimeCellHandle,
    key: usize,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.int(i64::try_from(key).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let value = values.int(value)?;
    values.array_set(array, key, value)
}

/// Inserts one integer stat field under a string PHP array key.
pub(in crate::interpreter) fn eval_stat_array_set_string_key(
    array: RuntimeCellHandle,
    key: &str,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.int(value)?;
    values.array_set(array, key, value)
}

/// Converts unsigned stat metadata into the signed integer payload used by PHP cells.
pub(in crate::interpreter) fn eval_u64_to_i64(value: u64) -> Result<i64, EvalStatus> {
    i64::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)
}
