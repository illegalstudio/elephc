//! Purpose:
//! Exports runtime include/require execution through the optional bridge.
//! Accepts an already boxed PHP path value and executes the selected file in
//! the caller's persistent eval scope.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_include`.
//! - The generated web reset through `__elephc_eval_include_request_reset`.
//!
//! Key details:
//! - Include values, warnings, request-scoped once tracking, and Throwable propagation stay in Magician.
//! - The path cell is borrowed for the duration of the call.

use super::util::clear_result;
#[cfg(not(test))]
use super::util::write_outcome;
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ElephcEvalScope, ABI_VERSION};
#[cfg(not(test))]
use crate::context::EvalCallFrame;
use crate::errors::EvalStatus;
#[cfg(not(test))]
use crate::interpreter;
#[cfg(not(test))]
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::RuntimeCellHandle;
use std::ffi::c_void;

#[cfg(test)]
mod native_state_registration_tests {
    use super::*;
    thread_local! { static CELL: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }

    unsafe extern "C" fn lookup(path: *const u8, length: u64) -> *mut u64 {
        if unsafe { std::slice::from_raw_parts(path, length as usize) } == b"/native.php" {
            CELL.with(std::cell::Cell::as_ptr)
        } else { std::ptr::null_mut() }
    }
    unsafe extern "C" fn other_lookup(_: *const u8, _: u64) -> *mut u64 { std::ptr::null_mut() }
    unsafe extern "C" fn reset() { CELL.with(|cell| cell.set(0)); }

    #[test]
    fn native_include_registration_uses_the_context_request_state() {
        let mut context = ElephcEvalContext::new();
        unsafe {
            reset();
            assert_eq!(__elephc_eval_register_native_include_state(&mut context, Some(lookup), Some(reset)), 0);
        }
        context.mark_included_file("/native.php");
        CELL.with(|cell| assert_eq!(cell.get(), 1));
        unsafe {
            assert_eq!(__elephc_eval_register_native_include_state(&mut context, Some(lookup), Some(reset)), 0);
            assert_eq!(__elephc_eval_register_native_include_state(&mut context, Some(other_lookup), Some(reset)), EvalStatus::RuntimeFatal.code());
            reset();
        }
        assert!(!context.has_included_file("/native.php"));
        context.mark_included_file("/native.php");
        CELL.with(|cell| assert_eq!(cell.get(), 1));
        unsafe { reset(); }
    }

    #[test]
    fn native_include_registration_rejects_missing_callbacks_and_bad_abi() {
        let mut context = ElephcEvalContext::new();
        let mut bad = ElephcEvalContext::for_abi_version(ABI_VERSION + 1);
        unsafe {
            assert_eq!(__elephc_eval_register_native_include_state(&mut context, None, Some(reset)), EvalStatus::RuntimeFatal.code());
            assert_eq!(__elephc_eval_register_native_include_state(&mut bad, Some(lookup), Some(reset)), EvalStatus::AbiMismatch.code());
        }
    }
}

/// Connects inclusion state to the generated program's actual native guard cells.
/// This registers state access only, not a compiled source provider or declaration.
///
/// # Safety
/// Context must be null or live. Callbacks must remain valid for the program's
/// lifetime, must not execute PHP/reenter, and lookup must return null or an aligned
/// writable u64 cell. All native/bridge cell access must be serialized by the caller.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_include_state(
    ctx: *mut ElephcEvalContext,
    lookup: Option<unsafe extern "C" fn(*const u8, u64) -> *mut u64>,
    reset: Option<unsafe extern "C" fn()>,
) -> i32 {
    std::panic::catch_unwind(|| {
        let context = unsafe { ctx.as_ref() };
        if context.is_some_and(|context| context.abi_version() != ABI_VERSION) {
            return EvalStatus::AbiMismatch.code();
        }
        let (Some(lookup), Some(reset)) = (lookup, reset) else {
            return EvalStatus::RuntimeFatal.code();
        };
        if unsafe { crate::context::install_native_include_hooks(context, crate::context::NativeIncludeHooks { lookup, reset }) } {
            EvalStatus::Ok.code()
        } else {
            eprintln!("Fatal error: conflicting native inclusion-state owner");
            EvalStatus::RuntimeFatal.code()
        }
    }).unwrap_or(EvalStatus::RuntimeFatal.code())
}

/// Clears dynamic include bookkeeping before a generated web request starts.
///
/// CLI binaries never call this entry point, so their include registry naturally
/// lasts for the single PHP request represented by the process lifetime.
#[no_mangle]
pub extern "C" fn __elephc_eval_include_request_reset() {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_include_request_reset");
    let _ = std::panic::catch_unwind(|| {
        crate::context::reset_global_eval_function_contexts();
        crate::context::reset_global_eval_autoload_contexts();
        #[cfg(not(test))]
        crate::context::reset_global_eval_classes();
        // Statics are request state shared by every context; they go with the arena too.
        crate::context::reset_request_statics();
        // The null-handle context lives for the PROCESS, not the request, so its declarations
        // survive a boundary that just emptied the global registry. Clearing them is what lets
        // the next request re-declare and re-publish; see `forget_declared_class_likes`.
        #[cfg(not(test))]
        crate::ffi::context::forget_shared_null_handle_declarations();
        // These registries are keyed by an address the arena wipe invalidates: an object identity
        // for the first, a context and a retained callable for the second. An entry that survives
        // the boundary is reachable from the generated runtime on the next request and answers
        // for storage that no longer belongs to it.
        #[cfg(not(test))]
        crate::ffi::dynamic_destructors::reset_dynamic_object_contexts();
        // Same hazard: closure child edges keyed by object identity, holding arena cells.
        #[cfg(not(test))]
        crate::runtime_hooks::reset_object_owners();
        #[cfg(not(test))]
        crate::ffi::ob_handlers::reset_ob_handlers();
        // Last, because the two owner registries above finalize contexts of their own and this
        // frees whatever they left: a context the live-object counters refuse to release inside a
        // request has nothing left to protect once the arena is gone.
        #[cfg(not(test))]
        crate::ffi::context::reset_retained_eval_contexts();
        // After every context above is finalized: finalization moves each context's constants
        // into the orphan table, and all of them belong to the request that just ended.
        #[cfg(not(test))]
        crate::context::reset_global_eval_constants();
        // Context cleanup may execute PHP destructors, which must still observe
        // the current request's inclusion state until cleanup has completed.
        crate::context::reset_global_eval_included_files();
        // Last of all: every finalization above may have pooled its context, and a pooled
        // context still holds the class-likes of the request that just ended.
        #[cfg(not(test))]
        crate::ffi::context::drain_pooled_eval_contexts();
    });
}

/// `__elephc_eval_include`'s fifth argument is a FLAGS word, not just `once`.
///
/// The x86_64 C ABI passes six integer arguments in registers and this call already uses all
/// six, so a seventh flag would have to travel on the stack on one target and in a register on
/// the other. Packing it as a bit keeps one ABI for both. Emitted by `lower_dynamic_include`
/// in `src/codegen/lower_inst/builtins/eval/calls.rs`; the two must agree.
const INCLUDE_FLAG_ONCE: u64 = 1;
/// The include was written as a statement, so nothing reads the value it evaluates to.
const INCLUDE_FLAG_VALUE_DISCARDED: u64 = 2;

/// Answers a compiled include without an eval context when nothing has to run: `1` means the
/// include evaluates to `true` and is done, `0` means the caller must take `__elephc_eval_include`.
/// `flags` is the same word `__elephc_eval_include` takes.
///
/// # Safety
/// `path` must be null or readable for `length` bytes for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_include_fast(path: *const u8, length: u64, flags: u64) -> i32 {
    if path.is_null() {
        return 0;
    }
    std::panic::catch_unwind(|| {
        let bytes = unsafe { std::slice::from_raw_parts(path, length as usize) };
        let Ok(path) = std::str::from_utf8(bytes) else {
            return 0;
        };
        #[cfg(not(test))]
        {
            i32::from(interpreter::fast_include_answer(
                path,
                flags & INCLUDE_FLAG_ONCE != 0,
                flags & INCLUDE_FLAG_VALUE_DISCARDED != 0,
            ))
        }
        #[cfg(test)]
        {
            let _ = (path, flags);
            0
        }
    })
    .unwrap_or(0)
}

/// Executes one runtime include/require against a materialized caller scope.
///
/// # Safety
/// Context, scope, path, and result pointers must be null or valid for their ABI roles;
/// a non-null path must identify a live boxed PHP runtime cell for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_include(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    path: *mut c_void,
    required: u64,
    flags: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_include");
    std::panic::catch_unwind(|| unsafe {
        execute_include_inner(
            ctx,
            scope,
            path,
            required != 0,
            flags & INCLUDE_FLAG_ONCE != 0,
            flags & INCLUDE_FLAG_VALUE_DISCARDED != 0,
            out,
        )
    })
    .unwrap_or_else(|_| {
        if crate::eval_trace::enabled() {
            eprintln!("[elephc-eval-trace] phase=include_panic status=RuntimeFatal");
        }
        EvalStatus::RuntimeFatal.code()
    })
}

/// Runs the include ABI body after the exported wrapper installs a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_include`; every non-null pointer must remain valid for this call.
unsafe fn execute_include_inner(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    path: *mut c_void,
    required: bool,
    once: bool,
    value_discarded: bool,
    out: *mut ElephcEvalResult,
) -> i32 {
    if !ctx.is_null() && (*ctx).abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if path.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    clear_result(out);
    execute_materialized_include(
        ctx,
        scope,
        RuntimeCellHandle::from_raw(path),
        required,
        once,
        value_discarded,
        out,
    )
}

/// Returns the PHP name of the include form, which is the frame's `function`.
#[cfg(not(test))]
fn eval_include_frame_name(required: bool, once: bool) -> &'static str {
    match (required, once) {
        (true, true) => "require_once",
        (true, false) => "require",
        (false, true) => "include_once",
        (false, false) => "include",
    }
}

/// Executes the include in production builds through elephc runtime value hooks.
///
/// # Safety
/// Scope and result pointers must be null or point to bridge-owned ABI storage.
#[cfg(not(test))]
unsafe fn execute_materialized_include(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    path: RuntimeCellHandle,
    required: bool,
    once: bool,
    value_discarded: bool,
    out: *mut ElephcEvalResult,
) -> i32 {
    let mut fallback_context;
    let context = if let Some(ctx) = ctx.as_mut() {
        ctx
    } else {
        fallback_context = ElephcEvalContext::new();
        crate::context::sync_global_eval_aot_metadata(&mut fallback_context);
        &mut fallback_context
    };
    let mut fallback_scope;
    let scope = if let Some(scope) = scope.as_mut() {
        scope
    } else {
        fallback_scope = ElephcEvalScope::new();
        &mut fallback_scope
    };
    context.sync_global_eval_classes();
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    // php describes an include as a frame of its own, named for the form that was written and
    // carrying the path as its single argument, so code in the included file sees the include
    // above itself. The path cell belongs to the caller for the whole call and a frame does not
    // retain its arguments, so it can be named here without a copy.
    let frame = EvalCallFrame::function(
        eval_include_frame_name(required, once),
        Some(vec![path]),
        context,
    );
    context.push_call_frame(frame);
    let outcome = interpreter::execute_include_outcome_with_context(
        context,
        scope,
        path,
        required,
        once,
        value_discarded,
        &mut values,
    );
    context.pop_call_frame();
    match outcome {
        Ok(outcome) => {
            trace_bridge_include("include_ok", None, context);
            write_outcome(outcome, out).code()
        }
        Err(status) => {
            trace_bridge_include("include_error", Some(status), context);
            status.code()
        }
    }
}

/// Emits the opt-in trace for an include that crossed from COMPILED code into the interpreter.
///
/// Three fields, because the first is the only one that answers "what did this request include?"
/// and the other two cannot be trusted on their own:
/// - `path` is the file the include actually resolved to, taken from the slot the interpreter's
///   include driver filled on its way out. `<unresolved>` means the path expression itself failed,
///   so no file was ever named.
/// - `caller_file` is the source the CODEGEN attributed to the compiled function that issued the
///   include: a class's declaring file, a function's declaring file, or the module entry.
/// - `caller_span_line` is the line of the include's AST span, which belongs to whichever physical
///   file CONTRIBUTED that statement. After the autoload and include passes splice hundreds of
///   files into one program the two stop agreeing, and the disagreement is silent: a `require` in
///   `vendor/autoload_runtime.php` reports the nine-line entry file and line 9, and Composer's
///   `include $file;` reports that same entry file and line 576. Neither line exists in the file
///   named beside it. `path` is what a census must read.
#[cfg(not(test))]
fn trace_bridge_include(phase: &str, status: Option<EvalStatus>, context: &ElephcEvalContext) {
    if !crate::eval_trace::enabled() {
        return;
    }
    let target = crate::eval_trace::take_include_target()
        .unwrap_or_else(|| std::path::PathBuf::from("<unresolved>"));
    let call_site = context.call_site();
    match status {
        Some(status) => eprintln!(
            "[elephc-eval-trace] kind=include phase={phase} status={status:?} path={target:?} caller_file={:?} caller_span_line={}",
            call_site.0, call_site.2,
        ),
        None => eprintln!(
            "[elephc-eval-trace] kind=include phase={phase} path={target:?} caller_file={:?} caller_span_line={}",
            call_site.0, call_site.2,
        ),
    }
}

/// Keeps crate unit tests independent from generated runtime assembly wrappers.
///
/// # Safety
/// Result storage must be null or valid for the duration of the call.
#[cfg(test)]
unsafe fn execute_materialized_include(
    _ctx: *mut ElephcEvalContext,
    _scope: *mut ElephcEvalScope,
    _path: RuntimeCellHandle,
    _required: bool,
    _once: bool,
    _value_discarded: bool,
    _out: *mut ElephcEvalResult,
) -> i32 {
    EvalStatus::UnsupportedConstruct.code()
}
