//! Purpose:
//! Implements PHP `gc_status()` for eval execution.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The result exposes the PHP 8 twelve-field status shape with runtime-backed counters.
//! - Timing values are zero because the elephc collector does not instrument wall-clock phases.

use super::super::super::*;

const GC_STATUS_RUNNING: u64 = 5;
const GC_STATUS_PROTECTED: u64 = 6;
const GC_STATUS_RUNS: u64 = 7;
const GC_STATUS_COLLECTED: u64 = 8;
const GC_STATUS_ROOTS: u64 = 9;

eval_builtin! {
    contract: "gc_status",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct zero-argument `gc_status()` call.
pub(in crate::interpreter) fn eval_builtin_gc_status(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_gc_status_result(values)
}

/// Evaluates `gc_status()` from an already materialized empty argument list.
pub(in crate::interpreter) fn eval_gc_status_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_gc_status_result(values)
}

/// Builds the complete PHP 8 collector status associative array.
fn eval_gc_status_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(12)?;
    let running = values.gc_status_metric(GC_STATUS_RUNNING)? != 0;
    let value = values.bool_value(running)?;
    result = set_status_entry(result, "running", value, values)?;
    let protected = values.gc_status_metric(GC_STATUS_PROTECTED)? != 0;
    let value = values.bool_value(protected)?;
    result = set_status_entry(result, "protected", value, values)?;
    let value = values.bool_value(false)?;
    result = set_status_entry(result, "full", value, values)?;
    let runs = values.gc_status_metric(GC_STATUS_RUNS)?;
    let value = values.int(runs)?;
    result = set_status_entry(result, "runs", value, values)?;
    let collected = values.gc_status_metric(GC_STATUS_COLLECTED)?;
    let value = values.int(collected)?;
    result = set_status_entry(result, "collected", value, values)?;
    let value = values.int(10_001)?;
    result = set_status_entry(result, "threshold", value, values)?;
    let value = values.int(16_384)?;
    result = set_status_entry(result, "buffer_size", value, values)?;
    let roots = values.gc_status_metric(GC_STATUS_ROOTS)?;
    let value = values.int(roots)?;
    result = set_status_entry(result, "roots", value, values)?;
    for key in [
        "application_time",
        "collector_time",
        "destructor_time",
        "free_time",
    ] {
        let value = values.float(0.0)?;
        result = set_status_entry(result, key, value, values)?;
    }
    Ok(result)
}

/// Inserts one already boxed metric under a string key.
fn set_status_entry(
    result: RuntimeCellHandle,
    key: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    values.array_set(result, key, value)
}
