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

use std::{cell::OnceCell, collections::HashMap, rc::Rc};
use super::*;

struct NativeString { identity: OnceCell<Identity>, length: u64, interned: bool }
struct Identity(u64);

impl Drop for Identity {
    /// Releases the metadata owner's lease without invoking native allocation or PHP code.
    fn drop(&mut self) { strings::release(self.0); }
}

thread_local! { static NATIVE: RefCell<HashMap<u64, Rc<NativeString>>> = RefCell::new(HashMap::new()); }

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

/// Creates a distinct unresolved origin for a newly produced native string, including empty strings.
/// The caller must retire or replace this origin before reusing or changing the allocation's bytes.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_fresh_v1(owner: u64, length: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| create(owner, length, false))).unwrap_or(1)
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

/// Stabilizes a native value by sharing its complete known origin, or creating a fresh destination.
/// Unknown source addresses are never retained, since they may be reusable scratch or foreign storage.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_persist_v1(destination: u64, source: u64, length: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if destination == 0 || length > isize::MAX as u64 { return 1; }
        if let Some(origin) = origin(source, length) { replace(destination, origin); 0 }
        else { create(destination, length, false) }
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

/// Releases all native metadata leases before the host resets this thread's request arena.
#[no_mangle]
pub extern "C" fn elephc_mbstring_native_string_reset_v1() -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        let values = NATIVE.with(|values| std::mem::take(&mut *values.borrow_mut()));
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
fn create(owner: u64, length: u64, interned: bool) -> i32 {
    if owner == 0 || length > isize::MAX as u64 { return 1; }
    replace(owner, Rc::new(NativeString { identity: OnceCell::new(), length, interned }));
    0
}

/// Clones a complete logical origin while keeping the native allocation map borrow short.
fn origin(owner: u64, length: u64) -> Option<Rc<NativeString>> {
    NATIVE.with(|values| values.borrow().get(&owner).filter(|entry| entry.length == length).cloned())
}

/// Publishes a replacement before retiring the previous origin outside the map borrow.
fn replace(owner: u64, replacement: Rc<NativeString>) {
    let previous = NATIVE.with(|values| values.borrow_mut().insert(owner, replacement));
    drop(previous);
}

/// Removes metadata under a short borrow, then releases its lease after the request map is available again.
fn forget(owner: u64) {
    let previous = NATIVE.with(|values| values.borrow_mut().remove(&owner));
    drop(previous);
}
