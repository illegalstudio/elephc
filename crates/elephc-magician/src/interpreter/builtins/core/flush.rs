//! Purpose:
//! Implements PHP `flush()` for eval execution.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - A no-op, as in AOT code: elephc's writes are unbuffered, so there is no system write
//!   buffer to flush, and `flush()` never touches the `ob_*` buffers.

use super::super::super::*;

eval_builtin! {
    contract: "flush",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct zero-argument `flush()` call.
pub(in crate::interpreter) fn eval_builtin_flush(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.null()
}

/// Evaluates `flush()` from an already materialized empty argument list.
pub(in crate::interpreter) fn eval_flush_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.null()
}
