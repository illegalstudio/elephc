//! Purpose:
//! Stages property dimension write operands before selecting live receiver storage.
//!
//! Called from:
//! - Literal and runtime-name property append and indexed assignment statements.
//!
//! Key details:
//! - Computed receivers run once before operands; variable and static loads run afterward.
//! - Compound reads follow RHS evaluation and native write-fetch, including in eval.
//! - Every retained operand is released after normal completion or an exception.
//! - Eval-owned static arrays detach their boxed cell before runtime payload COW writes.

use super::*;
use super::property_write_receivers::{PreparedPropertyName, prepare_index, finish_name};

/// Supplies a literal property or a computed member name at its original source position.
pub(super) enum DimensionProperty<'a> {
    Literal(&'a str),
    Dynamic(&'a EvalExpr),
}

/// Executes a dimension write without snapshotting a variable or static receiver before the RHS.
pub(super) fn eval_property_dimension_write(
    object: &EvalExpr, property: DimensionProperty<'_>, index: Option<&EvalExpr>,
    op: Option<EvalBinOp>, value: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut operands = Vec::new();
    let mut receiver_roots = Vec::new();
    let mut result = (|| {
        let mut receiver = Some(prepare_property_write_receiver(object, &mut operands, context, scope, values)?);
        receiver_roots.extend(0..operands.len());
        let property = match property {
            DimensionProperty::Literal(name) => PreparedPropertyName::Literal(name.to_string()),
            DimensionProperty::Dynamic(name) => PreparedPropertyName::Dynamic(
                prepare_index(name, &mut operands, context, scope, values)?),
        };
        let late_index = index.filter(|index| matches!(index, EvalExpr::LoadVar(_)));
        let staged_index = if late_index.is_some() { None } else { index };
        let (key, assigned) = stage_operands(staged_index, value, &mut operands, context, scope, values)?;
        let early = if matches!(object, EvalExpr::LoadVar(_)) { None } else {
            let object = finish_dimension_receiver(receiver.take().ok_or(EvalStatus::RuntimeFatal)?,
                &mut operands, &mut receiver_roots, context, scope, values)?;
            Some(object)
        };
        let property = match property {
            PreparedPropertyName::Literal(name) => name,
            PreparedPropertyName::Dynamic(name) => finish_name(name, &mut operands, context, scope, values)?,
        };
        let object = if let Some(object) = early { object } else {
            let object = finish_dimension_receiver(receiver.take().ok_or(EvalStatus::RuntimeFatal)?,
                &mut operands, &mut receiver_roots, context, scope, values)?;
            object
        };
        write_selected(object, &property, key, late_index, op, assigned, &mut operands,
            &receiver_roots, context, scope, values)
    })();
    for operand in operands.into_iter().rev() {
        let released = eval_release_value(context, values, operand);
        if result.is_ok() { result = released; }
    }
    result
}

/// Records receiver-only captures so a native child can outlive its retired parent.
fn finish_dimension_receiver(
    plan: super::property_write_receivers::PreparedWriteReceiver<'_>, operands: &mut Vec<RuntimeCellHandle>,
    roots: &mut Vec<usize>, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let start = operands.len();
    let object = finish_property_write_receiver(plan, operands, context, scope, values)?;
    operands.push(object);
    roots.extend(start..operands.len());
    Ok(object)
}

/// Evaluates a computed key before the RHS and a variable key after it, each exactly once.
pub(super) fn stage_operands(
    index: Option<&EvalExpr>, value: &EvalExpr, operands: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(Option<RuntimeCellHandle>, RuntimeCellHandle), EvalStatus> {
    let late_key = matches!(index, Some(EvalExpr::LoadVar(_)));
    let early = if late_key { None } else {
        index.map(|key| eval_owned_expr(key, context, scope, values)).transpose()?
    };
    if let Some(key) = early { operands.push(key); }
    let assigned = eval_owned_expr(value, context, scope, values)?;
    operands.push(assigned);
    let key = if late_key {
        let key = eval_owned_expr(index.ok_or(EvalStatus::RuntimeFatal)?, context, scope, values)?;
        operands.push(key);
        Some(key)
    } else { early };
    Ok((key, assigned))
}

/// Selects a native writable child or ordinary property only after key and RHS evaluation.
fn write_selected(
    object: RuntimeCellHandle, property: &str, index: Option<RuntimeCellHandle>, late_index: Option<&EvalExpr>,
    op: Option<EvalBinOp>, assigned: RuntimeCellHandle, operands: &mut Vec<RuntimeCellHandle>,
    receiver_roots: &[usize], context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    validate_property_write_receiver(object, property, "modify", context, values)?;
    let native = super::super::runtime_ops::simplexml_properties::invoke(
        values, object, property, elephc_builtin_contract::ObjectProperty::WriteFetch, None,
    )?;
    if let Some(current) = native {
        let current = if current.is_borrowed() { values.retain(current)? } else { current };
        operands.push(current);
        retire_native_parent(operands, receiver_roots, context, values)?;
        let index = finish_dimension_index(index, late_index, operands, context, scope, values)?;
        return match write_value(current, index, op, assigned, operands, context, values)? {
            None => Ok(()), Some(_) => Err(EvalStatus::RuntimeFatal),
        };
    }
    let selected = if index.is_none() && late_index.is_none() && op.is_none() {
        super::property_append_fetch::select(object, property, context, values)?
    } else { None };
    let dynamic = selected.is_some();
    let current = match selected {
        Some(current) => current,
        None => property_dimension_guards::ordinary_property(object, property,
            if op.is_some() { property_dimension_guards::DimensionFetchMode::ReadWrite }
            else { property_dimension_guards::DimensionFetchMode::Write }, context, values)?,
    };
    let current = if current.is_borrowed() { values.retain(current)? } else { current };
    operands.push(current);
    let stored = if matches!(values.type_tag(current)?, EVAL_TAG_OBJECT | EVAL_TAG_NULL) {
        property_dimension_guards::native_child_uses_storage(object, property, current, context, values)?
    } else { false };
    if stored && super::super::runtime_ops::simplexml_properties::is_native(values, current, "")? {
        retire_native_parent(operands, receiver_roots, context, values)?;
        let index = finish_dimension_index(index, late_index, operands, context, scope, values)?;
        return match write_value(current, index, op, assigned, operands, context, values)? {
            None => Ok(()), Some(_) => Err(EvalStatus::RuntimeFatal),
        };
    }
    let index = finish_dimension_index(index, late_index, operands, context, scope, values)?;
    if let Some(array) = write_value(current, index, op, assigned, operands, context, values)? {
        if dynamic {
            super::property_append_fetch::write(object, property, array, context, values)?;
        } else {
            eval_property_set_result(object, property, array, context, values)?;
        }
    }
    Ok(())
}

/// Reads a bare variable dimension after selection and native parent retirement.
fn finish_dimension_index(
    staged: Option<RuntimeCellHandle>, late: Option<&EvalExpr>, operands: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(late) = late else { return Ok(staged); };
    let key = eval_owned_expr(late, context, scope, values)?;
    operands.push(key);
    Ok(Some(key))
}

/// Retires receiver captures after its selected native child is independently rooted.
fn retire_native_parent(
    operands: &mut Vec<RuntimeCellHandle>, receiver_roots: &[usize],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    // Remove each capture before releasing it so throwing destructors cannot cause a second
    // release during outer cleanup. Reversed indices preserve every remaining capture index.
    for root in receiver_roots.iter().rev() {
        let owner = operands.remove(*root);
        eval_release_value(context, values, owner)?;
    }
    Ok(())
}

/// Writes an already selected container and returns an independent array requiring writeback.
pub(super) fn write_value(
    current: RuntimeCellHandle, index: Option<RuntimeCellHandle>, op: Option<EvalBinOp>,
    assigned: RuntimeCellHandle, operands: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    write_value_in_storage(current, index, op, assigned, ArrayWriteStorage::Snapshot,
        operands, context, values)
}

/// Distinguishes a getter snapshot from an independent cell already published in a static slot.
enum ArrayWriteStorage {
    Snapshot,
    Published,
}

/// Shares dimension semantics while allowing a published static cell to use runtime payload COW.
fn write_value_in_storage(
    current: RuntimeCellHandle, index: Option<RuntimeCellHandle>, op: Option<EvalBinOp>,
    assigned: RuntimeCellHandle, storage: ArrayWriteStorage, operands: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let append = index.is_none();
    if values.type_tag(current)? == EVAL_TAG_OBJECT {
        if !eval_array_access_object_matches(current, context, values)? { return Err(EvalStatus::RuntimeFatal); }
        if index.is_none() {
            reject_weak_map_append(current, context, values)?;
            if let Some(returned) = super::super::runtime_ops::simplexml_properties::append(values, current, assigned)? {
                operands.push(returned);
                return Ok(None);
            }
        }
        let index = match index {
            Some(index) => index,
            None => { let key = values.null()?; operands.push(key); key },
        };
        let value = compound_value(current, index, op, assigned, operands, context, values)?;
        let returned = match super::super::runtime_ops::simplexml_properties::dimension(
            values, current, elephc_builtin_contract::ObjectDimension::Set, vec![index, value],
        )? {
            Some(value) => value,
            None => eval_method_call_result(current, "offsetSet", vec![index, value], context, values)?,
        };
        operands.push(returned);
        return Ok(None);
    }
    if values.type_tag(current)? == EVAL_TAG_STRING {
        if let Some(index) = index {
            if op.is_some() {
                return eval_throw_error("Cannot use assign-op operators with string offsets", context, values);
            }
            let replacement = eval_string_offset_write(current, index, assigned, context, values)?;
            operands.push(replacement);
            return Ok(Some(replacement));
        }
    }
    let index = match index {
        Some(index) if values.type_tag(index)? == EVAL_TAG_STRING => {
            if let Some(key) = eval_numeric_string_array_key(&values.string_bytes(index)?) {
                let key = values.int(key)?; operands.push(key); key
            } else { index }
        }
        Some(index) => index,
        None => {
            let key = if values.is_array_like(current)? {
                super::array_append::eval_array_append_index(current, context, values)?
            }
                else { values.int(0)? };
            operands.push(key); key
        },
    };
    let reuse = matches!(storage, ArrayWriteStorage::Published) && values.is_array_like(current)?;
    let array = if reuse { current }
        else if values.is_array_like(current)? { values.array_clone_shallow(current)? }
        else { eval_array_autovivification(Some(current), append, context, values)? };
    if !reuse {
        operands.push(array);
        if values.type_tag(current)? == EVAL_TAG_ASSOC {
            values.array_copy_index_history(current, array)?;
        }
        context.clone_array_element_aliases(current, array, None);
    }
    let promoted = eval_array_set_target_for_index(array, index, values)?;
    if promoted != array {
        operands.push(promoted);
        values.array_copy_index_history(array, promoted)?;
        context.clone_array_element_aliases(array, promoted, None);
    }
    let array = promoted;
    let value = compound_value(array, index, op, assigned, operands, context, values)?;
    for declared in context.local_reference_types(current) {
        validate_generic_local_element(Some(&declared), array, index, value, context, values)?;
    }
    eval_array_element_reference_write(array, index, value, context, values)?;
    values.array_set(array, index, value)?;
    Ok(Some(array))
}

/// Delays direct static-property append until the RHS has selected the live static slot.
pub(super) fn eval_static_property_array_append_result(
    class_name: &str, property: &str, value: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    static_write(class_name, property, None, None, value, context, scope, values)
}

/// Delays direct static-property dimension assignment and compound reads until operands finish.
pub(super) fn eval_static_property_array_set_result(
    class_name: &str, property: &str, index: &EvalExpr, op: Option<EvalBinOp>, value: &EvalExpr,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    static_write(class_name, property, Some(index), op, value, context, scope, values)
}

/// Roots static operands and the post-operand receiver until insertion or exceptional cleanup.
fn static_write(
    class_name: &str, property: &str, index: Option<&EvalExpr>, op: Option<EvalBinOp>, value: &EvalExpr,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut operands = Vec::new();
    let mut result = (|| {
        let (key, assigned) = stage_operands(index, value, &mut operands, context, scope, values)?;
        let current = eval_static_property_get_result(class_name, property, context, values)?;
        let current = if current.is_borrowed() { values.retain(current)? } else { current };
        operands.push(current);
        let (current, storage) = match detach_live_static_array(
            class_name, property, current, &mut operands, context, values,
        )? {
            Some(current) => {
                operands.push(current);
                (current, ArrayWriteStorage::Published)
            }
            None => (current, ArrayWriteStorage::Snapshot),
        };
        if let Some(array) = write_value_in_storage(
            current, key, op, assigned, storage, &mut operands, context, values,
        )? {
            eval_static_property_set_result(class_name, property, array, context, values)?;
        }
        Ok(())
    })();
    for operand in operands.into_iter().rev() {
        let released = eval_release_value(context, values, operand);
        if result.is_ok() { result = released; }
    }
    result
}

/// Publishes a detached eval static box and retires its old receiver lease before payload mutation.
fn detach_live_static_array(
    class_name: &str, property: &str, current: RuntimeCellHandle,
    operands: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !values.is_array_like(current)? || values.is_reference(current)? { return Ok(None); }
    let resolved = resolve_eval_static_member_class_name(class_name, context)?;
    let Some((declaring, declared)) = context.class_property(&resolved, property) else { return Ok(None); };
    if !declared.is_static()
        || context.static_property_alias(&declaring, declared.name()).is_some()
        || context.static_property(&declaring, declared.name()) != Some(current)
    { return Ok(None); }

    // The same-cell setter validates write visibility and types without changing slot ownership.
    eval_static_property_set_result(&resolved, property, current, context, values)?;
    let replacement = copy_scope_value(current, context, values)?;
    // This transfers the new owner to storage even if retiring the displaced owner reports an error.
    store_static_property_value(&declaring, declared.name(), replacement, context, values)?;
    let receiver = operands.pop().ok_or(EvalStatus::RuntimeFatal)?;
    debug_assert_eq!(receiver, current);
    eval_release_value(context, values, receiver)?;
    // Retaining the box protects it from reentrant slot replacement without sharing its payload.
    values.retain(replacement).map(Some)
}

/// Applies a compound operator to the selected post-RHS dimension without reevaluating the RHS.
fn compound_value(
    array: RuntimeCellHandle, index: RuntimeCellHandle, op: Option<EvalBinOp>,
    assigned: RuntimeCellHandle, operands: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(op) = op else { return Ok(assigned); };
    let current = eval_array_get_result(array, index, context, values)?;
    let current = if current.is_borrowed() { values.retain(current)? } else { current };
    operands.push(current);
    let result = eval_binary_result(op, current, assigned, context, values)?;
    let result = if result.is_borrowed() { values.retain(result)? } else { result };
    operands.push(result);
    Ok(result)
}
