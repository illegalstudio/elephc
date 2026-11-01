//! Purpose:
//! Exposes runtime Reflection owner construction through the optional native bridge.
//!
//! Called from:
//! - Generated object construction whose metadata depends on runtime argument values.
//!
//! Key details:
//! - Positional arguments are borrowed boxed Mixed cells owned by the generated caller.
//! - Results and catchable throwables use the shared `ElephcEvalResult` ABI contract.

use super::util::{abi_name_to_string, clear_result, write_outcome};
use crate::abi::ElephcEvalResult;
use crate::context::ElephcEvalContext;
use crate::errors::EvalStatus;
use crate::interpreter;
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::slice;

/// Constructs one supported Reflection owner from runtime argument cells.
///
/// # Safety
/// `name_ptr` must be readable for `name_len` bytes. `args` must be readable for
/// `arg_count` runtime-cell pointers, and `out` may be null.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_reflection_new_object(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_reflection_new_object");
    std::panic::catch_unwind(|| unsafe {
        reflection_new_object_inner(ctx, name_ptr, name_len, args, arg_count, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Runs Reflection construction after the exported wrapper installs a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_reflection_new_object`; all pointer ranges must remain readable
/// for the duration of the call.
unsafe fn reflection_new_object_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    if !ctx.is_null() && (*ctx).abi_version() != crate::abi::ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(arg_count) = usize::try_from(arg_count) else {
        return EvalStatus::RuntimeFatal.code();
    };
    if arg_count > 0 && args.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let args = if arg_count == 0 {
        Vec::new()
    } else {
        slice::from_raw_parts(args, arg_count)
            .iter()
            .map(|arg| RuntimeCellHandle::from_raw(*arg))
            .collect()
    };
    clear_result(out);
    let mut fallback_context;
    let context = if let Some(ctx) = ctx.as_mut() {
        ctx
    } else {
        fallback_context = ElephcEvalContext::new();
        crate::context::sync_global_eval_aot_metadata(&mut fallback_context);
        &mut fallback_context
    };
    context.sync_global_eval_classes();
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let arg_count = args.len();
    match interpreter::execute_reflection_new_object_outcome(
        context,
        &name,
        args,
        &mut values,
    ) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => {
            // Without a note the bridge prints the bare `eval() runtime failed`, which is all a
            // Symfony --web worker said when it died at boot. A deeper frame that described the
            // failure already wins (first writer), so this only fills the silence.
            crate::errors::note_eval_runtime_failure(
                format!("could not construct {name} with {arg_count} argument(s) ({status:?})"),
                "",
                None,
            );
            status.code()
        }
    }
}
