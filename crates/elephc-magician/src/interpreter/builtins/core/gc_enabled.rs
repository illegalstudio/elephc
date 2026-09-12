//! Purpose:
//! Implements PHP `gc_enabled()` for eval execution.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The query reads the same generated-runtime flag used by AOT automatic safe points.

use super::super::super::*;

eval_builtin! {
    contract: "gc_enabled",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct zero-argument `gc_enabled()` call.
pub(in crate::interpreter) fn eval_builtin_gc_enabled(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_gc_enabled_result(values)
}

/// Evaluates `gc_enabled()` from an already materialized empty argument list.
pub(in crate::interpreter) fn eval_gc_enabled_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_gc_enabled_result(values)
}

/// Boxes the shared runtime's automatic-collection enabled state.
fn eval_gc_enabled_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let enabled = values.gc_enabled()?;
    values.bool_value(enabled)
}
