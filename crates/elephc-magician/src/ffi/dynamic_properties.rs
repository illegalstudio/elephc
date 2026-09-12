//! Purpose:
//! Identifies eval-declared property slots distinct from private native parent fields.
//!
//! Called from:
//! - Generated native property bridges after a private parent slot rejects the current scope.
//!
//! Key details:
//! - This selects storage only; Magician checks PHP visibility before calling the bridge.
//! - A matching eval field never grants access to or overwrites the parent's private field.

use super::dynamic_destructors::dynamic_object_owner_context;
use super::util::abi_name_to_string;
use crate::abi::ABI_VERSION;
use crate::eval_ir::EvalVisibility;

/// Reports a backed, non-private eval field with its own slot behind a private native field.
///
/// # Safety
/// The identity must refer to a live object during the synchronous bridge call. The property
/// pointer must be readable for `property_len` bytes; no pointer is retained by this query.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_object_has_separate_property(
    identity: u64,
    property_ptr: *const u8,
    property_len: u64,
) -> u64 {
    std::panic::catch_unwind(|| {
        let Some(context) = dynamic_object_owner_context(identity) else { return 0; };
        let Some(context) = (unsafe { context.as_ref() }) else { return 0; };
        if context.abi_version() != ABI_VERSION { return 0; }
        let Ok(name) = abi_name_to_string(property_ptr, property_len) else { return 0; };
        let Some(class) = context.dynamic_object_class(identity) else { return 0; };
        let Some((_, property)) = context.class_property(class.name(), &name) else { return 0; };
        u64::from(!property.is_static() && !property.is_virtual() && !property.is_abstract()
            && property.visibility() != EvalVisibility::Private)
    }).unwrap_or(0)
}
