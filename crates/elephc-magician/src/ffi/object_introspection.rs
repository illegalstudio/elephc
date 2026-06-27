//! Purpose:
//! Exports post-barrier object introspection for values that may have been
//! created from eval-declared classes. Generated code uses this ABI when native
//! type metadata must consult the persistent eval context.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_object_class_name`.
//! - Generated EIR backend assembly through `__elephc_eval_object_is_a`.
//!
//! Key details:
//! - Class-name lookups return boxed Mixed string cells through `ElephcEvalResult`.
//! - Predicate probes fail closed as false when ABI inputs are invalid.

use super::util::{abi_name_to_string, clear_result, write_outcome};
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ABI_VERSION};
use crate::errors::EvalStatus;
use crate::interpreter::{self, EvalOutcome};
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};

const CLASS_LOOKUP_GET_CLASS: u64 = 0;
const CLASS_LOOKUP_GET_PARENT_CLASS: u64 = 1;

/// Resolves `get_class()` or `get_parent_class()` against eval dynamic objects.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `object_or_class` must point at a
/// boxed runtime cell, `lookup_kind` must be a supported lookup discriminator,
/// and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_object_class_name(
    ctx: *mut ElephcEvalContext,
    object_or_class: *mut RuntimeCell,
    lookup_kind: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_object_class_name_inner(ctx, object_or_class, lookup_kind, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Tests whether an object satisfies a class/interface relation in the eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `object` must point at a boxed
/// runtime cell. `target_ptr` must be readable for `target_len` bytes when
/// `target_len > 0`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_object_is_a(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    target_ptr: *const u8,
    target_len: u64,
    exclude_self: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_object_is_a_inner(ctx, object, target_ptr, target_len, exclude_self)
    })
    .unwrap_or(0)
}

/// Runs the eval object class-name ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_object_class_name`; callers must provide a valid
/// context, a boxed runtime cell, and optional writable result storage.
#[cfg(not(test))]
unsafe fn eval_object_class_name_inner(
    ctx: *mut ElephcEvalContext,
    object_or_class: *mut RuntimeCell,
    lookup_kind: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if object_or_class.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let lookup = match lookup_kind {
        CLASS_LOOKUP_GET_CLASS => "get_class",
        CLASS_LOOKUP_GET_PARENT_CLASS => "get_parent_class",
        _ => return EvalStatus::RuntimeFatal.code(),
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_object_class_name(
        context,
        lookup,
        RuntimeCellHandle::from_raw(object_or_class),
        &mut values,
    ) {
        Ok(result) => write_outcome(EvalOutcome::Value(result), out).code(),
        Err(status) => status.code(),
    }
}

/// Runs the eval object relation ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_object_is_a`; invalid handles or unreadable target
/// storage fail closed as false.
#[cfg(not(test))]
unsafe fn eval_object_is_a_inner(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    target_ptr: *const u8,
    target_len: u64,
    exclude_self: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION || object.is_null() {
        return 0;
    }
    let Ok(target) = abi_name_to_string(target_ptr, target_len) else {
        return 0;
    };
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_object_is_a(
        context,
        RuntimeCellHandle::from_raw(object),
        &target,
        exclude_self != 0,
        &mut values,
    ) {
        Ok(result) => i32::from(result),
        Err(_) => 0,
    }
}
