//! Purpose:
//! Tests INI graph reconstruction against independent host ownership and leased string identities.
//!
//! Called from:
//! - The mbstring bridge's focused restore integration test binary.
//!
//! Key details:
//! - Nonzero original graph indices expose accidental identity remapping during compaction.
//! - Construction failures and final alias release must retire every acquired identity lease.

use super::*;
use elephc_builtin_contract::mbstring_abi::{ini::*, *};
use elephc_mbstring::abi::*;

/// Accepts protected diagnostics without executing PHP in this pure ownership fixture.
unsafe extern "C" fn diagnostic(_: *mut c_void, _: u32, _: *const u8, _: u64) -> i32 { 0 }

/// Acquires an actual engine-owned fresh identity, keeping its wire lease alive for restoration.
fn string(bytes: &[u8]) -> MbResultV1 {
    let host = MbIniHostV1 { version: 1, size: std::mem::size_of::<MbIniHostV1>() as u32,
        context: std::ptr::null_mut(), diagnostic: Some(diagnostic) };
    let argument = MbArgV1 { kind: ARG_STRING, value: 0, bytes: bytes.as_ptr(), len: bytes.len() as u64 };
    let mut result = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_ini_v1(INI_STRING_NEW, &argument, 1, &host, &mut result) }, 0);
    assert_eq!(result.kind, RESULT_INI_STRING);
    result
}

/// Builds a host array while retaining exactly the nonzero identities supplied for string values.
unsafe extern "C" fn build_ini(context: *mut c_void, count: u64, keys: *const MbHostValueV1,
    values: *const MbHostValueV1, indexed: u64, identities: *const u64) -> u64 {
    let identities = unsafe { std::slice::from_raw_parts(identities, count as usize) };
    for (index, &identity) in identities.iter().enumerate() {
        assert_eq!(identity != 0, unsafe { (*values.add(index)).tag } == HOST_STRING);
    }
    unsafe { build_values(context, count, keys, values, indexed, Some(identities)) }
}

/// Appends a deliberately ordered identity trailer without changing original graph indices.
fn wire(graph: &ArrayGraph, records: &[(u64, u64, u64)]) -> (Vec<u8>, u64) {
    let mut bytes = graph.encode();
    let length = bytes.len() as u64;
    for &(array, entry, identity) in records {
        for value in [array, entry, identity] { bytes.extend_from_slice(&value.to_le_bytes()); }
    }
    (bytes, length)
}

/// Restores a graph through the actual identity-aware ABI using the common ownership fixture.
fn restore_ini(bytes: &[u8], length: u64, host: &mut Host) -> u64 {
    unsafe { elephc_mbstring_restore_ini_v1(bytes.as_ptr(), bytes.len() as u64, length,
        Some(build_ini), Some(release), (host as *mut Host).cast()) }
}

/// Preserves binary strings, equal-but-distinct identities, empty strings, and a shared indexed child.
#[test]
fn mbstring_restore_ini_preserves_original_indices_and_leases() {
    let mut first = string(b"x\0\xff");
    let mut second = string(b"x\0\xff");
    let mut empty = string(b"");
    let (a, b, c) = (first.value as u64, second.value as u64, empty.value as u64);
    assert_ne!(a, b);
    let graph = ArrayGraph::new(2, vec![vec![(Key::Int(0), Value::Unsupported)],
        vec![(Key::Int(0), Value::String(b"x\0\xff".to_vec())), (Key::Int(1), Value::Null)],
        vec![(Key::String(b"first".to_vec()), Value::Array(1)), (Key::Int(-1), Value::Array(1)),
            (Key::String(b"second".to_vec()), Value::String(b"x\0\xff".to_vec())),
            (Key::String(b"empty".to_vec()), Value::String(vec![]))]]).unwrap();
    let (bytes, length) = wire(&graph, &[(2, 3, c), (2, 2, b), (1, 0, a)]);
    let mut host = Host::default();
    let root = restore_ini(&bytes, length, &mut host);
    assert_ne!(root, 0);
    assert_eq!(host.layouts, [1, 0]);
    for result in [&mut first, &mut second, &mut empty] { unsafe { elephc_mbstring_release_v1(result); } }
    let entries = &host.owners[&root];
    let (Stored::Array(child), Stored::Array(alias)) = (&entries[0].1, &entries[1].1) else { panic!("missing aliases"); };
    assert!(Rc::ptr_eq(child, alias));
    assert!(matches!(&child[0].1, Stored::String(bytes, id) if bytes == b"x\0\xff" && *id == a));
    assert!(matches!(&entries[2].1, Stored::String(bytes, id) if bytes == b"x\0\xff" && *id == b));
    assert!(matches!(&entries[3].1, Stored::String(bytes, id) if bytes.is_empty() && *id == c));
    for identity in [a, b, c] {
        assert_eq!(elephc_mbstring_ini_string_retain_v1(identity), 0);
        assert_eq!(elephc_mbstring_ini_string_release_v1(identity), 0);
    }
    unsafe { release((&mut host as *mut Host).cast(), root); }
    assert!(host.owners.is_empty());
    for identity in [a, b, c] { assert_ne!(elephc_mbstring_ini_string_retain_v1(identity), 0); }
}

/// Releases already completed string-bearing children when any later construction callback rejects.
#[test]
fn mbstring_restore_ini_releases_failed_construction() {
    for fail_at in 1..=3 {
        let mut value = string(b"child");
        let identity = value.value as u64;
        let graph = ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::Array(1)), (Key::Int(1), Value::Array(2))],
            vec![(Key::Int(0), Value::String(b"child".to_vec()))], vec![]]).unwrap();
        let (bytes, length) = wire(&graph, &[(1, 0, identity)]);
        let mut host = Host { fail_at, ..Host::default() };
        assert_eq!(restore_ini(&bytes, length, &mut host), 0);
        assert_eq!(host.calls, fail_at);
        assert!(host.owners.is_empty());
        unsafe { elephc_mbstring_release_v1(&mut value); }
        assert_ne!(elephc_mbstring_ini_string_retain_v1(identity), 0);
    }
}

/// Rejects malformed identity framing and unleased or mismatched strings before any host allocation.
#[test]
fn mbstring_restore_ini_rejects_invalid_identities() {
    let mut value = string(b"abc");
    let identity = value.value as u64;
    let graph = ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::String(b"abc".to_vec()))]]).unwrap();
    let (bytes, length) = wire(&graph, &[(0, 0, identity)]);
    let mut host = Host::default();
    for end in 0..bytes.len() { assert_eq!(restore_ini(&bytes[..end], length, &mut host), 0); }
    for records in [vec![(0, 0, 0)], vec![(0, 0, i64::MAX as u64)], vec![(0, 1, identity)],
        vec![(0, 0, identity), (0, 0, identity)]] {
        let (bytes, length) = wire(&graph, &records);
        assert_eq!(restore_ini(&bytes, length, &mut host), 0);
    }
    let different = ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::String(b"xyz".to_vec()))]]).unwrap();
    let (other, prefix) = wire(&different, &[(0, 0, identity)]);
    assert_eq!(restore_ini(&other, prefix, &mut host), 0);
    assert_eq!(restore_ini(&bytes, u64::MAX, &mut host), 0);
    assert_eq!(unsafe { elephc_mbstring_restore_ini_v1(std::ptr::null(), 1, 0,
        Some(build_ini), Some(release), (&mut host as *mut Host).cast()) }, 0);
    assert_eq!(unsafe { elephc_mbstring_restore_ini_v1(bytes.as_ptr(), bytes.len() as u64, length,
        None, Some(release), (&mut host as *mut Host).cast()) }, 0);
    unsafe { elephc_mbstring_release_v1(&mut value); }
    assert_eq!(restore_ini(&bytes, length, &mut host), 0);
    assert_eq!(host.calls, 0);
}
