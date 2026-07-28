//! Purpose:
//! Shared filesystem path conversion and permission helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem` re-exports.
//!
//! Key details:
//! - Helpers return PHP-compatible false/null/string/int cells via `RuntimeValueOps`.

use super::super::super::*;

/// Converts one eval value to a filesystem path string.
pub(in crate::interpreter) fn eval_path_string(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let filename = values.string_bytes(filename)?;
    Ok(String::from_utf8_lossy(&filename).into_owned())
}

/// Returns whether a path can be opened for reading by the current process.
pub(in crate::interpreter) fn eval_path_is_readable(path: &std::path::Path) -> bool {
    std::fs::File::open(path).is_ok() || std::fs::read_dir(path).is_ok()
}

/// Returns whether a path has any executable bit set in its Unix mode.
pub(in crate::interpreter) fn eval_path_is_executable(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
    std::fs::metadata(path)
        .map(|metadata| metadata.mode() & 0o111 != 0)
        .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        path.is_file()
            && path
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|extension| {
                    matches!(extension.to_ascii_lowercase().as_str(), "exe" | "com" | "bat" | "cmd")
                })
    }
}

/// Returns whether a path can be written by the current process.
pub(in crate::interpreter) fn eval_path_is_writable(path: &std::path::Path) -> bool {
    if path.is_file() {
        return std::fs::OpenOptions::new().write(true).open(path).is_ok();
    }
    if !path.is_dir() {
        return false;
    }
    let probe = path.join(format!(
        ".elephc_magician_writable_probe_{}",
        std::process::id()
    ));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}
