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
    eval_file_probe_result(name, filename, values)
}

/// Computes one local filesystem predicate and returns a PHP boolean.
pub(in crate::interpreter) fn eval_file_probe_result(
    name: &str,
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
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
    eval_file_stat_scalar_result(name, filename, values)
}

/// Returns scalar stat metadata, using PHP false for failure where native elephc does.
pub(in crate::interpreter) fn eval_file_stat_scalar_result(
    name: &str,
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
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

/// Evaluates PHP `file_get_contents($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_file_get_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_get_contents_result(filename, values)
}

/// Reads a local file or supported wrapper into a PHP string, or returns false on failure.
pub(in crate::interpreter) fn eval_file_get_contents_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    match eval_read_path_or_wrapper_bytes(&path) {
        Ok(bytes) => values.string_bytes_value(&bytes),
        Err(_) => {
            values.warning("Warning: file_get_contents(): Failed to open stream\n")?;
            values.bool_value(false)
        }
    }
}

/// Evaluates PHP `file($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_file(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_result(filename, values)
}

/// Reads one local file or supported wrapper and returns indexed line byte strings.
pub(in crate::interpreter) fn eval_file_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let bytes = match eval_read_path_or_wrapper_bytes(&path) {
        Ok(bytes) => bytes,
        Err(_) => {
            values.warning("Warning: file_get_contents(): Failed to open stream\n")?;
            return values.array_new(0);
        }
    };
    eval_file_lines_array(&bytes, values)
}

/// Splits file payload bytes into runtime array entries, preserving trailing newlines.
pub(in crate::interpreter) fn eval_file_lines_array(
    bytes: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(0)?;
    let mut line_start = 0;
    let mut line_index = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        result =
            eval_array_set_indexed_bytes(result, line_index, &bytes[line_start..=index], values)?;
        line_start = index + 1;
        line_index += 1;
    }
    if line_start < bytes.len() {
        result = eval_array_set_indexed_bytes(result, line_index, &bytes[line_start..], values)?;
    }
    Ok(result)
}

/// Evaluates PHP `readfile($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_readfile(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_readfile_result(filename, values)
}

/// Streams one local file or supported wrapper to eval output.
pub(in crate::interpreter) fn eval_readfile_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(local_path) = stream_wrappers::local_filesystem_path(&path) {
        let path = std::path::Path::new(&local_path);
        if path.is_dir() {
            return values.int(-1);
        }
    }
    let bytes = match eval_read_path_or_wrapper_bytes(&path) {
        Ok(bytes) => bytes,
        Err(_) => return values.bool_value(false),
    };
    let output = values.string_bytes_value(&bytes)?;
    values.echo(output)?;
    values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Evaluates PHP `file_put_contents($filename, $data)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_file_put_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename, data] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    let data = eval_expr(data, context, scope, values)?;
    eval_file_put_contents_result(filename, data, values)
}

/// Writes a PHP string to a local file or supported wrapper and returns a byte count.
pub(in crate::interpreter) fn eval_file_put_contents_result(
    filename: RuntimeCellHandle,
    data: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let data = values.string_bytes(data)?;
    if stream_wrappers::is_phar_stream(&path) {
        return match elephc_phar::put_url_bytes(path.as_bytes(), &data) {
            Some(len) => values.int(i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?),
            None => values.bool_value(false),
        };
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    match std::fs::write(path, &data) {
        Ok(()) => values.int(i64::try_from(data.len()).map_err(|_| EvalStatus::RuntimeFatal)?),
        Err(_) => values.bool_value(false),
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
    eval_filesize_result(filename, values)
}

/// Returns one local file or supported wrapper size in bytes, or zero on failure.
pub(in crate::interpreter) fn eval_filesize_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
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
    eval_filetype_result(filename, values)
}

/// Returns the PHP filetype string for one path, or false when lstat fails.
pub(in crate::interpreter) fn eval_filetype_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
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
    eval_stat_array_result(name, filename, values)
}

/// Builds PHP's stat array for one local path, or returns false on stat failure.
pub(in crate::interpreter) fn eval_stat_array_result(
    name: &str,
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
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

/// Reads bytes from supported direct path or stream-wrapper URLs.
fn eval_read_path_or_wrapper_bytes(path: &str) -> Result<Vec<u8>, ()> {
    if stream_wrappers::is_data_stream(path) {
        return stream_wrappers::decode_data_uri(path).ok_or(());
    }
    if stream_wrappers::is_phar_stream(path) {
        return elephc_phar::extract_url_bytes(path.as_bytes()).ok_or(());
    }
    let Some(path) = stream_wrappers::local_filesystem_path(path) else {
        return Err(());
    };
    std::fs::read(path).map_err(|_| ())
}
