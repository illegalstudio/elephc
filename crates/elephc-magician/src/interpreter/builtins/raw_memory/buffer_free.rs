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
    let buffer = eval_expr(buffer, context, scope, values)?;
    eval_buffer_free_result(buffer, values)
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
    let value = eval_expr(&EvalExpr::LoadVar(variable.to_string()), context, scope, values)?;
    super::buffer_registry::eval_buffer_release(value, values)?;
    let null = values.null()?;
    for replaced in scope.set_respecting_references(
        variable.to_string(),
        null,
        ScopeCellOwnership::Owned,
    ) {
        values.release(replaced)?;
    }
    values.null()
}
