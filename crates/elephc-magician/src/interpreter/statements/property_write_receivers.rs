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
    Guarded(Box<Self>, &'a EvalExpr),
    Late(&'a EvalExpr),
    Value(RuntimeCellHandle, Option<elephc_generics::Type>),
    Static { class: String, property: String },
    Property(Box<Self>, String),
    DynamicProperty(Box<Self>, PreparedWriteIndex<'a>, bool),
    Index(Box<Self>, PreparedWriteIndex<'a>),
}

/// Variable dimensions remain late loads; computed dimensions are independently owned.
#[derive(Clone)]
pub(super) enum PreparedWriteIndex<'a> { Late(&'a EvalExpr), Value(RuntimeCellHandle, Option<ArgumentSource>) }

/// Literal names need no conversion; computed name values are converted after the RHS.
pub(super) enum PreparedPropertyName<'a> { Literal(String), Dynamic(PreparedWriteIndex<'a>) }

/// Evaluates computed receiver operands without fetching or materializing writable properties.
pub(super) fn prepare_property_write_receiver<'a>(
    expr: &'a EvalExpr, owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<PreparedWriteReceiver<'a>, EvalStatus> {
    let plan = match expr {
        EvalExpr::LoadVar(_) | EvalExpr::StaticPropertyGet { .. } => PreparedWriteReceiver::Late(expr),
        EvalExpr::DynamicStaticPropertyGet { class_name, property } => {
            let class = eval_owned_expr(class_name, context, scope, values)?;
            owners.push(class);
            let class = eval_dynamic_class_name(class.borrowed(), context, values)?;
            PreparedWriteReceiver::Static { class, property: property.clone() }
        }
        EvalExpr::DynamicStaticPropertyNameGet { class_name, property } => {
            let class = eval_owned_expr(class_name, context, scope, values)?;
            owners.push(class);
            let class = eval_dynamic_class_name(class.borrowed(), context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            PreparedWriteReceiver::Static { class, property }
        }
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
            let (value, source) = evaluate_receiver_source(expr, context, scope, values)?;
            owners.push(value);
            PreparedWriteReceiver::Value(value, source)
        }
    };
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed)
        && matches!(expr, EvalExpr::PropertyGet { .. } | EvalExpr::StaticPropertyGet { .. } | EvalExpr::ArrayGet { .. }) {
        Ok(PreparedWriteReceiver::Guarded(Box::new(plan), expr))
    } else { Ok(plan) }
}

/// Fetches unset properties for write while keeping every dimension hop non-creating and quiet.
pub(super) fn finish_property_unset_receiver(
    plan: PreparedWriteReceiver<'_>, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    finish_receiver(plan, owners, context, scope, values, true, true).map(|(value, _)| value)
}

/// Fetches an unset dimension receiver whose selected value will feed another dimension.
pub(super) fn finish_dimension_unset_receiver(
    plan: PreparedWriteReceiver<'_>, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    finish_receiver(plan, owners, context, scope, values, true, false).map(|(value, _)| value)
}

/// Resolves the final receiver together with the declaration captured at each actual fetch.
pub(super) fn finish_sourced_write_receiver(plan: PreparedWriteReceiver<'_>, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<elephc_generics::Type>), EvalStatus> {
    finish_receiver(plan, owners, context, scope, values, false, false)
}

/// Captures computed callable/member returns once while variable loads remain late.
pub(super) fn evaluate_receiver_source(expr: &EvalExpr, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<elephc_generics::Type>), EvalStatus> {
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) {
        mixed_eval::evaluate(expr, context, scope, values)
    } else {
        let source = local_binding_origins::source_view(expr, context, scope).as_ref().map(crate::eval_ir::generic_type);
        eval_owned_expr(expr, context, scope, values).map(|value| (value, source))
    }
}

/// Resolves prepared storage with distinct quiet unset and checked write-consumer behavior.
fn finish_receiver(
    plan: PreparedWriteReceiver<'_>, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
    unset: bool, as_object: bool,
) -> Result<(RuntimeCellHandle, Option<elephc_generics::Type>), EvalStatus> {
    let (value, source) = match plan {
        PreparedWriteReceiver::Guarded(plan, expr) => {
            let (value, source) = finish_receiver(*plan, owners, context, scope, values, unset, as_object)?;
            let source = mixed_guards::projection_source(expr, context, scope).as_ref().map(crate::eval_ir::generic_type).or(source);
            return Ok((value, source));
        }
        PreparedWriteReceiver::Late(expr) => return evaluate_receiver_source(expr, context, scope, values),
        PreparedWriteReceiver::Value(value, source) => return values.retain(value).map(|value| (value, source)),
        PreparedWriteReceiver::Static { class, property } => {
            let expr = EvalExpr::StaticPropertyGet { class_name: class.clone(), property: property.clone() };
            let source = local_binding_origins::source_view(&expr, context, scope).as_ref().map(crate::eval_ir::generic_type);
            (eval_static_property_get_result(&class, &property, context, values)?, source)
        }
        PreparedWriteReceiver::Property(parent, property) => {
            let (parent, source) = finish_receiver(*parent, owners, context, scope, values, unset, true)?;
            owners.push(parent);
            if !unset { write_source_domains::object(source.as_ref(), context, values)?; }
            let source = nullsafe_chains::property_source(parent, &property, context, values)?;
            let value = if unset && !matches!(values.type_tag(parent)?, EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE) { values.null()? }
                else if unset { eval_unset_property_get_result(parent, &property, context, values)? }
                else { eval_write_property_get_result(parent, &property, context, values)? };
            (value, source)
        }
        PreparedWriteReceiver::DynamicProperty(parent, property, name_first) => {
            let mut property = Some(property);
            let early = if name_first && !unset { Some(finish_name_with_source(property.take().unwrap(), owners, context, scope, values)?) } else { None };
            let (parent, source) = finish_receiver(*parent, owners, context, scope, values, unset, true)?;
            owners.push(parent);
            if unset && !matches!(values.type_tag(parent)?, EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE) { return values.null().map(|value| (value, None)); }
            let (property, name_owner) = if let Some(property) = early { property } else {
                finish_name_with_source(property.take().unwrap(), owners, context, scope, values)?
            };
            if !unset { write_source_domains::object(source.as_ref(), context, values)?; }
            let source = nullsafe_chains::property_source(parent, &property, context, values)?;
            let selected = if unset { eval_unset_property_get_result(parent, &property, context, values) }
                else { eval_write_property_get_result(parent, &property, context, values) };
            // The temporary name expires after selection, before the outer write fetches storage.
            let selected = selected.and_then(|value| if value.is_borrowed() { values.retain(value) } else { Ok(value) });
            let released = retire_name_source(name_owner, owners, context, values);
            return match (selected, released) {
                (Ok(value), Ok(())) => Ok((value, source)),
                (Ok(value), Err(status)) => { let _ = eval_release_value(context, values, value); Err(status) }
                (Err(status), _) => Err(status),
            };
        }
        PreparedWriteReceiver::Index(parent, index) => {
            let (parent, source) = finish_receiver(*parent, owners, context, scope, values, unset, false)?;
            owners.push(parent);
            let (index, key_source) = finish_index_with_source(index, owners, context, scope, values)?;
            if unset {
                let value = unset_dimension(parent, index, as_object, owners, context, values)?;
                return if value.is_borrowed() { values.retain(value).map(|value| (value, None)) } else { Ok((value, None)) };
            }
            let key_source = write_source_domains::source_type(key_source.as_ref(), index, context, values)?;
            write_source_domains::offset(source.as_ref(), key_source.as_ref(), true, context, values)?;
            let value = match super::super::runtime_ops::simplexml_properties::dimension(
                values, parent.borrowed(), elephc_builtin_contract::ObjectDimension::WriteSelect, vec![index.borrowed()],
            )? {
                Some(value) => value,
                None => eval_array_get_quiet_result(parent, index, context, values)?,
            };
            (value, source.as_ref().and_then(local_binding_origins::declared_offset_result))
        }
    };
    if value.is_borrowed() { values.retain(value).map(|value| (value, source)) } else { Ok((value, source)) }
}

/// Reads an unset dimension without inserting an XML sibling or warning on a missing array key.
fn unset_dimension(
    parent: RuntimeCellHandle, index: RuntimeCellHandle, as_object: bool, owners: &mut Vec<RuntimeCellHandle>,
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
        EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE => {
            require_unset_array_access(parent, context, values)?;
            eval_array_get_quiet_result(parent, index, context, values)
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let exists = values.array_key_exists(index, parent)?;
            let present = values.truthy(exists);
            let released = values.release(exists);
            if !present.and_then(|present| released.map(|()| present))? { return values.null(); }
            eval_array_get_quiet_result(parent, index, context, values)
        }
        EVAL_TAG_NULL => values.null(),
        EVAL_TAG_BOOL if !values.truthy(parent)? => {
            values.warning("Deprecated: Automatic conversion of false to array is deprecated")?;
            values.null()
        }
        EVAL_TAG_STRING => eval_throw_error(if as_object {
            "Cannot use string offset as an object"
        } else { "Cannot use string offset as an array" }, context, values),
        _ => eval_throw_error("Cannot unset offset in a non-array variable", context, values),
    }
}

/// Rejects non-indexable objects with a catchable PHP Error before an unset traversal reads them.
pub(super) fn require_unset_array_access(
    value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if is_unset_closure(value, context, values)? {
        return eval_throw_error("Cannot use object of type Closure as array", context, values);
    }
    if eval_array_access_object_matches(value, context, values)? { return Ok(()); }
    let identity = values.object_identity(value)?;
    let class = match context.dynamic_object_class_name(identity) {
        Some(class) => class.to_string(),
        None => eval_runtime_object_class_name(value, values)?,
    };
    eval_throw_error(&format!("Cannot use object of type {class} as array"), context, values)
}

/// Recognizes host callable cells and eval-backed Closure objects without requiring class hooks.
pub(super) fn is_unset_closure(
    value: RuntimeCellHandle, context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_CALLABLE => Ok(true),
        EVAL_TAG_OBJECT => Ok(values.object_identity(value).ok()
            .is_some_and(|identity| context.dynamic_object_is_class(identity, "Closure"))),
        _ => Ok(false),
    }
}

/// Reads an update target as a lazy native selection, with the ordinary getter as fallback.
pub(super) fn eval_write_property_get_result(
    object: RuntimeCellHandle, property: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    validate_property_write_receiver(object, property, "modify", context, values)?;
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
    with_write_operands(object, None, rest, false, context, scope, values,
        |args, _, _, context, scope, values| consume(args, context, scope, values))
}

/// Preserves dynamic name evaluation before the RHS and receiver fetches after it.
pub(super) fn with_eval_named_property_write_operands<V: RuntimeValueOps>(
    object: &EvalExpr, property: DimensionProperty<'_>, args: &[&EvalExpr],
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], &str, &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    with_write_operands(object, Some(property), args, false, context, scope, values,
        |args, property, _, context, scope, values| consume(args, property.unwrap_or(""), context, scope, values))
}

/// Preserves an owned mutation result while the shared receiver and name leases are retired.
pub(super) fn with_eval_named_property_write_result<V: RuntimeValueOps>(
    object: &EvalExpr, property: DimensionProperty<'_>, args: &[&EvalExpr],
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], &str, &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut produced = None;
    let status = with_eval_named_property_write_operands(object, property, args, context, scope, values,
        |args, property, context, scope, values| {
            produced = Some(consume(args, property, context, scope, values)?);
            Ok(())
        });
    match (status, produced) {
        (Ok(()), Some(value)) => Ok(value),
        (Err(status), value) => {
            if let Some(value) = value { let _ = release_expr_result(value, context, values); }
            Err(status)
        }
        (Ok(()), None) => Err(EvalStatus::RuntimeFatal),
    }
}

/// Preserves source-aware RHS metadata through the same late receiver and member-name owner scope.
pub(super) fn with_eval_sourced_property_write<V: RuntimeValueOps>(
    object: &EvalExpr, property: DimensionProperty<'_>, value: &EvalExpr,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(RuntimeCellHandle, &str, RuntimeCellHandle, Option<&ArgumentSource>,
        &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V) -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    with_write_operands(object, Some(property), &[value], true, context, scope, values,
        |args, property, sources, context, scope, values| {
            consume(args[0], property.unwrap_or(""), args[1], sources[0].as_ref(), context, scope, values)
        })
}

/// Shares receiver preparation and reverse cleanup for literal and runtime-name mutations.
fn with_write_operands<V: RuntimeValueOps>(
    object: &EvalExpr, property: Option<DimensionProperty<'_>>, args: &[&EvalExpr], sourced: bool,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], Option<&str>, &[Option<ArgumentSource>], &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
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
        let mut sources = Vec::with_capacity(args.len());
        for arg in args {
            let (value, source) = if sourced { evaluate_member_value(arg, context, scope, values)? }
                else { (eval_owned_expr(arg, context, scope, values)?, None) };
            owners.push(value);
            suffix.push(value);
            sources.push(source);
        }
        let mut receiver = if matches!(object, EvalExpr::LoadVar(_)) { None } else {
            let (value, source) = finish_sourced_write_receiver(plan.take().ok_or(EvalStatus::RuntimeFatal)?, &mut owners, context, scope, values)?;
            owners.push(value);
            Some((value, source))
        };
        let property = match property {
            Some(PreparedPropertyName::Literal(name)) => Some(name),
            Some(PreparedPropertyName::Dynamic(name)) => Some(finish_name(name, &mut owners, context, scope, values)?),
            None => None,
        };
        if receiver.is_none() {
            let (value, source) = finish_sourced_write_receiver(plan.take().ok_or(EvalStatus::RuntimeFatal)?, &mut owners, context, scope, values)?;
            owners.push(value);
            receiver = Some((value, source));
        }
        let (receiver, receiver_source) = receiver.ok_or(EvalStatus::RuntimeFatal)?;
        write_source_domains::object(receiver_source.as_ref(), context, values)?;
        let mut operands = vec![receiver];
        operands.extend(suffix);
        consume(&operands, property.as_deref(), &sources, context, scope, values)
    })();
    result.and(release_receivers(owners, context, values))
}

/// Captures a computed dimension or name value while leaving a bare variable as a late load.
pub(super) fn prepare_index<'a>(
    expr: &'a EvalExpr, owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<PreparedWriteIndex<'a>, EvalStatus> {
    if matches!(expr, EvalExpr::LoadVar(_)) { return Ok(PreparedWriteIndex::Late(expr)); }
    let (value, source) = evaluate_member_value(expr, context, scope, values)?;
    owners.push(value);
    Ok(PreparedWriteIndex::Value(value, source))
}

/// Reads a late variable only after the RHS and keeps the resulting cell owned through the write.
pub(super) fn finish_index(
    index: PreparedWriteIndex<'_>, owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    finish_index_with_source(index, owners, context, scope, values).map(|(value, _)| value)
}

/// Resolves one prepared key and retains its original producer independently from the receiver source.
pub(super) fn finish_index_with_source(index: PreparedWriteIndex<'_>, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<ArgumentSource>), EvalStatus> {
    match index {
        PreparedWriteIndex::Value(value, source) => Ok((value, source)),
        PreparedWriteIndex::Late(expr) => {
            let (value, source) = evaluate_member_value(expr, context, scope, values)?;
            owners.push(value);
            Ok((value, source))
        }
    }
}

/// Converts a captured name while its source owner remains protected from callback mutations.
pub(super) fn finish_name(
    name: PreparedWriteIndex<'_>, owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    finish_name_with_source(name, owners, context, scope, values).map(|(name, _)| name)
}

/// Keeps the source lease identifiable until its property traversal has selected a value.
fn finish_name_with_source(
    name: PreparedWriteIndex<'_>, owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(String, RuntimeCellHandle), EvalStatus> {
    let source = finish_index(name, owners, context, scope, values)?;
    let converted = eval_string_context_value(source, context, values)?;
    let bytes = values.string_bytes(converted);
    let released = if converted != source { release_expr_result(converted, context, values) } else { Ok(()) };
    let bytes = bytes.and_then(|bytes| released.map(|()| bytes))?;
    String::from_utf8(bytes).map(|name| (name, source)).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Retires a captured name without shifting indices held by dimension receiver plans.
fn retire_name_source(
    source: RuntimeCellHandle, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let index = owners.iter().rposition(|value| *value == source).ok_or(EvalStatus::RuntimeFatal)?;
    // Dimension handoff records refer to capture indices, so leave harmless owned storage.
    let placeholder = values.null()?;
    let source = std::mem::replace(&mut owners[index], placeholder);
    eval_release_value(context, values, source)
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
