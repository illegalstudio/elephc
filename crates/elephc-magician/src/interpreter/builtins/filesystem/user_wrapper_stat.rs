//! Purpose:
//! Interprets userspace stream-wrapper stat results for path and stream builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::file_exists`, `stat`,
//!   `filesize`, and `filetype` for wrapper-backed paths.
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

/// The `url_stat()` flags php passes each caller.
///
/// Measured on php 8.5.6 one call at a time, with `clearstatcache(true)` between probes — php's
/// one-entry stat cache answers the second read of a path, so without that the dispatch never
/// happens and the flags never show. `PHP_STREAM_URL_STAT_NOCACHE` is 4, `_QUIET` 2, `_LINK` 1.
pub(in crate::interpreter) fn eval_url_stat_flags(name: &str) -> i64 {
    match name {
        // The only predicate that does not follow the last symlink.
        "is_link" => 7,
        // Silent about their own failures.
        "file_exists" | "is_dir" | "is_executable" | "is_file" | "is_readable" | "is_writable"
        | "is_writeable" => 6,
        // Report a failure, and do not follow the last symlink.
        "filetype" | "lstat" => 5,
        // Report a failure.
        _ => 4,
    }
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
        "is_file" => eval_mode_kind(mode) == libc::S_IFREG as i64,
        "is_link" => eval_mode_kind(mode) == libc::S_IFLNK as i64,
        "is_readable" | "is_writable" | "is_writeable" | "is_executable" => {
            let uid = eval_user_wrapper_stat_int_field(stat, "uid", values)?.unwrap_or(0);
            let gid = eval_user_wrapper_stat_int_field(stat, "gid", values)?.unwrap_or(0);
            let which = match name {
                "is_readable" => 0,
                "is_executable" => 2,
                _ => 1,
            };
            mode & (eval_access_triad_base(uid, gid) >> which) != 0
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(result)
}

/// Picks the permission triad php tests for a wrapper path, as that triad's `S_IRUSR`-style bit.
///
/// php does NOT run `access(2)` on a `scheme://` path — it compares the array's `uid`/`gid` against
/// the process and tests ONE bit of the mode, so the answer can contradict the filesystem entirely.
/// Measured on php 8.5.6: the owner triad wins on a uid match whatever the group bits say; failing
/// that the group triad wins on `getgid()` OR any supplementary group; failing that the world triad
/// answers, so `0400` owned by another user reads as unreadable and `0004` as readable. Shifting
/// the returned bit right by 0/1/2 walks it to read/write/execute.
fn eval_access_triad_base(uid: i64, gid: i64) -> i64 {
    // SAFETY: getuid/getgid take no arguments and cannot fail. getgroups writes at most the slot
    // count it is given; a process in more groups than that falls through to php's world triad
    // rather than reading past the buffer.
    unsafe {
        if uid == i64::from(libc::getuid()) {
            return 0o400;
        }
        if gid == i64::from(libc::getgid()) {
            return 0o40;
        }
        let mut groups = [0 as libc::gid_t; 64];
        let written = libc::getgroups(groups.len() as libc::c_int, groups.as_mut_ptr());
        if written > 0
            && groups[..written as usize]
                .iter()
                .any(|group| i64::from(*group) == gid)
        {
            return 0o40;
        }
    }
    0o4
}

/// php's stat fields, in the documented order its numeric keys follow.
const STAT_FIELDS: [&str; 13] = [
    "dev", "ino", "mode", "nlink", "uid", "gid", "rdev", "size", "atime", "mtime", "ctime",
    "blksize", "blocks",
];

/// Rebuilds php's canonical 26-entry stat array out of a wrapper's own array.
///
/// `stat()` does NOT hand the wrapper's array back: php fills a `php_stream_statbuf` from it and
/// then converts that, so the result always carries the 13 numeric keys as well as the 13 string
/// ones and every field the wrapper did not name reads as `0`. Measured on php 8.5.6 — a wrapper
/// answering `['mode' => 0100644]` still gives `count(stat(...)) === 26`.
pub(in crate::interpreter) fn eval_user_wrapper_stat_array_from_stat(
    stat: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !values.truthy(stat)? {
        return values.bool_value(false);
    }
    let mut result = values.assoc_new(STAT_FIELDS.len() * 2)?;
    for (index, field) in STAT_FIELDS.iter().enumerate() {
        let value = eval_user_wrapper_stat_int_field(stat, field, values)?.unwrap_or(0);
        result = super::stat::eval_stat_array_set_int_key(result, index, value, values)?;
        result = super::stat::eval_stat_array_set_string_key(result, field, value, values)?;
    }
    Ok(result)
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
        // `filemtime` used to be carved out here to answer `int(0)`, alone among the eight names
        // this function serves. PHP returns `false` from every one of them when the field cannot
        // be read, so the carve-out was a divergence, and it was invisible from the compiled side
        // where the same call now yields `false`.
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
