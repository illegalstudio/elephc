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
use crate::interpreter::builtins::collection_builder::{
    with_collection_cells, with_collection_result,
};

/// Inserts an already evaluated RHS without taking its owner, releasing the automatic key.
pub(super) fn eval_array_append_owned_value_with_type(
    array: RuntimeCellHandle, value: RuntimeCellHandle, declared: &[EvalParameterType],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_collection_result(values, |values, result| {
        with_collection_cells(values, |values, owners| {
            let index = owners.stage(eval_array_append_index(array, context, values)?);
            for ty in context.local_reference_types(array) {
                validate_generic_local_element(
                    Some(&ty),
                    array,
                    index.borrowed(),
                    value,
                    context,
                    values,
                )?;
            }
            for ty in declared {
                validate_generic_local_element(
                    Some(ty),
                    array,
                    index.borrowed(),
                    value,
                    context,
                    values,
                )?;
            }
            result.replace(Some(values.array_set(
                array,
                index.borrowed(),
                value,
            )?));
            Ok(())
        })
    })
}

/// Selects an owned automatic key from host history and raises PHP's exhaustion Error.
pub(super) fn eval_array_append_index(
    array: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(index) = values.array_next_index(array)? else {
        return eval_throw_error(
            "Cannot add element to the array as the next element is already occupied",
            context, values,
        );
    };
    values.int(index)
}
