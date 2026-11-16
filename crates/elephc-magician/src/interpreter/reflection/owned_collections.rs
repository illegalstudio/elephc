//! Purpose:
//! Retires temporary metadata keys and values after retaining array insertion.
//!
//! Called from:
//! - Reflection owner and attribute metadata materialization.
//!
//! Key details:
//! - Runtime array_set and object_array_push borrow all inputs and transfer no additional
//!   receiver owner.
//! - Failure retires both operands and the partially populated temporary array.
//! - Reflector lists (`getMethods()`, `getParameters()`, ...) are direct object arrays, the
//!   storage compiled `array<Class>` readers expect; metadata maps stay boxed Mixed arrays.

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

/// Inserts one owned value under a key the context owns (a borrowed handle), retaining the
/// value for the array and releasing the temporary key on every outcome.
///
/// A borrowed value is the context's own cell (an eval class constant or enum case); the array
/// retains it and nothing is released. An owned value (a fresh cell from the native catalog)
/// is released after insertion, so the array holds its only remaining owner.
pub(in crate::interpreter) fn insert_retained_element(
    values: &mut impl RuntimeValueOps,
    array: RuntimeCellHandle,
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = if value.is_borrowed() {
        match values.retain(value) {
            Ok(owned) => owned,
            Err(status) => {
                let _ = values.release(key);
                let _ = values.release(array);
                return Err(status);
            }
        }
    } else {
        value
    };
    insert_owned_element(values, array, key, value)
}

/// Appends one owned reflector object to a direct object array, releasing the caller's owner
/// on every outcome; failure also retires the partially populated array.
pub(in crate::interpreter) fn push_owned_object(
    values: &mut impl RuntimeValueOps,
    array: RuntimeCellHandle,
    object: RuntimeCellHandle,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let pushed = values.object_array_push(array, object);
    let released = values.release(object);
    match pushed {
        Ok(array) => {
            released?;
            Ok(array)
        }
        Err(status) => {
            let _ = released;
            let _ = values.release(array);
            Err(status)
        }
    }
}
