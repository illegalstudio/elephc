//! Purpose:
//! Executes increment, unset, append, and indexed array mutation statements.
//!
//! Called from:
//! - `crate::interpreter::statements::execute_stmt()`.
//!
//! Key details:
//! - Object ArrayAccess and plain runtime arrays preserve reference and release semantics.

use super::*;

/// Applies member increment/decrement to a runtime value using PHP numeric semantics.
pub(super) fn eval_inc_dec_value(
    current: RuntimeCellHandle,
    increment: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let one = values.int(1)?;
    let result = if increment {
        values.add(current, one)
    } else {
        values.sub(current, one)
    };
    let released = values.release(one);
    match (result, released) {
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = values.release(value);
            Err(status)
        }
        (Ok(value), Ok(())) => Ok(value),
    }
}

/// Keeps the old property value alive through the RHS and balances every compound-assignment operand.
pub(super) fn eval_property_compound_assign_result(
    object: RuntimeCellHandle,
    property: &str,
    op: EvalBinOp,
    right: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let current = eval_property_get_result(object, property, context, values)?;
    with_eval_value_lease(current, context, values, |current, context, values| {
        with_eval_void_operands(&[right], context, scope, values, |args, context, _, values| {
            let value = eval_binary_result(op, current, args[0], context, values)?;
            with_eval_value_lease(value, context, values, |value, context, values| {
                eval_property_set_result(object, property, value, context, values)
            })
        })
    })
}

/// Reads, updates, and writes one object property after the receiver/name are evaluated.
pub(super) fn eval_property_inc_dec_result(
    object: RuntimeCellHandle,
    property: &str,
    increment: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let current = eval_property_get_result(object, property, context, values)?;
    with_eval_value_lease(current, context, values, |current, context, values| {
        let value = eval_inc_dec_value(current, increment, values)?;
        with_eval_value_lease(value, context, values, |value, context, values| {
            eval_property_set_result(object, property, value, context, values)
        })
    })
}

/// Reads, updates, and writes one static property after the receiver/name are resolved.
pub(super) fn eval_static_property_inc_dec_result(
    class_name: &str,
    property: &str,
    increment: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let current = eval_static_property_get_result(class_name, property, context, values)?;
    let value = eval_inc_dec_value(current, increment, values)?;
    eval_static_property_set_result(class_name, property, value, context, values)
}

/// Consumes an eval owner even when its dynamic destructor throws, preserving the pending exception.
pub(in crate::interpreter) fn eval_release_value(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    let destructor = values.final_object_identity_for_release(value).and_then(|identity| {
        match identity {
            Some(identity) => eval_dynamic_destructor_for_release(identity, value, context, values),
            None => Ok(()),
        }
    });
    // Native release collects any further child exceptions with the context's pending throw.
    // Do not return early: the destructor consumes its receiver lease, not this final owner.
    let released = values.release(value);
    destructor.and(released)
}

/// Calls a dynamic eval `__destruct()` hook immediately before the runtime frees the object.
pub(super) fn eval_dynamic_destructor_for_release(
    identity: u64,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_dynamic_destructor_for_object_cell(identity, object, context, values).map(|_| ())
}

/// Calls a dynamic eval `__destruct()` hook for an already-boxed object cell.
pub(crate) fn eval_dynamic_destructor_for_object_cell(
    identity: u64,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(class_name) = context
        .dynamic_object_class(identity)
        .map(|class| class.name().to_string())
    else {
        return Ok(false);
    };
    let Some((declaring_class, method)) = context.class_method(&class_name, "__destruct") else {
        return Ok(false);
    };
    if !context.begin_dynamic_object_destructor(identity) {
        return Ok(true);
    }
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &class_name,
        &method,
        object,
        Vec::new(),
        context,
        values,
    );
    let release_result = match result {
        Ok(result) => values.release(result),
        Err(status) => Err(status),
    };
    context.finish_dynamic_object_destructor(identity);
    release_result.map(|_| true)
}

/// Executes `unset($object[$key])` through `ArrayAccess::offsetUnset()`.
pub(super) fn eval_array_unset_element_stmt(
    array: &EvalExpr,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match array {
        EvalExpr::LoadVar(name) => {
            let existing = scope_entry(context, scope, name)
                .filter(|entry| entry.flags().is_visible())
                .map(|entry| (entry.cell(), entry.flags().ownership));
            let Some((array, ownership)) = existing else {
                return Ok(());
            };
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                for replaced in set_scope_cell(context, scope, name.clone(), array, ownership)? {
                    values.release(replaced)?;
                }
            }
            return Ok(());
        }
        EvalExpr::PropertyGet { object, property } => {
            return with_eval_void_operands(&[object], context, scope, values, |args, context, scope, values| {
                eval_property_array_unset_result(args[0], property, index, context, scope, values)
            });
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            return with_eval_void_operands(&[object], context, scope, values, |args, context, scope, values| {
                let property = eval_dynamic_member_name(property, context, scope, values)?;
                eval_property_array_unset_result(args[0], &property, index, context, scope, values)
            });
        }
        EvalExpr::StaticPropertyGet {
            class_name,
            property,
        } => {
            let array = eval_static_property_get_result(class_name, property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_static_property_set_result(class_name, property, array, context, values)?;
            }
            return Ok(());
        }
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let array = eval_static_property_get_result(&class_name, property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_static_property_set_result(&class_name, property, array, context, values)?;
            }
            return Ok(());
        }
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            let array = eval_static_property_get_result(&class_name, &property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_static_property_set_result(&class_name, &property, array, context, values)?;
            }
            return Ok(());
        }
        _ => {}
    }
    with_eval_void_operands(&[array], context, scope, values, |args, context, scope, values| {
        eval_array_access_unset_result(args[0], index, context, scope, values)
    })
}

/// Releases the old property read and the rebuilt array after an indexed unset writes back.
fn eval_property_array_unset_result(
    object: RuntimeCellHandle,
    property: &str,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_property_get_result(object, property, context, values)?;
    with_eval_value_lease(array, context, values, |array, context, values| {
        if let Some(replacement) = eval_array_unset_target_result(array, index, context, scope, values)? {
            with_eval_value_lease(replacement, context, values, |replacement, context, values| {
                eval_property_set_result(object, property, replacement, context, values)
            })?;
        }
        Ok(())
    })
}

/// Unsets one offset from an already-resolved array-like target and returns a replacement array.
pub(super) fn eval_array_unset_target_result(
    array: RuntimeCellHandle,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        eval_array_access_unset_result(array, index, context, scope, values)?;
        return Ok(None);
    }
    let tag = values.type_tag(array)?;
    if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let index = eval_owned_array_set_index(index, context, scope, values)?;
    let result = (|| {
        let key = eval_array_reference_key(index, values)?;
        let replacement = eval_array_without_key_result(array, index, values)?;
        context.clone_array_element_aliases(array, replacement, key.as_ref());
        Ok(replacement)
    })();
    let released = eval_release_value(context, values, index);
    match (result, released) {
        (Err(status), _) => Err(status),
        (Ok(result), Err(status)) => {
            let _ = values.release(result);
            Err(status)
        }
        (Ok(result), Ok(())) => Ok(Some(result)),
    }
}

/// Executes `unset($object[$key])` through `ArrayAccess::offsetUnset()`.
pub(super) fn eval_array_access_unset_result(
    array: RuntimeCellHandle,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    with_eval_void_operands(&[index], context, scope, values, |args, context, _, values| {
        if values.type_tag(array)? != EVAL_TAG_OBJECT {
            return Err(EvalStatus::UnsupportedConstruct);
        }
        if !eval_array_access_object_matches(array, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let result = eval_method_call_result(array, "offsetUnset", args.to_vec(), context, values)?;
        eval_release_value(context, values, result)
    })
}

/// Rebuilds an array without the strict-equal key requested by `unset($array[$key])`.
pub(super) fn eval_array_without_key_result(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let tag = values.type_tag(array)?;
    let mut result = if tag == EVAL_TAG_ASSOC {
        builtins::collection_builder::EvalArrayBuilder::assoc(values, len.saturating_sub(1))?
    } else {
        builtins::collection_builder::EvalArrayBuilder::indexed(values, len.saturating_sub(1))?
    };
    for position in 0..len {
        let key = result.values().array_iter_key(array, position)?;
        let copied = (|| {
            let equal = result.values().compare(EvalBinOp::StrictEq, key, index)?;
            let matches = result.values().truthy(equal);
            let released = result.values().release(equal);
            let matches = matches?;
            released?;
            if !matches {
                result.entry(|values| values.array_get(array, key), |values, _| values.retain(key))?;
            }
            Ok(())
        })();
        let released = result.values().release(key);
        copied?;
        released?;
    }
    Ok(result.finish())
}

/// Executes `$var[] = value` and dispatches object writes through `ArrayAccess::offsetSet()`.
pub(super) fn eval_array_append_var_stmt(
    name: &str,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let existing = scope_entry(context, scope, name)
        .filter(|entry| entry.flags().is_visible())
        .map(|entry| (entry.cell(), entry.flags().ownership));
    if let Some((object, _)) = existing {
        if values.type_tag(object)? != EVAL_TAG_OBJECT {
            return eval_non_object_array_append_var_stmt(
                name, value, existing, context, scope, values,
            );
        }
        let offset = values.null()?;
        let value = eval_expr(value, context, scope, values)?;
        if !eval_array_access_object_matches(object, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let result =
            eval_method_call_result(object, "offsetSet", vec![offset, value], context, values)?;
        values.release(result)?;
        return Ok(());
    }

    eval_non_object_array_append_var_stmt(name, value, existing, context, scope, values)
}

/// Executes the non-object `$var[] = value` path with the existing array semantics.
pub(super) fn eval_non_object_array_append_var_stmt(
    name: &str,
    value: &EvalExpr,
    existing: Option<(RuntimeCellHandle, ScopeCellOwnership)>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut ownership = ScopeCellOwnership::Owned;
    let array = if let Some((cell, flags_ownership)) = existing {
        if values.is_array_like(cell)? {
            let tag = values.type_tag(cell)?;
            if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
                return Err(EvalStatus::UnsupportedConstruct);
            }
            ownership = flags_ownership;
            cell
        } else {
            values.array_new(1)?
        }
    } else {
        values.array_new(1)?
    };
    let index = eval_array_append_key(array, values)?;
    let value = eval_expr(value, context, scope, values)?;
    let array = values.array_set(array, index, value)?;
    for replaced in set_scope_cell(context, scope, name.to_string(), array, ownership)? {
        values.release(replaced)?;
    }
    Ok(())
}

/// Executes `$var[index] = value` and dispatches object writes through `ArrayAccess::offsetSet()`.
pub(super) fn eval_array_set_var_stmt(
    name: &str,
    index: &EvalExpr,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let existing = scope_entry(context, scope, name)
        .filter(|entry| entry.flags().is_visible())
        .map(|entry| (entry.cell(), entry.flags().ownership));
    if let Some((object, _)) = existing {
        if values.type_tag(object)? != EVAL_TAG_OBJECT {
            return eval_non_object_array_set_var_stmt(
                name, index, value, existing, context, scope, values,
            );
        }
        let index = eval_expr(index, context, scope, values)?;
        let value = eval_expr(value, context, scope, values)?;
        if !eval_array_access_object_matches(object, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let result =
            eval_method_call_result(object, "offsetSet", vec![index, value], context, values)?;
        values.release(result)?;
        return Ok(());
    }

    eval_non_object_array_set_var_stmt(name, index, value, existing, context, scope, values)
}

/// Writes a local array element while retaining operands and updating persistent references.
pub(super) fn eval_non_object_array_set_var_stmt(
    name: &str,
    index: &EvalExpr,
    value: &EvalExpr,
    existing: Option<(RuntimeCellHandle, ScopeCellOwnership)>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = if let Some((cell, _)) = existing {
        if values.is_array_like(cell)? {
            values.retain(cell)?
        } else {
            values.array_new(1)?
        }
    } else {
        values.array_new(1)?
    };
    let mut operands = vec![array];
    let mut result = (|| {
        let index = eval_owned_array_set_index(index, context, scope, values)?;
        operands.push(index);
        let value = eval_owned_expr(value, context, scope, values)?;
        operands.push(value);
        let replacement = eval_array_set_target_for_index(array, index, values)?;
        if replacement != array {
            operands.push(replacement);
            context.clone_array_element_aliases(array, replacement, None);
        }
        eval_array_element_reference_write(replacement, index, value, context, values)?;
        values.array_set(replacement, index, value)?;
        if scope_entry(context, scope, name).is_some_and(|entry| {
            entry.flags().is_visible() && entry.cell() == replacement
        }) {
            // An in-place write preserves the slot's existing owner or call-frame borrow.
            return Ok(());
        }
        write_back_owned_variable_ref_target(scope, name, replacement, context, values)
    })();
    for operand in operands.into_iter().rev() {
        let released = eval_release_value(context, values, operand);
        if result.is_ok() { result = released; }
    }
    result
}

/// Propagates an array-element reference write without borrowing the array's value owner.
fn eval_array_element_reference_write(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(key) = eval_array_reference_key(index, values)? else { return Ok(()); };
    let Some(target) = context.array_element_alias(array, &key).cloned() else { return Ok(()); };
    match target {
        EvalReferenceTarget::Variable { scope, name } => {
            let scope = unsafe { scope.as_mut() }.ok_or(EvalStatus::RuntimeFatal)?;
            write_back_owned_variable_ref_target(scope, &name, value, context, values)
        }
        EvalReferenceTarget::Cell { .. } => {
            context.bind_array_element_alias(array, key, EvalReferenceTarget::Cell { cell: value });
            Ok(())
        }
        _ => write_back_method_ref_target(&target, value, context, values),
    }
}

/// Writes an indexed property or appends to it while preserving COW and every operand's owner.
pub(super) fn eval_property_array_write_result(
    object: RuntimeCellHandle,
    property: &str,
    index: Option<&EvalExpr>,
    op: Option<EvalBinOp>,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut operands = Vec::new();
    let mut result = (|| {
        let current = eval_property_get_result(object, property, context, values)?;
        let current = if current.is_borrowed() { values.retain(current)? } else { current };
        operands.push(current);
        if values.type_tag(current)? == EVAL_TAG_OBJECT {
            if !eval_array_access_object_matches(current, context, values)? {
                return Err(EvalStatus::RuntimeFatal);
            }
            let index = match index {
                Some(index) => eval_owned_expr(index, context, scope, values)?,
                None => values.null()?,
            };
            operands.push(index);
            let value = eval_property_array_set_value(current, index, op, value, context, scope, values)?;
            let value = if value.is_borrowed() { values.retain(value)? } else { value };
            operands.push(value);
            let returned = eval_method_call_result(current, "offsetSet", vec![index, value], context, values)?;
            operands.push(returned);
            return Ok(());
        }
        let index = match index {
            Some(index) => eval_owned_array_set_index(index, context, scope, values)?,
            None if values.is_array_like(current)? => eval_array_append_key(current, values)?,
            None => values.int(0)?,
        };
        operands.push(index);
        let array = if values.is_array_like(current)? {
            values.array_clone_shallow(current)?
        } else {
            values.array_new(1)?
        };
        operands.push(array);
        context.clone_array_element_aliases(current, array, None);
        let value = eval_property_array_set_value(array, index, op, value, context, scope, values)?;
        let value = if value.is_borrowed() { values.retain(value)? } else { value };
        operands.push(value);
        eval_array_element_reference_write(array, index, value, context, values)?;
        // Runtime setters mutate the receiver cell in place, retaining only the inserted value.
        values.array_set(array, index, value)?;
        eval_property_set_result(object, property, array, context, values)
    })();
    for operand in operands.into_iter().rev() {
        let released = eval_release_value(context, values, operand);
        if result.is_ok() { result = released; }
    }
    result
}

/// Computes the value written by a simple or compound property-array assignment.
pub(super) fn eval_property_array_set_value(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    op: Option<EvalBinOp>,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(op) = op else {
        return eval_expr(value, context, scope, values);
    };
    let current = eval_array_get_result(array, index, context, values)?;
    let current = if current.is_borrowed() { values.retain(current)? } else { current };
    let result = with_eval_operands(&[value], context, scope, values, |args, context, _, values| {
        eval_binary_result(op, current, args[0], context, values)
    });
    let released = eval_release_value(context, values, current);
    match (result, released) {
        (Err(status), _) => Err(status),
        (Ok(result), Err(status)) => {
            let _ = release_expr_result(result, context, values);
            Err(status)
        }
        (Ok(result), Ok(())) => Ok(result),
    }
}

/// Executes `Class::$property[] = value`, including ArrayAccess static-property values.
pub(super) fn eval_static_property_array_append_result(
    class_name: &str,
    property: &str,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_static_property_get_result(class_name, property, context, values)?;
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        if !eval_array_access_object_matches(array, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let offset = values.null()?;
        let value = eval_expr(value, context, scope, values)?;
        let result =
            eval_method_call_result(array, "offsetSet", vec![offset, value], context, values)?;
        values.release(result)?;
        return Ok(());
    }
    let array = if values.is_array_like(array)? {
        let tag = values.type_tag(array)?;
        if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            return Err(EvalStatus::UnsupportedConstruct);
        }
        array
    } else {
        values.array_new(1)?
    };
    let index = eval_array_append_key(array, values)?;
    let value = eval_expr(value, context, scope, values)?;
    let array = values.array_set(array, index, value)?;
    eval_static_property_set_result(class_name, property, array, context, values)
}

/// Executes `Class::$property[index] = value` and compound indexed static-property writes.
pub(super) fn eval_static_property_array_set_result(
    class_name: &str,
    property: &str,
    index: &EvalExpr,
    op: Option<EvalBinOp>,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_static_property_get_result(class_name, property, context, values)?;
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        if !eval_array_access_object_matches(array, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let index = eval_expr(index, context, scope, values)?;
        let value = eval_property_array_set_value(array, index, op, value, context, scope, values)?;
        let result =
            eval_method_call_result(array, "offsetSet", vec![index, value], context, values)?;
        values.release(result)?;
        return Ok(());
    }
    let index = eval_array_set_index(index, context, scope, values)?;
    let array = if values.is_array_like(array)? {
        array
    } else {
        values.array_new(1)?
    };
    let array = eval_array_set_target_for_index(array, index, values)?;
    let value = eval_property_array_set_value(array, index, op, value, context, scope, values)?;
    let array = values.array_set(array, index, value)?;
    eval_static_property_set_result(class_name, property, array, context, values)
}

/// Evaluates an array-set index and normalizes PHP integer-string keys.
pub(super) fn eval_array_set_index(
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let index = eval_expr(index, context, scope, values)?;
    if values.type_tag(index)? != EVAL_TAG_STRING {
        return Ok(index);
    }
    let bytes = values.string_bytes(index)?;
    match eval_numeric_string_array_key(&bytes) {
        Some(key) => values.int(key),
        None => Ok(index),
    }
}

/// Normalizes an array mutation key into an owned cell, consuming temporary string inputs.
fn eval_owned_array_set_index(
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_operands(&[index], context, scope, values, |args, _, _, values| {
        let index = args[0];
        if values.type_tag(index)? == EVAL_TAG_STRING {
            let bytes = values.string_bytes(index)?;
            if let Some(key) = eval_numeric_string_array_key(&bytes) {
                return values.int(key);
            }
        }
        values.retain(index)
    })
}

/// Converts indexed arrays to associative arrays before writing a non-numeric string key.
pub(super) fn eval_array_set_target_for_index(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(array)? != EVAL_TAG_ARRAY || values.type_tag(index)? != EVAL_TAG_STRING {
        return Ok(array);
    }
    let len = values.array_len(array)?;
    let mut assoc = builtins::collection_builder::EvalArrayBuilder::assoc(values, len + 1)?;
    for position in 0..len {
        let key = assoc.values().array_iter_key(array, position)?;
        let inserted = assoc.entry(
            |values| values.array_get(array, key),
            |values, _| values.retain(key),
        );
        let released = assoc.values().release(key);
        inserted?;
        released?;
    }
    Ok(assoc.finish())
}
