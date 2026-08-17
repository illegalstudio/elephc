//! Purpose:
//! Eval home for `curl_multi_exec(CurlMultiHandle $multi_handle, int &$still_running): int`.
//!
//! Called from:
//! - `crate::interpreter::expressions::calls::eval_call` (the `&[EvalCallArg]` interception
//!   that keeps the by-reference target reachable).
//! - `crate::interpreter::builtins::curl` dispatch (the by-value fallbacks).
//! - `crate::interpreter::builtins::registry::dynamic_mutation` (dynamic callables).
//!
//! Key details:
//! - `$still_running` IS A REQUIRED BY-REFERENCE PARAMETER, exactly as in php-src, and the
//!   bridge answers BOTH of this function's outputs packed into one integer
//!   (`crate::curl_ffi::multi_perform` unpacks it, including the low half's hand-rolled
//!   sign extension, exactly as `crate::curl_prelude::curl_multi_exec` does in PHP).
//! - THE BY-VALUE PATHS WARN AND KEEP GOING rather than throwing. Real PHP 8.4.20 raises a
//!   catchable `Error: curl_multi_exec(): Argument #2 ($still_running) could not be passed
//!   by reference` (measured), and AOT rejects the same call at COMPILE time against the
//!   prelude's `int &$still_running`. eval has neither a checker nor a by-ref-capable
//!   dispatch on those two paths, so it does what every other by-ref builtin in this
//!   interpreter already does — `preg_match()`'s `$matches`, `flock()`'s `$would_block`,
//!   `settype()`'s `$var` — and emits the crate's standard
//!   "must be passed by reference, value given" warning. That is a pre-existing,
//!   interpreter-wide shape, not something curl introduces.

use crate::curl_ffi as ffi;

eval_builtin! {
    name: "curl_multi_exec",
    area: Curl,
    params: [multi_handle, still_running: by_ref],
    by_ref: [still_running],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_multi_exec($multi_handle, $still_running)` over full eval call metadata,
/// writing the still-running count back through the by-reference target.
pub(in crate::interpreter) fn eval_builtin_curl_multi_exec_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    let (bound, _) =
        bind_evaluated_ref_builtin_args(&["multi_handle", "still_running"], &evaluated_args, false)?;
    let multi_handle = required_evaluated_ref_arg(&bound, 0)?;
    let Some(still_running) = optional_evaluated_ref_arg(&bound, 1) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let target = still_running.ref_target.clone();
    let (running, code) = eval_curl_multi_exec_perform(multi_handle.value, context, values)?;
    match target {
        Some(target) => eval_curl_multi_exec_write_back(&target, running, context, values)?,
        None => eval_curl_multi_exec_warn_by_value(values)?,
    }
    values.int(code)
}

/// Evaluates `curl_multi_exec()` over plain eval expressions, which still carry enough to
/// resolve the by-reference lvalue.
pub(in crate::interpreter) fn eval_builtin_curl_multi_exec(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle, still_running] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let multi_handle = eval_expr(multi_handle, context, scope, values)?;
    let (_, target) = eval_call_arg_value(still_running, context, scope, values)?;
    let (running, code) = eval_curl_multi_exec_perform(multi_handle, context, values)?;
    match target {
        Some(target) => eval_curl_multi_exec_write_back(&target, running, context, values)?,
        None => eval_curl_multi_exec_warn_by_value(values)?,
    }
    values.int(code)
}

/// Dispatches evaluated `curl_multi_exec()` calls through the builtin leaf. This path has
/// no reference targets at all (see this file's header), so it warns and drops the count.
pub(in crate::interpreter) fn eval_curl_multi_exec_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle, _still_running] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let (_, code) = eval_curl_multi_exec_perform(*multi_handle, context, values)?;
    eval_curl_multi_exec_warn_by_value(values)?;
    values.int(code)
}

/// Runs one `curl_multi_exec()` against a known-writable `$still_running` target — the
/// shape `crate::interpreter::builtins::registry::dynamic_mutation` needs for a dynamic
/// callable (`call_user_func_array('curl_multi_exec', …)`, `$f($mh, $n)`, …).
pub(in crate::interpreter) fn eval_curl_multi_exec_with_target(
    multi_handle: RuntimeCellHandle,
    target: &EvalReferenceTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (running, code) = eval_curl_multi_exec_perform(multi_handle, context, values)?;
    eval_curl_multi_exec_write_back(target, running, context, values)?;
    values.int(code)
}

/// Drives every attached transfer once, answering `(still_running, CURLMcode)`.
fn eval_curl_multi_exec_perform(
    multi_handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(i64, i64), EvalStatus> {
    let raw = eval_curl_multi_raw("curl_multi_exec", multi_handle, context, values)?;
    let (running, code) = ffi::multi_perform(raw);
    Ok((running, code))
}

/// Writes the still-running count into the caller's `$still_running` lvalue.
fn eval_curl_multi_exec_write_back(
    target: &EvalReferenceTarget,
    running: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let running = values.int(running)?;
    eval_write_direct_ref_target(
        target,
        running,
        context,
        values,
        Some(ScopeCellOwnership::Owned),
    )
}

/// The by-value diagnostic this interpreter uses for every by-reference builtin parameter
/// it cannot write back through.
fn eval_curl_multi_exec_warn_by_value(
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    values.warning(
        "curl_multi_exec(): Argument #2 ($still_running) must be passed by reference, value given",
    )
}
