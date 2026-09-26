//! Purpose:
//! Transfers INI string identities across native calls with explicit, bounded host ownership.
//!
//! Called from:
//! - INI result publication, retained setter arguments, and the ordinary bridge result release entry.
//!
//! Key details:
//! - Every exported identity owns one lease; zero remaining leases remove its registry allocation.
//! - Registry locks protect pure Rust values only and never surround PHP callbacks.
//! - Native string copies retain a lease and their final release relinquishes it, including after reset.

use std::{collections::HashMap, sync::{Mutex, OnceLock}};
use crate::state::IniString;
use super::*;

struct Lease { string: IniString, count: u64 }
static LEASES: OnceLock<Mutex<HashMap<u64, Lease>>> = OnceLock::new();

/// Keeps result identities owned by Rust until successful wire publication transfers their leases.
pub(super) struct Reply { outcome: Outcome, strings: Vec<IniString> }

impl From<Outcome> for Reply {
    /// Wraps ordinary scalars or failures without adding identity ownership.
    fn from(outcome: Outcome) -> Self { Self { outcome, strings: Vec::new() } }
}

impl Reply {
    /// Copies result bytes while retaining the exact PHP string identity separately.
    pub(super) fn string(string: IniString) -> Self {
        Self { outcome: Outcome { value: string.identity() as i64, bytes: string.to_vec(), ..Outcome::empty(RESULT_INI_STRING) },
            strings: vec![string] }
    }

    /// Appends one identity record for each string cell, preserving raw global/local aliases.
    pub(super) fn array(graph: crate::arrays::ArrayGraph, strings: Vec<(usize, usize, IniString)>) -> Self {
        let mut bytes = graph.encode();
        let value = bytes.len() as i64;
        for (array, entry, string) in &strings {
            for number in [*array as u64, *entry as u64, string.identity()] { bytes.extend_from_slice(&number.to_le_bytes()); }
        }
        Self { outcome: Outcome { bytes, value, ..Outcome::empty(RESULT_INI_ARRAY) },
            strings: strings.into_iter().map(|(_, _, string)| string).collect() }
    }

    /// Registers leases only after successful operation completion, rolling back on a failed publication.
    pub(super) fn into_wire(self) -> Result<MbResultV1, i32> {
        let mut guard = Published(Vec::new());
        for string in self.strings { let id = string.identity(); acquire(string)?; guard.0.push(id); }
        let result = self.outcome.into_wire();
        guard.0.clear();
        Ok(result)
    }
}

/// Releases any registered leases if publication exits before transferring the complete wire result.
struct Published(Vec<u64>);

impl Drop for Published {
    /// Reclaims only this incomplete result's leases, preserving other host owners of the same identities.
    fn drop(&mut self) { for &identity in &self.0 { release(identity); } }
}

/// Acquires one host lease without retaining historical strings after their final owner disappears.
pub(super) fn acquire(string: IniString) -> Result<(), i32> {
    let mut leases = LEASES.get_or_init(Mutex::default).lock().map_err(|_| 1)?;
    let lease = leases.entry(string.identity()).or_insert(Lease { string, count: 0 });
    lease.count = lease.count.checked_add(1).ok_or(1)?;
    Ok(())
}

/// Resolves only a currently leased identity, cloning Rust ownership before releasing the registry lock.
pub(in crate::abi) fn lookup(identity: u64) -> Result<IniString, i32> {
    let leases = LEASES.get_or_init(Mutex::default).lock().map_err(|_| 1)?;
    leases.get(&identity).map(|lease| lease.string.clone()).ok_or(1)
}

/// Acquires a lease for a native string copy without allocating another result buffer.
pub(super) fn retain(identity: u64) -> i32 { lookup(identity).and_then(acquire).map_or(1, |()| 0) }

/// Releases one live host lease and removes its registry entry when the final host owner disappears.
pub(super) fn release(identity: u64) -> i32 {
    let Ok(mut leases) = LEASES.get_or_init(Mutex::default).lock() else { return 1; };
    let Some(lease) = leases.get_mut(&identity) else { return 1; };
    lease.count -= 1;
    if lease.count == 0 { leases.remove(&identity); }
    0
}

/// Reclaims the identity leases in an untouched bridge-owned result before releasing its byte buffer.
pub(crate) unsafe fn release_result(result: &MbResultV1) {
    if result.kind == RESULT_INI_STRING { release(result.value as u64); }
    if result.kind == RESULT_INI_ARRAY && result.value >= 0 && result.value as u64 <= result.len && !result.bytes.is_null() {
        let bytes = unsafe { std::slice::from_raw_parts(result.bytes, result.len as usize) };
        for record in bytes[result.value as usize..].chunks_exact(24) {
            release(u64::from_le_bytes(record[16..].try_into().expect("complete identity record")));
        }
    }
}
