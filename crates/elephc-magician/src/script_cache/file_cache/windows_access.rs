//! Purpose:
//! Applies PHP OPcache's Windows CRT access check to Unicode file-cache paths.
//!
//! Called from:
//! - `super::is_usable_cache_directory()` after absolute-path and directory validation.
//!
//! Key details:
//! - PHP 8.5.10 calls `_access` with mode `04` for read-only and `06` otherwise.
//! - `_waccess` is the same CRT operation over UTF-16, avoiding active-code-page loss.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

unsafe extern "C" {
    /// Wide-character form of the Windows CRT `_access` function used by OPcache.
    #[link_name = "_waccess"]
    fn windows_waccess(path: *const u16, mode: libc::c_int) -> libc::c_int;
}

/// Checks one Unicode directory with OPcache's exact Windows read/write mode.
pub(super) fn directory_access(path: &Path, read_only: bool) -> bool {
    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if wide.contains(&0) {
        return false;
    }
    wide.push(0);
    // SAFETY: `wide` is a live NUL-terminated UTF-16 path and the shared helper returns only
    // the Windows CRT's supported `04` or `06` access modes.
    unsafe { windows_waccess(wide.as_ptr(), super::cache_directory_access_mode(read_only)) == 0 }
}
