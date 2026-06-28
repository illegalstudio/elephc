//! Purpose:
//! Interprets userspace stream-wrapper stat results for path and stream builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::file_io` when a path belongs
//!   to a registered eval userspace stream wrapper.
//! - `crate::interpreter::builtins::filesystem::streams` when `fstat()` sees a
//!   userspace-wrapper stream resource.
//!
//! Key details:
//! - The wrapper owns the stat array shape. These helpers read the PHP-standard
//!   string keys used by file probes, scalar stat builtins, and `filetype()`.

use super::super::super::*;

/// Dispatches `fstat()` to a wrapper object's `stream_stat()`.
pub(in crate::interpreter) fn eval_user_wrapper_fstat_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(None);
    };
    let Some((declaring_class, stream_stat)) =
        eval_user_wrapper_method(&info.class_name, "stream_stat", context)
    else {
        return values.bool_value(false).map(Some);
    };
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &info.class_name,
        &stream_stat,
        info.object,
        Vec::new(),
        context,
        values,
    )?;
    Ok(Some(result))
}

/// Computes one filesystem predicate from a userspace wrapper `url_stat()` result.
pub(in crate::interpreter) fn eval_user_wrapper_file_probe_from_stat(
    name: &str,
    stat: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !values.truthy(stat)? {
        return values.bool_value(false);
    }
    let mode = eval_user_wrapper_stat_int_field(stat, "mode", values)?.unwrap_or(0);
    let result = match name {
        "file_exists" => true,
        "is_dir" => eval_mode_kind(mode) == libc::S_IFDIR as i64,
        "is_executable" => mode & 0o111 != 0,
        "is_file" => eval_mode_kind(mode) == libc::S_IFREG as i64,
        "is_link" => eval_mode_kind(mode) == libc::S_IFLNK as i64,
        "is_readable" => mode & 0o444 != 0,
        "is_writable" | "is_writeable" => mode & 0o222 != 0,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(result)
}

/// Returns one scalar stat builtin value from a userspace wrapper stat array.
pub(in crate::interpreter) fn eval_user_wrapper_file_stat_scalar_from_stat(
    name: &str,
    stat: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let field = match name {
        "fileatime" => "atime",
        "filectime" => "ctime",
        "filegroup" => "gid",
        "fileinode" => "ino",
        "filemtime" => "mtime",
        "fileowner" => "uid",
        "fileperms" => "mode",
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    match eval_user_wrapper_stat_int_field(stat, field, values)? {
        Some(value) => values.int(value),
        None if name == "filemtime" => values.int(0),
        None => values.bool_value(false),
    }
}

/// Extracts one integer field from a userspace wrapper stat result.
pub(in crate::interpreter) fn eval_user_wrapper_stat_int_field(
    stat: RuntimeCellHandle,
    field: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<i64>, EvalStatus> {
    if !values.truthy(stat)? {
        return Ok(None);
    }
    let key = values.string(field)?;
    let value = values.array_get(stat, key)?;
    Ok(Some(eval_int_value(value, values)?))
}

/// Maps one POSIX mode value to PHP's `filetype()` label.
pub(in crate::interpreter) fn eval_filetype_label_from_mode(mode: i64) -> &'static str {
    match eval_mode_kind(mode) {
        kind if kind == libc::S_IFREG as i64 => "file",
        kind if kind == libc::S_IFDIR as i64 => "dir",
        kind if kind == libc::S_IFLNK as i64 => "link",
        kind if kind == libc::S_IFCHR as i64 => "char",
        kind if kind == libc::S_IFBLK as i64 => "block",
        kind if kind == libc::S_IFIFO as i64 => "fifo",
        kind if kind == libc::S_IFSOCK as i64 => "socket",
        _ => "unknown",
    }
}

/// Masks one POSIX mode value down to its file-kind bits.
fn eval_mode_kind(mode: i64) -> i64 {
    mode & (libc::S_IFMT as i64)
}
