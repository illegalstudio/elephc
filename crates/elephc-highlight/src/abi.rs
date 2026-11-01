//! Purpose:
//! The C entry compiled programs call to highlight PHP source.
//!
//! Called from:
//! - The `__elephc_highlight_string`/`__elephc_highlight_file` prelude functions a compiled
//!   program carries when it calls `highlight_string()`/`highlight_file()`.
//!
//! Key details:
//! - The source arrives as a pointer plus an explicit length, so a source holding NUL bytes is
//!   rendered whole (the caller's C-string conversion copies every byte before its terminator).
//! - The answer is a NUL-terminated string owned by a thread-local slot and valid until the next
//!   call on the same thread; the caller copies it into a PHP string immediately.

use std::cell::RefCell;
use std::ffi::{c_char, CString};

thread_local! {
    static RENDERED: RefCell<CString> = RefCell::new(CString::default());
}

/// C ABI: renders `len` bytes of PHP source at `source` as `highlight_string($source, true)`
/// does, and answers the HTML as a NUL-terminated string.
///
/// # Safety
/// `source` must point to at least `len` readable bytes, or be null with `len <= 0`.
#[no_mangle]
pub unsafe extern "C" fn elephc_highlight_render(source: *const c_char, len: i64) -> *const c_char {
    let bytes: &[u8] = if source.is_null() || len <= 0 {
        &[]
    } else {
        std::slice::from_raw_parts(source.cast::<u8>(), len as usize)
    };
    let mut html = crate::render::highlight_document(bytes);
    // The highlighter copies source bytes through, NUL included; a C string cannot carry one,
    // so the answer stops there as any C-string boundary would.
    if let Some(nul) = html.iter().position(|&byte| byte == 0) {
        html.truncate(nul);
    }
    let html = CString::new(html).unwrap_or_default();
    RENDERED.with(|slot| {
        let mut slot = slot.borrow_mut();
        *slot = html;
        slot.as_ptr()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    /// The C entry answers the same document as the Rust renderer, and reads exactly `len` bytes.
    #[test]
    fn c_entry_renders_the_given_bytes() {
        let source = b"<?php echo 1;\nignored";
        let html = unsafe { CStr::from_ptr(elephc_highlight_render(source.as_ptr().cast(), 14)) };
        assert_eq!(
            html.to_bytes(),
            crate::render::highlight_document(&source[..14]).as_slice()
        );
    }

    /// A null or empty source renders PHP's empty document.
    #[test]
    fn empty_source_renders_the_empty_document() {
        let html = unsafe { CStr::from_ptr(elephc_highlight_render(std::ptr::null(), 0)) };
        assert_eq!(html.to_bytes(), crate::render::highlight_document(b"").as_slice());
    }
}
