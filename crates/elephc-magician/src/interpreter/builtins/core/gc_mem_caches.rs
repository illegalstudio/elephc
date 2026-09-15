//! Purpose:
//! Implements PHP `gc_mem_caches()` for eval execution.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The generated runtime owns the allocator-cache policy and byte count.

use super::super::super::*;

eval_builtin! {
    contract: "gc_mem_caches",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct zero-argument `gc_mem_caches()` call.
pub(in crate::interpreter) fn eval_builtin_gc_mem_caches(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_gc_mem_caches_result(values)
}

/// Evaluates `gc_mem_caches()` from an already materialized empty argument list.
pub(in crate::interpreter) fn eval_gc_mem_caches_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_gc_mem_caches_result(values)
}

/// Flushes allocator caches and boxes the reclaimed-byte count.
fn eval_gc_mem_caches_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let reclaimed = values.gc_mem_caches()?;
    values.int(reclaimed)
}
