//! Purpose:
//! Defines the result block the iconv bridge writes back, including the packed
//! encoding compiled programs unpack into a PHP array.
//!
//! Called from:
//! - `crate::abi::dispatch` while completing one call.
//! - `__rt_iconv_call` in the generated runtime, which reads the same layout.
//!
//! Key details:
//! - Owned buffers are `Box<[u8]>` leaks, so `release` can rebuild them with an exact
//!   capacity; nothing else may free them.
//! - The packed array format is length-prefixed throughout, so the runtime unpacker
//!   needs no delimiter scanning: `[count][ [key_len][key][value_count][ [len][bytes] … ] … ]`.
//! - A `value_count` of one is a plain string entry; a larger count is a PHP list, which
//!   is how `iconv_mime_decode_headers()` reports a repeated field name.

/// The call produced PHP `false`.
pub const KIND_FALSE: i64 = 0;
/// The call produced an integer.
pub const KIND_INT: i64 = 1;
/// The call produced a byte string.
pub const KIND_STRING: i64 = 2;
/// The call produced PHP `true`.
pub const KIND_TRUE: i64 = 3;
/// The call produced a packed associative array.
pub const KIND_ARRAY: i64 = 4;
/// The call must throw `iconv_strpos()`'s out-of-range `$offset` `ValueError`.
pub const KIND_OFFSET_VALUE_ERROR: i64 = 5;

/// Everything one iconv operation reports back to its caller.
#[repr(C)]
pub struct IconvResultBlock {
    /// One of the `KIND_*` constants.
    pub kind: i64,
    /// Integer payload for `KIND_INT`.
    pub int_value: i64,
    /// Owned payload for `KIND_STRING` and `KIND_ARRAY`.
    pub bytes: *mut u8,
    /// Length of `bytes`.
    pub len: u64,
    /// Owned diagnostic line to print before materializing the result.
    pub diagnostic: *mut u8,
    /// Length of `diagnostic`.
    pub diagnostic_len: u64,
}

impl IconvResultBlock {
    /// Resets the block to "produced PHP false, nothing owned".
    ///
    /// # Safety
    /// `block` must point at writable storage for one result block.
    pub unsafe fn reset(block: *mut IconvResultBlock) {
        (*block).kind = KIND_FALSE;
        (*block).int_value = 0;
        (*block).bytes = std::ptr::null_mut();
        (*block).len = 0;
        (*block).diagnostic = std::ptr::null_mut();
        (*block).diagnostic_len = 0;
    }

    /// Stores an owned byte payload under the given kind.
    ///
    /// # Safety
    /// `block` must point at a reset result block.
    pub unsafe fn set_bytes(block: *mut IconvResultBlock, kind: i64, bytes: Vec<u8>) {
        let (ptr, len) = leak(bytes);
        (*block).kind = kind;
        (*block).bytes = ptr;
        (*block).len = len;
    }

    /// Stores the diagnostic line the runtime should print before the result.
    ///
    /// # Safety
    /// `block` must point at a reset result block.
    pub unsafe fn set_diagnostic(block: *mut IconvResultBlock, message: String) {
        let (ptr, len) = leak(message.into_bytes());
        (*block).diagnostic = ptr;
        (*block).diagnostic_len = len;
    }

    /// Frees both owned payloads and resets the block.
    ///
    /// # Safety
    /// `block` must point at a block this crate filled in and has not released yet.
    pub unsafe fn release(block: *mut IconvResultBlock) {
        reclaim((*block).bytes, (*block).len);
        reclaim((*block).diagnostic, (*block).diagnostic_len);
        IconvResultBlock::reset(block);
    }
}

/// Packs ordered name/value entries into the runtime's length-prefixed array format.
pub fn pack_entries(entries: &[(Vec<u8>, Vec<Vec<u8>>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(entries.len() as u64).to_ne_bytes());
    for (key, values) in entries {
        out.extend_from_slice(&(key.len() as u64).to_ne_bytes());
        out.extend_from_slice(key);
        out.extend_from_slice(&(values.len() as u64).to_ne_bytes());
        for value in values {
            out.extend_from_slice(&(value.len() as u64).to_ne_bytes());
            out.extend_from_slice(value);
        }
    }
    out
}

/// Leaks an owned byte buffer as an exact-capacity pointer/length pair.
fn leak(bytes: Vec<u8>) -> (*mut u8, u64) {
    let boxed = bytes.into_boxed_slice();
    let len = boxed.len() as u64;
    (Box::into_raw(boxed).cast::<u8>(), len)
}

/// Rebuilds and drops a buffer previously produced by [`leak`].
fn reclaim(ptr: *mut u8, len: u64) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            ptr,
            len as usize,
        )));
    }
}
