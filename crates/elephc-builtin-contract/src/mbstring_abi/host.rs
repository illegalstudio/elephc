//! Purpose:
//! Defines the borrowed host-value and array-iteration ABI used to snapshot PHP arrays.
//!
//! Called from:
//! - Native/eval runtime readers and elephc_mbstring_snapshot_v1.
//!
//! Key details:
//! - The tag/low/high triple matches concrete runtime value tags on every supported target.
//! - Array identities and unsupported payloads remain opaque to Rust; only string bytes are read.
//! - Readers must not mutate source arrays, execute PHP callbacks, or unwind across the C ABI.
//! - The host retains the complete reachable graph until snapshotting returns.

use std::ffi::c_void;

/// A signed integer carried bit-for-bit in the low word.
pub const HOST_INT: u64 = 0;
/// A borrowed binary string pointer and byte length.
pub const HOST_STRING: u64 = 1;
/// An IEEE-754 double carried without changing its low-word bits.
pub const HOST_FLOAT: u64 = 2;
/// A boolean whose low word is exactly zero or one.
pub const HOST_BOOL: u64 = 3;
/// An opaque indexed-array identity traversed through the host reader.
pub const HOST_INDEXED_ARRAY: u64 = 4;
/// An opaque associative-array identity traversed through the host reader.
pub const HOST_ASSOC_ARRAY: u64 = 5;
/// An object, resource, or other value unsupported by recursive mbstring operations.
pub const HOST_UNSUPPORTED: u64 = 6;
/// A normalized PHP null with no owned payload.
pub const HOST_NULL: u64 = 8;

/// The array traversal has finished without publishing an entry.
pub const ITER_END: u64 = 0;
/// The reader published one entry and advanced its opaque cursor.
pub const ITER_ENTRY: u64 = 1;
/// The host reader rejected the descriptor or encountered invalid metadata.
pub const ITER_ERROR: u64 = 2;

/// One concrete value; strings use a borrowed byte pointer/length and arrays use an opaque low word.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct MbHostValueV1 { pub tag: u64, pub lo: u64, pub hi: u64 }

impl MbHostValueV1 {
    /// Creates a null placeholder that owns no host resources.
    pub const fn null() -> Self { Self { tag: HOST_NULL, lo: 0, hi: 0 } }
}

/// Reads the next ordered entry, starting with cursor zero, and returns ITER_END/ENTRY/ERROR.
/// An entry must advance to a previously unseen cursor; the cursor may be nonmonotonic.
/// Only ITER_ENTRY publishes key/value outputs. Returned string ranges must remain valid
/// until the next reader invocation or the enclosing snapshot returns, whichever comes first.
pub type MbArrayNextV1 = unsafe extern "C" fn(
    context: *mut c_void, array: *const MbHostValueV1, cursor: *mut u64,
    key: *mut MbHostValueV1, value: *mut MbHostValueV1,
) -> u64;

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the host descriptor offsets consumed by both target emitters.
    #[test]
    fn mbstring_host_value_layout() {
        assert_eq!(std::mem::size_of::<MbHostValueV1>(), 24);
        assert_eq!(std::mem::offset_of!(MbHostValueV1, tag), 0);
        assert_eq!(std::mem::offset_of!(MbHostValueV1, lo), 8);
        assert_eq!(std::mem::offset_of!(MbHostValueV1, hi), 16);
    }
}
