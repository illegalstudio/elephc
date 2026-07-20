//! Purpose:
//! Eval registry entry and implementation for `ptr_null`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Null raw pointers are represented as the integer address zero.

use super::super::super::*;


eval_builtin! {
    name: "ptr_null",
    area: RawMemory,
    params: [],
    direct: PtrNull,
    values: PtrNull,
}

/// Evaluates PHP `ptr_null()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_ptr_null(
    args: &[EvalExpr],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_ptr_null_result(values)
}

/// Dispatches by-value `ptr_null()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_null_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_ptr_null_result(values)
}

/// Returns the raw null pointer address.
fn eval_ptr_null_result(values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(0)
}
