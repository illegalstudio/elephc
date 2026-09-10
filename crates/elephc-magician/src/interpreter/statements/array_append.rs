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

/// Owns an unset key and replaces numeric-string temporaries with their normalized integer key.
pub(super) fn eval_owned_unset_index(
    expr: &EvalExpr, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = eval_owned_expr(expr, context, scope, values)?;
    let normalized = (|| {
        if values.type_tag(key)? == EVAL_TAG_STRING {
            let bytes = values.string_bytes(key)?;
            if let Some(index) = eval_numeric_string_array_key(&bytes) {
                return values.int(index);
            }
        }
        Ok(key)
    })();
    if normalized.as_ref().is_ok_and(|result| *result == key) { return normalized; }
    let cleanup = values.release(key);
    match (normalized, cleanup) {
        (Ok(result), Err(status)) => { let _ = values.release(result); Err(status) }
        (Ok(result), Ok(())) => Ok(result),
        (Err(status), _) => Err(status),
    }
}

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
