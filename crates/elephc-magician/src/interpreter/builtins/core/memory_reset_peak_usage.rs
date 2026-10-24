//! Purpose:
//! Implements PHP `memory_reset_peak_usage()` for eval execution.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The generated runtime resets both peak watermarks to current usage.

use super::super::super::*;

/// Runtime selector that resets the peak watermarks (`GcControlOp::MemoryResetPeak`).
const MEMORY_RESET_PEAK: u64 = 18;

eval_builtin! {
    contract: "memory_reset_peak_usage",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct zero-argument `memory_reset_peak_usage()` call.
pub(in crate::interpreter) fn eval_builtin_memory_reset_peak_usage(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_memory_reset_peak_usage_result(values)
}

/// Evaluates `memory_reset_peak_usage()` from an already materialized empty argument list.
pub(in crate::interpreter) fn eval_memory_reset_peak_usage_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_memory_reset_peak_usage_result(values)
}

/// Resets the peak counters and returns PHP null.
fn eval_memory_reset_peak_usage_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.gc_status_metric(MEMORY_RESET_PEAK)?;
    values.null()
}
