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
    eval_array_append_value_with_type(array, expr, None, context, scope, values)
}

/// Validates an explicitly typed local append at the same operand and index boundary.
pub(super) fn eval_array_append_value_with_type(
    array: RuntimeCellHandle, expr: &EvalExpr, declared: Option<&EvalParameterType>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_owned_expr(expr, context, scope, values)?;
    let declared = declared.cloned().into_iter().collect::<Vec<_>>();
    let inserted = eval_array_append_owned_value_with_type(array, value, &declared, context, values);
    let cleanup = values.release(value);
    inserted.and_then(|result| cleanup.map(|()| result))
}

/// Inserts an already evaluated RHS without taking its owner, releasing the automatic key.
pub(super) fn eval_array_append_owned_value_with_type(
    array: RuntimeCellHandle, value: RuntimeCellHandle, declared: &[EvalParameterType],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(index) = values.array_next_index(array)? else {
        return eval_throw_error(
            "Cannot add element to the array as the next element is already occupied",
            context, values,
        );
    };
    let index = values.int(index)?;
    let inserted = (|| {
        for ty in context.local_reference_types(array) {
            validate_generic_local_element(Some(&ty), array, index, value, context, values)?;
        }
        for ty in declared { validate_generic_local_element(Some(ty), array, index, value, context, values)?; }
        values.array_set(array, index, value)
    })();
    let cleanup = values.release(index);
    inserted.and_then(|result| cleanup.map(|()| result))
}
