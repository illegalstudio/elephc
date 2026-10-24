//! Purpose:
//! Implements PHP `memory_get_usage()` for eval execution.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The generated runtime owns the heap counters; eval reads them through the shared
//!   GC status selector ABI, so eval and compiled code report the same heap.
//! - `$real_usage` selects the adjacent "real" metric (selector + 1).

use super::super::super::*;

/// Runtime selector for live managed-heap bytes (`GcControlOp::MemoryUsage`).
const MEMORY_USAGE: u64 = 14;

eval_builtin! {
    contract: "memory_get_usage",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct `memory_get_usage($real_usage = false)` call.
pub(in crate::interpreter) fn eval_builtin_memory_get_usage(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_memory_metric_result(MEMORY_USAGE, None, values),
        [real_usage] => {
            let real_usage = eval_expr(real_usage, context, scope, values)?;
            eval_memory_metric_result(MEMORY_USAGE, Some(real_usage), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates `memory_get_usage()` from an already materialized argument list.
pub(in crate::interpreter) fn eval_memory_get_usage_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => eval_memory_metric_result(MEMORY_USAGE, None, values),
        [real_usage] => eval_memory_metric_result(MEMORY_USAGE, Some(*real_usage), values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Reads one memory metric, moving to its "real" selector when `$real_usage` is truthy.
pub(in crate::interpreter) fn eval_memory_metric_result(
    base_metric: u64,
    real_usage: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let real = real_usage
        .map(|value| values.truthy(value))
        .transpose()?
        .unwrap_or(false);
    let bytes = values.gc_status_metric(base_metric + u64::from(real))?;
    values.int(bytes)
}
