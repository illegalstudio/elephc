//! Purpose:
//! Copies borrowed host array graphs into the pointer-free mbstring wire format.
//!
//! Called from:
//! - Shared AOT/eval argument adapters before invoking a request-state operation.
//!
//! Key details:
//! - The reader executes outside the Rust request-state borrow and must not unwind or mutate arrays.
//! - Strings are copied before another reader invocation; arrays retain identity without Rust cycles.
//! - Malformed metadata or reader failures discard the partial graph and return an explicit fatal result.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use elephc_builtin_contract::mbstring_abi::{array::{ArrayGraph, Key, Value}, host::*};
use super::*;

/// Snapshots an array through a nonmutating host reader and transfers one owned wire result.
///
/// # Safety
/// `root` must point to a readable host array descriptor and `out` to writable, aligned,
/// uninitialized or previously released result storage. `next` and `context` must obey
/// MbArrayNextV1's borrowing and non-unwinding contract for the complete reachable graph.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_snapshot_v1(
    root: *const MbHostValueV1, next: Option<MbArrayNextV1>, context: *mut c_void, out: *mut MbResultV1,
) {
    if out.is_null() { return; }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let Some(next) = next else { return Outcome::fatal(); };
        let Some(root) = (unsafe { root.as_ref() }) else { return Outcome::fatal(); };
        match unsafe { snapshot(*root, next, context) } {
            Some(graph) => Outcome { bytes: graph.encode(), ..Outcome::empty(RESULT_ARRAY) },
            None => Outcome::fatal(),
        }
    })).unwrap_or_else(|_| Outcome::fatal());
    unsafe { out.write(result.into_wire()); }
}

/// Walks each distinct host array once and preserves its insertion-ordered entries.
pub(super) unsafe fn snapshot(root: MbHostValueV1, next: MbArrayNextV1, context: *mut c_void) -> Option<ArrayGraph> {
    if !matches!(root.tag, HOST_INDEXED_ARRAY | HOST_ASSOC_ARRAY) { return None; }
    let mut identities = HashMap::from([((root.tag, root.lo), 0)]);
    let mut pending = vec![root];
    let mut arrays = Vec::new();
    while let Some(array) = pending.get(arrays.len()).copied() {
        let mut cursor = 0;
        let mut seen = HashSet::from([cursor]);
        let mut entries = Vec::new();
        loop {
            let mut key = MbHostValueV1::null();
            let mut value = MbHostValueV1::null();
            match unsafe { next(context, &array, &mut cursor, &mut key, &mut value) } {
                ITER_END => break,
                ITER_ENTRY if seen.insert(cursor) => {},
                _ => return None,
            }
            let key = match key.tag {
                HOST_INT => Key::Int(key.lo as i64),
                HOST_STRING => Key::String(unsafe { copy_string(key) }?),
                _ => return None,
            };
            let value = match value.tag {
                HOST_NULL => Value::Null,
                HOST_INT => Value::Int(value.lo as i64),
                HOST_FLOAT => Value::Float(value.lo),
                HOST_BOOL if value.lo <= 1 => Value::Bool(value.lo != 0),
                HOST_STRING => Value::String(unsafe { copy_string(value) }?),
                HOST_INDEXED_ARRAY | HOST_ASSOC_ARRAY => {
                    let identity = *identities.entry((value.tag, value.lo)).or_insert_with(|| {
                        let identity = pending.len();
                        pending.push(value);
                        identity
                    });
                    Value::Array(identity)
                }
                HOST_UNSUPPORTED => Value::Unsupported,
                _ => return None,
            };
            entries.push((key, value));
        }
        arrays.push(entries);
    }
    ArrayGraph::new(0, arrays)
}

/// Validates the borrowed range before copying bytes, without interpreting their encoding.
unsafe fn copy_string(value: MbHostValueV1) -> Option<Vec<u8>> {
    if value.hi > isize::MAX as u64 { return None; }
    if value.hi == 0 { return Some(Vec::new()); }
    if value.lo == 0 { return None; }
    Some(unsafe { std::slice::from_raw_parts(value.lo as *const u8, value.hi as usize).to_vec() })
}
