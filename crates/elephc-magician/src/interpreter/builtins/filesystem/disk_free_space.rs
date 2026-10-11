//! Purpose:
//! Declarative eval registry entry for `disk_free_space`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the disk-space helper.

eval_builtin! {
    contract: "disk_free_space",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `disk_free_space` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_disk_free_space_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_disk_space("disk_free_space", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `disk_free_space` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_disk_free_space_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [directory] => eval_disk_space_result("disk_free_space", *directory, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `disk_free_space($directory)` or `disk_total_space($directory)`.
pub(in crate::interpreter) fn eval_builtin_disk_space(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    eval_disk_space_result(name, directory, values)
}

/// Reports available or total filesystem bytes as a PHP float, or `false` on failure.
///
/// A zero-byte result remains a successful float; path conversion or `statvfs` failure returns
/// a boolean false cell.
pub(in crate::interpreter) fn eval_disk_space_result(
    name: &str,
    directory: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(directory)?;
    #[cfg(unix)]
    let result = eval_disk_space_bytes_unix(name, &bytes)?;
    #[cfg(windows)]
    let result = eval_disk_space_bytes_windows(name, &bytes)?;
    match result {
        Some(bytes) => values.float(bytes),
        None => values.bool_value(false),
    }
}

/// Queries Unix filesystem capacity through `statvfs`.
#[cfg(unix)]
fn eval_disk_space_bytes_unix(name: &str, bytes: &[u8]) -> Result<Option<f64>, EvalStatus> {
    let Ok(path) = CString::new(bytes) else {
        return Ok(None);
    };
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    let status = unsafe {
        // libc writes the statvfs fields for this NUL-terminated local path.
        libc::statvfs(path.as_ptr(), stats.as_mut_ptr())
    };
    if status != 0 {
        return Ok(None);
    }
    let stats = unsafe {
        // `statvfs` succeeded, so libc initialized the full stat buffer.
        stats.assume_init()
    };
    let block_size = if stats.f_frsize > 0 {
        stats.f_frsize
    } else {
        stats.f_bsize
    };
    let blocks = match name {
        "disk_free_space" => stats.f_bavail,
        "disk_total_space" => stats.f_blocks,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    Ok(Some((block_size as f64) * (blocks as f64)))
}

/// Queries Windows filesystem capacity through `GetDiskFreeSpaceExW`.
#[cfg(windows)]
fn eval_disk_space_bytes_windows(name: &str, bytes: &[u8]) -> Result<Option<f64>, EvalStatus> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        /// Reads total and available byte counts for the filesystem containing a Windows path.
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_for_caller: *mut u64,
            total_bytes: *mut u64,
            total_free: *mut u64,
        ) -> i32;
    }

    let path = String::from_utf8_lossy(bytes);
    let wide: Vec<u16> = std::ffi::OsStr::new(path.as_ref())
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut available = 0_u64;
    let mut total = 0_u64;
    let mut free = 0_u64;
    let status = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            &mut total,
            &mut free,
        )
    };
    if status == 0 {
        return Ok(None);
    }
    let bytes = match name {
        "disk_free_space" => available,
        "disk_total_space" => total,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    Ok(Some(bytes as f64))
}

#[cfg(all(test, unix))]
mod tests {
    use super::eval_disk_space_bytes_unix;

    /// Filesystem probes fail as PHP `false` candidates rather than as evaluator fatals.
    #[test]
    fn invalid_or_missing_path_returns_none() {
        assert!(matches!(
            eval_disk_space_bytes_unix("disk_free_space", b"bad\0path"),
            Ok(None)
        ));
        assert!(matches!(
            eval_disk_space_bytes_unix("disk_total_space", b"/definitely/not/a/real/path"),
            Ok(None)
        ));
    }
}
