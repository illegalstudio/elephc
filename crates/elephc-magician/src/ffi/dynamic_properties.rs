//! Purpose:
//! Identifies eval-declared object storage distinct from private native parent fields.
//!
//! Called from:
//! - Generated native property bridges after a private parent slot rejects the current scope.
//!
//! Key details:
//! - This selects storage only; Magician checks PHP visibility before calling the bridge.
//! - An eval runtime child never resolves a native parent's private name to that physical slot.

use super::dynamic_destructors::dynamic_object_owner_context;
use crate::abi::ABI_VERSION;

/// Reports whether a native private slot belongs to an eval runtime object's strict ancestor.
///
/// This callback is reached only after generated dispatch has matched a non-hidden private slot
/// and rejected the active scope. A registered eval object necessarily uses that native layout as
/// its parent storage, so PHP treats the plain property name as absent on the runtime child. The
/// name may then resolve to an eval-declared field or to the child's public dynamic-property hash.
///
/// # Safety
/// The identity must refer to a live object during the synchronous bridge call. The property
/// pointer must be readable for `property_len` bytes; no pointer is retained by this query.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_object_has_separate_property(
    identity: u64,
    _property_ptr: *const u8,
    _property_len: u64,
) -> u64 {
    std::panic::catch_unwind(|| {
        let Some(context) = dynamic_object_owner_context(identity) else { return 0; };
        let Some(context) = (unsafe { context.as_ref() }) else { return 0; };
        if context.abi_version() != ABI_VERSION { return 0; }
        u64::from(context.dynamic_object_class(identity).is_some())
    }).unwrap_or(0)
}
