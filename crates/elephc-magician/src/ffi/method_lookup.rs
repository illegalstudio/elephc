//! Purpose:
//! Exports pre-argument abstract-method validation for eval-owned receivers.
//!
//! Called from:
//! - Generated EIR MethodLookup assembly on all supported targets.
//!
//! Key details:
//! - Borrows the object's registered context and never invokes a user method.
//! - Ordinary native receivers return success for the AOT metadata guard.

use super::util::{abi_name_to_string, clear_result, write_outcome};
use crate::abi::{ElephcEvalResult, ABI_VERSION};
use crate::errors::EvalStatus;
use crate::interpreter::{self, RuntimeValueOps};
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};

/// Checks an eval-owned target and publishes a catchable PHP Error on failure.
///
/// # Safety
/// The object is a live borrowed boxed cell, method/scope byte ranges are readable UTF-8,
/// and out is writable.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_method_lookup_v1(
    object: *mut RuntimeCell, method_ptr: *const u8, method_len: u64,
    out: *mut ElephcEvalResult, scope_ptr: *const u8, scope_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        clear_result(out);
        if object.is_null() { return EvalStatus::Ok.code(); }
        let object = RuntimeCellHandle::from_raw(object).borrowed();
        let mut probe = ElephcRuntimeOps::new();
        let owner = probe.object_identity(object).ok().and_then(|identity|
            super::dynamic_destructors::dynamic_object_owner_context(identity));
        let Some(context) = owner.and_then(|context| context.as_mut()) else { return EvalStatus::Ok.code(); };
        if context.abi_version() != ABI_VERSION { return EvalStatus::AbiMismatch.code(); }
        let Ok(method) = abi_name_to_string(method_ptr, method_len) else { return EvalStatus::RuntimeFatal.code(); };
        let Ok(scope) = abi_name_to_string(scope_ptr, scope_len) else { return EvalStatus::RuntimeFatal.code(); };
        let mut values = ElephcRuntimeOps::with_context(context as *const _);
        context.push_class_scope(scope);
        let result = interpreter::execute_context_abstract_method_guard_outcome(context, object, &method, &mut values);
        context.pop_class_scope();
        match result {
            Ok(Some(outcome)) => write_outcome(outcome, out).code(),
            Ok(None) => EvalStatus::Ok.code(),
            Err(status) => status.code(),
        }
    }).unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}
