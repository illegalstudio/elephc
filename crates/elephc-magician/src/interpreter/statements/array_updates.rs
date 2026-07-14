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
    if increment {
        values.add(current, one)
    } else {
        values.sub(current, one)
    }
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
    let value = eval_inc_dec_value(current, increment, values)?;
    eval_property_set_result(object, property, value, context, values)
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

/// Releases one eval-owned value after running an eval-declared dynamic destructor if needed.
pub(super) fn eval_release_value(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    if let Some(identity) = values.final_object_identity_for_release(value)? {
        eval_dynamic_destructor_for_release(identity, value, context, values)?;
    }
    values.release(value)
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
            let object = eval_expr(object, context, scope, values)?;
            let array = eval_property_get_result(object, property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_property_set_result(object, property, array, context, values)?;
            }
            return Ok(());
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            let array = eval_property_get_result(object, &property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_property_set_result(object, &property, array, context, values)?;
            }
            return Ok(());
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
    let array = eval_expr(array, context, scope, values)?;
    eval_array_access_unset_result(array, index, context, scope, values)
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
    let index = eval_array_set_index(index, context, scope, values)?;
    eval_array_without_key_result(array, index, values).map(Some)
}

/// Executes `unset($object[$key])` through `ArrayAccess::offsetUnset()`.
pub(super) fn eval_array_access_unset_result(
    array: RuntimeCellHandle,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let index = eval_expr(index, context, scope, values)?;
    if values.type_tag(array)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    if !eval_array_access_object_matches(array, context, values)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = eval_method_call_result(array, "offsetUnset", vec![index], context, values)?;
    values.release(result)?;
    Ok(())
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
        values.assoc_new(len.saturating_sub(1))?
    } else {
        values.array_new(len.saturating_sub(1))?
    };
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let equal = values.compare(EvalBinOp::StrictEq, key, index)?;
        if values.truthy(equal)? {
            continue;
        }
        let value = values.array_get(array, key)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
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

/// Executes the non-object `$var[index] = value` path with the existing array semantics.
pub(super) fn eval_non_object_array_set_var_stmt(
    name: &str,
    index: &EvalExpr,
    value: &EvalExpr,
    existing: Option<(RuntimeCellHandle, ScopeCellOwnership)>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut ownership = ScopeCellOwnership::Owned;
    let array = if let Some((cell, flags_ownership)) = existing {
        if values.is_array_like(cell)? {
            ownership = flags_ownership;
            cell
        } else {
            values.array_new(1)?
        }
    } else {
        values.array_new(1)?
    };
    let index = eval_array_set_index(index, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    let array = eval_array_set_target_for_index(array, index, values)?;
    let array = values.array_set(array, index, value)?;
    for replaced in set_scope_cell(context, scope, name.to_string(), array, ownership)? {
        values.release(replaced)?;
    }
    Ok(())
}

/// Executes `$object->property[] = value`, dispatching ArrayAccess property values when needed.
pub(super) fn eval_property_array_append_result(
    object: RuntimeCellHandle,
    property: &str,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_property_get_result(object, property, context, values)?;
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
    eval_property_set_result(object, property, array, context, values)
}

/// Executes `$object->property[index] = value` and compound indexed property writes.
pub(super) fn eval_property_array_set_result(
    object: RuntimeCellHandle,
    property: &str,
    index: &EvalExpr,
    op: Option<EvalBinOp>,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_property_get_result(object, property, context, values)?;
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
    eval_property_set_result(object, property, array, context, values)
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
    let right = eval_expr(value, context, scope, values)?;
    eval_binary_result(op, current, right, context, values)
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
    let mut assoc = values.assoc_new(len + 1)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        assoc = values.array_set(assoc, key, value)?;
    }
    Ok(assoc)
}
