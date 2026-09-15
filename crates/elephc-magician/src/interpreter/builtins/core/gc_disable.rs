//! Purpose:
//! Implements PHP `gc_disable()` for eval execution.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Disabling affects generated-runtime automatic safe points shared with AOT code.

use super::super::super::*;

eval_builtin! {
    contract: "gc_disable",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct zero-argument `gc_disable()` call.
pub(in crate::interpreter) fn eval_builtin_gc_disable(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_gc_disable_result(values)
}

/// Evaluates `gc_disable()` from an already materialized empty argument list.
pub(in crate::interpreter) fn eval_gc_disable_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_gc_disable_result(values)
}

/// Disables automatic collection and returns the boxed void value used by eval.
fn eval_gc_disable_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.gc_disable()?;
    values.null()
}
