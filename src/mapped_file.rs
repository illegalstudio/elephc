//! Purpose:
//! Read-only memory map of a file, used to split the user assembly without reading it into the
//! heap.
//!
//! Called from:
//! - `crate::linker::assemble_parallel()`, which splits the user assembly.
//! - `crate::codegen::user_assembly`, which reads back the text the code generator spilled.
//!
//! Key details:
//! - A private read-only mapping is clean and file-backed: the kernel may drop its pages under
//!   memory pressure and fault them back in, so a 1.5 GB `.s` no longer costs 1.5 GB of the
//!   build's resident footprint the way `read_to_string` did.
//! - Any failure (empty file, `mmap` refusal, non-Unix host) returns `None`, and the caller
//!   assembles the file whole — the behavior it had before splitting existed.

use std::path::Path;

/// A read-only mapping of one file's bytes, unmapped on drop.
pub(crate) struct MappedFile {
    ptr: *mut std::ffi::c_void,
    len: usize,
}

impl MappedFile {
    /// Maps `path` read-only, or returns `None` when it cannot or need not be mapped.
    #[cfg(unix)]
    pub(crate) fn open(path: &Path) -> Option<Self> {
        use std::os::fd::AsRawFd;
        let file = std::fs::File::open(path).ok()?;
        let len = usize::try_from(file.metadata().ok()?.len()).ok()?;
        if len == 0 {
            return None;
        }
        // SAFETY: a fresh read-only private mapping of an open descriptor; the mapping outlives
        // the descriptor, which is allowed, and the file is not written while it is mapped.
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_PRIVATE,
                file.as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return None;
        }
        Some(Self { ptr, len })
    }

    /// Non-Unix hosts never map; the caller falls back to assembling the file whole.
    #[cfg(not(unix))]
    pub(crate) fn open(_path: &Path) -> Option<Self> {
        None
    }

    /// The mapped bytes.
    pub(crate) fn bytes(&self) -> &[u8] {
        // SAFETY: `ptr` maps exactly `len` readable bytes for as long as `self` lives.
        unsafe { std::slice::from_raw_parts(self.ptr as *const u8, self.len) }
    }
}

impl Drop for MappedFile {
    fn drop(&mut self) {
        #[cfg(unix)]
        // SAFETY: unmaps exactly the region `open` mapped, once.
        unsafe {
            libc::munmap(self.ptr, self.len);
        }
    }
}
