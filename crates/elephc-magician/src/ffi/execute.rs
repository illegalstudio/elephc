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
#[cfg(not(test))]
use crate::interpreter;
use crate::parser;
#[cfg(not(test))]
use crate::runtime_hooks::ElephcRuntimeOps;
use std::slice;

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
    std::panic::catch_unwind(|| unsafe { execute_eval_inner(ctx, scope, code_ptr, code_len, out) })
        .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
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
    let program = match parser::parse_fragment(code) {
        Ok(program) => program,
        Err(err) => return err.status().code(),
    };
    clear_result(out);
    execute_parsed_eval(ctx, scope, &program, out)
}

/// Executes a parsed eval program in production builds using elephc runtime hooks.
///
/// # Safety
/// `scope` and `out` must be null or valid pointers supplied by generated code.
#[cfg(not(test))]
unsafe fn execute_parsed_eval(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    program: &eval_ir::EvalProgram,
    out: *mut ElephcEvalResult,
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
    context.sync_global_eval_classes();
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_program_outcome_with_context(context, program, scope, &mut values) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}

/// Keeps crate unit tests independent from generated runtime assembly wrappers.
///
/// # Safety
/// `out` must be null or valid result storage supplied by the test caller.
#[cfg(test)]
unsafe fn execute_parsed_eval(
    _ctx: *mut ElephcEvalContext,
    _scope: *mut ElephcEvalScope,
    _program: &eval_ir::EvalProgram,
    _out: *mut ElephcEvalResult,
) -> i32 {
    EvalStatus::UnsupportedConstruct.code()
}
