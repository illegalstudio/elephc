//! Purpose:
//! Tests eval scope allocation, execute validation, scope cell flags, aliases,
//! unset, and dirty-state handling.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Fake runtime-cell pointers are stored and compared but never dereferenced.

use super::*;

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

/// Verifies execute maps invalid ABI code storage to runtime fatal instead of panicking.
#[test]
fn execute_rejects_null_code_pointer_with_nonzero_length() {
    let status = unsafe {
        __elephc_eval_execute(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
            1,
            std::ptr::null_mut(),
        )
    };

    assert_eq!(status, EvalStatus::RuntimeFatal.code());
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

/// Verifies the alias ABI maps a local eval variable to a global name.
#[test]
fn scope_mark_global_alias_records_target_name() {
    let scope = __elephc_eval_scope_new();
    let name = b"alias";
    let global_name = b"source";

    let status = unsafe {
        __elephc_eval_scope_mark_global_alias(
            scope,
            name.as_ptr(),
            name.len() as u64,
            global_name.as_ptr(),
            global_name.len() as u64,
        )
    };
    let target = unsafe { (*scope).global_alias_target("alias").map(str::to_string) };
    unsafe {
        __elephc_eval_scope_free(scope);
    }

    assert_eq!(status, EvalStatus::Ok.code());
    assert_eq!(target.as_deref(), Some("source"));
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
