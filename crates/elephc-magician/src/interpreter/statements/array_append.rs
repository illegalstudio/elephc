//! Purpose:
//! Inserts prepared append values using host-owned automatic array-index decisions.
//!
//! Called from:
//! - Variable, instance-property, and static-property array append statements.
//!
//! Key details:
//! - RHS evaluation precedes index selection and exhaustion diagnostics.
//! - Every temporary key and value is released after insertion or failure.

use super::*;

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
