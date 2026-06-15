//! Purpose:
//! Optional C ABI bridge for elephc's runtime `eval()` support.
//! Exposes the stable entry points linked only into programs that use eval.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_*` symbols.
//! - `cargo test -p elephc-eval` for ABI-shape validation.
//!
//! Key details:
//! - No Rust panic or Rust-specific enum crosses the ABI boundary.
//! - Non-test builds execute the base EvalIR subset through generated runtime
//!   value wrappers; crate unit tests keep a controlled stub because they do not
//!   link the generated runtime assembly object.

pub mod abi;
pub mod context;
pub mod errors;
pub mod eval_ir;
pub mod interpreter;
pub mod lower;
pub mod parser;
pub mod runtime_hooks;
pub mod scope;
pub mod value;

use abi::{
    ElephcEvalContext, ElephcEvalResult, ElephcEvalScope, ABI_VERSION, SCOPE_FLAG_BY_REF,
    SCOPE_FLAG_DIRTY, SCOPE_FLAG_OWNED, SCOPE_FLAG_PRESENT, SCOPE_FLAG_UNSET,
};
use errors::EvalStatus;
#[cfg(not(test))]
use runtime_hooks::ElephcRuntimeOps;
use scope::{ScopeCellOwnership, ScopeEntry};
use std::slice;
use value::{RuntimeCell, RuntimeCellHandle};

#[cfg(not(test))]
unsafe extern "C" {
    fn __elephc_eval_value_release(value: *mut RuntimeCell);
}

/// Returns the ABI version expected by generated elephc eval call sites.
#[no_mangle]
pub extern "C" fn __elephc_eval_abi_version() -> u32 {
    ABI_VERSION
}

/// Allocates a process-level eval context handle for generated code.
#[no_mangle]
pub extern "C" fn __elephc_eval_context_new() -> *mut ElephcEvalContext {
    Box::into_raw(Box::new(ElephcEvalContext::new()))
}

/// Frees a process-level eval context handle allocated by the eval bridge.
///
/// # Safety
/// `ctx` must be null or a pointer returned by `__elephc_eval_context_new`
/// that has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_free(ctx: *mut ElephcEvalContext) {
    if !ctx.is_null() {
        drop(Box::from_raw(ctx));
    }
}

/// Allocates a materialized activation scope handle for generated code.
#[no_mangle]
pub extern "C" fn __elephc_eval_scope_new() -> *mut ElephcEvalScope {
    Box::into_raw(Box::new(ElephcEvalScope::new()))
}

/// Frees a materialized activation scope handle allocated by the eval bridge.
///
/// # Safety
/// `scope` must be null or a pointer returned by `__elephc_eval_scope_new`
/// that has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_free(scope: *mut ElephcEvalScope) {
    if !scope.is_null() {
        let mut scope = Box::from_raw(scope);
        release_owned_scope_cells(&mut scope);
        drop(scope);
    }
}

/// Stores a named runtime cell in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`; names must be UTF-8 variable names.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_set(
    scope: *mut ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
    cell: *mut RuntimeCell,
    flags: u32,
) -> i32 {
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
    if let Some(replaced) = scope.set(name, RuntimeCellHandle::from_raw(cell), ownership) {
        release_scope_cell(replaced);
    }
    EvalStatus::Ok.code()
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
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_unset(
    scope: *mut ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    if let Some(replaced) = scope.unset(name) {
        release_scope_cell(replaced);
    }
    EvalStatus::Ok.code()
}

/// Clears dirty flags for every entry in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle allocated by the eval bridge.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_clear_dirty(scope: *mut ElephcEvalScope) -> i32 {
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    scope.mark_all_clean();
    EvalStatus::Ok.code()
}

/// Executes an eval fragment against a materialized caller scope.
///
/// The FFI shape is final for the initial bridge: context/scope are opaque
/// runtime handles, `code_ptr`/`code_len` identify the PHP fragment bytes, and
/// `out` receives the eval return cell when provided. Non-test builds execute
/// the current EvalIR subset; test builds return `UnsupportedConstruct` because
/// they do not link elephc's generated runtime value wrappers.
///
/// # Safety
/// Callers must pass valid pointers for any non-null handle and ensure
/// `code_ptr` is readable for `code_len` bytes when `code_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_execute(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    code_ptr: *const u8,
    code_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe { execute_eval_inner(ctx, scope, code_ptr, code_len, out) })
        .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Runs the eval ABI body after the exported wrapper has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_execute`; callers must provide valid handles and code
/// storage for every non-null pointer argument.
unsafe fn execute_eval_inner(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    code_ptr: *const u8,
    code_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    if !ctx.is_null() && (*ctx).abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if code_len > 0 && code_ptr.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let Ok(code_len) = usize::try_from(code_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let code = if code_len == 0 {
        &[]
    } else {
        slice::from_raw_parts(code_ptr, code_len)
    };
    let Ok(program) = parser::parse_fragment(code) else {
        return EvalStatus::ParseError.code();
    };
    if !out.is_null() {
        (*out).clear();
    }
    execute_parsed_eval(ctx, scope, &program, out)
}

/// Executes a parsed eval program in production builds using elephc runtime hooks.
///
/// # Safety
/// `scope` and `out` must be null or valid pointers supplied by generated code.
#[cfg(not(test))]
unsafe fn execute_parsed_eval(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    program: &eval_ir::EvalProgram,
    out: *mut ElephcEvalResult,
) -> i32 {
    let mut fallback_context;
    let context = if let Some(ctx) = ctx.as_mut() {
        ctx
    } else {
        fallback_context = ElephcEvalContext::new();
        &mut fallback_context
    };
    let mut fallback_scope;
    let scope = if let Some(scope) = scope.as_mut() {
        scope
    } else {
        fallback_scope = ElephcEvalScope::new();
        &mut fallback_scope
    };
    let mut values = ElephcRuntimeOps::new();
    match interpreter::execute_program_with_context(context, program, scope, &mut values) {
        Ok(result) => {
            if !out.is_null() {
                (*out).kind = 0;
                (*out).value_cell = result.as_ptr();
                (*out).error = std::ptr::null_mut();
            }
            EvalStatus::Ok.code()
        }
        Err(status) => status.code(),
    }
}

/// Keeps crate unit tests independent from generated runtime assembly wrappers.
///
/// # Safety
/// `out` must be null or valid result storage supplied by the test caller.
#[cfg(test)]
unsafe fn execute_parsed_eval(
    _ctx: *mut ElephcEvalContext,
    _scope: *mut ElephcEvalScope,
    _program: &eval_ir::EvalProgram,
    _out: *mut ElephcEvalResult,
) -> i32 {
    EvalStatus::UnsupportedConstruct.code()
}

/// Converts an ABI name byte slice into an owned Rust string.
fn abi_name_to_string(name_ptr: *const u8, name_len: u64) -> Result<String, EvalStatus> {
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
fn scope_entry_abi_flags(entry: ScopeEntry) -> u32 {
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
fn release_owned_scope_cells(scope: &mut ElephcEvalScope) {
    for cell in scope.drain_owned_cells() {
        release_scope_cell(cell);
    }
}

/// Releases one scope-owned runtime cell through the generated runtime wrapper.
fn release_scope_cell(cell: RuntimeCellHandle) {
    #[cfg(not(test))]
    unsafe {
        __elephc_eval_value_release(cell.as_ptr());
    }
    #[cfg(test)]
    {
        let _ = cell;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the exported version entry point reports the crate ABI constant.
    #[test]
    fn abi_version_matches_constant() {
        assert_eq!(__elephc_eval_abi_version(), ABI_VERSION);
    }

    /// Verifies the initial execute stub clears result storage and returns the
    /// documented unsupported status instead of panicking or succeeding.
    #[test]
    fn execute_stub_returns_unsupported_and_clears_result() {
        let mut result = ElephcEvalResult {
            kind: 99,
            value_cell: 1usize as *mut std::ffi::c_void,
            error: 2usize as *mut std::ffi::c_void,
        };
        let status = unsafe {
            __elephc_eval_execute(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                b"$x = 1;".as_ptr(),
                7,
                &mut result,
            )
        };
        assert_eq!(status, EvalStatus::UnsupportedConstruct.code());
        assert_eq!(result.kind, 0);
        assert!(result.value_cell.is_null());
        assert!(result.error.is_null());
    }

    /// Verifies context allocation returns a current-version opaque handle.
    #[test]
    fn context_new_returns_current_version_handle() {
        let ctx = __elephc_eval_context_new();
        assert!(!ctx.is_null());
        let version = unsafe { (*ctx).abi_version() };
        unsafe {
            __elephc_eval_context_free(ctx);
        }
        assert_eq!(version, ABI_VERSION);
    }

    /// Verifies scope allocation returns an empty opaque activation scope handle.
    #[test]
    fn scope_new_returns_empty_handle() {
        let scope = __elephc_eval_scope_new();
        assert!(!scope.is_null());
        let generation = unsafe { (*scope).generation() };
        unsafe {
            __elephc_eval_scope_free(scope);
        }
        assert_eq!(generation, 0);
    }

    /// Verifies execute rejects contexts whose ABI version no longer matches.
    #[test]
    fn execute_rejects_mismatched_context_version() {
        let mut ctx = ElephcEvalContext::for_abi_version(ABI_VERSION + 1);
        let status = unsafe {
            __elephc_eval_execute(
                &mut ctx,
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
            )
        };

        assert_eq!(status, EvalStatus::AbiMismatch.code());
    }

    /// Verifies execute maps invalid eval fragments to the stable parse status.
    #[test]
    fn execute_rejects_php_opening_tags_as_parse_errors() {
        let code = b"<?php echo 1;";
        let status = unsafe {
            __elephc_eval_execute(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                code.as_ptr(),
                code.len() as u64,
                std::ptr::null_mut(),
            )
        };

        assert_eq!(status, EvalStatus::ParseError.code());
    }

    /// Verifies scope set/get expose runtime-cell handles and dirty flags through the ABI.
    #[test]
    fn scope_set_get_round_trips_cell_and_flags() {
        let scope = __elephc_eval_scope_new();
        let name = b"x";
        let cell = 1usize as *mut RuntimeCell;
        let set_status = unsafe {
            __elephc_eval_scope_set(
                scope,
                name.as_ptr(),
                name.len() as u64,
                cell,
                SCOPE_FLAG_OWNED,
            )
        };
        let mut out_cell = std::ptr::null_mut();
        let mut out_flags = 0;
        let get_status = unsafe {
            __elephc_eval_scope_get(
                scope,
                name.as_ptr(),
                name.len() as u64,
                &mut out_cell,
                &mut out_flags,
            )
        };
        unsafe {
            __elephc_eval_scope_free(scope);
        }

        assert_eq!(set_status, EvalStatus::Ok.code());
        assert_eq!(get_status, EvalStatus::Ok.code());
        assert_eq!(out_cell, cell);
        assert_eq!(out_flags & SCOPE_FLAG_PRESENT, SCOPE_FLAG_PRESENT);
        assert_eq!(out_flags & SCOPE_FLAG_DIRTY, SCOPE_FLAG_DIRTY);
        assert_eq!(out_flags & SCOPE_FLAG_OWNED, SCOPE_FLAG_OWNED);
    }

    /// Verifies scope unset and clear-dirty expose missing/clean state through the ABI.
    #[test]
    fn scope_unset_and_clear_dirty_update_flags() {
        let scope = __elephc_eval_scope_new();
        let name = b"x";
        let cell = 1usize as *mut RuntimeCell;
        unsafe {
            __elephc_eval_scope_set(
                scope,
                name.as_ptr(),
                name.len() as u64,
                cell,
                SCOPE_FLAG_OWNED,
            );
            __elephc_eval_scope_clear_dirty(scope);
            __elephc_eval_scope_unset(scope, name.as_ptr(), name.len() as u64);
        }
        let mut out_cell = cell;
        let mut out_flags = 0;
        let get_status = unsafe {
            __elephc_eval_scope_get(
                scope,
                name.as_ptr(),
                name.len() as u64,
                &mut out_cell,
                &mut out_flags,
            )
        };
        unsafe {
            __elephc_eval_scope_free(scope);
        }

        assert_eq!(get_status, EvalStatus::Ok.code());
        assert!(out_cell.is_null());
        assert_eq!(out_flags & SCOPE_FLAG_UNSET, SCOPE_FLAG_UNSET);
        assert_eq!(out_flags & SCOPE_FLAG_DIRTY, SCOPE_FLAG_DIRTY);
        assert_eq!(out_flags & SCOPE_FLAG_PRESENT, 0);
    }
}
