//! Purpose:
//! Shared helpers for the eval C ABI layer.
//! Converts ABI byte slices, scope flags, and owned runtime cells between C
//! handles and the Rust bridge data structures.
//!
//! Called from:
//! - `crate::ffi::*` exported ABI modules.
//!
//! Key details:
//! - Pointer validation happens before any slice construction.
//! - Runtime cell release is delegated to generated wrappers outside tests.

use crate::abi::{
    ElephcEvalResult, ElephcEvalScope, SCOPE_FLAG_BY_REF, SCOPE_FLAG_DIRTY, SCOPE_FLAG_OWNED,
    SCOPE_FLAG_PRESENT, SCOPE_FLAG_UNSET,
};
use crate::errors::EvalStatus;
#[cfg(not(test))]
use crate::interpreter::EvalOutcome;
use crate::scope::{ScopeCellOwnership, ScopeEntry};
#[cfg(not(test))]
use crate::value::RuntimeCell;
use crate::value::RuntimeCellHandle;
use std::slice;

#[cfg(not(test))]
unsafe extern "C" {
    fn __elephc_eval_value_release(value: *mut RuntimeCell);
}

/// Converts an ABI name byte slice into an owned Rust string.
pub(crate) fn abi_name_to_string(
    name_ptr: *const u8,
    name_len: u64,
) -> Result<String, EvalStatus> {
    if name_len > 0 && name_ptr.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let name_len = usize::try_from(name_len).map_err(|_| EvalStatus::RuntimeFatal)?;
    let bytes = if name_len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(name_ptr, name_len) }
    };
    std::str::from_utf8(bytes)
        .map(|name| name.to_string())
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Converts internal scope-entry flags into stable ABI bit flags.
pub(crate) fn scope_entry_abi_flags(entry: ScopeEntry) -> u32 {
    let flags = entry.flags();
    let mut abi_flags = 0;
    if flags.present {
        abi_flags |= SCOPE_FLAG_PRESENT;
    }
    if flags.unset {
        abi_flags |= SCOPE_FLAG_UNSET;
    }
    if flags.dirty {
        abi_flags |= SCOPE_FLAG_DIRTY;
    }
    if flags.by_ref {
        abi_flags |= SCOPE_FLAG_BY_REF;
    }
    if flags.ownership == ScopeCellOwnership::Owned {
        abi_flags |= SCOPE_FLAG_OWNED;
    }
    abi_flags
}

/// Releases every owned cell currently held by a scope.
pub(crate) fn release_owned_scope_cells(scope: &mut ElephcEvalScope) {
    for cell in scope.drain_owned_cells() {
        release_scope_cell(cell);
    }
}

/// Releases one scope-owned runtime cell through the generated runtime wrapper.
pub(crate) fn release_scope_cell(cell: RuntimeCellHandle) {
    #[cfg(not(test))]
    unsafe {
        __elephc_eval_value_release(cell.as_ptr());
    }
    #[cfg(test)]
    {
        let _ = cell;
    }
}

/// Clears optional result storage before an ABI call writes a result.
pub(crate) unsafe fn clear_result(out: *mut ElephcEvalResult) {
    if !out.is_null() {
        (*out).clear();
    }
}

/// Writes an eval execution outcome into optional ABI result storage.
#[cfg(not(test))]
pub(crate) unsafe fn write_outcome(
    outcome: EvalOutcome,
    out: *mut ElephcEvalResult,
) -> EvalStatus {
    match outcome {
        EvalOutcome::Value(result) => {
            if !out.is_null() {
                (*out).kind = 0;
                (*out).value_cell = result.as_ptr();
                (*out).error = std::ptr::null_mut();
            }
            EvalStatus::Ok
        }
        EvalOutcome::Throwable(error) => {
            if !out.is_null() {
                (*out).kind = 3;
                (*out).value_cell = std::ptr::null_mut();
                (*out).error = error.as_ptr();
            }
            EvalStatus::UncaughtThrowable
        }
    }
}
