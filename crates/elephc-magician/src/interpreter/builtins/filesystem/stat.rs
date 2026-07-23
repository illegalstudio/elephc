//! Purpose:
//! Declarative eval registry entry for `stat`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stat-array helper.

eval_builtin! {
    name: "stat",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::stream_wrappers;
use super::*;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt as WindowsMetadataExt;

/// Dispatches direct eval calls for the `stat` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stat_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_stat_array("stat", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stat` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stat_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => eval_stat_array_result("stat", *filename, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
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
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(_) if name == "filemtime" => return values.int(0),
        Err(_) => return values.bool_value(false),
    };
    #[cfg(unix)]
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
    #[cfg(windows)]
    let windows_info = eval_windows_file_info(&path);
    #[cfg(windows)]
    match name {
        "fileatime" => values.int(eval_windows_filetime_seconds(metadata.last_access_time())),
        "filectime" => values.int(eval_windows_filetime_seconds(metadata.creation_time())),
        "filegroup" | "fileowner" => values.int(0),
        "fileinode" => values.int(
            windows_info
                .and_then(|info| i64::try_from(info.file_index).ok())
                .unwrap_or(0),
        ),
        "filemtime" => values.int(eval_windows_filetime_seconds(metadata.last_write_time())),
        "fileperms" => {
            let physical_mode = eval_windows_metadata_mode(&metadata);
            let mode = context
                .local_file_mode(&path)
                .map(|permissions| (physical_mode & !0o7777) | i64::from(permissions))
                .unwrap_or(physical_mode);
            values.int(mode)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
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
        "stat" => std::fs::metadata(&path),
        "lstat" => std::fs::symlink_metadata(&path),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let metadata = match metadata {
        Ok(metadata) => metadata,
        Err(_) => return values.bool_value(false),
    };
    #[cfg(unix)]
    return eval_stat_metadata_array(&metadata, values);
    #[cfg(windows)]
    eval_stat_metadata_array_with_windows_info(
        &metadata,
        eval_windows_file_info(&path),
        context.local_file_mode(&path),
        values,
    )
}

/// Converts filesystem metadata into PHP's numeric-and-string keyed stat array.
pub(in crate::interpreter) fn eval_stat_metadata_array(
    metadata: &std::fs::Metadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    #[cfg(unix)]
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
    #[cfg(windows)]
    return eval_stat_metadata_array_with_windows_info(metadata, None, None, values);
    #[cfg(unix)]
    {
    let mut result = values.assoc_new(fields.len() * 2)?;
    for (index, (name, value)) in fields.iter().enumerate() {
        result = eval_stat_array_set_int_key(result, index, *value, values)?;
        result = eval_stat_array_set_string_key(result, name, *value, values)?;
    }
    Ok(result)
    }
}

/// Converts Windows metadata and optional handle identity into PHP's stat array.
#[cfg(windows)]
fn eval_stat_metadata_array_with_windows_info(
    metadata: &std::fs::Metadata,
    info: Option<WindowsFileInfo>,
    mode_override: Option<u32>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (device, inode, links) = info
        .map(|info| {
            (
                i64::from(info.volume_serial),
                i64::try_from(info.file_index).unwrap_or(0),
                i64::from(info.number_of_links),
            )
        })
        .unwrap_or((0, 0, 1));
    let physical_mode = eval_windows_metadata_mode(metadata);
    let mode = mode_override
        .map(|permissions| (physical_mode & !0o7777) | i64::from(permissions))
        .unwrap_or(physical_mode);
    let fields = [
        ("dev", device),
        ("ino", inode),
        ("mode", mode),
        ("nlink", links),
        ("uid", 0),
        ("gid", 0),
        ("rdev", 0),
        ("size", eval_u64_to_i64(metadata.file_size())?),
        ("atime", eval_windows_filetime_seconds(metadata.last_access_time())),
        ("mtime", eval_windows_filetime_seconds(metadata.last_write_time())),
        ("ctime", eval_windows_filetime_seconds(metadata.creation_time())),
        ("blksize", 0),
        ("blocks", 0),
    ];
    let mut result = values.assoc_new(fields.len() * 2)?;
    for (index, (name, value)) in fields.iter().enumerate() {
        result = eval_stat_array_set_int_key(result, index, *value, values)?;
        result = eval_stat_array_set_string_key(result, name, *value, values)?;
    }
    Ok(result)
}

/// Stable file identity fields exposed by `GetFileInformationByHandle`.
#[cfg(windows)]
#[derive(Clone, Copy)]
struct WindowsFileInfo {
    volume_serial: u32,
    number_of_links: u32,
    file_index: u64,
}

/// Reads stable Windows volume, link-count, and file-index metadata for a path.
#[cfg(windows)]
fn eval_windows_file_info(path: &str) -> Option<WindowsFileInfo> {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[repr(C)]
    struct ByHandleFileInformation {
        attributes: u32,
        creation_time: FileTime,
        last_access_time: FileTime,
        last_write_time: FileTime,
        volume_serial: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        /// Reads filesystem identity and timestamps for one open Windows file handle.
        fn GetFileInformationByHandle(
            file: *mut c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
    }

    let file = std::fs::File::open(path).ok()?;
    let mut information = std::mem::MaybeUninit::<ByHandleFileInformation>::uninit();
    let status = unsafe {
        GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr())
    };
    if status == 0 {
        return None;
    }
    let information = unsafe { information.assume_init() };
    Some(WindowsFileInfo {
        volume_serial: information.volume_serial,
        number_of_links: information.number_of_links,
        file_index: (u64::from(information.file_index_high) << 32)
            | u64::from(information.file_index_low),
    })
}

/// Synthesizes PHP's POSIX-shaped mode bits from Windows metadata.
#[cfg(windows)]
fn eval_windows_metadata_mode(metadata: &std::fs::Metadata) -> i64 {
    const FILE_ATTRIBUTE_READONLY: u32 = 0x0000_0001;
    const S_IFDIR: i64 = 0o040000;
    const S_IFLNK: i64 = 0o120000;
    const S_IFREG: i64 = 0o100000;
    let kind = if metadata.file_type().is_symlink() {
        S_IFLNK
    } else if metadata.is_dir() {
        S_IFDIR
    } else {
        S_IFREG
    };
    let permissions = if metadata.file_attributes() & FILE_ATTRIBUTE_READONLY != 0 {
        0o444
    } else {
        0o666
    };
    kind | permissions
}

/// Converts a Windows FILETIME count to whole Unix epoch seconds.
#[cfg(windows)]
fn eval_windows_filetime_seconds(filetime: u64) -> i64 {
    const WINDOWS_TO_UNIX_EPOCH_100NS: u64 = 116_444_736_000_000_000;
    filetime
        .saturating_sub(WINDOWS_TO_UNIX_EPOCH_100NS)
        .checked_div(10_000_000)
        .and_then(|seconds| i64::try_from(seconds).ok())
        .unwrap_or(0)
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
