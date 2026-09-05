//! Purpose:
//! Implements PHP `gc_collect_cycles()` for eval execution.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The runtime hook bypasses the automatic-collection enabled flag, matching PHP.

use super::super::super::*;

eval_builtin! {
    contract: "gc_collect_cycles",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct zero-argument `gc_collect_cycles()` call.
pub(in crate::interpreter) fn eval_builtin_gc_collect_cycles(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_gc_collect_cycles_result(values)
}

/// Evaluates `gc_collect_cycles()` from an already materialized empty argument list.
pub(in crate::interpreter) fn eval_gc_collect_cycles_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_gc_collect_cycles_result(values)
}

/// Runs the shared runtime collector and boxes its reclaimed-node count.
fn eval_gc_collect_cycles_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let collected = values.gc_collect_cycles()?;
    values.int(collected)
}
