//! Purpose:
//! Retires temporary metadata keys and values after retaining array insertion.
//!
//! Called from:
//! - Reflection owner and attribute metadata materialization.
//!
//! Key details:
//! - Runtime array_set borrows all inputs and transfers no additional receiver owner.
//! - Failure retires both operands and the partially populated temporary array.

use super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

/// Inserts owned temporary operands, releasing their original owners on every outcome.
pub(in crate::interpreter) fn insert_owned_element(
    values: &mut impl RuntimeValueOps,
    array: RuntimeCellHandle,
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::from_owned(values, array);
    result.entry(|_| Ok(value), |_, _| Ok(key))?;
    Ok(result.finish())
}
