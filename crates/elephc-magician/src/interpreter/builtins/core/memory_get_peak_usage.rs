//! Purpose:
//! Implements PHP `memory_get_peak_usage()` for eval execution.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The generated runtime owns the heap counters; eval reads them through the shared
//!   GC status selector ABI, so eval and compiled code report the same heap.
//! - `$real_usage` selects the adjacent "real" metric (selector + 1) in the shared helper.

use super::super::super::*;
use super::memory_get_usage::eval_memory_metric_result;

/// Runtime selector for the live-byte high watermark (`GcControlOp::MemoryPeak`).
const MEMORY_PEAK: u64 = 16;

eval_builtin! {
    contract: "memory_get_peak_usage",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct `memory_get_peak_usage($real_usage = false)` call.
pub(in crate::interpreter) fn eval_builtin_memory_get_peak_usage(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_memory_metric_result(MEMORY_PEAK, None, values),
        [real_usage] => {
            let real_usage = eval_expr(real_usage, context, scope, values)?;
            eval_memory_metric_result(MEMORY_PEAK, Some(real_usage), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates `memory_get_peak_usage()` from an already materialized argument list.
pub(in crate::interpreter) fn eval_memory_get_peak_usage_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => eval_memory_metric_result(MEMORY_PEAK, None, values),
        [real_usage] => eval_memory_metric_result(MEMORY_PEAK, Some(*real_usage), values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
