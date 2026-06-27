//! Purpose:
//! Exports post-barrier static member operations for classes declared by eval
//! fragments. Generated code uses this ABI when native scoped-constant or
//! static-property access targets dynamic eval metadata.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_class_constant_fetch`.
//! - Generated EIR backend assembly through `__elephc_eval_static_property_get`.
//! - Generated EIR backend assembly through `__elephc_eval_static_property_set`.
//!
//! Key details:
//! - Returned value cells are retained before crossing back to generated code.
//! - Errors and thrown PHP objects use the shared `ElephcEvalResult` contract.

use super::util::{abi_name_to_string, clear_result, write_outcome};
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ABI_VERSION};
use crate::errors::EvalStatus;
use crate::interpreter::{self, EvalOutcome, RuntimeValueOps};
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};

/// Fetches a class-like constant through eval dynamic metadata.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The class and constant name
/// byte ranges must be readable when their lengths are non-zero, and `out` may
/// be null.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_class_constant_fetch(
    ctx: *mut ElephcEvalContext,
    class_ptr: *const u8,
    class_len: u64,
    constant_ptr: *const u8,
    constant_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_class_constant_fetch_inner(ctx, class_ptr, class_len, constant_ptr, constant_len, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Reads a static property through eval dynamic metadata.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The class and property name byte
/// ranges must be readable when their lengths are non-zero, and `out` may be
/// null.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_static_property_get(
    ctx: *mut ElephcEvalContext,
    class_ptr: *const u8,
    class_len: u64,
    property_ptr: *const u8,
    property_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_static_property_get_inner(ctx, class_ptr, class_len, property_ptr, property_len, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Writes a static property through eval dynamic metadata.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The target byte range must be a
/// readable `Class::property` name when non-empty, `value` must be a valid
/// runtime cell, and `out` may be null.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_static_property_set(
    ctx: *mut ElephcEvalContext,
    target_ptr: *const u8,
    target_len: u64,
    value: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_static_property_set_inner(ctx, target_ptr, target_len, value, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Runs the class-constant fetch ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_class_constant_fetch`; callers must provide a valid
/// context, readable class and constant name bytes, and optional result storage.
unsafe fn eval_class_constant_fetch_inner(
    ctx: *mut ElephcEvalContext,
    class_ptr: *const u8,
    class_len: u64,
    constant_ptr: *const u8,
    constant_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let Ok(class_name) = abi_name_to_string(class_ptr, class_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(constant_name) = abi_name_to_string(constant_ptr, constant_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_class_constant_fetch(
        context,
        &class_name,
        &constant_name,
        &mut values,
    )
    .and_then(|outcome| retain_value_outcome(outcome, &mut values))
    {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}

/// Runs the static-property get ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_static_property_get`; callers must provide a valid
/// context, readable class and property name bytes, and optional result storage.
unsafe fn eval_static_property_get_inner(
    ctx: *mut ElephcEvalContext,
    class_ptr: *const u8,
    class_len: u64,
    property_ptr: *const u8,
    property_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let Ok(class_name) = abi_name_to_string(class_ptr, class_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(property_name) = abi_name_to_string(property_ptr, property_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_static_property_get(
        context,
        &class_name,
        &property_name,
        &mut values,
    )
    .and_then(|outcome| retain_value_outcome(outcome, &mut values))
    {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}

/// Runs the static-property set ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_static_property_set`; callers must provide a valid
/// context, readable `Class::property` target bytes, a valid value cell, and
/// optional result storage for thrown PHP objects.
unsafe fn eval_static_property_set_inner(
    ctx: *mut ElephcEvalContext,
    target_ptr: *const u8,
    target_len: u64,
    value: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if value.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let Ok(target_name) = abi_name_to_string(target_ptr, target_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok((class_name, property_name)) = split_static_property_target(&target_name) else {
        return EvalStatus::RuntimeFatal.code();
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_static_property_set(
        context,
        class_name,
        property_name,
        RuntimeCellHandle::from_raw(value),
        &mut values,
    ) {
        Ok(Some(outcome)) => write_outcome(outcome, out).code(),
        Ok(None) => EvalStatus::Ok.code(),
        Err(status) => status.code(),
    }
}

/// Splits the packed static-property target used by the compact setter ABI.
fn split_static_property_target(target: &str) -> Result<(&str, &str), EvalStatus> {
    let Some((class_name, property_name)) = target.rsplit_once("::") else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if class_name.is_empty() || property_name.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok((class_name, property_name))
}

/// Retains value outcomes so generated code receives an owned boxed cell.
fn retain_value_outcome(
    outcome: EvalOutcome,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    match outcome {
        EvalOutcome::Value(value) => values.retain(value).map(EvalOutcome::Value),
        EvalOutcome::Throwable(error) => Ok(EvalOutcome::Throwable(error)),
    }
}
