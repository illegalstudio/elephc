//! Purpose:
//! Associates live native string allocations with retained shared INI identities.
//!
//! Called from:
//! - Native INI materialization, string persistence, final heap release, and request reset.
//!
//! Key details:
//! - Origins start inline; copying or resolving promotes them to shared metadata.
//! - Unique unresolved promoted origins reuse metadata after their temporary aliases retire.
//! - Internal allocation addresses use integer hashing; request resets keep the empty table.
//! - Resolved origins own Rust identity leases, never native heap references or PHP callbacks.
//! - The host forgets allocations before freeing/reusing them and clears metadata before arena reset.
//! - Copying a full tracked string preserves identity; copying a different-length view does not.

use std::{cell::OnceCell, collections::{HashMap, hash_map::Entry},
    hash::{BuildHasherDefault, Hasher}, rc::Rc};
use super::*;

struct SharedOrigin { identity: OnceCell<Identity>, length: u64, interned: bool }
struct Identity(u64);

enum NativeString { Fresh(u64), Literal(u64), Shared(Rc<SharedOrigin>) }

impl NativeString {
    /// Returns the exact view length without promoting an unshared origin.
    fn length(&self) -> u64 {
        match self { Self::Fresh(length) | Self::Literal(length) => *length, Self::Shared(entry) => entry.length }
    }

    /// Identifies immutable literals even after their first copy or INI resolution.
    fn interned(&self) -> bool {
        match self { Self::Fresh(_) => false, Self::Literal(_) => true, Self::Shared(entry) => entry.interned }
    }

    /// Allocates shared metadata only when another allocation or INI consumer needs its origin.
    fn share(&mut self) -> Rc<SharedOrigin> {
        if let Self::Shared(entry) = self { return entry.clone(); }
        let entry = Rc::new(SharedOrigin { identity: OnceCell::new(), length: self.length(), interned: self.interned() });
        *self = Self::Shared(entry.clone());
        entry
    }
}

/// Hashes allocator-owned addresses, never PHP-controlled string keys or string contents.
#[derive(Default)]
struct OwnerHasher(u64);

impl Hasher for OwnerHasher {
    /// Returns the mixed address used for both table bucket selection and control-byte matching.
    fn finish(&self) -> u64 { self.0 }

    /// Rejects accidental reuse for keys other than this registry's exact u64 addresses.
    fn write(&mut self, _: &[u8]) { unreachable!("native string owners are u64 keys"); }

    /// Mixes aligned low bits and high address bits without keyed byte-stream hashing.
    fn write_u64(&mut self, mut owner: u64) {
        owner = (owner ^ (owner >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        owner = (owner ^ (owner >> 27)).wrapping_mul(0x94d049bb133111eb);
        self.0 = owner ^ (owner >> 31);
    }
}

impl Drop for Identity {
    /// Releases the metadata owner's lease without invoking native allocation or PHP code.
    fn drop(&mut self) { strings::release(self.0); }
}

type NativeMap = HashMap<u64, NativeString, BuildHasherDefault<OwnerHasher>>;
thread_local! { static NATIVE: RefCell<NativeMap> = RefCell::new(NativeMap::default()); }

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
        .and_then(|entry| match entry {
            NativeString::Shared(entry) if entry.length == length => entry.identity.get(), _ => None,
        }).map_or(0, |identity| identity.0)))).unwrap_or(0)
}

/// Preserves a complete tracked source's identity when native persistence creates another allocation.
/// An untracked source or different-length view leaves the destination without inherited identity.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_copy_v1(destination: u64, source: u64, length: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if destination == 0 || length > isize::MAX as u64 { return 1; }
        if let Some(origin) = origin(source, length) { replace(destination, NativeString::Shared(origin)); 0 }
        else { forget(destination); 0 }
    })).unwrap_or(1)
}

/// Creates a distinct unresolved origin for a newly produced native string, including empty strings.
/// The caller must retire or replace this origin before reusing or changing the allocation's bytes.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_fresh_v1(owner: u64, length: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if owner == 0 || length > isize::MAX as u64 { return 1; }
        create_fresh(owner, length);
        0
    })).unwrap_or(1)
}

/// Records an immutable PHP literal without treating arbitrary static or scratch pointers as interned.
/// Repeated observations of the same live literal preserve any already resolved identity.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_literal_v1(owner: u64, length: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if owner == 0 || length > isize::MAX as u64 { return 1; }
        let previous = NATIVE.with(|values| match values.borrow_mut().entry(owner) {
            Entry::Occupied(mut slot) => {
                if slot.get().length() == length && slot.get().interned() { return None; }
                Some(slot.insert(NativeString::Literal(length)))
            },
            Entry::Vacant(slot) => { slot.insert(NativeString::Literal(length)); None },
        });
        drop(previous);
        0
    })).unwrap_or(1)
}

/// Stabilizes a native value by sharing its complete known origin, or creating a fresh destination.
/// Unknown source addresses are never retained, since they may be reusable scratch or foreign storage.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_persist_v1(destination: u64, source: u64, length: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if destination == 0 || length > isize::MAX as u64 { return 1; }
        if let Some(origin) = origin(source, length) { replace(destination, NativeString::Shared(origin)); }
        else { create_fresh(destination, length); }
        0
    })).unwrap_or(1)
}

/// Resolves a live native origin only when INI needs its bytes, preserving all previously copied aliases.
/// The nonzero returned identity is borrowed until the final native alias is forgotten or reset.
///
/// # Safety
/// The owner must still denote an immutable, readable byte range of the registered length. No other
/// thread may change that range during this call. The host must forget stale owners before reuse.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_native_string_resolve_v1(owner: u64, length: u64) -> u64 {
    catch_unwind(AssertUnwindSafe(|| {
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

/// Releases all native metadata leases before arena reset, retaining table capacity for the next request.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_reset_v1() -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        let mut previous = NATIVE.with(|values| std::mem::take(&mut *values.borrow_mut()));
        previous.clear();
        NATIVE.with(|values| *values.borrow_mut() = previous);
        0
    })).unwrap_or(1)
}

/// Acquires replacement ownership before publishing metadata, preserving the old owner on failure.
fn bind(owner: u64, length: u64, identity: u64) -> i32 {
    if owner == 0 || length > isize::MAX as u64 { return 1; }
    if elephc_mbstring_native_string_lookup_v1(owner, length) == identity && identity != 0 { return 0; }
    let Ok(string) = strings::lookup(identity) else { return 1; };
    if string.len() as u64 != length || strings::retain(identity) != 0 { return 1; }
    replace(owner, NativeString::Shared(Rc::new(SharedOrigin {
        identity: OnceCell::from(Identity(identity)), length, interned: false })));
    0
}

/// Records a validated fresh origin, reusing promoted metadata only without aliases or a resolved ID.
fn create_fresh(owner: u64, length: u64) {
    let previous = NATIVE.with(|values| match values.borrow_mut().entry(owner) {
        Entry::Occupied(mut slot) => {
            if let NativeString::Shared(origin) = slot.get_mut() {
                if let Some(origin) = Rc::get_mut(origin) {
                    if !origin.interned && origin.identity.get().is_none() {
                        origin.length = length;
                        return None;
                    }
                }
            }
            Some(slot.insert(NativeString::Fresh(length)))
        },
        Entry::Vacant(slot) => { slot.insert(NativeString::Fresh(length)); None },
    });
    drop(previous);
}

/// Clones a complete logical origin while keeping the native allocation map borrow short.
fn origin(owner: u64, length: u64) -> Option<Rc<SharedOrigin>> {
    NATIVE.with(|values| values.borrow_mut().get_mut(&owner).filter(|entry| entry.length() == length).map(NativeString::share))
}

/// Publishes a replacement before retiring the previous origin outside the map borrow.
fn replace(owner: u64, replacement: NativeString) {
    let previous = NATIVE.with(|values| values.borrow_mut().insert(owner, replacement));
    drop(previous);
}

/// Removes metadata under a short borrow, then releases its lease after the request map is available again.
fn forget(owner: u64) {
    let previous = NATIVE.with(|values| values.borrow_mut().remove(&owner));
    drop(previous);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps fresh refreshes and repeated literal observations free of shared metadata allocations.
    #[test]
    fn ini_native_unshared_origins_stay_inline() {
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
        let bytes = *b"a\0b";
        let owner = bytes.as_ptr() as u64;
        assert_eq!(elephc_mbstring_native_string_fresh_v1(owner, 0), 0);
        for length in [1, 3, 0, 2] {
            assert_eq!(elephc_mbstring_native_string_fresh_v1(owner, length), 0);
            NATIVE.with(|values| assert!(matches!(values.borrow().get(&owner), Some(NativeString::Fresh(current)) if *current == length)));
            assert_eq!(elephc_mbstring_native_string_lookup_v1(owner, length), 0);
        }
        for _ in 0..4 {
            assert_eq!(elephc_mbstring_native_string_literal_v1(owner, 3), 0);
            NATIVE.with(|values| assert!(matches!(values.borrow().get(&owner), Some(NativeString::Literal(3)))));
        }
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
    }

    /// Reuses promoted unresolved metadata when each temporary copy retires before the next refresh.
    #[test]
    fn ini_native_refresh_reuses_promoted_origin_after_alias_retirement() {
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
        let source = b"abcdefg".to_vec();
        let copy = source.clone();
        let owner = source.as_ptr() as u64;
        let destination = copy.as_ptr() as u64;
        assert_eq!(elephc_mbstring_native_string_fresh_v1(owner, 3), 0);
        assert_eq!(elephc_mbstring_native_string_persist_v1(destination, owner, 3), 0);
        assert_eq!(elephc_mbstring_native_string_forget_v1(destination), 0);
        let address = || NATIVE.with(|values| match values.borrow().get(&owner) {
            Some(NativeString::Shared(entry)) => Some(Rc::as_ptr(entry)), _ => None,
        });
        let original = address();
        assert!(original.is_some());
        for length in [4, 5, 3] {
            assert_eq!(elephc_mbstring_native_string_fresh_v1(owner, length), 0);
            assert_eq!(address(), original, "refresh must preserve unique unresolved promoted metadata");
            assert_eq!(elephc_mbstring_native_string_persist_v1(destination, owner, length), 0);
            assert_eq!(elephc_mbstring_native_string_forget_v1(destination), 0);
        }
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
    }

    /// Reuses a warmed request table while retiring all origins, including high aligned addresses.
    #[test]
    fn ini_native_reset_reuses_request_capacity() {
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
        for index in 1..=256 {
            assert_eq!(elephc_mbstring_native_string_fresh_v1(0x1000_0000_0000 + index * 4096, index), 0);
        }
        let capacity = NATIVE.with(|values| values.borrow().capacity());
        assert!(capacity >= 256);
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
        NATIVE.with(|values| {
            let values = values.borrow();
            assert!(values.is_empty());
            assert_eq!(values.capacity(), capacity);
        });
        for index in 1..=256 {
            let owner = 0x1000_0000_0000 + index * 4096;
            assert_eq!(elephc_mbstring_native_string_fresh_v1(owner, index + 1), 0);
            NATIVE.with(|values| assert_eq!(values.borrow().get(&owner).unwrap().length(), index + 1));
        }
        assert_eq!(NATIVE.with(|values| values.borrow().capacity()), capacity);
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
    }
}
