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

use super::*;

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
    let mut result = (|| {
        let early_object = if deferred_receiver(object) { None } else {
            let object = eval_property_write_receiver(object, context, scope, values)?;
            operands.push(object);
            Some(object)
        };
        let property = match property {
            DimensionProperty::Literal(name) => name.to_string(),
            DimensionProperty::Dynamic(name) => eval_dynamic_member_name(name, context, scope, values)?,
        };
        let (key, assigned) = stage_operands(index, value, &mut operands, context, scope, values)?;
        let object = if let Some(object) = early_object { object } else {
            let object = eval_property_write_receiver(object, context, scope, values)?;
            operands.push(object);
            object
        };
        write_selected(object, &property, key, op, assigned, &mut operands, context, values)
    })();
    for operand in operands.into_iter().rev() {
        let released = eval_release_value(context, values, operand);
        if result.is_ok() { result = released; }
    }
    result
}

/// Evaluates a computed key before the RHS and a variable key after it, each exactly once.
fn stage_operands(
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

/// Recognizes storage-only chains whose fetches belong after assignment operands.
fn deferred_receiver(object: &EvalExpr) -> bool {
    match object {
        EvalExpr::LoadVar(_) | EvalExpr::StaticPropertyGet { .. } => true,
        EvalExpr::PropertyGet { object, .. } => deferred_receiver(object),
        _ => false,
    }
}

/// Selects a native writable child or ordinary property only after key and RHS evaluation.
fn write_selected(
    object: RuntimeCellHandle, property: &str, index: Option<RuntimeCellHandle>,
    op: Option<EvalBinOp>, assigned: RuntimeCellHandle, operands: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let current = match super::super::runtime_ops::simplexml_properties::invoke(
        values, object, property, elephc_builtin_contract::ObjectProperty::WriteFetch, None,
    )? {
        Some(child) => child,
        None => eval_property_get_result(object, property, context, values)?,
    };
    let current = if current.is_borrowed() { values.retain(current)? } else { current };
    operands.push(current);
    if let Some(array) = write_value(current, index, op, assigned, operands, context, values)? {
        eval_property_set_result(object, property, array, context, values)?;
    }
    Ok(())
}

/// Writes an already selected container and returns an independent array requiring writeback.
fn write_value(
    current: RuntimeCellHandle, index: Option<RuntimeCellHandle>, op: Option<EvalBinOp>,
    assigned: RuntimeCellHandle, operands: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
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
    let index = match index {
        Some(index) if values.type_tag(index)? == EVAL_TAG_STRING => {
            if let Some(key) = eval_numeric_string_array_key(&values.string_bytes(index)?) {
                let key = values.int(key)?; operands.push(key); key
            } else { index }
        }
        Some(index) => index,
        None => {
            let key = if values.is_array_like(current)? { eval_array_append_key(current, values)? }
                else { values.int(0)? };
            operands.push(key); key
        },
    };
    let array = if values.is_array_like(current)? { values.array_clone_shallow(current)? }
        else { values.array_new(1)? };
    operands.push(array);
    context.clone_array_element_aliases(current, array, None);
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
        if let Some(array) = write_value(current, key, op, assigned, &mut operands, context, values)? {
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
