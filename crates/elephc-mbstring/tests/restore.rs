//! Purpose:
//! Tests completed array restoration, binary descriptors, alias ownership, and failure cleanup.
//!
//! Called from:
//! - The focused mbstring bridge integration harness.
//!
//! Key details:
//! - An independent host copies callback inputs into reference-counted arrays.
//! - Every injected builder failure must release all completed child owners.

use std::{collections::HashMap, ffi::c_void, rc::Rc};
use elephc_builtin_contract::mbstring_abi::{array::{ArrayGraph, Key, Value}, host::*};
use elephc_mbstring::abi::elephc_mbstring_restore_v1;

#[path = "restore/ini.rs"]
mod ini;

/// Independent host values retain raw scalar bits and shared child identities.
#[derive(Debug, PartialEq)]
enum Stored { Scalar(u64, u64), String(Vec<u8>, u64), Array(Rc<Vec<(Key, Stored)>>) }

impl Drop for Stored {
    /// Releases an acquired INI lease with its final independent host string owner.
    fn drop(&mut self) {
        if let Self::String(_, identity) = self {
            if *identity != 0 { assert_eq!(elephc_mbstring::abi::elephc_mbstring_ini_string_release_v1(*identity), 0); }
        }
    }
}

/// Tracks every transferred array owner and optionally rejects one construction call.
#[derive(Default)]
struct Host { owners: HashMap<u64, Rc<Vec<(Key, Stored)>>>, calls: usize, fail_at: usize, layouts: Vec<u64> }

/// Copies one borrowed binary descriptor before another callback can invalidate its source.
unsafe fn bytes(value: MbHostValueV1) -> Vec<u8> {
    if value.hi == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(value.lo as *const u8, value.hi as usize).to_vec() } }
}

/// Builds an independent array while retaining previously completed child arrays.
unsafe extern "C" fn build(context: *mut c_void, count: u64, keys: *const MbHostValueV1, values: *const MbHostValueV1, indexed: u64) -> u64 {
    unsafe { build_values(context, count, keys, values, indexed, None) }
}

/// Copies values and optional string identities under the same independent array ownership model.
unsafe fn build_values(context: *mut c_void, count: u64, keys: *const MbHostValueV1,
    values: *const MbHostValueV1, indexed: u64, identities: Option<&[u64]>) -> u64 {
    let host = unsafe { &mut *context.cast::<Host>() };
    host.calls += 1;
    host.layouts.push(indexed);
    if host.calls == host.fail_at { return 0; }
    let mut entries = Vec::new();
    for index in 0..count as usize {
        let key = unsafe { *keys.add(index) };
        let value = unsafe { *values.add(index) };
        let key = if key.tag == HOST_INT { Key::Int(key.lo as i64) } else { Key::String(unsafe { bytes(key) }) };
        let value = match value.tag {
            HOST_STRING => {
                let identity = identities.map_or(0, |ids| ids[index]);
                if identity != 0 { assert_eq!(elephc_mbstring::abi::elephc_mbstring_ini_string_retain_v1(identity), 0); }
                Stored::String(unsafe { bytes(value) }, identity)
            },
            HOST_INDEXED_ARRAY | HOST_ASSOC_ARRAY => Stored::Array(Rc::clone(&host.owners[&value.lo])),
            _ => Stored::Scalar(value.tag, value.lo),
        };
        entries.push((key, value));
    }
    let handle = host.calls as u64;
    host.owners.insert(handle, Rc::new(entries));
    handle
}

/// Consumes exactly one explicit host owner while nested arrays retain their own child references.
unsafe extern "C" fn release(context: *mut c_void, owner: u64) {
    unsafe { &mut *context.cast::<Host>() }.owners.remove(&owner);
}

/// Restores one graph through the actual C boundary and independent construction callbacks.
fn restore(graph: &ArrayGraph, host: &mut Host) -> u64 {
    let wire = graph.encode();
    unsafe { elephc_mbstring_restore_v1(wire.as_ptr(), wire.len() as u64, Some(build), Some(release), (host as *mut Host).cast()) }
}

/// Preserves ordered exact keys, binary strings, float bits, and shared child identity with balanced owners.
#[test]
fn mbstring_restore_preserves_values_and_aliases() {
    let graph = ArrayGraph::new(0, vec![vec![
        (Key::String(b"1".to_vec()), Value::Array(1)), (Key::Int(1), Value::Array(1)),
        (Key::String(b"\0\xff".to_vec()), Value::Float(0x7ff8_0000_0000_1234)),
        (Key::Int(i64::MIN), Value::Null), (Key::Int(9), Value::Bool(true)),
    ], vec![(Key::Int(0), Value::String(b"x\0\xff".to_vec())), (Key::Int(1), Value::Int(i64::MIN))]]).unwrap();
    let mut host = Host::default();
    let root = restore(&graph, &mut host);
    assert_ne!(root, 0);
    assert_eq!(host.calls, 2);
    assert_eq!(host.layouts, vec![1, 0]);
    assert_eq!(host.owners.len(), 1);
    let entries = &host.owners[&root];
    let (Stored::Array(first), Stored::Array(second)) = (&entries[0].1, &entries[1].1) else { panic!("missing nested arrays"); };
    assert!(Rc::ptr_eq(first, second));
    assert_eq!(entries[0].0, Key::String(b"1".to_vec()));
    assert_eq!(entries[1].0, Key::Int(1));
    assert_eq!(first[0].1, Stored::String(b"x\0\xff".to_vec(), 0));
    assert_eq!(first[1].1, Stored::Scalar(HOST_INT, i64::MIN as u64));
    assert_eq!(entries[2].1, Stored::Scalar(HOST_FLOAT, 0x7ff8_0000_0000_1234));
    assert_eq!(entries[3].1, Stored::Scalar(HOST_NULL, 0));
    assert_eq!(entries[4].1, Stored::Scalar(HOST_BOOL, 1));
    let child = Rc::downgrade(first);
    unsafe { release((&mut host as *mut Host).cast(), root); }
    assert!(host.owners.is_empty());
    assert!(child.upgrade().is_none());
}

/// Releases all previously completed nodes after each possible host construction failure.
#[test]
fn mbstring_restore_releases_failed_construction() {
    let graph = ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::Array(1)), (Key::Int(1), Value::Array(2))],
        vec![(Key::Int(0), Value::String(b"child".to_vec()))], vec![]]).unwrap();
    for fail_at in 1..=3 {
        let mut host = Host { fail_at, ..Host::default() };
        assert_eq!(restore(&graph, &mut host), 0);
        assert_eq!(host.calls, fail_at);
        assert!(host.owners.is_empty());
    }
}

/// Rejects unsupported values, cycles, truncated framing, and missing callbacks before allocation.
#[test]
fn mbstring_restore_rejects_invalid_results() {
    let mut host = Host::default();
    for value in [Value::Array(0), Value::Unsupported] {
        assert_eq!(restore(&ArrayGraph::new(0, vec![vec![(Key::Int(0), value)]]).unwrap(), &mut host), 0);
    }
    let wire = ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::String(b"abc".to_vec()))]]).unwrap().encode();
    for length in 0..wire.len() {
        assert_eq!(unsafe { elephc_mbstring_restore_v1(wire.as_ptr(), length as u64, Some(build), Some(release), (&mut host as *mut Host).cast()) }, 0);
    }
    assert_eq!(unsafe { elephc_mbstring_restore_v1(wire.as_ptr(), wire.len() as u64, None, Some(release), (&mut host as *mut Host).cast()) }, 0);
    assert_eq!(unsafe { elephc_mbstring_restore_v1(std::ptr::null(), 1, Some(build), Some(release), (&mut host as *mut Host).cast()) }, 0);
    assert_eq!(host.calls, 0);
    assert!(host.owners.is_empty());
}
