//! Purpose:
//! Executes shared query registration instructions over protected native storage callbacks.
//!
//! Called from:
//! - Native AOT/eval query register adapters using the V1 storage inventory.
//!
//! Key details:
//! - This executor owns no request-state borrow and performs no PHP name normalization.
//! - Child pins transfer before old cursors are released, including after pending callbacks.
//! - Root removal retires the nested cursor and resolves the writer's current root again.

use std::{ffi::c_void, mem::{align_of, size_of}, ptr};
use elephc_builtin_contract::mbstring_abi::{invoke::*, host::{MbHostValueV1, HOST_INT, HOST_STRING}};
use super::{catch_unwind, AssertUnwindSafe};

#[cfg(test)]
mod tests;

/// Applies one field's normalized Enter/Store/RemoveRoot plan through protected storage operations.
///
/// # Safety
/// Nonempty ranges and the advertised storage table must be readable and aligned, and `out`
/// must be writable. Context/writer handles and all callbacks remain valid through cleanup.
/// Callbacks must contain PHP exceptions and publish any acquired cursor before returning.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_query_apply_v1(
    context: *mut c_void, writer: *mut c_void, steps: *const MbQueryStepV1, count: u64,
    bytes: *const u8, length: u64, out: *mut MbQueryRegisteredV1, storage: *const MbQueryStorageV1,
) -> i32 {
    if out.is_null() || !out.is_aligned() { return 1; }
    unsafe { out.write(MbQueryRegisteredV1::default()); }
    let Some(callbacks) = (unsafe { snapshot_storage(storage) }) else { return 1; };
    if !valid_range(steps, count) || !valid_range(bytes, length) { return 1; }
    let steps = if count == 0 { &[] } else { unsafe { std::slice::from_raw_parts(steps, count as usize) } };
    let value = MbHostValueV1 { tag: HOST_STRING, lo: bytes as u64, hi: length };
    let mut runner = Runner { context, writer, callbacks, current: ptr::null_mut(), spare: ptr::null_mut(),
        pending: false, failed: false };
    let completed = catch_unwind(AssertUnwindSafe(|| unsafe { runner.execute(steps, &value, out) }));
    if completed.is_err() { runner.failed = true; }
    unsafe { runner.cleanup(); }
    if runner.pending { 2 } else if runner.failed { 1 } else { 0 }
}

/// Validated callbacks copied before PHP can reenter or alter caller-owned table storage.
#[derive(Clone, Copy)]
struct Callbacks {
    root: MbQueryRootV1, next: MbQueryNextV1, enter: MbQueryEnterV1,
    store: MbQueryStoreV1, remove: MbQueryRemoveV1, release: MbQueryReleaseV1,
}

/// Checks the readable header before loading a complete required callback inventory.
unsafe fn snapshot_storage(storage: *const MbQueryStorageV1) -> Option<Callbacks> {
    if storage.is_null() || !storage.is_aligned() { return None; }
    let version = unsafe { ptr::addr_of!((*storage).abi_version).read() };
    let size = unsafe { ptr::addr_of!((*storage).struct_size).read() };
    if version != 1 || size < size_of::<MbQueryStorageV1>() as u32 { return None; }
    let table = unsafe { storage.read() };
    Some(Callbacks { root: table.root?, next: table.next?, enter: table.enter?,
        store: table.store?, remove: table.remove?, release: table.release? })
}

/// Rejects null nonempty ranges, misalignment, impossible slice sizes, and address wrapping.
fn valid_range<T>(data: *const T, count: u64) -> bool {
    if count == 0 { return true; }
    if data.is_null() || (data as usize) % align_of::<T>() != 0 { return false; }
    let Some(bytes) = count.checked_mul(size_of::<T>() as u64) else { return false; };
    bytes <= isize::MAX as u64 && (data as usize).checked_add(bytes as usize).is_some()
}

/// Owns at most two cursor pins and tracks completed mutation independently of pending exceptions.
struct Runner {
    context: *mut c_void, writer: *mut c_void, callbacks: Callbacks,
    current: *mut c_void, spare: *mut c_void, pending: bool, failed: bool,
}

impl Runner {
    /// Resolves the current root, transfers nested cursors, and stops at a terminal store/removal.
    unsafe fn execute(&mut self, steps: &[MbQueryStepV1], value: &MbHostValueV1, out: *mut MbQueryRegisteredV1) {
        if steps.is_empty() { return; }
        if !unsafe { self.root() } || self.current.is_null() { return; }
        for step in steps {
            if !matches!(step.operation, QUERY_ENTER | QUERY_STORE | QUERY_REMOVE_ROOT)
                || step.append > 1 || (step.operation == QUERY_REMOVE_ROOT && step.append != 0) {
                self.failed = true;
                return;
            }
            let key = if step.append == 1 {
                let mut index = MbQueryIndexV1::default();
                let status = unsafe { (self.callbacks.next)(self.context, self.current, &mut index) };
                if !self.accept(status) { return; }
                match index.available {
                    0 => return,
                    1 => MbHostValueV1 { tag: HOST_INT, lo: index.index as u64, hi: 0 },
                    _ => { self.failed = true; return; },
                }
            } else {
                if !valid_key(&step.key) { self.failed = true; return; }
                step.key
            };
            if step.operation == QUERY_REMOVE_ROOT {
                // The nested cursor must not postpone destruction of the root entry being removed.
                unsafe { self.release_current(); }
                if self.failed || !unsafe { self.root() } { return; }
                if !self.current.is_null() {
                    let status = unsafe { (self.callbacks.remove)(self.context, self.current, &key) };
                    if !self.accept(status) { return; }
                }
                unsafe { (*out).nesting_exceeded = 1; }
                return;
            }
            if step.operation == QUERY_STORE {
                let status = unsafe { (self.callbacks.store)(self.context, self.current, &key, value) };
                self.accept(status);
                return;
            }
            let status = unsafe { (self.callbacks.enter)(self.context, self.current, &key, &mut self.spare) };
            if !self.accept(status) { return; }
            if self.spare.is_null() { self.failed = true; return; }
            let old = std::mem::replace(&mut self.current, std::mem::replace(&mut self.spare, ptr::null_mut()));
            let status = unsafe { (self.callbacks.release)(self.context, old) };
            if !self.accept(status) { return; }
        }
    }

    /// Acquires the writer's live array pin, including independently published metadata after failure.
    unsafe fn root(&mut self) -> bool {
        let status = unsafe { (self.callbacks.root)(self.context, self.writer, &mut self.current) };
        self.accept(status)
    }

    /// Records pending exceptions while preserving fatal-stop decisions for malformed callback statuses.
    fn accept(&mut self, status: i32) -> bool {
        match status {
            0 => true,
            2 => { self.pending = true; true },
            _ => { self.failed = true; false },
        }
    }

    /// Detaches the current pin before protected cleanup can reenter and retarget the writer.
    unsafe fn release_current(&mut self) {
        let current = std::mem::replace(&mut self.current, ptr::null_mut());
        if !current.is_null() {
            let status = unsafe { (self.callbacks.release)(self.context, current) };
            self.accept(status);
        }
    }

    /// Retires every published pin after success, fatal callback status, or contained Rust panic.
    unsafe fn cleanup(&mut self) {
        let spare = std::mem::replace(&mut self.spare, ptr::null_mut());
        if !spare.is_null() {
            let status = unsafe { (self.callbacks.release)(self.context, spare) };
            self.accept(status);
        }
        unsafe { self.release_current(); }
    }
}

/// Accepts only the normalized integer/string keys promised by the shared name planner.
fn valid_key(key: &MbHostValueV1) -> bool {
    match key.tag {
        HOST_INT => true,
        HOST_STRING => valid_range(key.lo as *const u8, key.hi),
        _ => false,
    }
}
