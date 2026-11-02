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
use crate::value::RuntimeCellHandle;
use std::slice;

/// Converts an ABI name byte slice into an owned Rust string.
pub(crate) fn abi_name_to_string(name_ptr: *const u8, name_len: u64) -> Result<String, EvalStatus> {
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
    abi_flags |= match flags.native_binding {
        crate::scope::NativeScopeBinding::Value => 0,
        crate::scope::NativeScopeBinding::ReferenceCell => crate::abi::SCOPE_FLAG_NATIVE_REF,
        crate::scope::NativeScopeBinding::GlobalName => crate::abi::SCOPE_FLAG_NATIVE_GLOBAL,
    };
    abi_flags
}

/// Releases every scope-owned cell and returns an accumulated owned destructor exception.
pub(crate) fn release_owned_scope_cells(scope: &mut ElephcEvalScope) -> Option<RuntimeCellHandle> {
    let cells = scope.drain_owned_cells();
    #[cfg(not(test))]
    {
        crate::runtime_hooks::release::release_native_cells(cells, None)
    }
    #[cfg(test)]
    {
        let _ = cells;
        None
    }
}

/// Releases one scope-owned cell without propagating a native exception through Rust.
pub(crate) fn release_scope_cell(cell: RuntimeCellHandle) -> Option<RuntimeCellHandle> {
    #[cfg(not(test))]
    {
        crate::runtime_hooks::release::release_native_cells([cell], None)
    }
    #[cfg(test)]
    {
        let _ = cell;
        None
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
pub(crate) unsafe fn write_outcome(outcome: EvalOutcome, out: *mut ElephcEvalResult) -> EvalStatus {
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

/// Copies a PHP data payload from the ABI without requiring symbol-name UTF-8 validity.
///
/// # Safety
/// A non-null pointer must be readable for `data_len` bytes; null is accepted only for length zero.
pub(crate) unsafe fn abi_data_bytes(data_ptr: *const u8, data_len: u64) -> Result<Vec<u8>, EvalStatus> {
    if data_len > 0 && data_ptr.is_null() { return Err(EvalStatus::RuntimeFatal); }
    let len = usize::try_from(data_len).map_err(|_| EvalStatus::RuntimeFatal)?;
    if len == 0 { return Ok(Vec::new()); }
    Ok(unsafe { slice::from_raw_parts(data_ptr, len) }.to_vec())
}

/// Announces one bridge entry point, so a fatal is always preceded by the call that raised it.
///
/// A refusal can happen in a region that emits nothing: a Symfony request ended in 527 consecutive
/// `object_is_a` lines and then a fatal, with the failing call somewhere between them and no line
/// to name it. Every `__elephc_eval_*` symbol announces itself here, and the last line before a
/// fatal is then the call that failed rather than the last call that happened to be instrumented.
///
/// It is a level of its own because it is loud -- a request makes hundreds of thousands of bridge
/// calls -- so `ELEPHC_EVAL_TRACE=1` keeps the readable phase trace, and `ffi` or `all` adds this.
pub(crate) fn trace_eval_ffi_entry(symbol: &str) {
    if !crate::eval_trace::ffi_enabled() {
        return;
    }
    eprintln!("[elephc-eval-trace] phase=ffi_entry symbol={symbol}");
}

/// Reads an object's runtime class name for a diagnostic, giving nothing up if it cannot.
#[cfg(not(test))]
pub(crate) fn eval_object_class_name_for_diagnostic(
    object: RuntimeCellHandle,
    values: &mut impl crate::interpreter::RuntimeValueOps,
) -> Option<String> {
    let handle = values.object_class_name(object).ok()?;
    let bytes = values.string_bytes(handle).ok();
    let _ = values.release(handle);
    bytes.and_then(|bytes| String::from_utf8(bytes).ok())
}
