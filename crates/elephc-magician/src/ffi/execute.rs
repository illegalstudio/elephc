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
#[cfg(not(test))]
use crate::context::EvalCallFrame;
use crate::errors::{report_fatal_diagnostic, EvalParseDiagnostic, EvalStatus};
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
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_execute");
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
    trace_eval_input(ctx, code);
    let parsed = parse_cache::parse_fragment_cached(code);
    let program = parsed.as_deref().map_err(|error| error.status());
    if let Err(err) = &parsed {
        trace_eval_parse_error(ctx, code, err, err.status().code());
    }
    clear_result(out);
    let status = execute_parsed_eval(ctx, scope, program, out, allow_deferred_catalog);
    // A parse failure is reported once, after the REPL has had its chance to recover the
    // submission: the status still being the parse error's own means nothing recovered it.
    if let Err(err) = &parsed {
        if status == err.status().code() {
            report_eval_parse_error(ctx, err);
        }
    }
    status
}

/// Prints the eval bridge fatal for a non-zero status the generated handler cannot describe.
///
/// WHY THE BRIDGE PRINTS THIS AND THE ASSEMBLY NO LONGER DOES, which is the same argument the
/// parse-error path already won. The generated handler carries a fixed string in the data
/// section; it cannot name the file, the line, or what the interpreter was doing, because none
/// of those exist until the fragment runs. So the side that owns the interpreter prints, and
/// `codegen::lower_inst::builtins::eval::status` is left with the exit.
///
/// The prefixes are unchanged so this is a strict addition: with nothing recorded, the bytes
/// written are exactly the constants the assembly used to emit. `status` is the ABI code, and
/// any code other than the two anonymous fatals prints nothing — a parse error has already
/// printed its own diagnostic, and an uncaught Throwable is reported by the runtime unwinder.
///
/// # Safety
/// Called from generated assembly with no arguments beyond the ABI status code.
#[no_mangle]
pub extern "C" fn __elephc_eval_report_runtime_fatal(status: i64) {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_report_runtime_fatal");
    let _ = std::panic::catch_unwind(|| {
        let prefix = if status == i64::from(EvalStatus::RuntimeFatal.code()) {
            "Fatal error: eval() runtime failed"
        } else if status == i64::from(EvalStatus::UnsupportedConstruct.code()) {
            "Fatal error: eval() fragment uses an unsupported construct"
        } else {
            return;
        };
        let clause = crate::errors::take_eval_runtime_failure()
            .map(|failure| failure.clause())
            .unwrap_or_default();
        crate::errors::report_bridge_fatal_diagnostic(&format!("{prefix}{clause}\n"));
    });
}

/// Returns whether opt-in eval bridge tracing is enabled for this process.
fn eval_trace_enabled() -> bool {
    crate::eval_trace::enabled()
}

/// Emits the exact eval input and propagated call-site metadata before cache lookup.
///
/// # Safety
/// `ctx` must be null or a valid eval context handle supplied to the FFI entry point.
unsafe fn trace_eval_input(ctx: *const ElephcEvalContext, code: &[u8]) {
    if !eval_trace_enabled() {
        return;
    }
    let _ = std::panic::catch_unwind(|| {
        let (file, dir, line, file_override) = unsafe { ctx.as_ref() }
            .map(ElephcEvalContext::call_site)
            .unwrap_or_else(|| (String::new(), String::new(), 0, None));
        eprintln!(
            "[elephc-eval-trace] phase=input ctx={ctx:p} code_ptr={:p} len={} file={file:?} dir={dir:?} line={line} file_override={file_override:?}",
            code.as_ptr(),
            code.len(),
        );
    });
}

/// Prints the PHP parse diagnostic for a fragment the interpreter could not parse.
///
/// The generated status handler cannot name the file, the line or the token — the assembly it
/// emits carries a fixed string — so the side that owns the parser prints the diagnostic and
/// the generated code only exits.
///
/// # Safety
/// `ctx` must be null or a valid eval context handle supplied to the FFI entry point.
unsafe fn report_eval_parse_error(ctx: *const ElephcEvalContext, error: &EvalParseDiagnostic) {
    let (file, _, line, _) = unsafe { ctx.as_ref() }
        .map(ElephcEvalContext::call_site)
        .unwrap_or_else(|| (String::new(), String::new(), 0, None));
    report_fatal_diagnostic(&error.eval_message(&file, line));
}

/// Emits the precise parser failure while preserving the original ABI status.
///
/// # Safety
/// `ctx` must be null or a valid eval context handle supplied to the FFI entry point.
unsafe fn trace_eval_parse_error(
    ctx: *const ElephcEvalContext,
    code: &[u8],
    error: &EvalParseDiagnostic,
    status: i32,
) {
    if !eval_trace_enabled() {
        return;
    }
    let _ = std::panic::catch_unwind(|| {
        let (file, dir, line, file_override) = unsafe { ctx.as_ref() }
            .map(ElephcEvalContext::call_site)
            .unwrap_or_else(|| (String::new(), String::new(), 0, None));
        eprintln!(
            "[elephc-eval-trace] phase=parse_error ctx={ctx:p} code_ptr={:p} len={} status={status} error={error:?} file={file:?} dir={dir:?} line={line} file_override={file_override:?}",
            code.as_ptr(),
            code.len(),
        );
    });
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
        // php describes `eval()` as a frame too, named `eval` and carrying no arguments -- the code
        // it ran is not one. Without it a function defined in evaluated code reported its caller as
        // whatever lay below the eval.
        let frame = EvalCallFrame::function("eval", None, context);
        context.push_call_frame(frame);
        context.push_eval_backtrace_boundary();
        let previous_site = context.enter_eval_code_site();
        let outcome = interpreter::execute_program_outcome_with_context(context, program, scope, &mut values);
        context.leave_eval_code_site(previous_site);
        context.pop_eval_backtrace_boundary();
        context.pop_call_frame();
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
        Err(status) => {
            if eval_trace_enabled() {
                let call_site = context.call_site();
                eprintln!(
                    "[elephc-eval-trace] phase=runtime_error status={status:?} file={:?} line={}",
                    call_site.0,
                    call_site.2,
                );
            }
            status.code()
        }
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
