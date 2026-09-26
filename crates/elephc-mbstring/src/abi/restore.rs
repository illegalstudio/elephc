//! Purpose:
//! Restores completed mbstring array graphs through allocator-neutral host callbacks.
//!
//! Called from:
//! - Native result materialization after a shared engine has returned an owned graph.
//!
//! Key details:
//! - Children finish before parents acquire them, preserving aliases without mutating shared arrays.
//! - Conversion outputs are acyclic; malformed or cyclic results fail before host allocation.
//! - An explicit ownership arena survives Rust panics and releases every nonreturned array.
//! - INI identities are validated before allocation and follow original graph indices through construction.

use std::ffi::c_void;
use elephc_builtin_contract::mbstring_abi::{array::{ArrayGraph, Key, Value}, host::*, output::*};
use super::{catch_unwind, AssertUnwindSafe};

/// Reconstructs an acyclic graph and transfers one owned indexed or associative root, or returns zero.
///
/// # Safety
/// `bytes` must describe a readable range for the entire call, with null allowed only at length zero.
/// Callbacks obey their ownership, borrowing, and non-unwinding contracts through final cleanup.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_restore_v1(
    bytes: *const u8, len: u64, build: Option<MbArrayBuildV1>, release: Option<MbArrayReleaseV1>, context: *mut c_void,
) -> u64 {
    let (Some(build), Some(release)) = (build, release) else { return 0; };
    unsafe { restore(bytes, len, None, Builder::Ordinary(build), release, context) }
}

/// Restores an INI graph with one independently retained identity for each native string value.
/// Invalid framing, unleased identities, or identity/byte mismatches fail before host allocation.
///
/// # Safety
/// The wire buffer and its identity leases remain alive and immutable throughout the call.
/// Callbacks satisfy MbIniArrayBuildV1/MbArrayReleaseV1 and never unwind or execute PHP.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_restore_ini_v1(
    bytes: *const u8, len: u64, graph_length: u64, build: Option<MbIniArrayBuildV1>,
    release: Option<MbArrayReleaseV1>, context: *mut c_void,
) -> u64 {
    let (Some(build), Some(release)) = (build, release) else { return 0; };
    unsafe { restore(bytes, len, Some(graph_length), Builder::Ini(build), release, context) }
}

/// Selects the versioned construction contract without changing ordinary callback arguments.
enum Builder { Ordinary(MbArrayBuildV1), Ini(MbIniArrayBuildV1) }

/// Validates either wire format, constructs reachable nodes, and releases the arena on every exit.
unsafe fn restore(bytes: *const u8, len: u64, graph_length: Option<u64>, build: Builder,
    release: MbArrayReleaseV1, context: *mut c_void) -> u64 {
    if len > isize::MAX as u64 || (len != 0 && bytes.is_null()) { return 0; }
    let mut owners = Vec::new();
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        let bytes = if len == 0 { &[] } else { std::slice::from_raw_parts(bytes, len as usize) };
        let (graph, identities) = if let Some(length) = graph_length {
            let (graph, records) = elephc_builtin_contract::mbstring_abi::ini::decode_ini_array(bytes, length)?;
            let mut identities: Vec<Vec<u64>> = graph.arrays().iter().map(|entries| vec![0; entries.len()]).collect();
            for (array, entry, identity) in records {
                let Value::String(value) = &graph.arrays()[array][entry].1 else { return None; };
                let string = super::ini::lookup_string(identity).ok()?;
                if &*string != value.as_slice() { return None; }
                identities[array][entry] = identity;
            }
            (graph, identities)
        } else { (ArrayGraph::decode(bytes)?.into_compact(), Vec::new()) };
        let order = postorder(&graph)?;
        let indexed: Vec<_> = graph.arrays().iter().map(|entries| entries.iter().enumerate()
            .all(|(index, (key, _))| *key == Key::Int(index as i64))).collect();
        owners.resize(graph.arrays().len(), 0);
        for index in order {
            let entries = &graph.arrays()[index];
            let keys: Vec<_> = entries.iter().map(|(key, _)| match key {
                Key::Int(value) => descriptor(HOST_INT, *value as u64, 0),
                Key::String(bytes) => string(bytes),
            }).collect();
            let values: Vec<_> = entries.iter().map(|(_, value)| match value {
                Value::Null => MbHostValueV1::null(),
                Value::Bool(value) => descriptor(HOST_BOOL, u64::from(*value), 0),
                Value::Int(value) => descriptor(HOST_INT, *value as u64, 0),
                Value::Float(bits) => descriptor(HOST_FLOAT, *bits, 0),
                Value::String(bytes) => string(bytes),
                Value::Array(child) => descriptor(if indexed[*child] { HOST_INDEXED_ARRAY } else { HOST_ASSOC_ARRAY }, owners[*child], 0),
                Value::Unsupported => unreachable!("validated completed output"),
            }).collect();
            let owner = match build {
                Builder::Ordinary(callback) => callback(context, entries.len() as u64, keys.as_ptr(), values.as_ptr(), u64::from(indexed[index])),
                Builder::Ini(callback) => callback(context, entries.len() as u64, keys.as_ptr(), values.as_ptr(),
                    u64::from(indexed[index]), identities[index].as_ptr()),
            };
            if owner == 0 { return None; }
            owners[index] = owner;
        }
        Some(graph.root())
    })).ok().flatten();
    let output = result.map_or(0, |root| std::mem::take(&mut owners[root]));
    for owner in owners.into_iter().rev().filter(|owner| *owner != 0) {
        unsafe { release(context, owner); }
    }
    output
}

/// Orders reachable nodes without Rust recursion and rejects cyclic or unsupported output values.
fn postorder(graph: &ArrayGraph) -> Option<Vec<usize>> {
    let mut states = vec![0_u8; graph.arrays().len()];
    let mut work = vec![(graph.root(), false)];
    let mut order = Vec::new();
    while let Some((index, leaving)) = work.pop() {
        if leaving {
            states[index] = 2;
            order.push(index);
            continue;
        }
        match states[index] { 1 => return None, 2 => continue, _ => {} }
        states[index] = 1;
        work.push((index, true));
        for (_, value) in graph.arrays()[index].iter().rev() {
            match value {
                Value::Unsupported => return None,
                Value::Array(child) => work.push((*child, false)),
                _ => {},
            }
        }
    }
    Some(order)
}

/// Forms one concrete descriptor without borrowing or retaining host storage.
fn descriptor(tag: u64, lo: u64, hi: u64) -> MbHostValueV1 { MbHostValueV1 { tag, lo, hi } }

/// Borrows binary bytes until the current construction callback has returned.
fn string(bytes: &[u8]) -> MbHostValueV1 { descriptor(HOST_STRING, bytes.as_ptr() as u64, bytes.len() as u64) }
