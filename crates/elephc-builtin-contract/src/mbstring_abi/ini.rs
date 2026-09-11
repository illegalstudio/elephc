//! Purpose:
//! Defines the native INI and PCRE2 MIME provider contracts used by the mbstring bridge.
//!
//! Called from:
//! - AOT/eval configuration adapters and the shared engine's request initialization.
//!
//! Key details:
//! - PHP diagnostics are protected callbacks; PCRE2 callbacks never execute PHP or unwind.
//! - Provider functions have process lifetime and allocate/free opaque handles with one allocator.

use std::ffi::c_void;
pub mod catalog;
pub mod core;
use super::invoke::MbDiagnosticV1;

/// Reads a raw directive from one coerced string argument, returning an owned INI string or false.
pub const INI_GET: u32 = 1;
/// Sets a directive from a coerced name and value, returning its previous owned INI string or false.
/// ARG_INI_STRING preserves an existing host string; ARG_STRING creates a new temporary value.
/// Native adapters must retain identities for original strings and getter/array results.
pub const INI_SET: u32 = 2;
/// Restores a directive from one coerced string, returning null after any diagnostics.
pub const INI_RESTORE: u32 = 3;
/// Reads the mbstring directive graph, taking one coerced boolean for details.
pub const INI_GET_ALL: u32 = 4;
/// Imports one fresh host string, including a noncanonical empty string, returning an owned identity.
pub const INI_STRING_NEW: u32 = 5;
/// Imports one interned host string, reusing any live literal/startup identity with the same bytes.
pub const INI_STRING_INTERNED: u32 = 6;

/// Borrows a live host lease as a setter argument, without copying bytes or acquiring another lease.
pub fn string_argument(identity: u64) -> super::MbArgV1 {
    super::MbArgV1 { kind: super::ARG_INI_STRING, value: identity as i64, bytes: std::ptr::null(), len: 0 }
}

/// Decodes a complete INI array and validates one unique identity record for each string-valued cell.
/// Records contain little-endian array index, entry index, and nonzero identity; keys are ordinary bytes.
pub fn decode_ini_array(bytes: &[u8], graph_length: u64) -> Option<(super::array::ArrayGraph, Vec<(usize, usize, u64)>)> {
    use super::array::{ArrayGraph, Value};
    let length = usize::try_from(graph_length).ok()?;
    let graph = ArrayGraph::decode(bytes.get(..length)?)?;
    let trailer = bytes.get(length..)?;
    if trailer.len() % 24 != 0 { return None; }
    let mut identities = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for record in trailer.chunks_exact(24) {
        let array = usize::try_from(u64::from_le_bytes(record[..8].try_into().ok()?)).ok()?;
        let entry = usize::try_from(u64::from_le_bytes(record[8..16].try_into().ok()?)).ok()?;
        let identity = u64::from_le_bytes(record[16..].try_into().ok()?);
        if identity == 0 || identity > i64::MAX as u64 || !seen.insert((array, entry))
            || !matches!(graph.arrays().get(array)?.get(entry)?.1, Value::String(_)) { return None; }
        identities.push((array, entry, identity));
    }
    let strings = graph.arrays().iter().flatten().filter(|(_, value)| matches!(value, Value::String(_))).count();
    (strings == identities.len()).then_some((graph, identities))
}

/// Copies one compile result into an opaque owned handle or a PCRE2 error and byte offset.
/// Zero means success with nonnull handle; positive means syntax failure with null handle.
/// Negative values indicate invalid ABI metadata, never a PHP syntax error.
pub type MbMimeCompileV1 = unsafe extern "C" fn(*mut *mut c_void, *const u8, u64, *mut u64) -> i32;
/// Matches explicit subject bytes, returning one, zero, or a negative PCRE2 failure code.
pub type MbMimeMatchV1 = unsafe extern "C" fn(*mut c_void, *const u8, u64) -> i32;
/// Releases one owned compiled handle; null is permitted and no PHP callback may run.
pub type MbMimeFreeV1 = unsafe extern "C" fn(*mut c_void);
/// Copies a NUL-terminated message, returning its length without NUL or a negative failure.
pub type MbMimeErrorV1 = unsafe extern "C" fn(i32, *mut u8, u64) -> i32;

/// Immutable versioned provider whose complete function set remains callable for process lifetime.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MbMimeRegexV1 {
    pub version: u32,
    pub size: u32,
    pub compile: Option<MbMimeCompileV1>,
    pub matches: Option<MbMimeMatchV1>,
    pub free: Option<MbMimeFreeV1>,
    pub error: Option<MbMimeErrorV1>,
}

/// Protected diagnostic adapter, borrowed only until an INI operation returns.
/// A pending throwable suppresses later callback delivery but the INI handler still finishes.
/// No callback may unwind through Rust, and it must preserve an existing pending exception.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MbIniHostV1 {
    pub version: u32,
    pub size: u32,
    pub context: *mut c_void,
    pub diagnostic: Option<MbDiagnosticV1>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rejects ambiguous, truncated, missing, and non-string identity records before host materialization.
    #[test]
    fn mbstring_ini_graph_identity_framing() {
        use super::super::array::{ArrayGraph, Key, Value};
        let graph = ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::String(b"a\0b".to_vec())), (Key::Int(1), Value::Null)]]).unwrap();
        let mut bytes = graph.encode();
        let length = bytes.len() as u64;
        for value in [0_u64, 0, 17] { bytes.extend_from_slice(&value.to_le_bytes()); }
        let (decoded, identities) = decode_ini_array(&bytes, length).unwrap();
        assert_eq!(decoded, graph);
        assert_eq!(identities, [(0, 0, 17)]);
        assert!(decode_ini_array(&bytes, u64::MAX).is_none());
        assert!(decode_ini_array(&bytes[..bytes.len() - 1], length).is_none());
        assert!(decode_ini_array(&bytes[..length as usize], length).is_none());
        for (field, value) in [(0, 1_u64), (1, 1), (1, u64::MAX), (2, 0), (2, u64::MAX)] {
            let mut corrupt = bytes.clone();
            let at = length as usize + field * 8;
            corrupt[at..at + 8].copy_from_slice(&value.to_le_bytes());
            assert!(decode_ini_array(&corrupt, length).is_none());
        }
        bytes.extend_from_within(length as usize..);
        assert!(decode_ini_array(&bytes, length).is_none());
    }

    /// Pins the C layouts consumed by generated assembly on every supported 64-bit target.
    #[test]
    fn mbstring_ini_and_mime_layouts() {
        assert_eq!(std::mem::size_of::<MbMimeRegexV1>(), 40);
        assert_eq!(std::mem::offset_of!(MbMimeRegexV1, compile), 8);
        assert_eq!(std::mem::offset_of!(MbMimeRegexV1, matches), 16);
        assert_eq!(std::mem::offset_of!(MbMimeRegexV1, free), 24);
        assert_eq!(std::mem::offset_of!(MbMimeRegexV1, error), 32);
        assert_eq!(std::mem::size_of::<MbIniHostV1>(), 24);
        assert_eq!(std::mem::offset_of!(MbIniHostV1, context), 8);
        assert_eq!(std::mem::offset_of!(MbIniHostV1, diagnostic), 16);
    }
}
