//! Purpose:
//! Eval registry entry and implementation for `buffer_len`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Resolves the handle through the eval buffer registry, so a freed buffer or a stale
//!   alias dies with the compiled-code use-after-free fatal instead of reading freed memory.

use super::super::super::*;


eval_builtin! {
    contract: "buffer_len",
    area: RawMemory,
    direct: BufferLen,
    values: BufferLen,
}

/// Evaluates PHP `buffer_len()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_buffer_len(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [buffer] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let buffer = eval_expr(buffer, context, scope, values)?;
    eval_buffer_len_result(buffer, values)
}

/// Dispatches by-value `buffer_len()` calls after argument binding.
pub(in crate::interpreter) fn eval_buffer_len_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [buffer] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_buffer_len_result(*buffer, values)
}

/// Answers the logical element count of a live eval buffer.
fn eval_buffer_len_result(
    buffer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let length = super::buffer_registry::eval_buffer_resolve(buffer, values)?.length;
    values.int(i64::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?)
}
