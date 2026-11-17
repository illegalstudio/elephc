//! Purpose:
//! Exports the hook a COMPILED `define()` calls so interpreted code can read its constant.
//!
//! Called from:
//! - Generated EIR backend assembly (`lower_define`) through `__elephc_eval_publish_constant`, in
//!   programs that link the eval bridge.
//!
//! Key details:
//! - The compiled program keeps a `define()`d value in a program global; without this hook an
//!   eval()ed fragment (a Blade view, an included file) saw `defined('NAME')` as false and a bare
//!   `NAME` as an undefined constant, although php has one constant namespace.
//! - The cell is a boxed Mixed the caller allocated in the request arena and hands over; the
//!   registry forgets it at the `--web` request boundary together with the arena.

use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::slice;

/// Publishes the compiled constant `name` with the boxed value `cell` to interpreted code.
///
/// # Safety
/// `name_ptr` must be readable for `name_len` bytes and `cell` must be a live boxed Mixed whose
/// ownership moves to the registry.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_publish_constant(
    name_ptr: *const u8,
    name_len: u64,
    cell: *mut RuntimeCell,
) {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_publish_constant");
    if name_ptr.is_null() || cell.is_null() {
        return;
    }
    let bytes = slice::from_raw_parts(name_ptr, name_len as usize);
    let Ok(name) = std::str::from_utf8(bytes) else {
        return;
    };
    let _ = std::panic::catch_unwind(|| {
        crate::context::publish_compiled_constant(name, RuntimeCellHandle::from_raw(cell));
    });
}

/// Answers 1 when interpreted code already defined the constant `name`, otherwise 0.
///
/// The compiled `define()` calls this before storing its value: a name an `eval()` defined first
/// must keep that value and make the compiled call report "already defined", as in php.
///
/// # Safety
/// `name_ptr` must be readable for `name_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_constant_is_defined(name_ptr: *const u8, name_len: u64) -> u64 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_constant_is_defined");
    if name_ptr.is_null() {
        return 0;
    }
    let bytes = slice::from_raw_parts(name_ptr, name_len as usize);
    let Ok(name) = std::str::from_utf8(bytes) else {
        return 0;
    };
    std::panic::catch_unwind(|| u64::from(crate::context::global_eval_constant_is_defined(name)))
        .unwrap_or(0)
}
