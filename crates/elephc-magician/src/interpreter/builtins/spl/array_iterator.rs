//! Purpose:
//! Implements PHP's `ArrayIterator`, which the eval interpreter knew only as a NAME.
//!
//! Called from:
//! - `eval_new_object_result()` for construction.
//! - `eval_method_call_result_with_evaluated_args()` for every member.
//! - `dynamic_object_is_a()` for the four interfaces it implements.
//!
//! Key details:
//! - The instance's state -- the backing array and the cursor -- lives on the context, because
//!   the runtime object it is built on carries no eval-visible storage. The array is OWNED:
//!   retained on the way in and given back when the object is forgotten.
//! - Symfony's header, parameter and attribute bags all return `new \ArrayIterator($this->…)`
//!   from `getIterator()`, so this is on the request path of any interpreted container.

use super::super::super::*;
pub(in crate::interpreter) use crate::context::eval_array_iterator_class_is_a as eval_array_iterator_is_a;
use super::super::array::iterator_to_array::eval_array_copy_preserve_keys;

/// Constructs one `ArrayIterator` from an array, an object, or nothing.
///
/// `php -n` 8.5.6 accepts all three: an array is used as-is, an object contributes its public
/// properties -- with `Deprecated: ArrayIterator::__construct(): Using an object as a backing
/// array for ArrayIterator is deprecated` -- and no argument at all gives an empty one.
pub(in crate::interpreter) fn eval_array_iterator_new(
    args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let storage = match args.first() {
        None => values.assoc_new(0)?,
        Some(arg) => match values.type_tag(arg.value)? {
            EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => values.retain(arg.value)?,
            EVAL_TAG_OBJECT => {
                values.warning(
                    "ArrayIterator::__construct(): Using an object as a backing array for \
                     ArrayIterator is deprecated, as it allows violating class constraints and \
                     invariants",
                )?;
                eval_object_array_cast_value(arg.value, context, values)?
            }
            EVAL_TAG_NULL => values.assoc_new(0)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
    };
    let object = values.new_object("ArrayIterator")?;
    let identity = values.object_identity(object)?;
    if let Some(replaced) = context.set_array_iterator(identity, storage) {
        eval_release_value(context, values, replaced)?;
    }
    eval_array_iterator_mirror_into_compiled_class(object, storage, context, values)?;
    Ok(object)
}

/// Gives a compiled `ArrayIterator` class the same initial array this instance was built with.
///
/// `new_object` allocates the program's COMPILED `ArrayIterator` when it has one, but its
/// constructor never ran, so its storage properties were uninitialized. The state above lives
/// on this context, where compiled code cannot see it: an instance an interpreted
/// `getIterator()` returned to a compiled `foreach` or `iterator_to_array()` read the compiled
/// `$values` and died on "must not be accessed before initialization". Running the compiled
/// constructor with the same array keeps the interpreter's own implementation -- which serves
/// every method, including ones the program never compiled -- while the compiled one sees the
/// array it was built from. A mutation made through the interpreter after construction is not
/// mirrored; returning a freshly built iterator is the shape this serves.
fn eval_array_iterator_mirror_into_compiled_class(
    object: RuntimeCellHandle,
    storage: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if eval_aot_method_dispatch_metadata_in_hierarchy("ArrayIterator", "__construct", context, values)?
        .is_none()
    {
        return Ok(());
    }
    eval_native_constructor_with_evaluated_args(
        "ArrayIterator",
        object,
        positional_args(vec![storage]),
        context,
        values,
    )
}

/// Returns whether one identity is a live `ArrayIterator`.
pub(in crate::interpreter) fn eval_object_is_array_iterator(
    identity: u64,
    context: &ElephcEvalContext,
) -> bool {
    context.array_iterator_storage(identity).is_some()
}

/// Dispatches one `ArrayIterator` member, or reports that the name is not one of them.
pub(in crate::interpreter) fn eval_array_iterator_method_result(
    identity: u64,
    method_name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(storage) = context.array_iterator_storage(identity) else {
        return Ok(None);
    };
    let position = context.array_iterator_position(identity).unwrap_or(0);
    let result = match method_name.to_ascii_lowercase().as_str() {
        "rewind" => {
            context.set_array_iterator_position(identity, 0);
            values.null()?
        }
        "valid" => {
            let len = values.array_len(storage)?;
            values.bool_value(position < len)?
        }
        "next" => {
            context.set_array_iterator_position(identity, position + 1);
            values.null()?
        }
        "key" => eval_array_iterator_key(storage, position, values)?,
        "current" => {
            let key = eval_array_iterator_key(storage, position, values)?;
            if values.is_null(key)? {
                values.release(key)?;
                values.null()?
            } else {
                let value = values.array_get(storage, key)?;
                values.release(key)?;
                value
            }
        }
        "count" => {
            let len = values.array_len(storage)?;
            values.int(len as i64)?
        }
        "getarraycopy" => eval_array_copy_preserve_keys(storage, values)?,
        "offsetget" => {
            let [key] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.array_get(storage, *key)?
        }
        "offsetexists" => {
            let [key] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.array_key_exists(*key, storage)?
        }
        "offsetset" => {
            let (key, value) = match evaluated_args {
                [key, value] => (*key, *value),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            // PHP's `$it[] = $v` passes null for the key, which appends.
            let key = if values.is_null(key)? {
                eval_array_append_key(storage, values)?
            } else {
                values.retain(key)?
            };
            let value = values.retain(value)?;
            let updated = values.array_set(storage, key, value)?;
            eval_array_iterator_store(identity, storage, updated, context, values)?;
            values.null()?
        }
        "offsetunset" => {
            let [key] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let updated = eval_array_iterator_without_key(storage, *key, values)?;
            eval_array_iterator_store(identity, storage, updated, context, values)?;
            values.null()?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// Replaces the backing array when a write produced a different cell, keeping ownership straight.
fn eval_array_iterator_store(
    identity: u64,
    previous: RuntimeCellHandle,
    updated: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if updated == previous {
        return Ok(());
    }
    let updated = values.retain(updated)?;
    if let Some(replaced) = context.replace_array_iterator_storage(identity, updated) {
        eval_release_value(context, values, replaced)?;
    }
    Ok(())
}

/// Returns the key at one cursor position, or null past the end.
fn eval_array_iterator_key(
    storage: RuntimeCellHandle,
    position: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if position >= values.array_len(storage)? {
        return values.null();
    }
    values.array_iter_key(storage, position)
}

/// Builds a copy of one array without a given key, which is how `offsetUnset` removes it.
fn eval_array_iterator_without_key(
    storage: RuntimeCellHandle,
    removed: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(storage)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(storage, position)?;
        let matches = values.compare(EvalBinOp::StrictEq, key, removed)?;
        let drop_it = values.truthy(matches)?;
        values.release(matches)?;
        if drop_it {
            values.release(key)?;
            continue;
        }
        let value = values.array_get(storage, key)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

