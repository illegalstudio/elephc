//! Purpose:
//! Prepare eval property write receivers before operands and resolve them afterward.
//!
//! Called from:
//! - Property assignment, updates, and dimension writes.
//!
//! Key details:
//! - Computed roots and keys run once; storage loads remain live across the RHS.
//! - Native XML selectors differ from nullable reads; ordinary getters keep their semantics.
//! - Every captured owner is released on success or failure.

use super::*;
use super::property_dimension_writes::DimensionProperty;

/// A receiver traversal with computed operands captured before the assignment RHS.
pub(super) enum PreparedWriteReceiver<'a> {
    Late(&'a EvalExpr),
    Value(RuntimeCellHandle),
    Property(Box<Self>, String),
    DynamicProperty(Box<Self>, PreparedWriteIndex<'a>, bool),
    Index(Box<Self>, PreparedWriteIndex<'a>),
}

/// Variable dimensions remain late loads; computed dimensions are independently owned.
#[derive(Clone, Copy)]
pub(super) enum PreparedWriteIndex<'a> { Late(&'a EvalExpr), Value(RuntimeCellHandle) }

/// Literal names need no conversion; computed name values are converted after the RHS.
pub(super) enum PreparedPropertyName<'a> { Literal(String), Dynamic(PreparedWriteIndex<'a>) }

/// Evaluates computed receiver operands without fetching or materializing writable properties.
pub(super) fn prepare_property_write_receiver<'a>(
    expr: &'a EvalExpr, owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<PreparedWriteReceiver<'a>, EvalStatus> {
    Ok(match expr {
        EvalExpr::LoadVar(_) | EvalExpr::StaticPropertyGet { .. } => PreparedWriteReceiver::Late(expr),
        EvalExpr::PropertyGet { object, property } => PreparedWriteReceiver::Property(
            Box::new(prepare_property_write_receiver(object, owners, context, scope, values)?), property.clone()),
        EvalExpr::DynamicPropertyGet { object, property } => {
            let parent = prepare_property_write_receiver(object, owners, context, scope, values)?;
            let property = prepare_index(property, owners, context, scope, values)?;
            PreparedWriteReceiver::DynamicProperty(Box::new(parent), property, matches!(object.as_ref(), EvalExpr::LoadVar(_)))
        }
        EvalExpr::ArrayGet { array, index } => {
            let parent = prepare_property_write_receiver(array, owners, context, scope, values)?;
            let index = prepare_index(index, owners, context, scope, values)?;
            PreparedWriteReceiver::Index(Box::new(parent), index)
        }
        _ => {
            let value = eval_owned_expr(expr, context, scope, values)?;
            owners.push(value);
            PreparedWriteReceiver::Value(value)
        }
    })
}

/// Resolves a prepared receiver after the RHS while retaining every traversed parent.
pub(super) fn finish_property_write_receiver(
    plan: PreparedWriteReceiver<'_>, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    finish_receiver(plan, owners, context, scope, values, false)
}

/// Fetches unset properties for write while keeping every dimension hop non-creating and quiet.
pub(super) fn finish_property_unset_receiver(
    plan: PreparedWriteReceiver<'_>, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    finish_receiver(plan, owners, context, scope, values, true)
}

/// Resolves prepared storage with distinct property and dimension behavior for unset.
fn finish_receiver(
    plan: PreparedWriteReceiver<'_>, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
    unset: bool,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = match plan {
        PreparedWriteReceiver::Late(expr) => return eval_owned_expr(expr, context, scope, values),
        PreparedWriteReceiver::Value(value) => return values.retain(value),
        PreparedWriteReceiver::Property(parent, property) => {
            let parent = finish_receiver(*parent, owners, context, scope, values, unset)?;
            owners.push(parent);
            if unset && values.type_tag(parent)? != EVAL_TAG_OBJECT { values.null()? }
            else if unset { eval_unset_property_get_result(parent, &property, context, values)? }
            else { eval_write_property_get_result(parent, &property, context, values)? }
        }
        PreparedWriteReceiver::DynamicProperty(parent, property, name_first) => {
            let early = if name_first && !unset { Some(finish_name(property, owners, context, scope, values)?) } else { None };
            let parent = finish_receiver(*parent, owners, context, scope, values, unset)?;
            owners.push(parent);
            if unset && values.type_tag(parent)? != EVAL_TAG_OBJECT { return values.null(); }
            let property = if let Some(property) = early { property } else {
                finish_name(property, owners, context, scope, values)?
            };
            if unset { eval_unset_property_get_result(parent, &property, context, values)? }
            else { eval_write_property_get_result(parent, &property, context, values)? }
        }
        PreparedWriteReceiver::Index(parent, index) => {
            let parent = finish_receiver(*parent, owners, context, scope, values, unset)?;
            owners.push(parent);
            let index = finish_index(index, owners, context, scope, values)?;
            if unset {
                let value = unset_dimension(parent, index, owners, context, values)?;
                return if value.is_borrowed() { values.retain(value) } else { Ok(value) };
            }
            match super::super::runtime_ops::simplexml_properties::dimension(
                values, parent.borrowed(), elephc_builtin_contract::ObjectDimension::WriteSelect, vec![index.borrowed()],
            )? {
                Some(value) => value,
                None => eval_array_get_result(parent, index, context, values)?,
            }
        }
    };
    if value.is_borrowed() { values.retain(value) } else { Ok(value) }
}

/// Reads an unset dimension without inserting an XML sibling or warning on a missing array key.
fn unset_dimension(
    parent: RuntimeCellHandle, index: RuntimeCellHandle, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if super::super::runtime_ops::simplexml_properties::is_native(values, parent, "")? {
        let no_create = values.bool_value(false)?;
        owners.push(no_create);
        return super::super::runtime_ops::simplexml_properties::dimension(values, parent.borrowed(),
            elephc_builtin_contract::ObjectDimension::WriteSelect,
            vec![index.borrowed(), no_create.borrowed(), no_create.borrowed()],
        )?.ok_or(EvalStatus::RuntimeFatal);
    }
    match values.type_tag(parent)? {
        EVAL_TAG_OBJECT => eval_array_get_result(parent, index, context, values),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let exists = values.array_key_exists(index, parent)?;
            let present = values.truthy(exists);
            let released = values.release(exists);
            if !present.and_then(|present| released.map(|()| present))? { return values.null(); }
            eval_array_get_result(parent, index, context, values)
        }
        _ => values.null(),
    }
}

/// Reads an update target as a lazy native selection, with the ordinary getter as fallback.
pub(super) fn eval_write_property_get_result(
    object: RuntimeCellHandle, property: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match super::super::runtime_ops::simplexml_properties::invoke(
        values, object, property, elephc_builtin_contract::ObjectProperty::WriteSelect, None,
    )? {
        Some(value) => Ok(value),
        None => eval_property_get_result(object, property, context, values),
    }
}

/// Stages the first operand as a write receiver and all following operands as ordinary values.
pub(super) fn with_eval_property_write_operands<V: RuntimeValueOps>(
    args: &[&EvalExpr], context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let Some((object, rest)) = args.split_first() else { return consume(&[], context, scope, values); };
    with_write_operands(object, None, rest, context, scope, values,
        |args, _, context, scope, values| consume(args, context, scope, values))
}

/// Preserves dynamic name evaluation before the RHS and receiver fetches after it.
pub(super) fn with_eval_named_property_write_operands<V: RuntimeValueOps>(
    object: &EvalExpr, property: DimensionProperty<'_>, args: &[&EvalExpr],
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], &str, &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    with_write_operands(object, Some(property), args, context, scope, values,
        |args, property, context, scope, values| consume(args, property.unwrap_or(""), context, scope, values))
}

/// Shares receiver preparation and reverse cleanup for literal and runtime-name mutations.
fn with_write_operands<V: RuntimeValueOps>(
    object: &EvalExpr, property: Option<DimensionProperty<'_>>, args: &[&EvalExpr],
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], Option<&str>, &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let mut owners = Vec::new();
    let result = (|| {
        let mut plan = Some(prepare_property_write_receiver(object, &mut owners, context, scope, values)?);
        let property = match property {
            Some(DimensionProperty::Literal(name)) => Some(PreparedPropertyName::Literal(name.to_string())),
            Some(DimensionProperty::Dynamic(expr)) => Some(PreparedPropertyName::Dynamic(
                prepare_index(expr, &mut owners, context, scope, values)?)),
            None => None,
        };
        let mut suffix = Vec::with_capacity(args.len());
        for arg in args {
            let value = eval_owned_expr(arg, context, scope, values)?;
            owners.push(value);
            suffix.push(value);
        }
        let mut receiver = if matches!(object, EvalExpr::LoadVar(_)) { None } else {
            let value = finish_property_write_receiver(plan.take().ok_or(EvalStatus::RuntimeFatal)?, &mut owners, context, scope, values)?;
            owners.push(value);
            Some(value)
        };
        let property = match property {
            Some(PreparedPropertyName::Literal(name)) => Some(name),
            Some(PreparedPropertyName::Dynamic(name)) => Some(finish_name(name, &mut owners, context, scope, values)?),
            None => None,
        };
        if receiver.is_none() {
            let value = finish_property_write_receiver(plan.take().ok_or(EvalStatus::RuntimeFatal)?, &mut owners, context, scope, values)?;
            owners.push(value);
            receiver = Some(value);
        }
        let receiver = receiver.ok_or(EvalStatus::RuntimeFatal)?;
        let mut operands = vec![receiver];
        operands.extend(suffix);
        consume(&operands, property.as_deref(), context, scope, values)
    })();
    result.and(release_receivers(owners, context, values))
}

/// Captures a computed dimension or name value while leaving a bare variable as a late load.
pub(super) fn prepare_index<'a>(
    expr: &'a EvalExpr, owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<PreparedWriteIndex<'a>, EvalStatus> {
    if matches!(expr, EvalExpr::LoadVar(_)) { return Ok(PreparedWriteIndex::Late(expr)); }
    let value = eval_owned_expr(expr, context, scope, values)?;
    owners.push(value);
    Ok(PreparedWriteIndex::Value(value))
}

/// Reads a late variable only after the RHS and keeps the resulting cell owned through the write.
pub(super) fn finish_index(
    index: PreparedWriteIndex<'_>, owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match index {
        PreparedWriteIndex::Value(value) => Ok(value),
        PreparedWriteIndex::Late(expr) => {
            let value = eval_owned_expr(expr, context, scope, values)?;
            owners.push(value);
            Ok(value)
        }
    }
}

/// Converts a captured name while its source owner remains protected from callback mutations.
pub(super) fn finish_name(
    name: PreparedWriteIndex<'_>, owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let source = finish_index(name, owners, context, scope, values)?;
    let converted = eval_string_context_value(source, context, values)?;
    let bytes = values.string_bytes(converted);
    let released = if converted != source { release_expr_result(converted, context, values) } else { Ok(()) };
    let bytes = bytes.and_then(|bytes| released.map(|()| bytes))?;
    String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Releases all captures even when an earlier receiver destructor throws.
pub(super) fn release_receivers(
    operands: Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut result = Ok(());
    for operand in operands.into_iter().rev() {
        let released = eval_release_value(context, values, operand);
        if result.is_ok() { result = released; }
    }
    result
}
