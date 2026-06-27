//! Purpose:
//! Exports post-barrier object introspection for values that may have been
//! created from eval-declared classes. Generated code uses this ABI when native
//! type metadata must consult the persistent eval context.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_object_class_name`.
//! - Generated EIR backend assembly through `__elephc_eval_object_is_a`.
//! - Generated EIR backend assembly through `__elephc_eval_object_is_a_dynamic`.
//! - Generated EIR backend assembly through `__elephc_eval_member_exists`.
//! - Generated EIR backend assembly through `__elephc_eval_class_relation`.
//!
//! Key details:
//! - Class-name and relation lookups return boxed Mixed cells through `ElephcEvalResult`.
//! - Named predicate probes fail closed as false; dynamic `instanceof` invalid
//!   targets return -1 so generated code can raise PHP's fatal diagnostic.

use super::util::{abi_name_to_string, clear_result, write_outcome};
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ABI_VERSION};
use crate::errors::EvalStatus;
use crate::interpreter::{self, EvalOutcome};
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};

const CLASS_LOOKUP_GET_CLASS: u64 = 0;
const CLASS_LOOKUP_GET_PARENT_CLASS: u64 = 1;
const MEMBER_LOOKUP_METHOD_EXISTS: u64 = 0;
const MEMBER_LOOKUP_PROPERTY_EXISTS: u64 = 1;
const CLASS_RELATION_IMPLEMENTS: u64 = 0;
const CLASS_RELATION_PARENTS: u64 = 1;
const CLASS_RELATION_USES: u64 = 2;

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

/// Tests whether an object satisfies a runtime string/object class target.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `object` and `target` must point
/// at boxed runtime cells. Returns -1 when the target is invalid for
/// `instanceof`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_object_is_a_dynamic(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    target: *mut RuntimeCell,
    exclude_self: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_object_is_a_dynamic_inner(ctx, object, target, exclude_self)
    })
    .unwrap_or(-1)
}

/// Tests whether a method or property exists in the eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `target` and `member` must point
/// at boxed runtime cells. `lookup_kind` must be a supported discriminator.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_member_exists(
    ctx: *mut ElephcEvalContext,
    target: *mut RuntimeCell,
    member: *mut RuntimeCell,
    lookup_kind: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_member_exists_inner(ctx, target, member, lookup_kind)
    })
    .unwrap_or(0)
}

/// Resolves class/interface/trait relation metadata in the eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `target` must point at a boxed
/// runtime cell. `relation_kind` must be a supported discriminator, and `out`
/// may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_class_relation(
    ctx: *mut ElephcEvalContext,
    target: *mut RuntimeCell,
    relation_kind: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_class_relation_inner(ctx, target, relation_kind, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
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

/// Runs the eval class-relation ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_class_relation`; callers must provide a valid context,
/// a boxed runtime cell target, and optional writable result storage.
#[cfg(not(test))]
unsafe fn eval_class_relation_inner(
    ctx: *mut ElephcEvalContext,
    target: *mut RuntimeCell,
    relation_kind: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if target.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let lookup = match relation_kind {
        CLASS_RELATION_IMPLEMENTS => "class_implements",
        CLASS_RELATION_PARENTS => "class_parents",
        CLASS_RELATION_USES => "class_uses",
        _ => return EvalStatus::RuntimeFatal.code(),
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_class_relation(
        context,
        lookup,
        RuntimeCellHandle::from_raw(target),
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

/// Runs the eval dynamic object relation ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_object_is_a_dynamic`; invalid target cells report -1
/// so generated code can use the normal dynamic-`instanceof` fatal path.
#[cfg(not(test))]
unsafe fn eval_object_is_a_dynamic_inner(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    target: *mut RuntimeCell,
    exclude_self: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return -1;
    };
    if context.abi_version() != ABI_VERSION || object.is_null() || target.is_null() {
        return -1;
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_object_is_a_dynamic(
        context,
        RuntimeCellHandle::from_raw(object),
        RuntimeCellHandle::from_raw(target),
        exclude_self != 0,
        &mut values,
    ) {
        Ok(result) => i32::from(result),
        Err(_) => -1,
    }
}

/// Runs the eval member-exists ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_member_exists`; invalid handles fail closed as false.
#[cfg(not(test))]
unsafe fn eval_member_exists_inner(
    ctx: *mut ElephcEvalContext,
    target: *mut RuntimeCell,
    member: *mut RuntimeCell,
    lookup_kind: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION || target.is_null() || member.is_null() {
        return 0;
    }
    let lookup = match lookup_kind {
        MEMBER_LOOKUP_METHOD_EXISTS => "method_exists",
        MEMBER_LOOKUP_PROPERTY_EXISTS => "property_exists",
        _ => return 0,
    };
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_member_exists(
        context,
        lookup,
        RuntimeCellHandle::from_raw(target),
        RuntimeCellHandle::from_raw(member),
        &mut values,
    ) {
        Ok(result) => i32::from(result),
        Err(_) => 0,
    }
}
