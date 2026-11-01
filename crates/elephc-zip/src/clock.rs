//! Purpose:
//! Supplies Unix timestamps for new entries without a libc/timezone dependency.
//!
//! Called from:
//! - `crate::model::Archive::add_from_string` and directory creation.
//!
//! Key details:
//! - Uses `SystemTime`; a pre-epoch clock answers 0.

/// Returns the current time as seconds since the Unix epoch.
pub(crate) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// Sets a file's modification time to `timestamp` seconds since the epoch.
pub(crate) fn set_mtime(path: &std::path::Path, timestamp: i64) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        let times = [
            libc_timespec { seconds: timestamp, nanoseconds: 0 },
            libc_timespec { seconds: timestamp, nanoseconds: 0 },
        ];
        let result = unsafe { utimensat(AT_FDCWD, c_path.as_ptr(), times.as_ptr(), 0) };
        if result == 0 { return Ok(()); }
        return Err(std::io::Error::last_os_error());
    }
    #[cfg(not(unix))]
    {
        let _ = (path, timestamp);
        Ok(())
    }
}

/// A minimal `timespec` matching the C layout on every supported target.
#[cfg(unix)]
#[repr(C)]
struct libc_timespec {
    /// Whole seconds.
    seconds: i64,
    /// Nanoseconds.
    nanoseconds: i64,
}

#[cfg(unix)]
const AT_FDCWD: i32 = -100;

#[cfg(unix)]
extern "C" {
    /// Sets file access/modification times at a path.
    fn utimensat(dirfd: i32, path: *const std::ffi::c_char, times: *const libc_timespec, flags: i32) -> i32;
}
