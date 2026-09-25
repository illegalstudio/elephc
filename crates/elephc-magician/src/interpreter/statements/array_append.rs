//! Purpose:
//! Evaluates append values and consumes host-owned automatic array-index decisions.
//!
//! Called from:
//! - Variable, instance-property, and static-property array append statements.
//!
//! Key details:
//! - RHS evaluation precedes index selection and exhaustion diagnostics.
//! - Every temporary key and value is released after insertion or failure.

use super::*;

/// Appends an owned RHS through the host's persistent index, preserving evaluation and cleanup order.
pub(super) fn eval_array_append_value(
    array: RuntimeCellHandle,
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_owned_expr(expr, context, scope, values)?;
    let inserted = (|| {
        let Some(index) = values.array_next_index(array)? else {
            return eval_throw_error(
                "Cannot add element to the array as the next element is already occupied",
                context, values,
            );
        };
        let index = values.int(index)?;
        let inserted = values.array_set(array, index, value);
        let cleanup = values.release(index);
        inserted.and_then(|result| cleanup.map(|()| result))
    })();
    let cleanup = values.release(value);
    inserted.and_then(|result| cleanup.map(|()| result))
}
