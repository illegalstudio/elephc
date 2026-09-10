//! Purpose:
//! Preserves raw INI string identity through getters, retained arguments, and warning reentry.
//!
//! Called from:
//! - Shared INI storage and the host's owned INI string protocol.
//!
//! Key details:
//! - Equal bytes do not imply the same PHP string; empty and interned strings have shared identities.
//! - Scalar INI getters intern one-byte results, while ini_get_all retains the original raw string.

use std::{collections::HashMap, ops::Deref, sync::{Arc, Mutex, OnceLock, Weak, atomic::{AtomicU64, Ordering}}};

/// An immutable string allocation with a process-unique identity independent of its byte address.
#[derive(Debug)]
struct Text { bytes: Box<[u8]>, identity: u64, interned: bool }

/// Retains a raw PHP INI string; cloning preserves identity and byte equality remains ordinary equality.
#[derive(Clone, Debug)]
pub struct IniString(Arc<Text>);

static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);
static INTERNED: OnceLock<Mutex<HashMap<Vec<u8>, Weak<Text>>>> = OnceLock::new();

impl Drop for Text {
    /// Removes the weak intern key immediately, without removing a newer allocation with equal bytes.
    fn drop(&mut self) {
        if !self.interned { return; }
        let Some(strings) = INTERNED.get() else { return; };
        let Ok(mut strings) = strings.lock() else { return; };
        if strings.get(self.bytes.as_ref()).is_some_and(|value| std::ptr::eq(value.as_ptr(), self)) {
            strings.remove(self.bytes.as_ref());
        }
    }
}

impl IniString {
    /// Allocates fresh runtime text, using PHP's canonical empty string for a zero-length value.
    pub fn new(bytes: &[u8]) -> Self {
        if bytes.is_empty() { Self::interned(bytes) } else { Self::allocate(bytes, false) }
    }

    /// Allocates distinct text even when empty, for host strings that are not canonical or interned.
    pub fn fresh(bytes: &[u8]) -> Self { Self::allocate(bytes, false) }

    /// Reuses the identity of a live interned literal or startup setting with these exact bytes.
    pub fn interned(bytes: &[u8]) -> Self {
        let mut strings = INTERNED.get_or_init(Mutex::default).lock().unwrap();
        if let Some(text) = strings.get(bytes).and_then(Weak::upgrade) { return Self(text); }
        let value = Self::allocate(bytes, true);
        strings.insert(bytes.to_vec(), Arc::downgrade(&value.0));
        value
    }

    /// Assigns a never-reused identity, failing before exhaustion could alias an earlier string.
    fn allocate(bytes: &[u8], interned: bool) -> Self {
        let identity = NEXT_IDENTITY.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| value.checked_add(1).filter(|next| *next <= i64::MAX as u64))
            .expect("INI string identity space exhausted");
        Self(Arc::new(Text { bytes: bytes.into(), identity, interned }))
    }

    /// Returns the immutable identity used by the owned host handle protocol.
    pub fn identity(&self) -> u64 { self.0.identity }

    /// Tests PHP string identity without conflating independent allocations containing equal bytes.
    pub fn same_identity(&self, other: &Self) -> bool { self.identity() == other.identity() }

    /// Applies ZVAL_SET_INI_STR's single-character interning to scalar getter and old-value results.
    pub(in crate::state) fn scalar_result(&self) -> Self {
        if !self.0.interned && self.len() <= 1 { Self::interned(self) } else { self.clone() }
    }
}

impl Deref for IniString {
    type Target = [u8];
    /// Borrows immutable bytes without exposing the identity allocation or its ownership.
    fn deref(&self) -> &[u8] { &self.0.bytes }
}

impl PartialEq for IniString {
    /// Compares byte values for ordinary result assertions; mutation guards use same_identity explicitly.
    fn eq(&self, other: &Self) -> bool { self.0.bytes == other.0.bytes }
}

impl Eq for IniString {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Releases intern-index byte storage immediately after the last alias dies and never recycles its ID.
    #[test]
    fn ini_interned_string_retires_its_weak_index_entry() {
        let bytes = b"standalone INI intern lifetime regression".repeat(1024);
        let original = IniString::interned(&bytes);
        let alias = original.clone();
        let identity = original.identity();
        drop(original);
        assert!(INTERNED.get().unwrap().lock().unwrap().contains_key(&bytes));
        drop(alias);
        assert!(!INTERNED.get().unwrap().lock().unwrap().contains_key(&bytes));
        assert_ne!(IniString::interned(&bytes).identity(), identity);
    }
}
