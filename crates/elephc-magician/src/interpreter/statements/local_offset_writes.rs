//! Purpose:
//! Executes local indexed and append writes against the live post-operand receiver.
//!
//! Called from:
//! - `statements::dispatch` for local array assignment statements.
//!
//! Key details:
//! - Index and RHS each run once before resolving the visible local and its storage kind.
//! - Array contracts, scalar errors, string offsets, and ArrayAccess share owner cleanup.

use super::*;

/// Appends to the receiver left in the local after evaluating the RHS.
pub(super) fn eval_array_append_var_stmt(
    name: &str, value: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_local_offset_write(name, None, value, context, scope, values)
}

/// Updates the receiver left in the local after evaluating the index and RHS.
pub(super) fn eval_array_set_var_stmt(
    name: &str, index: &EvalExpr, value: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_local_offset_write(name, Some(index), value, context, scope, values)
}

/// Dispatches a local write using live storage and releases every evaluated operand on all exits.
fn eval_local_offset_write(
    name: &str, index: Option<&EvalExpr>, value: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut operands = Vec::new();
    let mut result = (|| {
        let (index, key_source) = match index {
            Some(index) => {
                let (key, source) = evaluate_member_value(index, context, scope, values)?;
                operands.push(key); (Some(key), source)
            }
            None => (None, None),
        };
        let (value, producer) = evaluate_member_value(value, context, scope, values)?;
        operands.push(value);
        let receiver_source = local_binding_origins::source_view(&EvalExpr::LoadVar(name.to_string()), context, scope)
            .as_ref().map(crate::eval_ir::generic_type);
        let key_source_type = index.map(|key| write_source_domains::source_type(key_source.as_ref(), key, context, values)).transpose()?.flatten();
        write_source_domains::offset(receiver_source.as_ref(), key_source_type.as_ref(), index.is_some(), context, values)?;
        let current = visible_scope_cell(context, scope, name).map(|cell| values.retain(cell)).transpose()?;
        if let Some(current) = current { operands.push(current); }
        let tag = current.map(|cell| values.type_tag(cell)).transpose()?;
        if let Some(current) = current {
            if tag == Some(EVAL_TAG_OBJECT) {
                if !eval_array_access_object_matches(current, context, values)? {
                    let identity = values.object_identity(current)?;
                    let class_name = match context.dynamic_object_class_name(identity) {
                        Some(name) => name,
                        None => runtime_object_class_name(current, values)?,
                    };
                    return eval_throw_error(&format!("Cannot use object of type {class_name} as array"), context, values);
                }
                if index.is_none() {
                    if let Some(returned) = super::super::runtime_ops::simplexml_properties::append(values, current, value)? {
                        operands.push(returned);
                        return Ok(());
                    }
                }
                let index = match index {
                    Some(index) => index,
                    None => {
                        reject_weak_map_append(current, context, values)?;
                        let index = values.null()?;
                        operands.push(index);
                        index
                    }
                };
                let returned = match super::super::runtime_ops::simplexml_properties::dimension(
                    values, current, elephc_builtin_contract::ObjectDimension::Set, vec![index, value],
                )? {
                    Some(value) => value,
                    None => eval_method_call_result(current, "offsetSet", vec![index, value], context, values)?,
                };
                operands.push(returned);
                return Ok(());
            }
            if let Some(index) = index {
                if tag == Some(EVAL_TAG_STRING) {
                    let replacement = eval_string_offset_write(current, index, value, context, values)?;
                    operands.push(replacement);
                    validate_generic_local_contracts(scope, name, replacement, context, values)?;
                    return write_back_owned_variable_ref_target(scope, name, replacement, context, values);
                }
            }
        }
        let mut published_generation = None;
        let mut captured_parent = false;
        let array = match current {
            Some(cell) if matches!(tag, Some(EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)) => cell,
            _ => {
                let was_false = tag == Some(EVAL_TAG_BOOL)
                    && !values.truthy(current.ok_or(EvalStatus::RuntimeFatal)?)?;
                let array = eval_array_autovivification(current, index.is_none(), context, values)?;
                operands.push(array);
                write_back_owned_variable_ref_target(scope, name, array, context, values)?;
                published_generation = scope_entry(context, scope, name).map(|entry| entry.generation());
                if was_false {
                    let published = visible_scope_cell(context, scope, name).ok_or(EvalStatus::RuntimeFatal)?;
                    let captured = values.retain_value_cell(published)?;
                    operands.push(captured);
                    captured_parent = true;
                    captured
                } else { array }
            }
        };
        super::false_array_completion::with_completion(captured_parent, index, context, values, |context, values| {
            let declared = generic_local_element_contracts(scope, name, context);
            write_source_domains::elements(&declared, array, producer.as_ref(), value, context, values)?;
            let origin;
            let replacement = if let Some(index) = index {
                checked_array_keys::for_receiver(array, index, context, values)?;
                let index = match normalized_array_string_index(index, context.semantic_profile(), values)? {
                    Some(normalized) => { operands.push(normalized); normalized }
                    None => index,
                };
                origin = local_binding_origins::prepare_array_local_write(scope, name, array, Some(index),
                    value, producer.as_ref().map(|source| &source.ty), context, values)?;
                for ty in &declared { validate_generic_local_element(Some(ty), array, index, value, context, values)?; }
                let receiver = if origin.is_some() && !captured_parent {
                    let detached = copy_scope_value(array, context, values)?;
                    operands.push(detached); detached
                } else { array };
                let replacement = if captured_parent { array }
                    else { eval_array_set_target_for_index(receiver, index, values)? };
                if replacement != receiver {
                    operands.push(replacement);
                    context.clone_array_element_aliases(receiver, replacement, None);
                }
                eval_array_element_reference_write(replacement, index, value, context, values)?;
                values.array_set(replacement, index, value)?;
                replacement
            } else {
                origin = local_binding_origins::prepare_array_local_write(scope, name, array, None,
                    value, producer.as_ref().map(|source| &source.ty), context, values)?;
                let receiver = if origin.is_some() && !captured_parent {
                    let detached = copy_scope_value(array, context, values)?;
                    operands.push(detached); detached
                } else { array };
                eval_array_append_owned_value_with_type(receiver, value, &declared, context, values)?
            };
            if let Some(origin) = &origin {
                if let Some(reference) = visible_scope_cell(context, scope, name) {
                    if values.is_reference(reference)? {
                        array_reference_contracts::validate_new_owner(reference, replacement,
                            std::slice::from_ref(origin), context, values)?;
                    }
                }
            }
            if captured_parent { return Ok(()); }
            if published_generation.is_some_and(|generation|
                scope_entry(context, scope, name).map(|entry| entry.generation()) != Some(generation)) {
                return Ok(());
            }
            if visible_scope_cell(context, scope, name) == Some(replacement) {
                mark_scope_binding_changed(context, scope, name)?;
            } else {
                write_back_owned_variable_ref_target(scope, name, replacement, context, values)?;
            }
            local_binding_origins::publish_array_local_write(scope, name, origin, context, values)
        })
    })();
    for operand in operands.into_iter().rev() {
        let released = eval_release_value(context, values, operand);
        if result.is_ok() { result = released; }
    }
    result
}

/// Preserves the native WeakMap distinction between an absent offset and an explicit null key.
pub(super) fn reject_weak_map_append(
    object: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let identity = values.object_identity(object)?;
    let class_name = match context.dynamic_object_class_name(identity) {
        Some(name) => name,
        None => runtime_object_class_name(object, values)?,
    };
    if class_name.eq_ignore_ascii_case("WeakMap") {
        return eval_throw_error("Cannot append to WeakMap", context, values);
    }
    Ok(())
}

/// Allocates a normalized integer key only after the live receiver has selected array semantics.
fn normalized_array_string_index(
    index: RuntimeCellHandle, profile: elephc_semantics::SemanticProfile, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if values.type_tag(index)? == EVAL_TAG_STRING {
        if let Some(key) = eval_numeric_string_array_key(&values.string_bytes(index)?, profile) {
            return values.int(key).map(Some);
        }
    }
    Ok(None)
}
