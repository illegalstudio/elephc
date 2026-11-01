//! Purpose:
//! Exports materialized eval activation-scope handle operations.
//! Scope entries carry runtime cell pointers plus dirty, ownership, unset, and
//! by-reference metadata used by generated code around eval barriers.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_scope_*` symbols.
//!
//! Key details:
//! - Owned runtime cells are released when a scope is freed or overwritten.
//! - Global aliases store source-target names rather than copying values.

use super::util::{
    abi_name_to_string, release_owned_scope_cells, release_scope_cell, scope_entry_abi_flags,
};
use crate::abi::{ElephcEvalScope, SCOPE_FLAG_OWNED};
use crate::errors::EvalStatus;
use crate::scope::ScopeCellOwnership;
use crate::value::{RuntimeCell, RuntimeCellHandle};

/// Allocates a materialized activation scope handle for generated code.
#[no_mangle]
pub extern "C" fn __elephc_eval_scope_new() -> *mut ElephcEvalScope {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_scope_new");
    Box::into_raw(Box::new(ElephcEvalScope::new()))
}

/// Frees an activation scope completely and transfers any accumulated Throwable to native code.
///
/// # Safety
/// `scope` must be null or a pointer returned by `__elephc_eval_scope_new`
/// that has not already been freed.
#[export_name = "__elephc_eval_scope_free_v2"]
pub unsafe extern "C" fn __elephc_eval_scope_free(scope: *mut ElephcEvalScope) -> *mut RuntimeCell {
    if !scope.is_null() {
        let mut scope = Box::from_raw(scope);
        let thrown = release_owned_scope_cells(&mut scope);
        drop(scope);
        return thrown.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr);
    }
    std::ptr::null_mut()
}

/// Stores a named runtime cell in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`; names must be UTF-8 variable names.
/// `throwable` must point to writable pointer storage for an owned escaping exception.
#[export_name = "__elephc_eval_scope_set_v2"]
pub unsafe extern "C" fn __elephc_eval_scope_set(
    scope: *mut ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
    cell: *mut RuntimeCell,
    flags: u32,
    throwable: *mut *mut RuntimeCell,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_scope_set");
    if throwable.is_null() { return EvalStatus::RuntimeFatal.code(); }
    *throwable = std::ptr::null_mut();
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let ownership = if flags & SCOPE_FLAG_OWNED != 0 {
        ScopeCellOwnership::Owned
    } else {
        ScopeCellOwnership::Borrowed
    };
    let binding = match flags & (crate::abi::SCOPE_FLAG_NATIVE_REF | crate::abi::SCOPE_FLAG_NATIVE_GLOBAL) {
        0 => crate::scope::NativeScopeBinding::Value,
        crate::abi::SCOPE_FLAG_NATIVE_REF => crate::scope::NativeScopeBinding::ReferenceCell,
        crate::abi::SCOPE_FLAG_NATIVE_GLOBAL => crate::scope::NativeScopeBinding::GlobalName,
        _ => return EvalStatus::RuntimeFatal.code(),
    };
    if let Some(replaced) = scope.set_from_aot_with_native_binding(name, RuntimeCellHandle::from_raw(cell), ownership, binding) {
        if let Some(thrown) = release_scope_cell(replaced) {
            *throwable = thrown.as_ptr();
            return EvalStatus::UncaughtThrowable.code();
        }
    }
    EvalStatus::Ok.code()
}

/// Installs compiled native-binding metadata through an explicit capability
/// symbol, so an older bridge cannot silently discard the new flags.
///
/// # Safety
/// The pointer and lifetime requirements are the same as `__elephc_eval_scope_set`. Like it, this
/// is the versioned entry: the runtime's five-argument `__elephc_eval_scope_bind_native` adapter
/// supplies `throwable` and rethrows what lands there. Generated code called the six-argument
/// entry directly with five arguments, so the leftover sixth register was read (a null one made
/// every `global $g; eval('$g = 1;')` fail) and written through.
#[export_name = "__elephc_eval_scope_bind_native_v2"]
pub unsafe extern "C" fn __elephc_eval_scope_bind_native(
    scope: *mut ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
    cell: *mut RuntimeCell,
    flags: u32,
    throwable: *mut *mut RuntimeCell,
) -> i32 {
    unsafe { __elephc_eval_scope_set(scope, name_ptr, name_len, cell, flags, throwable) }
}

/// Looks up a named runtime cell in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. Output pointers may be null.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_get(
    scope: *const ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
    out_cell: *mut *mut RuntimeCell,
    out_flags: *mut u32,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_scope_get");
    let Some(scope) = scope.as_ref() else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let entry = scope.entry(&name);
    if !out_cell.is_null() {
        *out_cell = entry
            .filter(|entry| entry.flags().is_visible())
            .map(|entry| entry.cell().as_ptr())
            .unwrap_or(std::ptr::null_mut());
    }
    if !out_flags.is_null() {
        *out_flags = entry.map(scope_entry_abi_flags).unwrap_or(0);
    }
    EvalStatus::Ok.code()
}

/// Marks a named runtime cell as unset in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`; names must be UTF-8 variable names.
/// `throwable` must point to writable pointer storage for an owned escaping exception.
#[export_name = "__elephc_eval_scope_unset_v2"]
pub unsafe extern "C" fn __elephc_eval_scope_unset(
    scope: *mut ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
    throwable: *mut *mut RuntimeCell,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_scope_unset");
    if throwable.is_null() { return EvalStatus::RuntimeFatal.code(); }
    *throwable = std::ptr::null_mut();
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    if let Some(replaced) = scope.unset(name) {
        if let Some(thrown) = release_scope_cell(replaced) {
            *throwable = thrown.as_ptr();
            return EvalStatus::UncaughtThrowable.code();
        }
    }
    EvalStatus::Ok.code()
}

/// Marks a local eval-scope variable as an alias of a program-global variable.
///
/// # Safety
/// `scope` must be a valid eval scope handle. Name pointers must be readable
/// for their matching lengths when the length is greater than zero.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_mark_global_alias(
    scope: *mut ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
    global_name_ptr: *const u8,
    global_name_len: u64,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_scope_mark_global_alias");
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(global_name) = abi_name_to_string(global_name_ptr, global_name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    // An empty global name drops the alias: a compiled local published as a global alias by an
    // earlier eval in the same frame may since have been rebound to other storage.
    if global_name.is_empty() {
        scope.clear_global_alias(&name);
    } else {
        scope.mark_global_alias_to(name, global_name);
    }
    EvalStatus::Ok.code()
}

/// Clears dirty flags for every entry in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle allocated by the eval bridge.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_clear_dirty(scope: *mut ElephcEvalScope) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_scope_clear_dirty");
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    scope.mark_all_clean();
    EvalStatus::Ok.code()
}
