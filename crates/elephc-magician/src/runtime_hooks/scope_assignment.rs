//! Purpose:
//! Publishes AOT eval assignments through the existing protected native reference coordinator.
//!
//! Called from:
//! - The versioned scope-set FFI when its input flags select checked assignment.
//!
//! Key details:
//! - Snapshot installation stays separate from PHP assignment and retains its existing ownership.
//! - Native callbacks return before Rust consumes displaced owners or returns an escaping Throwable.

use super::{EvalStatus, RuntimeCellHandle};
use crate::{abi::ElephcEvalScope, scope::ScopeCellOwnership};

/// Consumes an assignment's input lease only when an existing persistent reference handles it.
pub(crate) fn assign_existing_reference(scope: &mut ElephcEvalScope, name: &str,
    value: RuntimeCellHandle, ownership: ScopeCellOwnership,
) -> Option<(EvalStatus, Option<RuntimeCellHandle>)> {
    let reference = scope.visible_cell(name)?;
    if unsafe { super::externs::__elephc_eval_value_is_reference(reference.as_ptr()) } == 0 {
        return None;
    }
    let mut previous = std::ptr::null_mut();
    let mut thrown = std::ptr::null_mut();
    let status = unsafe { super::externs::__elephc_eval_value_reference_replace_v2(
        reference.as_ptr(), value.as_ptr(), 0, &mut previous, &mut thrown) };
    if status == 0 || !previous.is_null() { scope.mark_reference_changed(name); }
    let previous = (!previous.is_null()).then(|| RuntimeCellHandle::from_raw(previous));
    let thrown = (!thrown.is_null()).then(|| RuntimeCellHandle::from_raw(thrown));
    let input = (ownership == ScopeCellOwnership::Owned).then_some(value);
    let thrown = super::release::release_native_cells(previous.into_iter().chain(input), thrown);
    Some((if thrown.is_some() { EvalStatus::UncaughtThrowable }
        else if status == 0 { EvalStatus::Ok } else { EvalStatus::RuntimeFatal }, thrown))
}

/// Detaches activation sources while the scope still owns their independently retained wrappers.
pub(crate) fn release_origins(scope: &mut ElephcEvalScope, name: Option<&str>) -> Option<RuntimeCellHandle> {
    let thrown = unsafe { match name {
        Some(name) => super::externs::__elephc_eval_scope_origin_unset_v1((scope as *mut ElephcEvalScope).cast(), name.as_ptr(), name.len()),
        None => super::externs::__elephc_eval_scope_origins_release_v1((scope as *mut ElephcEvalScope).cast()),
    } };
    (!thrown.is_null()).then(|| RuntimeCellHandle::from_raw(thrown))
}

/// Visits native source owners through the same immutable collector protocol as scope entries.
///
/// # Safety
/// The scope and callback context remain live, and the callback cannot execute PHP or mutate owners.
pub(crate) unsafe fn visit_origins(scope: *const std::ffi::c_void,
    visitor: Option<unsafe extern "C" fn(*const *mut crate::value::RuntimeCell, u64, *mut std::ffi::c_void)>,
    context: *mut std::ffi::c_void,
) {
    unsafe { super::externs::__elephc_eval_scope_visit_origins_v1(scope, visitor, context); }
}
