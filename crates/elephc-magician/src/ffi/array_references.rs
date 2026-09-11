//! Purpose:
//! Reads eval reference metadata for copied native mbstring array arguments.
//!
//! Called from:
//! - The protected native/eval encoding-list entry callback.
//!
//! Key details:
//! - Original array pointers are opaque identity keys, not borrowed runtime storage.
//! - Every temporary owner is released through a supplied protected native action.
//! - Pending eval Throwables are published before cleanup can produce a later exception.

use std::{ffi::c_void, panic::{catch_unwind, AssertUnwindSafe}};
use elephc_builtin_contract::mbstring_abi::{host::{MbHostValueV1, HOST_INT, HOST_STRING}, invoke::MbArrayReferenceHooksV2};
use crate::{abi::{ElephcEvalContext, ABI_VERSION}, context::EvalArrayReferenceKey,
    errors::EvalStatus, interpreter::array_references::read_owned_array_reference,
    runtime_hooks::ElephcRuntimeOps, value::RuntimeCellHandle};

/// Copies the current referenced entry or returns success with no owner when no alias exists.
///
/// # Safety
/// The context and hooks are valid for this call. `key` describes live borrowed key bytes,
/// and `out` points at writable owner storage. `original` is used only as an identity token.
/// Supplied native actions contain PHP exceptions and consume released/published owners.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_array_reference_value(
    context: *mut ElephcEvalContext, original: *const c_void, key: *const MbHostValueV1,
    hooks: *const MbArrayReferenceHooksV2, out: *mut *mut c_void,
) -> i32 {
    unsafe { resolve(context, original, key, hooks, out, std::ptr::null_mut()) }
}

/// Returns a copied reference value plus its retained original box for recursive graph traversal.
///
/// # Safety
/// All arguments obey the ordinary reference reader contract. `identity` is writable owner
/// storage consumed through the protected release hook, including on callback failure.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_array_graph_reference_value(
    context: *mut ElephcEvalContext, original: *const c_void, key: *const MbHostValueV1,
    hooks: *const MbArrayReferenceHooksV2, out: *mut *mut c_void, identity: *mut *mut c_void,
) -> i32 {
    if identity.is_null() { return 1; }
    unsafe { resolve(context, original, key, hooks, out, identity) }
}

/// Resolves reference metadata with an optional identity lease and balanced protected cleanup.
unsafe fn resolve(
    context: *mut ElephcEvalContext, original: *const c_void, key: *const MbHostValueV1,
    hooks: *const MbArrayReferenceHooksV2, out: *mut *mut c_void, identity: *mut *mut c_void,
) -> i32 {
    if out.is_null() { return 1; }
    unsafe { *out = std::ptr::null_mut(); }
    if !identity.is_null() { unsafe { *identity = std::ptr::null_mut(); } }
    let Some(hooks) = (unsafe { hooks.as_ref() }) else { return 1; };
    let mut owners = Vec::new();
    let mut pending = None;
    let mut status = catch_unwind(AssertUnwindSafe(|| {
        let result = unsafe { lookup(context, original, key, &mut owners, &mut pending) };
        match result {
            Ok(Some(value)) => {
                if identity.is_null() { owners.push(value); }
                else { unsafe { *identity = value.as_ptr(); } }
                normalized(unsafe { (hooks.clone_value)(std::ptr::null_mut(), value.as_ptr(), out) })
            },
            Ok(None) => 0,
            Err(_) => 1,
        }
    })).unwrap_or(1);
    if let Some(throwable) = pending {
        let published = normalized(unsafe { (hooks.publish_throw)(std::ptr::null_mut(), throwable.as_ptr()) });
        status = if published == 0 { 2 } else { published };
    }
    for owner in owners.into_iter().rev() {
        let released = normalized(unsafe { (hooks.release_owner)(std::ptr::null_mut(), owner.as_ptr()) });
        if released == 2 || (released != 0 && status != 2) { status = released; }
    }
    status
}

/// Resolves alias metadata and ends the context borrow before native copy or cleanup callbacks.
unsafe fn lookup(
    context: *mut ElephcEvalContext, original: *const c_void, key: *const MbHostValueV1,
    owners: &mut Vec<RuntimeCellHandle>, pending: &mut Option<RuntimeCellHandle>,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let context = unsafe { context.as_mut() }.ok_or(EvalStatus::RuntimeFatal)?;
    if context.abi_version() != ABI_VERSION { return Err(EvalStatus::RuntimeFatal); }
    let key = unsafe { key.as_ref() }.ok_or(EvalStatus::RuntimeFatal)?;
    let key = match key.tag {
        HOST_INT => EvalArrayReferenceKey::Int(key.lo as i64),
        HOST_STRING if key.hi <= isize::MAX as u64 && (key.hi == 0 || key.lo != 0) => {
            let bytes = if key.hi == 0 { Vec::new() }
                else { unsafe { std::slice::from_raw_parts(key.lo as *const u8, key.hi as usize).to_vec() } };
            EvalArrayReferenceKey::String(bytes)
        },
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let mut values = ElephcRuntimeOps::with_context(context);
    let result = read_owned_array_reference(
        RuntimeCellHandle::from_raw(original.cast_mut()).borrowed(),
        key,
        context,
        &mut values,
        owners,
    );
    if matches!(result, Err(EvalStatus::UncaughtThrowable)) { *pending = context.take_pending_throw(); }
    result
}

/// Keeps the shared success/fatal/pending status contract closed for unknown callback results.
fn normalized(status: i32) -> i32 { if matches!(status, 0 | 2) { status } else { 1 } }
