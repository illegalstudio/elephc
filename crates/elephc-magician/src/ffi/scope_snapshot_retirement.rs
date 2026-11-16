//! Purpose:
//! Returns native snapshot retirement failures without unwinding through Rust frames.
//!
//! Called from:
//! - The native scope retirement adapter after local writeback has completed.
//!
//! Key details:
//! - The scope remains allocated and retains interpreter-only bindings.
//! - Every displaced owner is consumed before an owned Throwable crosses the C boundary.

use crate::{abi::ElephcEvalScope, value::RuntimeCell};

/// Retires copied native locals and returns an escaping boxed destructor exception.
///
/// # Safety
/// The scope must be null or a live handle with no simultaneous interpreter execution.
#[export_name = "__elephc_eval_scope_retire_native_snapshots_v2"]
pub unsafe extern "C" fn retire_native_snapshots(scope: *mut ElephcEvalScope) -> *mut RuntimeCell {
    let Some(scope) = scope.as_mut() else { return std::ptr::null_mut(); };
    let owners = scope.retire_native_snapshots();
    #[cfg(not(test))]
    { crate::runtime_hooks::release::release_native_cells(owners, None)
        .map_or(std::ptr::null_mut(), crate::value::RuntimeCellHandle::as_ptr) }
    #[cfg(test)]
    { let _ = owners; std::ptr::null_mut() }
}
