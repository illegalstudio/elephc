//! Purpose:
//! Exports eval fragment execution through the optional bridge.
//! This layer validates ABI pointers, parses fragment bytes, and dispatches
//! parsed EvalIR to the interpreter with runtime hooks in production builds.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_execute`.
//!
//! Key details:
//! - Tests keep a controlled unsupported stub because generated runtime wrappers
//!   are not linked into the crate unit-test binary.

use super::util::clear_result;
#[cfg(not(test))]
use super::util::write_outcome;
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ElephcEvalScope, ABI_VERSION};
use crate::errors::EvalStatus;
use crate::eval_ir;
use crate::eval_ir::EVAL_SCOPE_ACCESS_ALL;
#[cfg(not(test))]
use crate::interpreter;
#[cfg(not(test))]
use crate::interpreter::RuntimeValueOps;
use crate::parse_cache;
#[cfg(not(test))]
use crate::runtime_hooks::ElephcRuntimeOps;
use std::slice;

/// Returns the conservative caller-scope name mask for one cacheable eval fragment.
///
/// Parse failures select the full scope for the current call. Oversized or complex fragments
/// also make full synchronization sticky because they can retain closures, references, handlers,
/// or other indirect access in the persistent activation context.
///
/// # Safety
/// `ctx` must be a valid current-ABI context and `code_ptr` must be readable for `code_len` bytes
/// when `code_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_access_mask(
    ctx: *mut ElephcEvalContext,
    code_ptr: *const u8,
    code_len: u64,
) -> u64 {
    std::panic::catch_unwind(|| unsafe { eval_scope_access_mask_inner(ctx, code_ptr, code_len) })
        .unwrap_or(EVAL_SCOPE_ACCESS_ALL)
}

/// Computes the scope-access mask after validating the raw source slice.
///
/// # Safety
/// Mirrors [`__elephc_eval_scope_access_mask`].
unsafe fn eval_scope_access_mask_inner(
    ctx: *mut ElephcEvalContext,
    code_ptr: *const u8,
    code_len: u64,
) -> u64 {
    let Some(context) = ctx.as_mut() else {
        return EVAL_SCOPE_ACCESS_ALL;
    };
    if context.abi_version() != ABI_VERSION || context.eval_scope_sync_requires_full() {
        return EVAL_SCOPE_ACCESS_ALL;
    }
    let Ok(code_len) = usize::try_from(code_len) else {
        context.require_full_eval_scope_sync();
        return EVAL_SCOPE_ACCESS_ALL;
    };
    if code_len > 0 && code_ptr.is_null() {
        return EVAL_SCOPE_ACCESS_ALL;
    }
    let code = if code_len == 0 {
        &[]
    } else {
        slice::from_raw_parts(code_ptr, code_len)
    };
    if !parse_cache::is_cacheable_fragment(code) {
        context.require_full_eval_scope_sync();
        return EVAL_SCOPE_ACCESS_ALL;
    }
    let mask = parse_cache::parse_fragment_cached(code)
        .map(|program| program.scope_access_mask())
        .unwrap_or(EVAL_SCOPE_ACCESS_ALL);
    if mask == EVAL_SCOPE_ACCESS_ALL {
        context.require_full_eval_scope_sync();
    }
    mask
}

/// Executes an eval fragment against a materialized caller scope.
///
/// The FFI shape is final for the initial bridge: context/scope are opaque
/// runtime handles, `code_ptr`/`code_len` identify the PHP fragment bytes, and
/// `out` receives the eval return cell when provided. Non-test builds execute
/// the current EvalIR subset; test builds return `UnsupportedConstruct` because
/// they do not link elephc's generated runtime value wrappers.
///
/// # Safety
/// Callers must pass valid pointers for any non-null handle and ensure
/// `code_ptr` is readable for `code_len` bytes when `code_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_execute(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    code_ptr: *const u8,
    code_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe { execute_eval_inner(ctx, scope, code_ptr, code_len, out, false) })
        .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Executes generated eval code with the conservative deferred-catalog capability.
///
/// Existing users of `__elephc_eval_execute` retain eager global-class synchronization.
///
/// # Safety
/// Uses the same valid-handle and source lifetime requirements as `__elephc_eval_execute`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_execute_with_deferred_catalog(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    code_ptr: *const u8,
    code_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        execute_eval_inner(ctx, scope, code_ptr, code_len, out, true)
    }).unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Runs the eval ABI body after the exported wrapper has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_execute`; callers must provide valid handles and code
/// storage for every non-null pointer argument.
unsafe fn execute_eval_inner(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    code_ptr: *const u8,
    code_len: u64,
    out: *mut ElephcEvalResult,
    allow_deferred_catalog: bool,
) -> i32 {
    if !ctx.is_null() && (*ctx).abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if code_len > 0 && code_ptr.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let Ok(code_len) = usize::try_from(code_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let code = if code_len == 0 {
        &[]
    } else {
        slice::from_raw_parts(code_ptr, code_len)
    };
    let program = parse_cache::parse_fragment_cached(code).map_err(|error| error.status());
    clear_result(out);
    execute_parsed_eval(ctx, scope, program.as_deref().map_err(|status| *status), out, allow_deferred_catalog)
}

/// Executes a parsed eval program in production builds using elephc runtime hooks.
///
/// # Safety
/// `scope` and `out` must be null or valid pointers supplied by generated code.
#[cfg(not(test))]
unsafe fn execute_parsed_eval(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    program: Result<&eval_ir::EvalProgram, EvalStatus>,
    out: *mut ElephcEvalResult,
    allow_deferred_catalog: bool,
) -> i32 {
    let mut fallback_context;
    let context = if let Some(ctx) = ctx.as_mut() {
        ctx
    } else {
        fallback_context = ElephcEvalContext::new();
        &mut fallback_context
    };
    let mut fallback_scope;
    let scope = if let Some(scope) = scope.as_mut() {
        scope
    } else {
        fallback_scope = ElephcEvalScope::new();
        &mut fallback_scope
    };
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let submission = crate::repl::take_submission_request();
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    // Numeric-only execution cannot observe global class metadata. Postponing this import
    // preserves the native-first declaration order when the same context is later upgraded.
    let globals = context.global_scope_ptr().filter(|ptr| *ptr != scope as *mut ElephcEvalScope)
        .and_then(|ptr| unsafe { ptr.as_ref() });
    let numeric_without_catalog = allow_deferred_catalog && !context.native_catalog_is_ready()
        && !context.eval_scope_sync_requires_full()
        && program.as_ref().is_ok_and(|program| super::native_catalog::metadata_free_fragment(
            program, scope, globals, |cell| values.type_tag(cell).ok()));
    if numeric_without_catalog {
        context.record_native_catalog_deferral();
    } else {
        context.sync_global_eval_classes();
    }
    let outcome = program.and_then(|program| {
        context.push_eval_backtrace_boundary();
        let outcome = interpreter::execute_program_outcome_with_context(context, program, scope, &mut values);
        context.pop_eval_backtrace_boundary();
        outcome
    });
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let outcome = match submission {
        Some(display) => interpreter::repl::finish_submission(outcome, display, context, &mut values),
        None => outcome,
    };
    match outcome {
        Ok(outcome) => match check_eval_escape(outcome, context, scope, &mut values) {
            Ok(outcome) => write_outcome(outcome, out).code(),
            Err(status) => status.code(),
        },
        Err(status) => status.code(),
    }
}

/// Validates every outcome, including recovered submissions, before native scope writeback.
/// Safety failures bypass REPL recovery because those cells cannot cross into native code.
#[cfg(not(test))]
fn check_eval_escape(
    outcome: interpreter::EvalOutcome,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut ElephcRuntimeOps,
) -> Result<interpreter::EvalOutcome, EvalStatus> {
    if !context.needs_pcntl_escape_scan() {
        return Ok(outcome);
    }
    let owned = match &outcome {
        interpreter::EvalOutcome::Value(result) | interpreter::EvalOutcome::Throwable(result) => *result,
    };
    let checked = (|| {
        if let interpreter::EvalOutcome::Value(result) = &outcome {
            if interpreter::value_contains_foreign_pcntl_callable(*result, context, values)? {
                return Err(EvalStatus::EscapingPcntlCallable);
            }
        }
        for candidate in eval_escape_scope_cells(context, scope) {
            if interpreter::value_contains_foreign_pcntl_callable(candidate, context, values)? {
                return Err(EvalStatus::EscapingPcntlCallable);
            }
        }
        Ok(())
    })();
    if let Err(status) = checked {
        let _ = values.release(owned);
        return Err(status);
    }
    Ok(outcome)
}

/// Collects every visible local and global cell that could cross the eval-to-AOT boundary.
#[cfg(not(test))]
fn eval_escape_scope_cells(
    context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
) -> Vec<crate::value::RuntimeCellHandle> {
    let mut cells = scope.aot_visible_cells();
    if let Some(global_scope) = context.global_scope_ptr() {
        let current_scope = scope as *const ElephcEvalScope as *mut ElephcEvalScope;
        if global_scope != current_scope {
            if let Some(global_scope) = unsafe { global_scope.as_ref() } {
                cells.extend(global_scope.aot_visible_cells());
            }
        }
    }
    cells.sort_unstable_by_key(|cell| cell.as_ptr() as usize);
    cells.dedup_by_key(|cell| cell.as_ptr() as usize);
    cells
}

/// Keeps crate unit tests independent from generated runtime assembly wrappers.
///
/// # Safety
/// `out` must be null or valid result storage supplied by the test caller.
#[cfg(test)]
unsafe fn execute_parsed_eval(
    _ctx: *mut ElephcEvalContext,
    _scope: *mut ElephcEvalScope,
    program: Result<&eval_ir::EvalProgram, EvalStatus>,
    _out: *mut ElephcEvalResult,
    _allow_deferred_catalog: bool,
) -> i32 {
    program.map_or_else(|status| status.code(), |_| EvalStatus::UnsupportedConstruct.code())
}
