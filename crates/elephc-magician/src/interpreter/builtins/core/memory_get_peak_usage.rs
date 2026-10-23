//! Purpose:
//! Eval registry entry and implementation for `memory_get_peak_usage`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Like `memory_get_usage`, eval answers `0`: the interpreter has no peak-footprint
//!   counter. The compiled AOT path reads the runtime `_gc_peak` high-water counter.

use super::super::super::*;

eval_builtin! {
    contract: "memory_get_peak_usage",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct `memory_get_peak_usage(?bool $real_usage = false)` call.
pub(in crate::interpreter) fn eval_builtin_memory_get_peak_usage(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_memory_get_peak_usage_result(values)
}

/// Evaluates `memory_get_peak_usage()` from an already materialized argument list.
pub(in crate::interpreter) fn eval_memory_get_peak_usage_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_memory_get_peak_usage_result(values)
}

/// Returns the eval interpreter's peak memory footprint, which this build reports as `0`.
pub(in crate::interpreter) fn eval_memory_get_peak_usage_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(0)
}
