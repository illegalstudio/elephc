//! Purpose:
//! Implements PHP `gc_status()` for eval execution.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The result exposes PHP's twelve-field shape with live counters, roots, and phase timings.
//! - Unsupported collector-buffer fields retain documented literal values.

use super::super::super::*;
use super::super::collection_builder::EvalArrayBuilder;

const GC_STATUS_RUNNING: u64 = 5;
const GC_STATUS_PROTECTED: u64 = 6;
const GC_STATUS_RUNS: u64 = 7;
const GC_STATUS_COLLECTED: u64 = 8;
const GC_STATUS_ROOTS: u64 = 9;
const GC_STATUS_APPLICATION_TIME: u64 = 10;
const GC_STATUS_COLLECTOR_TIME: u64 = 11;
const GC_STATUS_DESTRUCTOR_TIME: u64 = 12;
const GC_STATUS_FREE_TIME: u64 = 13;

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

/// Builds the PHP-shaped collector status associative array.
fn eval_gc_status_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::assoc(values, 12)?;
    for (key, metric) in [("running", GC_STATUS_RUNNING), ("protected", GC_STATUS_PROTECTED)] {
        result.string(key, |values| {
            let value = values.gc_status_metric(metric)? != 0;
            values.bool_value(value)
        })?;
    }
    result.string("full", |values| values.bool_value(false))?;
    for (key, metric) in [
        ("runs", Some(GC_STATUS_RUNS)), ("collected", Some(GC_STATUS_COLLECTED)),
        ("threshold", None), ("buffer_size", None), ("roots", Some(GC_STATUS_ROOTS)),
    ] {
        result.string(key, |values| {
            let value = match metric { Some(metric) => values.gc_status_metric(metric)?, None => 0 };
            values.int(value)
        })?;
    }
    for (key, metric) in [
        ("application_time", GC_STATUS_APPLICATION_TIME),
        ("collector_time", GC_STATUS_COLLECTOR_TIME),
        ("destructor_time", GC_STATUS_DESTRUCTOR_TIME),
        ("free_time", GC_STATUS_FREE_TIME),
    ] {
        result.string(key, |values| {
            let seconds = values.gc_status_time(metric)?;
            values.float(seconds)
        })?;
    }
    Ok(result.finish())
}
