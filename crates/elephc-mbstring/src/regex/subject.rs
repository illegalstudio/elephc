//! Purpose:
//! Owns immutable PHP subject bytes with initialized padding for native decoder lookahead.
//!
//! Called from:
//! - Shared regex matching, progressive search initialization, and replacement coordinators.
//!
//! Key details:
//! - PHP permits byte offsets inside encoded characters; even valid complete text needs trailing padding.
//! - Native pointers come from the complete allocation while PHP-visible slices exclude the padding.

use elephc_builtin_contract::mbstring_abi::regex::SUBJECT_PADDING;

/// An owned logical string with native lookahead storage that never appears in returned captures.
#[derive(Debug, PartialEq, Eq)]
pub struct Subject { bytes: Vec<u8>, length: usize }

impl Subject {
    /// Copies one logical subject and initializes the complete native padding before publishing it.
    pub fn new(bytes: &[u8]) -> Self {
        let mut storage = Vec::with_capacity(bytes.len().checked_add(SUBJECT_PADDING).expect("regex subject capacity"));
        storage.extend_from_slice(bytes);
        storage.resize(bytes.len() + SUBJECT_PADDING, 0);
        Self { bytes: storage, length: bytes.len() }
    }

    /// Borrows the entire allocated region, including native-only trailing lookahead bytes.
    pub(super) fn native_ptr(&self) -> *const u8 { self.bytes.as_ptr() }
}

impl std::ops::Deref for Subject {
    type Target = [u8];

    /// Exposes only the PHP string bytes to slicing, capture extraction, and encoding validation.
    fn deref(&self) -> &[u8] { &self.bytes[..self.length] }
}
