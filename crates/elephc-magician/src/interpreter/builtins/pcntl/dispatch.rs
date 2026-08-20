//! Purpose:
//! Binds PCNTL eval arguments while preserving reference targets and dispatches operations.
//!
//! Called from:
//! - Direct EvalIR calls, declarative hooks, and dynamic callable dispatch.
//!
//! Key details:
//! - Missing optional slots remain absent so by-reference defaults never masquerade as lvalues.
//! - Named and unpacked arguments are normalized before any process operation runs.

use super::*;

/// Whether a call can require writable caller storage or must degrade by-reference outputs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PcntlCallMode {
    /// Ordinary function-call semantics with live caller reference targets.
    Direct,
    /// Callable-by-value semantics that warn instead of writing unavailable targets.
    Callable,
}

/// Returns whether one PCNTL builtin exists on the current supported target.
pub(in crate::interpreter) fn eval_pcntl_builtin_is_available(name: &str) -> bool {
    match name {
        "pcntl_getqos_class" | "pcntl_setqos_class" => cfg!(target_os = "macos"),
        "pcntl_getcpu"
        | "pcntl_getcpuaffinity"
        | "pcntl_setcpuaffinity"
        | "pcntl_setns"
        | "pcntl_sigtimedwait"
        | "pcntl_sigwaitinfo"
        | "pcntl_unshare" => cfg!(target_os = "linux"),
        _ => true,
    }
}

/// Evaluates a source-level PCNTL call with named/spread binding and live references.
pub(in crate::interpreter) fn eval_builtin_pcntl_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated = eval_call_arg_values(args, context, scope, values)?;
    let bound = eval_pcntl_bind_args(name, &evaluated)?;
    eval_pcntl_bound_result(name, &bound, PcntlCallMode::Direct, context, values)
}

/// Evaluates positional expression hooks when registry dispatch is invoked directly.
pub(in crate::interpreter) fn eval_builtin_pcntl_expr_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(EvaluatedCallArg {
            name: None,
            value: eval_expr(arg, context, scope, values)?,
            ref_target: None,
        });
    }
    let bound = eval_pcntl_bind_args(name, &evaluated)?;
    eval_pcntl_bound_result(name, &bound, PcntlCallMode::Callable, context, values)
}

/// Evaluates an already-bound callable PCNTL invocation by value.
pub(in crate::interpreter) fn eval_pcntl_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated = evaluated_args
        .iter()
        .copied()
        .map(|value| EvaluatedCallArg {
            name: None,
            value,
            ref_target: None,
        })
        .collect::<Vec<_>>();
    let bound = eval_pcntl_bind_args(name, &evaluated)?;
    eval_pcntl_bound_result(name, &bound, PcntlCallMode::Callable, context, values)
}

/// Evaluates dynamic callable arguments while retaining any captured writeback targets.
pub(in crate::interpreter) fn eval_pcntl_evaluated_call(
    name: &str,
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mode = if evaluated_args.iter().any(|arg| arg.ref_target.is_some()) {
        PcntlCallMode::Direct
    } else {
        PcntlCallMode::Callable
    };
    let bound = eval_pcntl_bind_args(name, evaluated_args)?;
    eval_pcntl_bound_result(name, &bound, mode, context, values)
}

/// Binds named and positional arguments into fixed PCNTL parameter slots.
fn eval_pcntl_bind_args(
    name: &str,
    evaluated_args: &[EvaluatedCallArg],
) -> Result<Vec<Option<EvaluatedCallArg>>, EvalStatus> {
    let params = eval_builtin_param_names(name).ok_or(EvalStatus::RuntimeFatal)?;
    let (bound, variadic) = bind_evaluated_ref_builtin_args(params, evaluated_args, false)?;
    if !variadic.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(bound)
}

/// Routes one normalized PCNTL call to its cohesive operation family.
fn eval_pcntl_bound_result(
    name: &str,
    args: &[Option<EvaluatedCallArg>],
    mode: PcntlCallMode,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if name == "pcntl_exec" {
        return eval_pcntl_exec_result(args, values);
    }
    if let Some(result) = eval_pcntl_wait_result(name, args, mode, context, values)? {
        return Ok(result);
    }
    if let Some(result) = eval_pcntl_signal_result(name, args, mode, context, values)? {
        return Ok(result);
    }
    if let Some(result) = eval_pcntl_qos_result(name, args, context, values)? {
        return Ok(result);
    }
    if let Some(result) = eval_pcntl_scalar_result(name, args, context, values)? {
        return Ok(result);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Returns one supplied argument slot by parameter index.
pub(super) fn eval_pcntl_arg(
    args: &[Option<EvaluatedCallArg>],
    index: usize,
) -> Option<&EvaluatedCallArg> {
    args.get(index).and_then(Option::as_ref)
}

/// Returns a required supplied argument or a runtime arity failure.
pub(super) fn eval_pcntl_required_arg(
    args: &[Option<EvaluatedCallArg>],
    index: usize,
) -> Result<&EvaluatedCallArg, EvalStatus> {
    eval_pcntl_arg(args, index).ok_or(EvalStatus::RuntimeFatal)
}

/// Writes one PCNTL output to caller storage or emits callable-by-value degradation.
pub(super) fn eval_pcntl_write_ref(
    function: &str,
    argument_number: usize,
    parameter: &str,
    arg: &EvaluatedCallArg,
    value: RuntimeCellHandle,
    mode: PcntlCallMode,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(target) = arg.ref_target.as_ref() {
        return eval_write_direct_ref_target(
            target,
            value,
            context,
            values,
            Some(ScopeCellOwnership::Owned),
        );
    }
    if mode == PcntlCallMode::Callable {
        return values.warning(&format!(
            "{function}(): Argument #{argument_number} (${parameter}) must be passed by reference, value given"
        ));
    }
    Err(EvalStatus::RuntimeFatal)
}
