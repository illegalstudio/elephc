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
use super::array_parent_publication::{ArrayParentPlace, PreparedArrayParent, prepare_empty_array_parent};
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
        let (key, assigned, producer, key_source) = stage_operands_with_source(staged_index, value, &mut operands, context, scope, values)?;
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
        write_selected(object, &property, key, late_index, op, assigned, producer.as_ref(), key_source, &mut operands,
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
    let (object, source) = super::property_write_receivers::finish_sourced_write_receiver(plan, operands, context, scope, values)?;
    operands.push(object);
    write_source_domains::object(source.as_ref(), context, values)?;
    roots.extend(start..operands.len());
    Ok(object)
}

/// Preserves the original RHS producer alongside the shared early/late index and owner protocol.
pub(super) fn stage_operands_with_source(
    index: Option<&EvalExpr>, value: &EvalExpr, operands: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(Option<RuntimeCellHandle>, RuntimeCellHandle, Option<ArgumentSource>, Option<ArgumentSource>), EvalStatus> {
    let late_key = matches!(index, Some(EvalExpr::LoadVar(_)));
    let (early, mut key_source) = if late_key { (None, None) } else {
        match index {
            Some(key) => { let (value, source) = evaluate_member_value(key, context, scope, values)?; (Some(value), source) }
            None => (None, None),
        }
    };
    if let Some(key) = early { operands.push(key); }
    let (assigned, source) = evaluate_member_value(value, context, scope, values)?;
    operands.push(assigned);
    let key = if late_key {
        let (key, source) = evaluate_member_value(index.ok_or(EvalStatus::RuntimeFatal)?, context, scope, values)?;
        key_source = source;
        operands.push(key);
        Some(key)
    } else { early };
    Ok((key, assigned, source, key_source))
}

/// Selects a native writable child or ordinary property only after key and RHS evaluation.
fn write_selected(
    object: RuntimeCellHandle, property: &str, index: Option<RuntimeCellHandle>, late_index: Option<&EvalExpr>,
    op: Option<EvalBinOp>, assigned: RuntimeCellHandle, producer: Option<&ArgumentSource>, mut key_source: Option<ArgumentSource>, operands: &mut Vec<RuntimeCellHandle>,
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
        let index = finish_dimension_index(index, late_index, &mut key_source, operands, context, scope, values)?;
        return match write_value(current, index, op, assigned, operands, context, values)? {
            None => Ok(()), Some(_) => Err(EvalStatus::RuntimeFatal),
        };
    }
    retire_duplicate_receiver_captures(object, operands, receiver_roots, context, values)?;
    let probed = if late_index.is_some() {
        property_dimension_guards::probe_native_child(object, property, context, values)?
    } else { None };
    // Only a native child needs its key after parent retirement. Other storage retains its
    // previous key-before-getter order, without running ordinary accessors twice.
    let index = if probed.is_none() {
        finish_dimension_index(index, late_index, &mut key_source, operands, context, scope, values)?
    } else { index };
    let late_index = if probed.is_none() { None } else { late_index };
    let selected = if probed.is_none() && index.is_none() && late_index.is_none() && op.is_none() {
        super::property_append_fetch::select(object, property, context, values)?
    } else { None };
    let dynamic = selected.is_some();
    let current = match probed.or(selected) {
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
        let index = finish_dimension_index(index, late_index, &mut key_source, operands, context, scope, values)?;
        return match write_value(current, index, op, assigned, operands, context, values)? {
            None => Ok(()), Some(_) => Err(EvalStatus::RuntimeFatal),
        };
    }
    if matches!(values.type_tag(current)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)
        && property_dimension_guards::array_uses_writable_storage(object, property, context, values)?
        && values.final_object_identity_for_release(object)?.is_some() {
        retire_native_parent(operands, receiver_roots, context, values)?;
        let index = finish_dimension_index(index, late_index, &mut key_source, operands, context, scope, values)?;
        // The parent expired before mutation. Its selected array remains an independent
        // value, so publishing the updated copy back would access destroyed object storage.
        write_value(current, index, op, assigned, operands, context, values)?;
        return Ok(());
    }
    let index = finish_dimension_index(index, late_index, &mut key_source, operands, context, scope, values)?;
    if matches!(values.type_tag(current)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_STRING) {
        if let Some(index) = index {
            let source = write_source_domains::source_type(key_source.as_ref(), index, context, values)?;
            nullsafe_chains::validate_offset_key_source(None, source.as_ref(), context, values)?;
        }
    }
    let data_source = array_mutation_source(index, op, assigned, producer, context, values)?;
    let prepared = if !dynamic
        && property_dimension_guards::has_ordinary_backing_slot(object, property, context, values)?
    {
        prepare_empty_array_parent(
            ArrayParentPlace::Instance(object, property),
            current, index, op.is_some(), operands, context, scope, values)?
    } else { PreparedArrayParent::Snapshot(current) };
    let storage = if matches!(prepared, PreparedArrayParent::Captured(_)) {
        ArrayWriteStorage::Captured
    } else { ArrayWriteStorage::Snapshot };
    prepared.finish_write(index, context, values, |context, values| {
        let contracts = property_dimension_guards::array_contracts(object, property, context, values)?;
        if op.is_none() { write_source_domains::elements(&contracts, prepared.value(), producer, assigned, context, values)?; }
        if let Some(array) = write_value_in_storage(prepared.value(), index, op, assigned, storage, &contracts, operands, context, values)? {
            if matches!(prepared, PreparedArrayParent::Detached(_)) { return Ok(()); }
            if dynamic {
                super::property_append_fetch::write(object, property, array, context, values)?;
            } else {
                eval_array_property_set_result(object, property, array, data_source.as_ref(), context, values)?;
            }
        }
        Ok(())
    })
}

/// Drops duplicate leases of the final receiver without destroying it or shifting capture indices.
fn retire_duplicate_receiver_captures(
    object: RuntimeCellHandle, operands: &mut [RuntimeCellHandle], roots: &[usize],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some((&last, earlier)) = roots.split_last() else { return Ok(()); };
    if operands[last].as_ptr() != object.as_ptr() { return Ok(()); }
    for index in earlier.iter().rev() {
        if operands[*index].as_ptr() != object.as_ptr() { continue; }
        let placeholder = values.null()?;
        let previous = std::mem::replace(&mut operands[*index], placeholder);
        eval_release_value(context, values, previous)?;
    }
    Ok(())
}

/// Reads a bare variable dimension after selection and native parent retirement.
fn finish_dimension_index(
    staged: Option<RuntimeCellHandle>, late: Option<&EvalExpr>, source: &mut Option<ArgumentSource>, operands: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(late) = late else { return Ok(staged); };
    let (key, observed) = evaluate_member_value(late, context, scope, values)?;
    *source = observed;
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
    write_value_in_storage(current, index, op, assigned, ArrayWriteStorage::Snapshot, &[],
        operands, context, values)
}

/// Mutates an attached cell in place so its parent observes payload promotion and replacement.
pub(super) fn write_attached_value(
    current: RuntimeCellHandle, index: Option<RuntimeCellHandle>, op: Option<EvalBinOp>,
    assigned: RuntimeCellHandle, operands: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    write_value_in_storage(current, index, op, assigned, ArrayWriteStorage::Attached, &[],
        operands, context, values)
}

/// Mutates a freshly published ordinary child in place without using SPL storage hooks.
pub(super) fn write_captured_value(
    current: RuntimeCellHandle, index: Option<RuntimeCellHandle>, op: Option<EvalBinOp>,
    assigned: RuntimeCellHandle, operands: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    write_value_in_storage(current, index, op, assigned, ArrayWriteStorage::Captured, &[],
        operands, context, values)
}

/// Distinguishes copied snapshots, published static cells, and attached internal SPL cells.
#[derive(Clone, Copy)]
enum ArrayWriteStorage {
    Snapshot,
    Published,
    Attached,
    Captured,
}

/// Shares dimension semantics while preserving published or attached runtime storage identity.
fn write_value_in_storage(
    current: RuntimeCellHandle, index: Option<RuntimeCellHandle>, op: Option<EvalBinOp>,
    assigned: RuntimeCellHandle, storage: ArrayWriteStorage, declared: &[EvalParameterType], operands: &mut Vec<RuntimeCellHandle>,
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
    let attached = matches!(storage, ArrayWriteStorage::Attached);
    let captured = matches!(storage, ArrayWriteStorage::Captured);
    if values.type_tag(current)? == EVAL_TAG_STRING {
        if let Some(index) = index {
            if op.is_some() {
                return eval_throw_error("Cannot use assign-op operators with string offsets", context, values);
            }
            if matches!(storage, ArrayWriteStorage::Attached) {
                values.spl_storage_array_set(current, index, assigned)?;
                return Ok(None);
            }
            let replacement = eval_string_offset_write(current, index, assigned, context, values)?;
            operands.push(replacement);
            return Ok(Some(replacement));
        }
    }
    checked_array_keys::before_selected_parent_materialization(current, index, context, values)?;
    if attached {
        match values.type_tag(current)? {
            EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_NULL => {}
            EVAL_TAG_BOOL if !values.truthy(current)? => {
                values.warning("Deprecated: Automatic conversion of false to array is deprecated")?;
            }
            EVAL_TAG_STRING => {
                return eval_throw_error("[] operator not supported for strings", context, values);
            }
            _ => return eval_throw_error("Cannot use a scalar value as an array", context, values),
        }
    }
    let index = match index {
        Some(index) if values.type_tag(index)? == EVAL_TAG_STRING => {
            if let Some(key) = eval_numeric_string_array_key(&values.string_bytes(index)?, context.semantic_profile()) {
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
    let reuse = attached || captured
        || matches!(storage, ArrayWriteStorage::Published) && values.is_array_like(current)?;
    let array = if reuse { current }
        else if values.is_array_like(current)? { values.array_clone_shallow(current)? }
        else {
            let was_false = values.type_tag(current)? == EVAL_TAG_BOOL && !values.truthy(current)?;
            eval_false_array_parent_diagnostic(was_false, context, values)?;
            eval_array_autovivification(Some(current), append, context, values)?
        };
    if !reuse {
        operands.push(array);
        if values.type_tag(current)? == EVAL_TAG_ASSOC {
            values.array_copy_index_history(current, array)?;
        }
        context.clone_array_element_aliases(current, array, None);
    }
    let promoted = if attached || captured {
        // The runtime setter promotes the payload through this installed Mixed cell. Building
        // an independent associative box here would detach the write from its captured parent.
        array
    } else {
        eval_array_set_target_for_index(array, index, values)?
    };
    if promoted != array {
        operands.push(promoted);
        values.array_copy_index_history(array, promoted)?;
        context.clone_array_element_aliases(array, promoted, None);
    }
    let array = promoted;
    checked_array_keys::for_receiver(array, index, context, values)?;
    let value = compound_value(array, index, op, assigned, operands, context, values)?;
    for declared in declared {
        validate_generic_local_element(Some(declared), array, index, value, context, values)?;
    }
    for declared in context.local_reference_types(current) {
        validate_generic_local_element(Some(&declared), array, index, value, context, values)?;
    }
    eval_array_element_reference_write(array, index, value, context, values)?;
    if attached {
        values.spl_storage_array_set(array, index, value)?;
    } else {
        values.array_set(array, index, value)?;
    }
    Ok((!attached && !captured).then_some(array))
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
        let (key, assigned, producer, key_source) = stage_operands_with_source(index, value, &mut operands, context, scope, values)?;
        let data_source = array_mutation_source(key, op, assigned, producer.as_ref(), context, values)?;
        let current = eval_static_property_get_result(class_name, property, context, values)?;
        let current = if current.is_borrowed() { values.retain(current)? } else { current };
        operands.push(current);
        let prepared = prepare_empty_array_parent(
            ArrayParentPlace::Static(class_name, property),
            current, key, op.is_some(), &mut operands, context, scope, values)?;
        prepared.finish_write(key, context, values, |context, values| {
            let current = prepared.value();
            let (current, storage) = if matches!(prepared, PreparedArrayParent::Captured(_)) {
                (current, ArrayWriteStorage::Captured)
            } else { match detach_live_static_array(
                class_name, property, current, &mut operands, context, values,
            )? {
                Some(current) => {
                    operands.push(current);
                    (current, ArrayWriteStorage::Published)
                }
                None => (current, ArrayWriteStorage::Snapshot),
            } };
            let contracts = eval_static_property_array_contracts(class_name, property, context, values)?;
            if matches!(values.type_tag(current)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_STRING) {
                if let Some(key) = key {
                    let source = write_source_domains::source_type(key_source.as_ref(), key, context, values)?;
                    nullsafe_chains::validate_offset_key_source(None, source.as_ref(), context, values)?;
                }
            }
            if op.is_none() { write_source_domains::elements(&contracts, current, producer.as_ref(), assigned, context, values)?; }
            if let Some(array) = write_value_in_storage(
                current, key, op, assigned, storage, &contracts, &mut operands, context, values,
            )? {
                if !matches!(prepared, PreparedArrayParent::Detached(_)) {
                    eval_array_static_property_set_result(class_name, property, array, data_source.as_ref(), context, values)?;
                }
            }
            Ok(())
        })
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
