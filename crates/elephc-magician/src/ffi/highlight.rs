//! Purpose:
//! Exposes the interpreter's PHP-exact highlighter to generated code.
//!
//! Called from:
//! - Generated `highlight_string()`/`highlight_file()` lowering through
//!   `__elephc_eval_highlight_string` and `__elephc_eval_highlight_file`.
//!
//! Key details:
//! - The subject cell is BORROWED: the caller keeps the box it staged and releases it.
//! - The answer is one owned boxed Mixed cell (`string` or `bool`), matching PHP's `string|bool`
//!   return; a null answer means the renderer itself failed.

use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};

/// Renders one subject and answers an owned boxed `string|bool` cell.
fn render(name: &str, subject: *mut RuntimeCell, return_html: i64) -> *mut RuntimeCell {
    if subject.is_null() {
        return std::ptr::null_mut();
    }
    let mut values = ElephcRuntimeOps::new();
    match crate::interpreter::eval_highlight_render(
        name,
        RuntimeCellHandle::from_raw(subject),
        return_html != 0,
        &mut values,
    ) {
        Ok(cell) => cell.as_ptr(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Renders `highlight_string($subject, $return)` for generated code.
///
/// # Safety
/// `subject` must be a live boxed cell borrowed for the duration of the call; the returned cell
/// is owned by the caller.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_highlight_string(
    subject: *mut RuntimeCell,
    return_html: i64,
) -> *mut RuntimeCell {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_highlight_string");
    std::panic::catch_unwind(|| render("highlight_string", subject, return_html))
        .unwrap_or(std::ptr::null_mut())
}

/// Renders `highlight_file($subject, $return)` for generated code.
///
/// # Safety
/// `subject` must be a live boxed cell borrowed for the duration of the call; the returned cell
/// is owned by the caller.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_highlight_file(
    subject: *mut RuntimeCell,
    return_html: i64,
) -> *mut RuntimeCell {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_highlight_file");
    std::panic::catch_unwind(|| render("highlight_file", subject, return_html))
        .unwrap_or(std::ptr::null_mut())
}
