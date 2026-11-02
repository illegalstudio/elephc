//! Purpose:
//! Exports post-barrier calls into functions declared by eval fragments.
//! Generated code can call zero-argument, positional, or argument-array forms
//! after probing dynamic function existence.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_call_function*`.
//!
//! Key details:
//! - Calls return either a value cell or an uncaught throwable cell through
//!   `ElephcEvalResult`.
//! - Argument pointer arrays are borrowed from generated code and not released here.

use super::util::{abi_name_to_string, clear_result, write_outcome};
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ABI_VERSION};
use crate::errors::EvalStatus;
use crate::interpreter;
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::slice;

/// Calls a zero-argument function previously declared through `eval()`.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_call_function_zero_args(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_call_function_zero_args");
    std::panic::catch_unwind(|| unsafe {
        call_eval_function_inner(ctx, name_ptr, name_len, std::ptr::null(), 0, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Calls a function previously declared through `eval()` with positional cells.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. `args` must be readable for
/// `arg_count` runtime-cell pointers when `arg_count > 0`, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_call_function(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_call_function");
    std::panic::catch_unwind(|| unsafe {
        call_eval_function_inner(ctx, name_ptr, name_len, args, arg_count, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Calls a function previously declared through `eval()` with an argument array/hash.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. `arg_array` must be a boxed Mixed
/// indexed or associative array cell, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_call_function_array(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    arg_array: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_call_function_array");
    std::panic::catch_unwind(|| unsafe {
        call_eval_function_array_inner(ctx, name_ptr, name_len, arg_array, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Runs the dynamic function-call ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_call_function`; callers must provide a valid context,
/// readable function-name bytes, and readable argument pointer storage.
#[cfg(not(test))]
unsafe fn call_eval_function_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    // A null handle is how generated code says "no context declares this name, resolve it
    // yourself": the bridge answers for the backtrace pair, the OPcache family, the procedural
    // date aliases and every builtin the interpreter implements. Refusing null made those calls
    // a runtime fatal. The include, eval, symbol and object-construction entries all take a null
    // handle this way already.
    let context = if let Some(context) = ctx.as_mut() {
        if context.abi_version() != ABI_VERSION {
            return EvalStatus::AbiMismatch.code();
        }
        context
    } else {
        crate::ffi::context::shared_null_handle_context()
    };
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
            .map(|arg| RuntimeCellHandle::from_raw(*arg).borrowed())
            .collect()
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match execute_context_function_with_namespace_fallback(context, &name, args, &mut values) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => trace_dynamic_function_call_failure(context, &name, status).code(),
    }
}

/// Runs the dynamic function-call-array ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_call_function_array`; callers must provide a valid
/// context, readable function-name bytes, and a boxed array/hash argument cell.
#[cfg(not(test))]
unsafe fn call_eval_function_array_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    arg_array: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    // Same null-handle contract as the positional entry above.
    let context = if let Some(context) = ctx.as_mut() {
        if context.abi_version() != ABI_VERSION {
            return EvalStatus::AbiMismatch.code();
        }
        context
    } else {
        crate::ffi::context::shared_null_handle_context()
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    if arg_array.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match execute_context_function_array_with_namespace_fallback(
        context,
        &name,
        RuntimeCellHandle::from_raw(arg_array).borrowed(),
        &mut values,
    ) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => trace_dynamic_function_call_failure(context, &name, status).code(),
    }
}

/// Emits an opt-in dynamic-function failure trace without changing PHP-visible diagnostics.
fn trace_dynamic_function_call_failure(
    context: &ElephcEvalContext,
    name: &str,
    status: EvalStatus,
) -> EvalStatus {
    if crate::eval_trace::enabled() {
        let fallback = name.rsplit_once('\\').map(|(_, bare)| bare);
        eprintln!(
            "[elephc-eval-trace] phase=dynamic_function_call name={name:?} status={status:?} namespaced_exists={} global_fallback={fallback:?} global_exists={}",
            context.has_function(name),
            fallback.is_some_and(|bare| context.has_function(bare)),
        );
    }
    status
}

/// Calls a dynamic function by its namespace candidate and PHP's global fallback when needed.
fn execute_context_function_with_namespace_fallback(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut ElephcRuntimeOps,
) -> Result<interpreter::EvalOutcome, EvalStatus> {
    let name = name.to_ascii_lowercase();
    // PHP resolves an UNQUALIFIED call inside a namespace to the namespaced function when one
    // is declared, and to the GLOBAL one otherwise — it does not attempt the namespaced name
    // first and recover from whatever that attempt reports. Attempting it first only worked
    // while the failure happened to be `UnsupportedConstruct`: `hash_final($ctx)` inside
    // `namespace Symfony\Component\Config\Resource` resolved the namespaced spelling far enough
    // to fail as `RuntimeFatal`, which fell straight through with no fallback at all. Deciding
    // by DECLARATION, the way PHP does, removes the dependence on which error came back.
    if let Some((_, bare)) = name.rsplit_once('\\') {
        if !context.has_function(&name) {
            let bare = bare.to_string();
            return match interpreter::execute_context_function_outcome(
                context,
                &bare,
                args.clone(),
                values,
            ) {
                Err(EvalStatus::UnsupportedConstruct) => {
                    if let Some(result) = execute_global_eval_function_owner(&bare, args.clone()) {
                        return result;
                    }
                    // The global name is not known either: report against the name the caller
                    // actually wrote, so the diagnostic still names the namespaced spelling.
                    interpreter::execute_context_function_outcome(context, &name, args, values)
                }
                result => result,
            };
        }
    }
    match interpreter::execute_context_function_outcome(context, &name, args.clone(), values) {
        Err(EvalStatus::UnsupportedConstruct) => {
            if let Some(result) = execute_global_eval_function_owner(&name, args.clone()) {
                return result;
            }
            name.rsplit_once('\\')
                .map(|(_, bare)| {
                    interpreter::execute_context_function_outcome(context, bare, args, values)
                })
                .unwrap_or(Err(EvalStatus::UnsupportedConstruct))
        }
        result => result,
    }
}

/// Executes a runtime-included global function through the context that owns its declaration.
#[cfg(not(test))]
fn execute_global_eval_function_owner(
    name: &str,
    args: Vec<RuntimeCellHandle>,
) -> Option<Result<interpreter::EvalOutcome, EvalStatus>> {
    let owner = crate::context::global_eval_function_owner_context(name)?;
    let owner = unsafe { owner.as_mut() }?;
    let call_name = name.rsplit_once('\\').map_or(name, |(_, bare)| bare);
    let mut values = ElephcRuntimeOps::with_context(owner as *const ElephcEvalContext);
    Some(interpreter::execute_context_function_outcome(
        owner,
        call_name,
        args,
        &mut values,
    ))
}

/// Keeps unit-test builds independent from the process-global runtime registry.
#[cfg(test)]
fn execute_global_eval_function_owner(
    _name: &str,
    _args: Vec<RuntimeCellHandle>,
) -> Option<Result<interpreter::EvalOutcome, EvalStatus>> {
    None
}

/// Calls a dynamic function-array form with PHP's namespace-to-global fallback.
fn execute_context_function_array_with_namespace_fallback(
    context: &mut ElephcEvalContext,
    name: &str,
    args: RuntimeCellHandle,
    values: &mut ElephcRuntimeOps,
) -> Result<interpreter::EvalOutcome, EvalStatus> {
    let name = name.to_ascii_lowercase();
    match interpreter::execute_context_function_call_array_outcome(context, &name, args, values) {
        Err(EvalStatus::UnsupportedConstruct) => name
            .rsplit_once('\\')
            .map(|(_, bare)| {
                interpreter::execute_context_function_call_array_outcome(
                    context, bare, args, values,
                )
            })
            .unwrap_or(Err(EvalStatus::UnsupportedConstruct)),
        result => result,
    }
}
