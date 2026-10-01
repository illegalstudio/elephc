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
        let identity = next_identity(&NEXT_IDENTITY)
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

/// Atomically advances a bounded identity counter without newer or deprecated atomic APIs.
/// Failed weak exchanges retry with the observed value; exhaustion leaves the counter unchanged.
fn next_identity(counter: &AtomicU64) -> Option<u64> {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        let next = current.checked_add(1).filter(|next| *next <= i64::MAX as u64)?;
        match counter.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(identity) => return Some(identity),
            Err(observed) => current = observed,
        }
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

    /// Exhaustion and overflow never change the counter or make an identity reusable.
    #[test]
    fn ini_identity_counter_stops_before_exhaustion() {
        let counter = AtomicU64::new(i64::MAX as u64 - 1);
        assert_eq!(next_identity(&counter), Some(i64::MAX as u64 - 1));
        assert_eq!(counter.load(Ordering::Relaxed), i64::MAX as u64);
        assert_eq!(next_identity(&counter), None);
        assert_eq!(counter.load(Ordering::Relaxed), i64::MAX as u64);
        let overflow = AtomicU64::new(u64::MAX);
        assert_eq!(next_identity(&overflow), None);
        assert_eq!(overflow.load(Ordering::Relaxed), u64::MAX);
    }

    /// Concurrent allocation keeps every identity unique despite weak-exchange retries.
    #[test]
    fn ini_identity_counter_is_unique_under_contention() {
        let counter = AtomicU64::new(1);
        let identities = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8).map(|_| {
                scope.spawn(|| (0..1000).map(|_| next_identity(&counter).unwrap()).collect::<Vec<_>>())
            }).collect();
            handles.into_iter().flat_map(|handle| handle.join().unwrap()).collect::<std::collections::HashSet<_>>()
        });
        assert_eq!(identities.len(), 8000);
        assert_eq!(counter.load(Ordering::Relaxed), 8001);
        assert!(identities.contains(&1));
        assert!(identities.contains(&8000));
    }

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
