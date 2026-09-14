//! Purpose:
//! Verifies native allocation metadata ownership independently of allocator pointer reuse.
//!
//! Called from:
//! - The mbstring native INI metadata integration harness.
//!
//! Key details:
//! - Bind/copy addresses are opaque; resolving an origin reads a still-live native allocation.
//! - Metadata owns leases without keeping native storage alive; reset affects only the current thread.

use elephc_builtin_contract::mbstring_abi::{*, ini::*};
use elephc_mbstring::abi::*;

/// Supplies a silent diagnostic callback for string imports, which never emit PHP diagnostics.
unsafe extern "C" fn diagnostic(_: *mut std::ffi::c_void, _: u32, _: *const u8, _: u64) -> i32 { 0 }

/// Imports one fresh string and transfers a single retained lease out of its temporary result.
fn identity(bytes: &[u8]) -> u64 {
    let argument = MbArgV1::string(bytes);
    let host = MbIniHostV1 { version: 1, size: std::mem::size_of::<MbIniHostV1>() as u32,
        context: std::ptr::null_mut(), diagnostic: Some(diagnostic) };
    let mut result = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_ini_v1(INI_STRING_NEW, &argument, 1, &host, &mut result) }, 0);
    let identity = result.value as u64;
    assert_eq!(elephc_mbstring_ini_string_retain_v1(identity), 0);
    unsafe { elephc_mbstring_release_v1(&mut result); }
    identity
}

/// Preserves aliases made before INI access without keeping their first native allocation alive.
#[test]
fn ini_native_lazy_origins_survive_prebinding_aliases_and_pointer_reuse() {
    assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
    let source = b"ASCII".to_vec();
    let copy = source.clone();
    let third = copy.clone();
    let pointer = |bytes: &[u8]| bytes.as_ptr() as u64;
    assert_eq!(elephc_mbstring_native_string_fresh_v1(pointer(&source), 5), 0);
    assert_eq!(elephc_mbstring_native_string_persist_v1(pointer(&copy), pointer(&source), 5), 0);
    assert_eq!(elephc_mbstring_native_string_copy_v1(pointer(&third), pointer(&copy), 5), 0);
    assert_eq!(elephc_mbstring_native_string_lookup_v1(pointer(&copy), 5), 0, "origins must stay unresolved until needed");
    assert_eq!(elephc_mbstring_native_string_forget_v1(pointer(&source)), 0);
    drop(source);
    let id = unsafe { elephc_mbstring_native_string_resolve_v1(pointer(&copy), 5) };
    assert_ne!(id, 0);
    assert_eq!(elephc_mbstring_native_string_lookup_v1(pointer(&third), 5), id);
    assert_eq!(unsafe { elephc_mbstring_native_string_resolve_v1(pointer(&third), 5) }, id);
    assert_eq!(elephc_mbstring_native_string_fresh_v1(pointer(&copy), 5), 0);
    let replacement = unsafe { elephc_mbstring_native_string_resolve_v1(pointer(&copy), 5) };
    assert_ne!(replacement, id, "equal bytes after replacement must not reuse the previous identity");
    assert_eq!(elephc_mbstring_native_string_lookup_v1(pointer(&third), 5), id);
    assert_eq!(unsafe { elephc_mbstring_native_string_resolve_v1(pointer(&third), 4) }, 0);
    assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
    assert_eq!(elephc_mbstring_ini_string_retain_v1(id), 1);
    assert_eq!(elephc_mbstring_ini_string_retain_v1(replacement), 1);
}

/// Distinguishes interned literals from fresh equal bytes, including zero-byte and one-byte strings.
#[test]
fn ini_native_origins_do_not_intern_scratch_or_fresh_short_strings() {
    for bytes in [b"".as_slice(), b"a", b"ASCII"] {
        let mut first = vec![0_u8; bytes.len().max(1)];
        let mut second = first.clone();
        first[..bytes.len()].copy_from_slice(bytes);
        second[..bytes.len()].copy_from_slice(bytes);
        let a = first.as_ptr() as u64;
        let b = second.as_ptr() as u64;
        let length = bytes.len() as u64;
        assert_eq!(elephc_mbstring_native_string_literal_v1(a, length), 0);
        assert_eq!(elephc_mbstring_native_string_literal_v1(b, length), 0);
        let interned = unsafe { elephc_mbstring_native_string_resolve_v1(a, length) };
        assert_ne!(interned, 0);
        assert_eq!(unsafe { elephc_mbstring_native_string_resolve_v1(b, length) }, interned);
        assert_eq!(elephc_mbstring_native_string_literal_v1(a, length), 0);
        assert_eq!(elephc_mbstring_native_string_lookup_v1(a, length), interned);
        assert_eq!(elephc_mbstring_native_string_fresh_v1(b, length), 0);
        let fresh = unsafe { elephc_mbstring_native_string_resolve_v1(b, length) };
        assert_ne!(fresh, 0);
        assert_ne!(fresh, interned);
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
        assert_eq!(elephc_mbstring_ini_string_retain_v1(fresh), 1);
        assert_eq!(elephc_mbstring_ini_string_retain_v1(interned), 1);
    }
    assert_eq!(elephc_mbstring_native_string_fresh_v1(0, 0), 1);
    assert_eq!(elephc_mbstring_native_string_literal_v1(1, u64::MAX), 1);
    assert_eq!(unsafe { elephc_mbstring_native_string_resolve_v1(0, 0) }, 0);
    let scratch = b"ASCII".to_vec();
    let first = scratch.clone();
    let second = scratch.clone();
    let source = scratch.as_ptr() as u64;
    let a = first.as_ptr() as u64;
    let b = second.as_ptr() as u64;
    assert_eq!(elephc_mbstring_native_string_persist_v1(a, source, 5), 0);
    assert_eq!(elephc_mbstring_native_string_persist_v1(b, source, 5), 0);
    let one = unsafe { elephc_mbstring_native_string_resolve_v1(a, 5) };
    let two = unsafe { elephc_mbstring_native_string_resolve_v1(b, 5) };
    assert_ne!(one, 0);
    assert_ne!(two, 0);
    assert_ne!(one, two, "separate copies from scratch must have distinct origins even for equal bytes");
    assert_eq!(elephc_mbstring_native_string_lookup_v1(source, 5), 0, "scratch source addresses must never be retained");
    assert_eq!(elephc_mbstring_native_string_persist_v1(b, a, 4), 0);
    let short = unsafe { elephc_mbstring_native_string_resolve_v1(b, 4) };
    assert_ne!(short, 0);
    assert_ne!(short, one, "a different-length view is a separate logical value");
    assert_eq!(elephc_mbstring_native_string_lookup_v1(a, 5), one);
    assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
    for id in [one, two, short] { assert_eq!(elephc_mbstring_ini_string_retain_v1(id), 1); }
}

/// Exercises repeated binding, source copying, length mismatches, allocation reuse, reset, and lease expiry.
#[test]
fn ini_native_metadata_preserves_aliases_and_retires_reused_allocations() {
    assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
    let id = identity(b"ASCII");
    assert_eq!(elephc_mbstring_native_string_bind_v1(0, 5, id), 1);
    assert_eq!(elephc_mbstring_native_string_bind_v1(0x100, 4, id), 1);
    assert_eq!(elephc_mbstring_native_string_bind_v1(0x100, 5, id), 0);
    assert_eq!(elephc_mbstring_native_string_bind_v1(0x100, 5, id), 0);
    assert_eq!(elephc_mbstring_ini_string_release_v1(id), 0);
    assert_eq!(elephc_mbstring_native_string_copy_v1(0x200, 0x100, u64::MAX), 1);
    assert_eq!(elephc_mbstring_native_string_copy_v1(0x200, 0x100, 5), 0);
    assert_eq!(elephc_mbstring_native_string_forget_v1(0x100), 0);
    assert_eq!(elephc_mbstring_native_string_lookup_v1(0x100, 5), 0);
    assert_eq!(elephc_mbstring_native_string_lookup_v1(0x200, 5), id);
    assert_eq!(elephc_mbstring_native_string_lookup_v1(0x200, 4), 0);
    assert_eq!(elephc_mbstring_native_string_copy_v1(0x300, 0x200, 4), 0);
    assert_eq!(elephc_mbstring_native_string_lookup_v1(0x300, 4), 0);
    assert_eq!(elephc_mbstring_native_string_forget_v1(0x200), 0);
    assert_eq!(elephc_mbstring_ini_string_retain_v1(id), 1, "repeated bind must not acquire an extra lease");
    assert_eq!(elephc_mbstring_native_string_bind_v1(0x100, 5, id), 1);
    let next = identity(b"ASCII");
    assert_ne!(id, next);
    assert_eq!(elephc_mbstring_native_string_bind_v1(0x100, 5, next), 0);
    std::thread::spawn(move || {
        assert_eq!(elephc_mbstring_native_string_lookup_v1(0x100, 5), 0);
        assert_eq!(elephc_mbstring_native_string_bind_v1(0x100, 5, next), 0);
        assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
    }).join().unwrap();
    assert_eq!(elephc_mbstring_native_string_lookup_v1(0x100, 5), next);
    assert_eq!(elephc_mbstring_ini_string_release_v1(next), 0);
    assert_eq!(elephc_mbstring_native_string_reset_v1(), 0);
    assert_eq!(elephc_mbstring_ini_string_retain_v1(next), 1);
    assert_eq!(elephc_mbstring_native_string_lookup_v1(0x100, 5), 0);
}
