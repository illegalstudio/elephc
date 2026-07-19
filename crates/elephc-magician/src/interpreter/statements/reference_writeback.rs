//! Purpose:
//! Binds and writes back method by-reference targets across runtime storage shapes.
//!
//! Called from:
//! - Eval and native method invocation after argument binding.
//!
//! Key details:
//! - Invoker slots, arrays, nested arrays, object properties, and static properties preserve aliases.

use super::*;

/// Returns the calling-scope class when PHP's private-method shadowing rule
/// selects it as the dispatch scope: `$obj->m()` from class scope S calls S's
/// own private instance method `m` — never the receiver's override — when S
/// declares one and the receiver is an instance of S. The returned scope
/// string drives the native bridge's hidden private shadow slot directly.
pub(in crate::interpreter) fn eval_private_scope_shadow_bridge_scope(
    receiver_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some(scope_class) = context.current_class_scope() else {
        return Ok(None);
    };
    let scope_class = scope_class.to_string();
    if !same_eval_class_name(receiver_class, &scope_class)
        && !native_class_is_a(receiver_class, &scope_class, context)
    {
        return Ok(None);
    }
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata(&scope_class, method_name, values)?
    else {
        return Ok(None);
    };
    if visibility == EvalVisibility::Private
        && !is_static
        && !is_abstract
        && same_eval_class_name(&declaring_class, &scope_class)
    {
        Ok(Some(declaring_class))
    } else {
        Ok(None)
    }
}

/// Binds method parameters into a fresh method scope and marks by-reference params as aliases.
pub(in crate::interpreter) fn bind_method_scope_args(
    method_scope: &mut ElephcEvalScope,
    params: &[String],
    parameter_is_by_ref: &[bool],
    bound_args: &[BoundMethodArg],
) {
    for (position, (name, bound_arg)) in params.iter().zip(bound_args.iter()).enumerate() {
        if parameter_is_by_ref.get(position).copied().unwrap_or(false) {
            method_scope.set_reference(
                name.clone(),
                name.clone(),
                bound_arg.value,
                ScopeCellOwnership::Borrowed,
            );
            if let Some(target) = bound_arg.ref_target.clone() {
                method_scope.set_reference_target(name.clone(), target);
            }
        } else {
            method_scope.set(name.clone(), bound_arg.value, ScopeCellOwnership::Borrowed);
        }
    }
    alias_duplicate_method_ref_args(method_scope, params, bound_args);
}

/// Creates local aliases when two by-reference method parameters point at the same caller variable.
pub(super) fn alias_duplicate_method_ref_args(
    method_scope: &mut ElephcEvalScope,
    params: &[String],
    bound_args: &[BoundMethodArg],
) {
    for (position, bound_arg) in bound_args.iter().enumerate() {
        let Some(target) = bound_arg.ref_target.as_ref() else {
            continue;
        };
        let Some(param) = params.get(position) else {
            continue;
        };
        for previous_position in 0..position {
            let Some(previous_target) = bound_args[previous_position].ref_target.as_ref() else {
                continue;
            };
            if !same_method_ref_target(target, previous_target) {
                continue;
            }
            if let Some(previous_param) = params.get(previous_position) {
                method_scope.set_reference(
                    param.clone(),
                    previous_param.clone(),
                    bound_args[previous_position].value,
                    ScopeCellOwnership::Borrowed,
                );
            }
            break;
        }
    }
}

/// Returns true when two evaluated arguments target the same caller-side variable.
pub(super) fn same_method_ref_target(left: &EvalReferenceTarget, right: &EvalReferenceTarget) -> bool {
    match (left, right) {
        (
            EvalReferenceTarget::Variable {
                scope: left_scope,
                name: left_name,
            },
            EvalReferenceTarget::Variable {
                scope: right_scope,
                name: right_name,
            },
        ) => left_scope == right_scope && left_name == right_name,
        (
            EvalReferenceTarget::ArrayElement {
                scope: left_scope,
                array_name: left_name,
                index: left_index,
            },
            EvalReferenceTarget::ArrayElement {
                scope: right_scope,
                array_name: right_name,
                index: right_index,
            },
        ) => left_scope == right_scope && left_name == right_name && left_index == right_index,
        (
            EvalReferenceTarget::NestedArrayElement {
                array_target: left_target,
                index: left_index,
            },
            EvalReferenceTarget::NestedArrayElement {
                array_target: right_target,
                index: right_index,
            },
        ) => left_index == right_index && same_method_ref_target(left_target, right_target),
        (
            EvalReferenceTarget::ObjectProperty {
                object: left_object,
                property: left_property,
                access_scope: left_access_scope,
            },
            EvalReferenceTarget::ObjectProperty {
                object: right_object,
                property: right_property,
                access_scope: right_access_scope,
            },
        ) => {
            left_object == right_object
                && left_property == right_property
                && left_access_scope == right_access_scope
        }
        (
            EvalReferenceTarget::Cell { cell: left_cell },
            EvalReferenceTarget::Cell { cell: right_cell },
        ) => left_cell == right_cell,
        (
            EvalReferenceTarget::InvokerSlot {
                slot: left_slot,
                source_tag: left_source_tag,
            },
            EvalReferenceTarget::InvokerSlot {
                slot: right_slot,
                source_tag: right_source_tag,
            },
        ) => left_slot == right_slot && left_source_tag == right_source_tag,
        (
            EvalReferenceTarget::StaticProperty {
                class_name: left_class_name,
                property: left_property,
                access_scope: left_access_scope,
            },
            EvalReferenceTarget::StaticProperty {
                class_name: right_class_name,
                property: right_property,
                access_scope: right_access_scope,
            },
        ) => {
            left_class_name == right_class_name
                && left_property == right_property
                && left_access_scope == right_access_scope
        }
        _ => false,
    }
}

/// Writes completed by-reference method parameter values back to their caller-side variables.
pub(in crate::interpreter) fn write_back_method_ref_args(
    params: &[String],
    bound_args: &[BoundMethodArg],
    method_scope: &ElephcEvalScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (position, bound_arg) in bound_args.iter().enumerate() {
        let Some(param) = params.get(position) else {
            continue;
        };
        if let Some(target) = bound_arg.ref_target.as_ref() {
            let Some(entry) = method_scope
                .entry(param)
                .filter(|entry| entry.flags().is_visible() && entry.flags().by_ref)
            else {
                continue;
            };
            write_back_method_ref_target(target, entry.cell(), context, values)?;
        }
        write_back_method_variadic_ref_args(param, bound_arg, method_scope, context, values)?;
    }
    Ok(())
}

/// Writes element-level changes from a by-reference variadic method parameter back to callers.
pub(super) fn write_back_method_variadic_ref_args(
    param: &str,
    bound_arg: &BoundMethodArg,
    method_scope: &ElephcEvalScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if bound_arg.variadic_ref_targets.is_empty() {
        return Ok(());
    }
    let Some(entry) = method_scope
        .entry(param)
        .filter(|entry| entry.flags().is_visible() && entry.flags().by_ref)
    else {
        return Ok(());
    };
    if entry.cell() != bound_arg.value {
        return Ok(());
    }
    for (key, target) in &bound_arg.variadic_ref_targets {
        let value = values.array_get(entry.cell(), *key)?;
        write_back_method_ref_target(target, value, context, values)?;
    }
    Ok(())
}

/// Stores one by-reference result in the original caller-side target.
pub(in crate::interpreter) fn write_back_method_ref_target(
    target: &EvalReferenceTarget,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match target {
        EvalReferenceTarget::Variable { scope, name } => {
            let Some(scope) = (unsafe { scope.as_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            for replaced in set_scope_cell(
                context,
                scope,
                name.clone(),
                value,
                ScopeCellOwnership::Borrowed,
            )? {
                values.release(replaced)?;
            }
            Ok(())
        }
        EvalReferenceTarget::ArrayElement {
            scope,
            array_name,
            index,
        } => {
            let Some(scope) = (unsafe { scope.as_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            write_back_method_array_element_ref_target(
                scope, array_name, *index, value, context, values,
            )
        }
        EvalReferenceTarget::NestedArrayElement {
            array_target,
            index,
        } => write_back_method_nested_array_element_ref_target(
            array_target,
            *index,
            value,
            context,
            values,
        ),
        EvalReferenceTarget::ObjectProperty {
            object,
            property,
            access_scope,
        } => write_back_method_object_property_ref_target(
            *object,
            property,
            access_scope.clone(),
            value,
            context,
            values,
        ),
        EvalReferenceTarget::StaticProperty {
            class_name,
            property,
            access_scope,
        } => write_back_method_static_property_ref_target(
            class_name,
            property,
            access_scope.clone(),
            value,
            context,
            values,
        ),
        EvalReferenceTarget::Cell { .. } => Ok(()),
        EvalReferenceTarget::InvokerSlot { slot, source_tag } => {
            write_back_invoker_slot_ref_target(*slot, *source_tag, value, values)
        }
    }
}

/// Reads a value from a native descriptor-invoker by-reference slot.
pub(super) fn eval_invoker_slot_ref_target_value(
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
            let value = unsafe { *(slot as *const RuntimeCellHandle) };
            values.retain(value)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Writes a value back into a native descriptor-invoker by-reference slot.
pub(super) fn write_back_invoker_slot_ref_target(
    slot: usize,
    source_tag: u64,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match source_tag {
        EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_BOOL | EVAL_TAG_RESOURCE => {
            let word = values.raw_value_word(value)?;
            unsafe {
                *(slot as *mut u64) = word;
            }
            Ok(())
        }
        EVAL_TAG_STRING => write_back_invoker_string_slot(slot, value, values),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE => {
            write_back_invoker_heap_slot(slot, source_tag, value, values)
        }
        EVAL_TAG_MIXED => {
            let retained = values.retain(value)?;
            let replaced = unsafe {
                let slot = slot as *mut RuntimeCellHandle;
                let replaced = *slot;
                *slot = retained;
                replaced
            };
            values.release(replaced)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Writes a boxed string value back into a native descriptor-invoker string slot.
pub(super) fn write_back_invoker_string_slot(
    slot: usize,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_STRING {
        return Err(EvalStatus::RuntimeFatal);
    }
    let ptr = values.raw_value_word(value)?;
    let len = values.raw_value_high_word(value)?;
    let retained = values.retain_raw_string_words(ptr, len)?;
    let replaced = unsafe {
        let slot = slot as *mut [u64; 2];
        let replaced = *slot;
        *slot = [retained.0, retained.1];
        replaced
    };
    values.release_raw_string_words(replaced[0], replaced[1])
}

/// Writes a boxed heap value back into a native descriptor-invoker raw heap slot.
pub(super) fn write_back_invoker_heap_slot(
    slot: usize,
    source_tag: u64,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if values.type_tag(value)? != source_tag {
        return Err(EvalStatus::RuntimeFatal);
    }
    let word = values.raw_value_word(value)?;
    let retained = values.retain_raw_heap_word(word)?;
    let replaced = unsafe {
        let slot = slot as *mut u64;
        let replaced = *slot;
        *slot = retained;
        replaced
    };
    values.release_raw_heap_word(replaced)
}

/// Stores one by-reference method result in a caller-side array element.
pub(super) fn write_back_method_array_element_ref_target(
    scope: &mut ElephcEvalScope,
    array_name: &str,
    index: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut ownership = ScopeCellOwnership::Owned;
    let array = if let Some(existing) =
        scope_entry(context, scope, array_name).filter(|entry| entry.flags().is_visible())
    {
        if values.is_array_like(existing.cell())? {
            ownership = existing.flags().ownership;
            values.array_clone_shallow(existing.cell())?
        } else {
            eval_new_array_for_index(index, values)?
        }
    } else {
        eval_new_array_for_index(index, values)?
    };
    let array = values.array_set(array, index, value)?;
    for replaced in set_scope_cell(context, scope, array_name.to_string(), array, ownership)? {
        values.release(replaced)?;
    }
    Ok(())
}

/// Stores one by-reference method result in an element of a nested caller-side array target.
pub(super) fn write_back_method_nested_array_element_ref_target(
    array_target: &EvalReferenceTarget,
    index: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let current = eval_reference_target_value(array_target, context, values)?;
    let array = if values.is_array_like(current)? {
        values.array_clone_shallow(current)?
    } else {
        eval_new_array_for_index(index, values)?
    };
    let array = values.array_set(array, index, value)?;
    write_back_method_ref_target(array_target, array, context, values)
}

/// Stores one by-reference method result in a caller-side object property.
pub(super) fn write_back_method_object_property_ref_target(
    object: RuntimeCellHandle,
    property: &str,
    access_scope: ElephcEvalExecutionScope,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let previous_scope = context.replace_execution_scope(access_scope);
    let result = eval_property_set_result(object, property, value, context, values);
    context.replace_execution_scope(previous_scope);
    result
}

/// Stores one by-reference method result in a caller-side static property.
pub(super) fn write_back_method_static_property_ref_target(
    class_name: &str,
    property: &str,
    access_scope: ElephcEvalExecutionScope,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let previous_scope = context.replace_execution_scope(access_scope);
    let result = eval_static_property_set_result(class_name, property, value, context, values);
    context.replace_execution_scope(previous_scope);
    result
}

/// Creates an indexed or associative array according to the first write key.
pub(super) fn eval_new_array_for_index(
    index: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(index)? == EVAL_TAG_STRING {
        values.assoc_new(1)
    } else {
        values.array_new(1)
    }
}
