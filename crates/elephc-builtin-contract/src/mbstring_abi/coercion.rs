//! Purpose:
//! Defines the host-value input and deferred actions for mbstring parameter preparation.
//!
//! Called from:
//! - The mbstring preparation C ABI and native/eval argument adapters.
//!
//! Key details:
//! - Numeric tags match concrete native values; object names and strings are borrowed bytes.
//! - Prepared result buffers use the ordinary release API, but diagnostics use binary records.
//! - Deferred actions run only after Rust returns and the host delivers prior diagnostics.

use super::MbResultV1;

/// Borrow the original string range retained by the host input descriptor.
pub const PREPARED_BORROWED_STRING: u64 = 256;
/// Keep the original array and snapshot it after all outer argument coercions.
pub const PREPARED_ARRAY: u64 = 257;
/// Format the original float using the host's current PHP precision, without additional warnings.
pub const PREPARED_FLOAT_STRING: u64 = 258;
/// Invoke the original Stringable object through the host's protected callback boundary.
pub const PREPARED_STRINGABLE: u64 = 259;
/// Preserve an explicitly accepted PHP null without scalar conversion.
pub const PREPARED_NULL: u64 = 260;
/// Construct an ArgumentCountError using the result's owned binary message.
pub const PREPARED_ARGUMENT_COUNT_ERROR: u64 = 261;
/// Resolve the original callable value through the accompanying protected callback host.
pub const PREPARED_CALLABLE: u64 = 262;
/// The object has a supported string-conversion handler.
pub const INPUT_STRINGABLE: u64 = 1;
/// The resource is closed, although PHP internal TypeErrors still name it resource.
pub const INPUT_CLOSED_RESOURCE: u64 = 1;
/// An array payload is identical to the current request's cached mb_list_encodings array.
pub const INPUT_ENCODING_CATALOG: u64 = 1;
/// A string's numeric payload borrows a live INI identity owned by its retained host value.
/// Only the INI-aware description callback supplies this flag; ordinary text calls leave it zero.
pub const INPUT_INI_IDENTITY: u64 = 1;
/// A concrete object, with its binary class name supplied separately from its opaque identity.
pub const INPUT_OBJECT: u64 = 6;
/// A concrete resource tag, kept distinct from unsupported snapshot values.
pub const INPUT_RESOURCE: u64 = 9;

/// A borrowed input: string/object-name bytes, numeric payload bits, and kind-specific capability flags.
/// Unused byte ranges and flags must be zero; null also requires a zero payload.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct MbCoercionInputV1 {
    pub kind: u64,
    pub value: u64,
    pub bytes: *const u8,
    pub len: u64,
    pub flags: u64,
}

/// Owns a prepared scalar/error or deferred action, with the ordinary result release layout.
/// `diagnostics` is a sequence of `(level u64 LE, length u64 LE, message bytes)` records.
/// Ready scalar kinds reuse RESULT_INT/BOOL/STRING; TypeErrors reuse RESULT_TYPE_ERROR.
pub type MbCoercionResultV1 = MbResultV1;

/// A protected host string result whose owner belongs to the native runtime allocator.
/// The host releases `owner` through its ordinary GC helper, never the Rust bridge release API.
/// Failure leaves all three words zero; success may return an empty binary string.
#[derive(Debug)]
#[repr(C)]
pub struct MbHostStringV1 {
    pub bytes: *const u8,
    pub len: u64,
    pub owner: *mut std::ffi::c_void,
}

/// Decodes complete ordered diagnostic records while rejecting malformed metadata before slicing.
pub fn decode_diagnostics(mut bytes: &[u8]) -> Option<Vec<(u32, &[u8])>> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        let header = bytes.get(..16)?;
        let level = u64::from_le_bytes(header[..8].try_into().ok()?);
        if !matches!(level, 2 | 8192) { return None; }
        let length = usize::try_from(u64::from_le_bytes(header[8..].try_into().ok()?)).ok()?;
        let payload = bytes.get(16..)?;
        result.push((level as u32, payload.get(..length)?));
        bytes = payload.get(length..)?;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the descriptor offsets read by target-independent C callers and both emitters.
    #[test]
    fn mbstring_coercion_input_layout() {
        assert_eq!(std::mem::size_of::<MbCoercionInputV1>(), 40);
        assert_eq!(std::mem::offset_of!(MbCoercionInputV1, kind), 0);
        assert_eq!(std::mem::offset_of!(MbCoercionInputV1, value), 8);
        assert_eq!(std::mem::offset_of!(MbCoercionInputV1, bytes), 16);
        assert_eq!(std::mem::offset_of!(MbCoercionInputV1, len), 24);
        assert_eq!(std::mem::offset_of!(MbCoercionInputV1, flags), 32);
        assert_eq!(std::mem::size_of::<MbHostStringV1>(), 24);
        assert_eq!(std::mem::offset_of!(MbHostStringV1, bytes), 0);
        assert_eq!(std::mem::offset_of!(MbHostStringV1, len), 8);
        assert_eq!(std::mem::offset_of!(MbHostStringV1, owner), 16);
    }

    /// Rejects incomplete headers, impossible lengths, unknown levels, and trailing partial records.
    #[test]
    fn mbstring_coercion_diagnostic_framing() {
        let mut bytes = [2_u64.to_le_bytes(), 3_u64.to_le_bytes()].concat();
        bytes.extend_from_slice(b"a\0\xff");
        assert_eq!(decode_diagnostics(&bytes), Some(vec![(2, b"a\0\xff".as_slice())]));
        for end in 1..bytes.len() { assert!(decode_diagnostics(&bytes[..end]).is_none()); }
        bytes.push(0);
        assert!(decode_diagnostics(&bytes).is_none());
        assert!(decode_diagnostics(&[255; 16]).is_none());
        assert_eq!(decode_diagnostics(&[]), Some(vec![]));
    }
}
