//! Purpose:
//! Applies an ordered capture graph through protected native or eval reference writes.
//!
//! Called from:
//! - MbInvokeHostV4 capture-fill adapters after shared regex execution.
//!
//! Key details:
//! - The complete flat graph is validated before any destination mutation.
//! - Each store resolves the current destination; pending throws do not skip later captures.
//! - Neither request state nor native value ownership is borrowed across host callbacks.

use std::ffi::c_void;
use elephc_builtin_contract::{RuntimeBuiltinStatus, mbstring_abi::{
    array::{ArrayGraph, Key, Value}, host::{MbHostValueV1, HOST_BOOL, HOST_INT, HOST_STRING},
    invoke::MbCaptureStoreV1,
}};
use super::{catch_unwind, AssertUnwindSafe};

/// Applies borrowed captures in order while preserving a pending PHP exception.
/// The writer remains caller-owned. Unknown callback statuses stop further mutation,
/// preserving an earlier pending throwable. Malformed graphs fail before host callbacks.
///
/// # Safety
/// `bytes` identifies `len` readable bytes, with null accepted only for an empty range.
/// The nonnull writer and context remain valid through every non-unwinding store callback.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_capture_apply_v1(
    context: *mut c_void, writer: *mut c_void, bytes: *const u8, len: u64,
    store: Option<MbCaptureStoreV1>,
) -> i32 {
    let mut pending = false;
    let complete = catch_unwind(AssertUnwindSafe(|| unsafe {
        let store = store?;
        if writer.is_null() || len > isize::MAX as u64 || (len != 0 && bytes.is_null()) { return None; }
        let bytes = if len == 0 { &[] } else { std::slice::from_raw_parts(bytes, len as usize) };
        let graph = ArrayGraph::decode(bytes)?;
        if graph.arrays().len() != 1 { return None; }
        let entries = &graph.arrays()[graph.root()];
        if !entries.iter().all(|(_, value)| matches!(value, Value::String(_) | Value::Bool(false))) {
            return None;
        }
        for (key, value) in entries {
            let key = match key {
                Key::Int(value) => MbHostValueV1 { tag: HOST_INT, lo: *value as u64, hi: 0 },
                Key::String(bytes) => string(bytes),
            };
            let value = match value {
                Value::String(bytes) => string(bytes),
                Value::Bool(false) => MbHostValueV1 { tag: HOST_BOOL, lo: 0, hi: 0 },
                _ => unreachable!("capture graph was validated before host mutation"),
            };
            match store(context, writer, &key, &value) {
                0 => {},
                2 => pending = true,
                _ => return None,
            }
        }
        Some(())
    })).ok().flatten().is_some();
    if pending { RuntimeBuiltinStatus::PendingThrowable as i32 }
    else if complete { RuntimeBuiltinStatus::Success as i32 }
    else { RuntimeBuiltinStatus::RuntimeFatal as i32 }
}

/// Borrows a binary string in the shared native host descriptor representation.
fn string(bytes: &[u8]) -> MbHostValueV1 {
    MbHostValueV1 { tag: HOST_STRING, lo: bytes.as_ptr() as u64, hi: bytes.len() as u64 }
}
