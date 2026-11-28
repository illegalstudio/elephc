//! Purpose:
//! Eval registry entry and implementation for `buffer_free`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Direct calls stay source-sensitive so a local buffer variable can be nulled.
//! - Freeing the nulled local again is a no-op, while a stale alias dies with the
//!   use-after-free fatal, matching compiled `buffer_free()`.

use super::super::super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

eval_builtin! {
    contract: "buffer_free",
    area: RawMemory,
    source_arguments: true,
    direct: BufferFree,
    values: BufferFree,
}

/// Evaluates PHP `buffer_free()` and nulls direct local variables when possible.
pub(in crate::interpreter) fn eval_builtin_buffer_free(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let [EvalExpr::LoadVar(variable)] = args {
        return eval_buffer_free_direct_variable(variable, context, scope, values);
    }
    let [buffer] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let buffer = RuntimeOwnerSlot::new(eval_expr(buffer, context, scope, values)?);
    let result = with_runtime_owners(&buffer, || {
        eval_buffer_free_result(buffer.borrowed(), values)
    });
    finish_eval_argument_values(result, [buffer.take()], context, values)
}

/// Dispatches by-value `buffer_free()` calls after argument binding.
pub(in crate::interpreter) fn eval_buffer_free_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [buffer] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_buffer_free_result(*buffer, values)
}

/// Frees an eval buffer through the registry and returns PHP null.
fn eval_buffer_free_result(
    buffer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::buffer_registry::eval_buffer_release(buffer, values)?;
    values.null()
}

/// Frees a local buffer variable and replaces the source variable with null.
fn eval_buffer_free_direct_variable(
    variable: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = RuntimeOwnerSlot::new(eval_expr(
        &EvalExpr::LoadVar(variable.to_string()),
        context,
        scope,
        values,
    )?);
    with_runtime_owners(&value, || {
        super::buffer_registry::eval_buffer_release(value.borrowed(), values)?;
        let null = values.null()?;
        let replaced = scope.set_respecting_references(
            variable.to_string(),
            null,
            ScopeCellOwnership::Owned,
        );
        crate::interpreter::scope_cells::retire_replaced_scope_cells(
            replaced, context, values,
        )?;
        let result = values.null();
        finish_eval_argument_values(result, [value.take()], context, values)
    })
}
