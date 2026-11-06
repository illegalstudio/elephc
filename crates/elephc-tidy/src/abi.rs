//! Purpose:
//! Shared C ABI plumbing for the `elephc-tidy` bridge: byte readers, panic
//! containment, the thread-local return-string cells, and the 1/0 flag helper.
//!
//! Called from:
//! - `crate::document`, `crate::option`, and `crate::node`.
//!
//! Key details:
//! - Returned strings live in `thread_local!` cells, one per entry point, valid
//!   until the next call of the same entry point on the same thread. A global
//!   `Mutex<CString>` would be a cross-thread use-after-free and fails
//!   `tests/ffi_buffer_hygiene.rs`.
//! - Nothing here unwinds: every public entry wraps its body in `catch_unwind`
//!   and answers the failure value instead.
//! - Input strings that may contain NUL bytes are read through an explicit
//!   length (`read_bytes`); paths and option names use NUL-terminated C strings
//!   exactly like libtidy's own API.

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::ffi::TidyBuffer;

/// The pointer every string entry point answers when its cell is unusable.
pub(crate) static EMPTY_C_STRING: [c_char; 1] = [0];

thread_local! {
    /// `elephc_tidy_output` return cell.
    pub(crate) static DOC_OUTPUT: RefCell<CString> = RefCell::new(CString::default());
    /// `elephc_tidy_error_buffer` return cell.
    pub(crate) static DOC_ERROR: RefCell<CString> = RefCell::new(CString::default());
    /// `elephc_tidy_option_name` return cell.
    pub(crate) static OPTION_NAME: RefCell<CString> = RefCell::new(CString::default());
    /// `elephc_tidy_option_value_str` return cell.
    pub(crate) static OPTION_VALUE: RefCell<CString> = RefCell::new(CString::default());
    /// `elephc_tidy_option_doc` return cell.
    pub(crate) static OPTION_DOC: RefCell<CString> = RefCell::new(CString::default());
    /// `elephc_tidy_node_name` return cell.
    pub(crate) static NODE_NAME: RefCell<CString> = RefCell::new(CString::default());
    /// `elephc_tidy_node_text` return cell.
    pub(crate) static NODE_TEXT: RefCell<CString> = RefCell::new(CString::default());
    /// `elephc_tidy_node_attr_name` return cell.
    pub(crate) static ATTR_NAME: RefCell<CString> = RefCell::new(CString::default());
    /// `elephc_tidy_node_attr_value` return cell.
    pub(crate) static ATTR_VALUE: RefCell<CString> = RefCell::new(CString::default());
    /// `elephc_tidy_release_date` return cell.
    pub(crate) static RELEASE_DATE: RefCell<CString> = RefCell::new(CString::default());
}

/// Copies a NUL-terminated C string argument; a null pointer reads as empty.
///
/// # Safety
/// `ptr` must be null or point at a NUL-terminated byte string.
pub(crate) unsafe fn c_bytes(ptr: *const c_char) -> Vec<u8> {
    if ptr.is_null() {
        Vec::new()
    } else {
        CStr::from_ptr(ptr).to_bytes().to_vec()
    }
}

/// Copies exactly `len` bytes from an input pointer, including interior NULs.
///
/// # Safety
/// `ptr` must point at `len` readable bytes when `len` is non-zero.
pub(crate) unsafe fn read_bytes(ptr: *const u8, len: usize) -> Vec<u8> {
    if len == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(ptr, len).to_vec()
    }
}

/// Stores `bytes` in a thread-local cell and returns a pointer valid until the
/// next call that uses the same cell on this thread.
pub(crate) fn stash(
    cell: &'static std::thread::LocalKey<RefCell<CString>>,
    bytes: Vec<u8>,
) -> *const c_char {
    // The C ABI returns NUL-terminated strings; a byte sequence cannot carry an
    // interior NUL through this shape. Truncate at the first NUL rather than
    // concatenating the surrounding fragments into text that never existed.
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    let owned = CString::new(bytes[..end].to_vec()).unwrap_or_default();
    cell.try_with(|slot| {
        *slot.borrow_mut() = owned;
        slot.borrow().as_ptr()
    })
    .unwrap_or(EMPTY_C_STRING.as_ptr())
}

/// Runs `body` under `catch_unwind`, answering `fallback` when it panics.
pub(crate) fn guarded<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(fallback)
}

/// Converts a Rust boolean into the ABI's `1` / `0`.
pub(crate) fn flag(value: bool) -> i64 {
    i64::from(value)
}

/// Copies a libtidy buffer's text, excluding its trailing NUL.
pub(crate) fn buffer_text(buffer: &TidyBuffer) -> Vec<u8> {
    if buffer.bp.is_null() || buffer.size == 0 {
        return Vec::new();
    }
    let len = (buffer.size - 1) as usize;
    unsafe { std::slice::from_raw_parts(buffer.bp, len).to_vec() }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the shared ABI helpers.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - The stash test proves interior NULs truncate the returned bytes, and the
    //!   guard test proves panics become fallbacks.

    use super::*;
    use std::ffi::CStr;

    /// Stashed bytes are readable through the returned pointer.
    #[test]
    fn stash_returns_a_readable_c_string() {
        let pointer = stash(&NODE_NAME, b"body".to_vec());
        assert_eq!(unsafe { CStr::from_ptr(pointer) }.to_bytes(), b"body");
    }

    /// Interior NUL bytes truncate the C string instead of merging the fragments.
    #[test]
    fn stash_truncates_at_interior_nul() {
        let pointer = stash(&NODE_TEXT, b"a\0b".to_vec());
        assert_eq!(unsafe { CStr::from_ptr(pointer) }.to_bytes(), b"a");
    }

    /// A panic is contained and becomes the declared fallback.
    #[test]
    fn guarded_contains_panics() {
        assert_eq!(guarded(-1, || panic!("boom")), -1);
    }

    /// Exact-length reads preserve every byte, including NUL.
    #[test]
    fn read_bytes_preserves_interior_nuls() {
        let bytes = b"a\0b";
        assert_eq!(unsafe { read_bytes(bytes.as_ptr(), bytes.len()) }, bytes);
    }
}
