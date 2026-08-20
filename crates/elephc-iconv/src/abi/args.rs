//! Purpose:
//! Defines the argument block compiled programs fill in before calling into the
//! iconv bridge, and the safe accessors the dispatcher reads it through.
//!
//! Called from:
//! - `crate::abi::dispatch` while decoding one call.
//! - The AOT lowering in `src/codegen/lower_inst/builtins/iconv.rs`, which must keep its
//!   slot offsets identical to this layout.
//!
//! Key details:
//! - Every argument occupies one uniform 32-byte slot, so the generated code stages all
//!   arguments through the same target-neutral stack-field helpers.
//! - `present` distinguishes an omitted or `null` argument from an explicitly empty
//!   string, which PHP resolves to two different charsets.
//! - All pointers are borrowed for the duration of one call; the bridge never stores them.

/// One staged PHP argument: a string, an integer, or nothing at all.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IconvArgSlot {
    /// Non-zero when the PHP call supplied this argument.
    pub present: i64,
    /// Borrowed byte pointer for string arguments.
    pub ptr: *const u8,
    /// Byte length for string arguments.
    pub len: u64,
    /// Value for integer arguments.
    pub int_value: i64,
}

/// Number of argument slots the block reserves.
pub const SLOT_COUNT: usize = 8;

/// Argument block staged on the caller's stack for one iconv operation.
#[repr(C)]
pub struct IconvCallArgs {
    /// Selected operation, one of the `OP_*` constants.
    pub op: i64,
    /// Reserved so the slot array starts on a 16-byte boundary.
    pub reserved: i64,
    /// Staged PHP arguments in the order each operation documents.
    pub slots: [IconvArgSlot; SLOT_COUNT],
}

impl IconvCallArgs {
    /// Returns one slot's bytes when the argument was supplied.
    ///
    /// # Safety
    /// The slot's pointer and length must describe a readable range for this call.
    pub unsafe fn bytes(&self, index: usize) -> Option<&[u8]> {
        let slot = self.slots.get(index)?;
        if slot.present == 0 {
            return None;
        }
        if slot.ptr.is_null() || slot.len == 0 {
            return Some(&[]);
        }
        Some(std::slice::from_raw_parts(slot.ptr, slot.len as usize))
    }

    /// Returns one slot's bytes, treating an omitted argument as empty.
    ///
    /// # Safety
    /// Same requirements as [`IconvCallArgs::bytes`].
    pub unsafe fn bytes_or_empty(&self, index: usize) -> &[u8] {
        self.bytes(index).unwrap_or(&[])
    }

    /// Returns one slot's integer when the argument was supplied.
    pub fn int(&self, index: usize) -> Option<i64> {
        let slot = self.slots.get(index)?;
        if slot.present == 0 {
            return None;
        }
        Some(slot.int_value)
    }

    /// Returns one slot's integer or the documented PHP default.
    pub fn int_or(&self, index: usize, default: i64) -> i64 {
        self.int(index).unwrap_or(default)
    }
}

/// `iconv(string $from_encoding, string $to_encoding, string $string)`.
pub const OP_CONVERT: i64 = 0;
/// `iconv_strlen(string $string, ?string $encoding = null)`.
pub const OP_STRLEN: i64 = 1;
/// `iconv_substr(string $string, int $offset, ?int $length = null, ?string $encoding = null)`.
pub const OP_SUBSTR: i64 = 2;
/// `iconv_strpos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null)`.
pub const OP_STRPOS: i64 = 3;
/// `iconv_strrpos(string $haystack, string $needle, ?string $encoding = null)`.
pub const OP_STRRPOS: i64 = 4;
/// `iconv_mime_encode(string $field_name, string $field_value, array $options = [])`.
pub const OP_MIME_ENCODE: i64 = 5;
/// `iconv_mime_decode(string $string, int $mode = 0, ?string $encoding = null)`.
pub const OP_MIME_DECODE: i64 = 6;
/// `iconv_mime_decode_headers(string $headers, int $mode = 0, ?string $encoding = null)`.
pub const OP_MIME_DECODE_HEADERS: i64 = 7;
/// `iconv_get_encoding(string $type = "all")`.
pub const OP_GET_ENCODING: i64 = 8;
/// `iconv_set_encoding(string $type, string $encoding)`.
pub const OP_SET_ENCODING: i64 = 9;
