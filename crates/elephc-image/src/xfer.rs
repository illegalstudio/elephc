//! Purpose:
//! Shared binary-transfer plumbing for the metadata bridges (Exif + IPTC). Holds
//! three process-global cells — an input staging buffer, an output buffer, and a
//! key/value result list — plus the C ABI accessors the prelude uses to move
//! bytes across the boundary without raw PHP-owned pointers.
//!
//! Called from:
//! - `crate::exif` and `crate::iptc` (Rust side) fill/read these cells.
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`:
//!   `elephc_img_in_ptr` (write a PHP string in), `elephc_img_out_ptr` (read bytes
//!   out, paired with the per-call length the producer returns), and
//!   `elephc_img_kv_count` / `elephc_img_kv_key` / `elephc_img_kv_val` (enumerate
//!   a parsed `key => value` result, e.g. EXIF fields or IPTC datasets).
//!
//! Key details:
//! - elephc programs are single-threaded and every transfer is a synchronous
//!   fill→consume pair, so a pointer returned here stays valid until the next call
//!   that rewrites the same cell. The prelude always copies (`ptr_read_string`)
//!   before issuing another bridge call.
//! - Values are arbitrary bytes (IPTC datasets are binary, EXIF UserComment may
//!   contain NULs), so the key/value accessors report a length and the prelude
//!   uses the length-based `ptr_read_string`, never NUL scanning.

use std::sync::{Mutex, OnceLock};

use crate::{ffi_guard, lock_recover};

/// Input staging buffer: the prelude resizes it via `elephc_img_in_ptr`, copies a
/// PHP string into it with `ptr_write_string`, then calls a parser/embedder that
/// reads the first `len` bytes back through [`in_bytes`].
fn in_cell() -> &'static Mutex<Vec<u8>> {
    static CELL: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();
    CELL.get_or_init(Mutex::default)
}

/// Output buffer holding the most recent produced bytes (an EXIF field value, a
/// tag name, an extracted thumbnail, or an embedded-IPTC JPEG). The producer
/// returns the byte length and the prelude copies it out via `elephc_img_out_ptr`
/// + `ptr_read_string`.
fn out_cell() -> &'static Mutex<Vec<u8>> {
    static CELL: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();
    CELL.get_or_init(Mutex::default)
}

/// Parsed `key => value` result list, populated by an EXIF or IPTC parse and
/// enumerated by the prelude. A key may repeat (IPTC datasets such as keywords
/// occur multiple times); the prelude appends repeats into a sub-array.
fn kv_list() -> &'static Mutex<Vec<(String, Vec<u8>)>> {
    static CELL: OnceLock<Mutex<Vec<(String, Vec<u8>)>>> = OnceLock::new();
    CELL.get_or_init(Mutex::default)
}

/// Replaces the output buffer with `bytes` and returns its length, the value the
/// producing entry point hands back to the prelude.
pub(crate) fn set_out(bytes: Vec<u8>) -> i64 {
    let len = bytes.len() as i64;
    *lock_recover(out_cell()) = bytes;
    len
}

/// Returns a copy of the first `len` bytes of the input staging buffer, or `None`
/// if `len` is negative or exceeds what the prelude actually staged.
pub(crate) fn in_bytes(len: i64) -> Option<Vec<u8>> {
    if len < 0 {
        return None;
    }
    let guard = lock_recover(in_cell());
    let len = len as usize;
    if guard.len() < len {
        return None;
    }
    Some(guard[..len].to_vec())
}

/// Stores a freshly parsed `key => value` list, returning the entry count for the
/// parser to return to the prelude.
pub(crate) fn set_kv(list: Vec<(String, Vec<u8>)>) -> i64 {
    let n = list.len() as i64;
    *lock_recover(kv_list()) = list;
    n
}

/// Resizes the input staging buffer to `len` zero bytes and returns a writable
/// pointer to its start, or null for a non-positive length. Backs the binary
/// `string` inputs of `iptcparse` and `iptcembed`.
#[no_mangle]
pub extern "C" fn elephc_img_in_ptr(len: i64) -> *mut u8 {
    ffi_guard(std::ptr::null_mut(), move || {
        if len <= 0 {
            return std::ptr::null_mut();
        }
        let mut guard = lock_recover(in_cell());
        guard.clear();
        guard.resize(len as usize, 0);
        guard.as_mut_ptr()
    })
}

/// Returns a read pointer to the output buffer's bytes, valid until the next call
/// that rewrites the buffer. The prelude reads it immediately after a producer
/// returned a non-negative length.
#[no_mangle]
pub extern "C" fn elephc_img_out_ptr() -> *const u8 {
    ffi_guard(std::ptr::null(), move || {
        lock_recover(out_cell()).as_ptr()
    })
}

/// Returns the number of entries in the current parsed key/value result list.
#[no_mangle]
pub extern "C" fn elephc_img_kv_count() -> i64 {
    ffi_guard(-1, move || {
        lock_recover(kv_list()).len() as i64
    })
}

/// Copies the key of result entry `index` into the output buffer and returns its
/// byte length, or `-1` if the index is out of range.
#[no_mangle]
pub extern "C" fn elephc_img_kv_key(index: i64) -> i64 {
    ffi_guard(-1, move || {
        let guard = lock_recover(kv_list());
        let Some((key, _)) = usize::try_from(index).ok().and_then(|i| guard.get(i)) else {
            return -1;
        };
        let bytes = key.clone().into_bytes();
        drop(guard);
        set_out(bytes)
    })
}

/// Copies the value of result entry `index` into the output buffer and returns its
/// byte length, or `-1` if the index is out of range.
#[no_mangle]
pub extern "C" fn elephc_img_kv_val(index: i64) -> i64 {
    ffi_guard(-1, move || {
        let guard = lock_recover(kv_list());
        let Some((_, val)) = usize::try_from(index).ok().and_then(|i| guard.get(i)) else {
            return -1;
        };
        let bytes = val.clone();
        drop(guard);
        set_out(bytes)
    })
}
