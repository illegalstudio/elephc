//! Purpose:
//! Defines the version-one mbstring bridge wire layout shared by engine and emitters.
//!
//! Called from:
//! - The mbstring bridge and AOT/eval runtime adapters.
//!
//! Key details:
//! - Arguments borrow their payloads; only the bridge may release result buffers.
//! - All supported targets use 64-bit pointers and the same C field offsets.

pub mod array;
pub mod coercion;
pub mod callback;
pub mod host;
pub mod output;
pub mod output_handler;
pub mod invoke;
pub mod ini;
pub mod regex;
pub mod exception;

/// A null or omitted argument, distinguished by the call's argument count.
pub const ARG_NULL: u64 = 0;
/// An integer argument stored in `MbArgV1::value`.
pub const ARG_INT: u64 = 1;
/// A byte string stored in `MbArgV1::bytes` and `MbArgV1::len`.
pub const ARG_STRING: u64 = 2;
/// A boolean argument stored as zero or one in `MbArgV1::value`.
pub const ARG_BOOL: u64 = 3;
/// An array graph encoded by `array::ArrayGraph::encode`, borrowed in bytes/len.
pub const ARG_ARRAY: u64 = 4;
/// An owned INI string identity in value, accepted only by the dedicated INI protocol.
/// Its host lease must remain live; bytes and len are empty because the engine owns the text.
pub const ARG_INI_STRING: u64 = 5;
/// Array metadata: the retained payload is the current request's cached encoding catalog.
/// Hosts establish this by identity; identical contents do not qualify.
pub const ARRAY_ENCODING_CATALOG: i64 = 1;

/// Tests a wire kind against a shared parameter, including nullable and union members.
pub fn accepts_argument_kind(ty: crate::TypeSpec, kind: u64) -> bool {
    use crate::TypeSpec;
    match ty {
        TypeSpec::Null => kind == ARG_NULL,
        TypeSpec::Int => kind == ARG_INT,
        TypeSpec::Bool => kind == ARG_BOOL,
        TypeSpec::Str => kind == ARG_STRING,
        TypeSpec::Array => kind == ARG_ARRAY,
        TypeSpec::Nullable(inner) => kind == ARG_NULL || accepts_argument_kind(*inner, kind),
        TypeSpec::Union(members) => members.iter().any(|&member| accepts_argument_kind(member, kind)),
        _ => false,
    }
}

/// One borrowed argument in PHP parameter order after shared argument planning.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MbArgV1 {
    pub kind: u64,
    pub value: i64,
    pub bytes: *const u8,
    pub len: u64,
}

impl MbArgV1 {
    /// Creates an absent payload without borrowing any storage.
    pub const fn null() -> Self { Self { kind: ARG_NULL, value: 0, bytes: std::ptr::null(), len: 0 } }

    /// Borrows a string that the caller must keep alive through the bridge call.
    pub fn string(bytes: &[u8]) -> Self { Self { kind: ARG_STRING, value: 0, bytes: bytes.as_ptr(), len: bytes.len() as u64 } }

    /// Borrows a packed array graph whose complete buffer must remain alive through the call.
    pub fn array(bytes: &[u8]) -> Self { Self { kind: ARG_ARRAY, value: 0, bytes: bytes.as_ptr(), len: bytes.len() as u64 } }

    /// Stores a boolean without allocating or borrowing storage.
    pub const fn boolean(value: bool) -> Self { Self { kind: ARG_BOOL, value: value as i64, bytes: std::ptr::null(), len: 0 } }

    /// Stores an integer without allocating or borrowing storage.
    pub const fn integer(value: i64) -> Self { Self { kind: ARG_INT, value, bytes: std::ptr::null(), len: 0 } }
}

/// The operation produced an integer in `MbResultV1::value`.
pub const RESULT_INT: u64 = 1;
/// The operation produced an owned byte string.
pub const RESULT_STRING: u64 = 2;
/// A catchable ValueError message is stored in the owned byte payload.
pub const RESULT_VALUE_ERROR: u64 = 3;
/// A catchable TypeError message is stored in the owned byte payload.
pub const RESULT_TYPE_ERROR: u64 = 4;
/// A catchable Error message is stored in the owned byte payload.
pub const RESULT_ERROR: u64 = 5;
/// The operation failed internally; callers must fail rather than fabricate a value.
pub const RESULT_FATAL: u64 = 6;
/// This ABI does not implement the requested operation or argument count.
pub const RESULT_UNSUPPORTED: u64 = 7;
/// The operation produced a boolean in `MbResultV1::value`.
pub const RESULT_BOOL: u64 = 8;
/// An indexed string array, encoded as little-endian lengths followed by each byte payload.
/// `value` is the element count; `bytes`/`len` own the complete packed buffer.
pub const RESULT_STRING_ARRAY: u64 = 9;
/// An owned array graph encoded by `array::ArrayGraph::encode`; value is reserved as zero.
pub const RESULT_ARRAY: u64 = 10;
/// A string-array result materialized through the host's request-local encoding catalog cache.
/// Only V2 invocation hosts receive this kind; direct wire and V1 invocation retain kind 9.
pub const RESULT_ENCODING_CATALOG: u64 = 11;
/// The operation succeeded with PHP null and no payload ownership.
pub const RESULT_NULL: u64 = 12;
/// An owned INI string: ordinary owned bytes plus one identity lease in value.
/// Hosts preserve that lease in native string metadata and release it when the string dies.
pub const RESULT_INI_STRING: u64 = 13;
/// An owned INI graph followed by identity records, decoded through ini::decode_ini_array.
/// Value is the graph prefix length; each trailer record owns one string identity lease.
pub const RESULT_INI_ARRAY: u64 = 14;
/// Oldest-first exception records encoded by `exception::encode`; value is the record count.
pub const RESULT_EXCEPTION_CHAIN: u64 = 15;

/// Packs binary strings without exporting Rust containers or nested allocation ownership.
pub fn encode_string_array(values: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in values {
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(value);
    }
    bytes
}

/// Borrows packed strings only when the count, lengths, and complete byte range agree.
pub fn decode_string_array(mut bytes: &[u8], count: usize) -> Option<Vec<&[u8]>> {
    if count > bytes.len() / 8 { return None; }
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let length = u64::from_le_bytes(bytes.get(..8)?.try_into().ok()?);
        let length = usize::try_from(length).ok()?;
        bytes = &bytes[8..];
        values.push(bytes.get(..length)?);
        bytes = &bytes[length..];
    }
    bytes.is_empty().then_some(values)
}

/// An owned bridge result, released exactly once through `elephc_mbstring_release_v1`.
#[repr(C)]
pub struct MbResultV1 {
    pub kind: u64,
    pub value: i64,
    pub bytes: *mut u8,
    pub len: u64,
    /// Complete diagnostic lines, including severity prefixes and newline separators.
    pub diagnostics: *mut u8,
    pub diagnostics_len: u64,
}

impl Default for MbResultV1 {
    /// Creates an empty fatal result, safe to release even when no bridge is installed.
    fn default() -> Self {
        Self { kind: RESULT_FATAL, value: 0, bytes: std::ptr::null_mut(), len: 0,
            diagnostics: std::ptr::null_mut(), diagnostics_len: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins every field offset consumed by generated assembly on the supported targets.
    #[test]
    fn mbstring_v1_layout() {
        assert_eq!(std::mem::size_of::<MbArgV1>(), 32);
        assert_eq!(std::mem::offset_of!(MbArgV1, kind), 0);
        assert_eq!(std::mem::offset_of!(MbArgV1, value), 8);
        assert_eq!(std::mem::offset_of!(MbArgV1, bytes), 16);
        assert_eq!(std::mem::offset_of!(MbArgV1, len), 24);
        assert_eq!(std::mem::size_of::<MbResultV1>(), 48);
        assert_eq!(std::mem::offset_of!(MbResultV1, kind), 0);
        assert_eq!(std::mem::offset_of!(MbResultV1, value), 8);
        assert_eq!(std::mem::offset_of!(MbResultV1, bytes), 16);
        assert_eq!(std::mem::offset_of!(MbResultV1, len), 24);
        assert_eq!(std::mem::offset_of!(MbResultV1, diagnostics), 32);
        assert_eq!(std::mem::offset_of!(MbResultV1, diagnostics_len), 40);
    }
    /// Verifies packed arrays preserve binary/empty elements and reject inconsistent framing.
    #[test]
    fn mbstring_string_array_wire_framing() {
        let values = vec![Vec::new(), b"a\0b".to_vec(), vec![0xff, 0x80]];
        let packed = encode_string_array(&values);
        assert_eq!(decode_string_array(&packed, 3), Some(values.iter().map(Vec::as_slice).collect()));
        assert_eq!(decode_string_array(&packed, 2), None);
        assert_eq!(decode_string_array(&packed[..packed.len() - 1], 3), None);
        assert_eq!(decode_string_array(&u64::MAX.to_le_bytes(), 1), None);
        assert_eq!(decode_string_array(&[], 0), Some(Vec::new()));
        assert_eq!(decode_string_array(&[], 1), None);
    }

}
