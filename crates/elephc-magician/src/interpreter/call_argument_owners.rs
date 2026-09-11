//! Purpose:
//! Keeps unpacked call arguments alive through invocation and reference writeback.
//! Releases their independent owners at the call boundary, including failures.
//!
//! Called from:
//! - Callable dispatch when preparing an argument array.
//!
//! Key details:
//! - A copied array value stabilizes traversal while preserving PHP references.
//! - Reference owners must not survive the call and alter later COW decisions.

use super::*;

/// Invokes a callable with owned array reads and releases every temporary after writeback.
pub(in crate::interpreter) fn with_array_call_arguments<V: RuntimeValueOps>(
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut V,
    invoke: impl FnOnce(Vec<EvaluatedCallArg>, &mut ElephcEvalContext, &mut V) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_array_call_argument_result(
        array,
        context,
        values,
        invoke,
        |value, values| {
            let value = promote_borrowed_result(value, values)?;
            Ok((value, Some(value)))
        },
        |value, context, values| eval_release_value(context, values, value),
    )
}

/// Keeps argument owners through an optional reflection result and releases an escaped object on cleanup failure.
pub(in crate::interpreter) fn with_optional_array_call_arguments<V: RuntimeValueOps>(
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut V,
    invoke: impl FnOnce(Vec<EvaluatedCallArg>, &mut ElephcEvalContext, &mut V) -> Result<Option<RuntimeCellHandle>, EvalStatus>,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    with_array_call_argument_result(
        array,
        context,
        values,
        invoke,
        |result, values| {
            let result = result
                .map(|value| promote_borrowed_result(value, values))
                .transpose()?;
            Ok((result, result))
        },
        |result, context, values| match result {
            Some(value) => eval_release_value(context, values, value),
            None => Ok(()),
        },
    )
}

/// Shares argument acquisition and cleanup across ordinary and optional invocation results.
fn with_array_call_argument_result<R, V: RuntimeValueOps>(
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut V,
    invoke: impl FnOnce(Vec<EvaluatedCallArg>, &mut ElephcEvalContext, &mut V) -> Result<R, EvalStatus>,
    preserve_result: impl FnOnce(
        R,
        &mut V,
    ) -> Result<(R, Option<RuntimeCellHandle>), EvalStatus>,
    release_result: impl FnOnce(R, &mut ElephcEvalContext, &mut V) -> Result<(), EvalStatus>,
) -> Result<R, EvalStatus> {
    if !values.is_array_like(array)? { return Err(EvalStatus::RuntimeFatal); }
    let copy = values.copy_value(array)?;
    context.copy_array_metadata(array, copy);
    let mut owners = vec![copy];
    let mut arguments = Vec::new();
    let mut saw_named = false;
    let mut preserved = None;
    let result = append_unpacked_call_arg_values_with_owners(
        copy, &mut arguments, &mut saw_named, context, values, Some(&mut owners),
    )
    .and_then(|()| {
        let arguments = arguments
            .into_iter()
            .map(|argument| EvaluatedCallArg {
                value: argument.value.borrowed(),
                ..argument
            })
            .collect();
        invoke(arguments, context, values)
    })
    .and_then(|result| {
        let (result, value) = preserve_result(result, values)?;
        preserved = value;
        Ok(result)
    });
    if preserved.map_or(true, |value| value.as_ptr() != copy.as_ptr()) {
        context.clear_array_metadata(copy);
    }
    let mut cleanup = Ok(());
    for value in owners.into_iter().rev() {
        if let Err(status) = eval_release_value(context, values, value) { cleanup = Err(status); }
    }
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = release_result(value, context, values);
            Err(status)
        }
    }
}

/// Appends one unpacked array's values using PHP named-argument key semantics.
pub(in crate::interpreter) fn append_unpacked_call_arg_values(
    array: RuntimeCellHandle,
    evaluated_args: &mut Vec<EvaluatedCallArg>,
    saw_named: &mut bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    append_unpacked_call_arg_values_with_owners(array, evaluated_args, saw_named, context, values, None)
}

/// Expands PHP array arguments while optionally tracking reference-preserving read owners.
pub(in crate::interpreter) fn append_unpacked_call_arg_values_with_owners(
    array: RuntimeCellHandle,
    evaluated_args: &mut Vec<EvaluatedCallArg>,
    saw_named: &mut bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    owners: Option<&mut Vec<RuntimeCellHandle>>,
) -> Result<(), EvalStatus> {
    append_unpacked_arguments(array, evaluated_args, saw_named, context, values, owners, true)
}

/// Captures unpacked builtin values without keeping persistent references as mutable argument values.
pub(in crate::interpreter) fn append_unpacked_value_call_args(
    array: RuntimeCellHandle, evaluated_args: &mut Vec<EvaluatedCallArg>, saw_named: &mut bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps, owners: &mut Vec<RuntimeCellHandle>,
) -> Result<(), EvalStatus> {
    append_unpacked_arguments(array, evaluated_args, saw_named, context, values, Some(owners), false)
}

/// Shares array key order and marker handling while selecting reference-preserving or value reads.
fn append_unpacked_arguments(
    array: RuntimeCellHandle, evaluated_args: &mut Vec<EvaluatedCallArg>, saw_named: &mut bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
    mut owners: Option<&mut Vec<RuntimeCellHandle>>, preserve_references: bool,
) -> Result<(), EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let ref_target = eval_array_reference_key(key, values)?
            .and_then(|key| context.array_element_alias(array, &key).cloned());
        let arg = match values.type_tag(key)? {
            EVAL_TAG_INT => {
                if *saw_named {
                    values.release(key)?;
                    return Err(EvalStatus::RuntimeFatal);
                }
                let read = if owners.is_some() && preserve_references {
                    values.array_get_preserving_references(array, key)
                } else {
                    values.array_get(array, key)
                };
                let value = match read {
                    Ok(value) => value,
                    Err(status) => {
                        values.release(key)?;
                        return Err(status);
                    }
                };
                if let Some(owners) = owners.as_deref_mut() { owners.push(value); }
                let original = value;
                let (value, ref_target) =
                    eval_invoker_ref_arg_value_and_target(value, ref_target, values)?;
                if value != original {
                    if let Some(owners) = owners.as_deref_mut() { owners.push(value); }
                }
                EvaluatedCallArg {
                    name: None,
                    value,
                    ref_target,
                }
            }
            EVAL_TAG_STRING => {
                *saw_named = true;
                let name = values.string_bytes(key)?;
                let name = match String::from_utf8(name) {
                    Ok(name) => name,
                    Err(_) => {
                        values.release(key)?;
                        return Err(EvalStatus::RuntimeFatal);
                    }
                };
                let read = if owners.is_some() && preserve_references {
                    values.array_get_preserving_references(array, key)
                } else {
                    values.array_get(array, key)
                };
                let value = match read {
                    Ok(value) => value,
                    Err(status) => {
                        values.release(key)?;
                        return Err(status);
                    }
                };
                if let Some(owners) = owners.as_deref_mut() { owners.push(value); }
                let original = value;
                let (value, ref_target) =
                    eval_invoker_ref_arg_value_and_target(value, ref_target, values)?;
                if value != original {
                    if let Some(owners) = owners.as_deref_mut() { owners.push(value); }
                }
                EvaluatedCallArg {
                    name: Some(name),
                    value,
                    ref_target,
                }
            }
            _ => {
                values.release(key)?;
                return Err(EvalStatus::RuntimeFatal);
            }
        };
        values.release(key)?;
        let arg = if preserve_references { arg } else { EvaluatedCallArg { ref_target: None, ..arg } };
        evaluated_args.push(arg);
    }
    Ok(())
}

/// Converts a descriptor-invoker ref marker into an eval-visible value and writeback target.
fn eval_invoker_ref_arg_value_and_target(
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    if values.is_reference(value)? {
        return Ok((value, ref_target.or(Some(EvalReferenceTarget::Cell { cell: value }))));
    }
    if values.type_tag(value)? != EVAL_TAG_INVOKER_REF_CELL {
        return Ok((value, ref_target));
    }
    let slot = values.raw_value_word(value)? as usize;
    let source_tag = values.raw_value_high_word(value)?;
    let value = eval_invoker_ref_slot_value(slot, source_tag, values)?;
    Ok((
        value,
        ref_target.or(Some(EvalReferenceTarget::InvokerSlot { slot, source_tag })),
    ))
}

/// Reads the current PHP value from a native descriptor-invoker by-reference slot.
fn eval_invoker_ref_slot_value(
    slot: usize,
    source_tag: u64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match source_tag {
        EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_BOOL | EVAL_TAG_RESOURCE => {
            let word = unsafe { *(slot as *const u64) };
            values.raw_word_value(source_tag, word)
        }
        EVAL_TAG_STRING => {
            let words = unsafe { *(slot as *const [u64; 2]) };
            values.raw_string_value(words[0], words[1])
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE => {
            let word = unsafe { *(slot as *const u64) };
            values.raw_word_value(source_tag, word)
        }
        EVAL_TAG_MIXED => {
            let value = RuntimeCellHandle::from_raw(unsafe {
                *(slot as *const *mut crate::value::RuntimeCell)
            });
            values.retain(value)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
