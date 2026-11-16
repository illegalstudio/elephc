//! Purpose:
//! The C entries compiled programs call to highlight PHP source.
//!
//! Called from:
//! - The `__elephc_highlight_string`/`__elephc_highlight_file` prelude functions a compiled
//!   program carries when it calls `highlight_string()`/`highlight_file()`.
//!
//! Key details:
//! - The source arrives as a pointer plus an explicit length, so a source holding NUL bytes is
//!   rendered whole (the caller's C-string conversion copies every byte before its terminator).
//! - The answer is binary-safe: [`elephc_highlight_render`] returns its byte length and
//!   [`elephc_highlight_rendered_ptr`] its bytes, which the prelude copies out with
//!   `__elephc_ptr_read_string`. php copies a source NUL byte through to the HTML, so the
//!   document cannot travel as a C string. The bytes live in a thread-local slot, valid until
//!   the next render on the same thread.

use std::cell::RefCell;
use std::ffi::c_char;

thread_local! {
    static RENDERED: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// C ABI: renders `len` bytes of PHP source at `source` as `highlight_string($source, true)`
/// does, keeps the HTML in this thread's slot and answers its length in bytes.
///
/// # Safety
/// `source` must point to at least `len` readable bytes, or be null with `len <= 0`.
#[no_mangle]
pub unsafe extern "C" fn elephc_highlight_render(source: *const c_char, len: i64) -> i64 {
    let bytes: &[u8] = if source.is_null() || len <= 0 {
        &[]
    } else {
        std::slice::from_raw_parts(source.cast::<u8>(), len as usize)
    };
    let html = crate::render::highlight_document(bytes);
    let length = html.len() as i64;
    RENDERED.with(|slot| *slot.borrow_mut() = html);
    length
}

/// C ABI: the bytes of the document the last [`elephc_highlight_render`] on this thread
/// produced; as many bytes as that call answered are readable until the next render.
#[no_mangle]
pub extern "C" fn elephc_highlight_rendered_ptr() -> *const c_char {
    RENDERED.with(|slot| slot.borrow().as_ptr().cast::<c_char>())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Copies the last rendered document out of the thread-local slot.
    fn rendered(length: i64) -> Vec<u8> {
        let ptr = elephc_highlight_rendered_ptr().cast::<u8>();
        unsafe { std::slice::from_raw_parts(ptr, length as usize) }.to_vec()
    }

    /// The C entry answers the same document as the Rust renderer, and reads exactly `len` bytes.
    #[test]
    fn c_entry_renders_the_given_bytes() {
        let source = b"<?php echo 1;\nignored";
        let length = unsafe { elephc_highlight_render(source.as_ptr().cast(), 14) };
        assert_eq!(rendered(length), crate::render::highlight_document(&source[..14]));
    }

    /// A null or empty source renders PHP's empty document.
    #[test]
    fn empty_source_renders_the_empty_document() {
        let length = unsafe { elephc_highlight_render(std::ptr::null(), 0) };
        assert_eq!(rendered(length), crate::render::highlight_document(b""));
    }

    /// A NUL byte in the source reaches the answer with everything after it: php copies the byte
    /// through, so `"a\0b"` keeps its closing quote and the closing tags.
    #[test]
    fn nul_byte_keeps_the_rest_of_the_document() {
        let source = b"<?php echo \"a\0b\";";
        let length = unsafe { elephc_highlight_render(source.as_ptr().cast(), source.len() as i64) };
        let html = rendered(length);
        assert_eq!(html, crate::render::highlight_document(source));
        assert!(html.ends_with(b"</code></pre>"));
    }
}
