//! Purpose:
//! Associates live native string allocations with retained shared INI identities.
//!
//! Called from:
//! - Native INI materialization, string persistence, final heap release, and request reset.
//!
//! Key details:
//! - Unresolved origins share metadata without copying bytes or acquiring string leases.
//! - Resolved origins own Rust identity leases, never native heap references or PHP callbacks.
//! - The host forgets allocations before freeing/reusing them and clears metadata before arena reset.
//! - Copying a full tracked string preserves identity; copying a different-length view does not.

use std::{cell::OnceCell, collections::{HashMap, hash_map::Entry}, hash::{BuildHasherDefault, Hasher}, rc::Rc};
use super::*;

/// Hashes a native allocation address: payloads are 16-byte aligned, so the low four bits carry
/// nothing, and a multiply-shift spreads the rest. Every freed string probes this map, where
/// SipHash was a sixth of the identity cost on a compiled Symfony request.
#[derive(Default)]
struct AddressHasher(u64);

impl Hasher for AddressHasher {
    /// Returns the mixed address.
    fn finish(&self) -> u64 { self.0 }
    /// Mixes arbitrary bytes for completeness; address keys always take `write_u64`.
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes { self.0 = (self.0.rotate_left(8) ^ u64::from(*byte)).wrapping_mul(0x9E37_79B9_7F4A_7C15); }
    }
    /// Mixes one address key.
    fn write_u64(&mut self, value: u64) { self.0 = (value >> 4).wrapping_mul(0x9E37_79B9_7F4A_7C15); }
}

type AddressMap<V> = HashMap<u64, V, BuildHasherDefault<AddressHasher>>;

struct NativeString { identity: OnceCell<Identity>, length: u64, interned: bool }
struct Identity(u64);

impl Drop for Identity {
    /// Releases the metadata owner's lease without invoking native allocation or PHP code.
    fn drop(&mut self) { strings::release(self.0); }
}

thread_local! { static NATIVE: RefCell<AddressMap<Rc<NativeString>>> = RefCell::new(AddressMap::default()); }

/// How many native allocations carry metadata, summed over every thread's map. The host's
/// `forget`/`copy`/`persist`/`fresh` hooks read it and return before calling Rust when it is zero:
/// then no allocation of any thread is tracked, so there is nothing to retire or share. A thread
/// with an empty map may still see a nonzero count from another thread, which only costs a call.
#[no_mangle]
pub static ELEPHC_MBSTRING_NATIVE_ENTRIES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Adjusts the shared entry count after one map insertion or removal.
fn count_entries(added: bool) {
    use std::sync::atomic::Ordering::Relaxed;
    if added { ELEPHC_MBSTRING_NATIVE_ENTRIES.fetch_add(1, Relaxed); }
    else { ELEPHC_MBSTRING_NATIVE_ENTRIES.fetch_sub(1, Relaxed); }
}

/// Associates an exact native allocation and byte length with one existing leased identity.
/// The host uses allocation starts, not interior pointers, and retires them before native reuse.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_bind_v1(owner: u64, length: u64, identity: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| bind(owner, length, identity))).unwrap_or(1)
}

/// Returns a borrowed identity owned by this thread's exact native allocation, or zero if untracked.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_lookup_v1(owner: u64, length: u64) -> u64 {
    catch_unwind(AssertUnwindSafe(|| NATIVE.with(|values| values.borrow().get(&owner)
        .filter(|entry| entry.length == length).and_then(|entry| entry.identity.get()).map_or(0, |identity| identity.0)))).unwrap_or(0)
}

/// Preserves a complete tracked source's identity when native persistence creates another allocation.
/// An untracked source or different-length view leaves the destination without inherited identity.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_copy_v1(destination: u64, source: u64, length: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if destination == 0 || length > isize::MAX as u64 { return 1; }
        if let Some(origin) = origin(source, length) { replace(destination, origin); 0 }
        else { forget(destination); 0 }
    })).unwrap_or(1)
}

/// Makes a newly produced native string a distinct logical value, including empty strings.
///
/// An untracked allocation already is one: `resolve` gives it a fresh origin when INI first needs
/// it. So this only retires whatever the address carried, instead of recording an entry per
/// string (a map insert per persisted string, then a removal per free, was most of the identity
/// cost of a compiled Symfony request).
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_fresh_v1(owner: u64, length: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if owner == 0 || length > isize::MAX as u64 { return 1; }
        forget(owner);
        0
    })).unwrap_or(1)
}

/// Records an immutable PHP literal without treating arbitrary static or scratch pointers as interned.
/// Repeated observations of the same live literal preserve any already resolved identity.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_literal_v1(owner: u64, length: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if origin(owner, length).is_some_and(|entry| entry.interned) { return 0; }
        create(owner, length, true)
    })).unwrap_or(1)
}

/// Stabilizes a native value by sharing its complete known origin; a copy of an untracked source is
/// left untracked, which is already a fresh distinct value (see `fresh`).
/// Unknown source addresses are never retained, since they may be reusable scratch or foreign storage.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_persist_v1(destination: u64, source: u64, length: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if destination == 0 || length > isize::MAX as u64 { return 1; }
        if let Some(origin) = origin(source, length) { replace(destination, origin); }
        else { forget(destination); }
        0
    })).unwrap_or(1)
}

/// Resolves a live native origin only when INI needs its bytes, preserving all previously copied aliases.
/// The nonzero returned identity is borrowed until the final native alias is forgotten or reset.
///
/// An allocation with no metadata at all gets a fresh origin here: the host keeps the creation
/// hooks dormant until the first INI input, so a string made before then (every string of a
/// request that never reaches `SharedIni`) is first seen by this call.
///
/// The same dormancy means a freed string can leave its entry behind: an allocation the host
/// reused before the hooks woke was never forgotten. Metadata recorded under ANOTHER length at
/// this exact allocation start is that stale record — the host passes allocation starts, never
/// interior views — so it is replaced by a fresh origin. Refusing it failed the whole INI call,
/// and the protected coordinator turned that into a silent `exit(1)`:
/// `ini_set('max_input_vars', '5'); ini_set('no_such', '1'); ini_set('max_input_vars', '6');` died on
/// the third call when `"6"` landed where a 14-byte INI name had lived.
///
/// # Safety
/// The owner must still denote an immutable, readable byte range of the registered length. No other
/// thread may change that range during this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_native_string_resolve_v1(owner: u64, length: u64) -> u64 {
    catch_unwind(AssertUnwindSafe(|| {
        if owner != 0 && length <= isize::MAX as u64 && origin(owner, length).is_none() {
            if tracked(owner) {
                forget(owner);
            }
            create(owner, length, false);
        }
        let entry = origin(owner, length)?;
        if let Some(identity) = entry.identity.get() { return Some(identity.0); }
        let bytes = unsafe { std::slice::from_raw_parts(owner as *const u8, length as usize) };
        let string = if entry.interned { IniString::interned(bytes) } else { IniString::fresh(bytes) };
        let identity = string.identity();
        strings::acquire(string).ok()?;
        entry.identity.set(Identity(identity)).ok()?;
        Some(identity)
    })).ok().flatten().unwrap_or(0)
}

/// Retires one allocation's metadata before native free-list insertion or in-place string mutation.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_forget_v1(owner: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| { forget(owner); 0 })).unwrap_or(1)
}

/// Releases all native metadata leases before the host resets this thread's request arena.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_reset_v1() -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        let values = NATIVE.with(|values| std::mem::take(&mut *values.borrow_mut()));
        ELEPHC_MBSTRING_NATIVE_ENTRIES.fetch_sub(values.len() as u64, std::sync::atomic::Ordering::Relaxed);
        drop(values);
        0
    })).unwrap_or(1)
}

/// Acquires replacement ownership before publishing metadata, preserving the old owner on failure.
fn bind(owner: u64, length: u64, identity: u64) -> i32 {
    if owner == 0 || length > isize::MAX as u64 { return 1; }
    if elephc_mbstring_native_string_lookup_v1(owner, length) == identity && identity != 0 { return 0; }
    let Ok(string) = strings::lookup(identity) else { return 1; };
    if string.len() as u64 != length || strings::retain(identity) != 0 { return 1; }
    replace(owner, Rc::new(NativeString { identity: OnceCell::from(Identity(identity)), length, interned: false }));
    0
}

/// Records one fresh or interned origin without reading bytes or retaining another native allocation.
/// A unique unresolved fresh origin has no observable identity or alias, so only its length changes.
fn create(owner: u64, length: u64, interned: bool) -> i32 {
    if owner == 0 || length > isize::MAX as u64 { return 1; }
    let previous = NATIVE.with(|values| match values.borrow_mut().entry(owner) {
        Entry::Occupied(mut slot) => {
            if !interned {
                if let Some(entry) = Rc::get_mut(slot.get_mut()) {
                    if !entry.interned && entry.identity.get().is_none() {
                        entry.length = length;
                        return None;
                    }
                }
            }
            Some(slot.insert(Rc::new(NativeString { identity: OnceCell::new(), length, interned })))
        },
        Entry::Vacant(slot) => {
            slot.insert(Rc::new(NativeString { identity: OnceCell::new(), length, interned }));
            None
        },
    });
    // Retiring a resolved origin can release an identity lease; keep that outside the map borrow.
    drop(previous);
    0
}

/// Whether any metadata, of any length, is recorded for this allocation.
fn tracked(owner: u64) -> bool {
    NATIVE.with(|values| values.borrow().contains_key(&owner))
}

/// Clones a complete logical origin while keeping the native allocation map borrow short.
fn origin(owner: u64, length: u64) -> Option<Rc<NativeString>> {
    NATIVE.with(|values| values.borrow().get(&owner).filter(|entry| entry.length == length).cloned())
}

/// Publishes a replacement before retiring the previous origin outside the map borrow.
fn replace(owner: u64, replacement: Rc<NativeString>) {
    let previous = NATIVE.with(|values| values.borrow_mut().insert(owner, replacement));
    if previous.is_none() { count_entries(true); }
    drop(previous);
}

/// Removes metadata under a short borrow, then releases its lease after the request map is available again.
fn forget(owner: u64) {
    let previous = NATIVE.with(|values| values.borrow_mut().remove(&owner));
    if previous.is_some() { count_entries(false); }
    drop(previous);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A freshly produced string records no metadata: an untracked allocation already is a distinct
    /// value, and `resolve` gives it an origin the first time INI needs it. `fresh` on an address
    /// that still carries metadata retires it, so a reused allocation never inherits the old value.
    #[test]
    fn ini_native_fresh_strings_stay_untracked_and_retire_stale_metadata() {
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
        let bytes = *b"a\0b";
        let owner = bytes.as_ptr() as u64;
        for length in [0, 1, 3, 2] {
            assert_eq!(elephc_mbstring_native_string_fresh_v1(owner, length), 0);
            assert!(!tracked(owner), "a fresh string records no metadata (length {length})");
            assert_eq!(elephc_mbstring_native_string_lookup_v1(owner, length), 0);
        }
        assert_eq!(create(owner, 3, false), 0);
        assert!(tracked(owner));
        assert_eq!(elephc_mbstring_native_string_fresh_v1(owner, 1), 0);
        assert!(!tracked(owner), "fresh retires the reused allocation's previous metadata");
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
    }

    /// An allocation still carrying metadata under ANOTHER length is a reused one whose free was
    /// never forgotten (the host's hooks were dormant). Resolving the new, shorter value must give
    /// it a fresh origin; refusing it failed the INI call and the coordinator exited the process.
    #[test]
    fn ini_native_resolution_replaces_stale_metadata_of_a_reused_allocation() {
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
        let bytes = *b"max_input_vars";
        let owner = bytes.as_ptr() as u64;
        let first = unsafe { elephc_mbstring_native_string_resolve_v1(owner, 14) };
        assert_ne!(first, 0, "the original value resolves");
        let reused = unsafe { elephc_mbstring_native_string_resolve_v1(owner, 1) };
        assert_ne!(reused, 0, "the reused allocation's new value resolves");
        assert_ne!(reused, first, "the new value is not the stale one");
        assert_eq!(elephc_mbstring_native_string_lookup_v1(owner, 14), 0, "the stale record is gone");
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
    }
}
