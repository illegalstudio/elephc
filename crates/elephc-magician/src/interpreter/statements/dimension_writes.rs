//! Purpose:
//! Mutates attached nested array paths and native object dimensions in opaque eval.
//!
//! Called from:
//! - The DimensionWrite statement dispatcher.
//!
//! Key details:
//! - Computed roots and keys are prepared before the RHS, with live storage resolved afterward.
//! - Array replacements propagate through retained parents; object mutations keep shared identity.
//! - Valid prefix keys publish null children before a later leaf-key failure.
//! - Every intermediate is released in reverse order on success and failure.

use super::*;
use super::property_dimension_writes::{
    stage_operands_with_source, write_attached_value, write_captured_value, write_value,
};
use super::array_parent_publication::{ArrayParentPlace, PreparedArrayParent, prepare_empty_array_parent};
use super::property_dimension_guards::{DimensionFetchMode, selected_property};
use super::property_write_receivers::{PreparedWriteReceiver, finish_index_with_source, finish_name};

/// One traversal parent which can receive a copied child or terminate attached propagation.
enum Parent {
    Variable(String),
    Discard,
    Static { class: String, property: String },
    Property { object: RuntimeCellHandle, property: String },
    Index { array: RuntimeCellHandle, index: RuntimeCellHandle, storage: ResolvedStorage },
}

/// Selects copy-on-write, SPL-attached or freshly captured mutation for one traversal value.
#[derive(Clone, Copy)]
enum ResolvedStorage {
    Snapshot,
    Attached,
    Captured,
}

/// One selected traversal value and the storage path needed for its next write.
struct ResolvedValue {
    value: RuntimeCellHandle,
    storage: ResolvedStorage,
    source: Option<elephc_generics::Type>,
}

/// Prepares source operands once and commits array replacements from the leaf toward its root.
pub(super) fn eval_dimension_write(
    array: &EvalExpr, index: Option<&EvalExpr>, op: Option<EvalBinOp>, value: &EvalExpr,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut owners = Vec::new();
    let mut result = (|| {
        let plan = prepare_property_write_receiver(array, &mut owners, context, scope, values)?;
        let (key, assigned, producer, key_source) = stage_operands_with_source(index, value, &mut owners, context, scope, values)?;
        let mut parents = Vec::new();
        let mode = if op.is_some() { DimensionFetchMode::ReadWrite } else { DimensionFetchMode::Write };
        let mut current = resolve(plan, &mut parents, &mut owners, mode, true, context, scope, values)?;
        let key_source = key.map(|key| write_source_domains::source_type(key_source.as_ref(), key, context, values)).transpose()?.flatten();
        write_source_domains::offset(current.source.as_ref(), key_source.as_ref(), key.is_some(), context, values)?;
        if op.is_none() {
            let contracts = current.source.as_ref().map(crate::eval_ir::parameter_type).into_iter().collect::<Vec<_>>();
            write_source_domains::elements(&contracts, current.value, producer.as_ref(), assigned, context, values)?;
        }
        if op.is_some() {
            if let [Parent::Variable(name)] = parents.as_slice() {
                let prepared = prepare_empty_array_parent(ArrayParentPlace::Variable(name), current.value,
                    key, true, &mut owners, context, scope, values)?;
                current.value = prepared.value();
                if matches!(prepared, PreparedArrayParent::Detached(_)) { parents[0] = Parent::Discard; }
            }
        }
        if values.type_tag(current.value)? != EVAL_TAG_OBJECT
            && !parents.iter().any(|parent| !matches!(parent, Parent::Index { .. })) {
            return eval_throw_error("Unsupported dimension write: temporary arrays have no attached storage", context, values);
        }
        let data_source = array_mutation_source(key, op, assigned, producer.as_ref(), context, values)?;
        let replacement = match current.storage {
            ResolvedStorage::Snapshot => {
                write_value(current.value, key, op, assigned, &mut owners, context, values)?
            }
            ResolvedStorage::Attached => {
                write_attached_value(current.value, key, op, assigned, &mut owners, context, values)?
            }
            ResolvedStorage::Captured => {
                write_captured_value(current.value, key, op, assigned, &mut owners, context, values)?
            }
        };
        propagate_replacement(replacement, &mut parents, data_source, &mut owners, context, scope, values)?;
        Ok(())
    })();
    for owner in owners.into_iter().rev() {
        let released = eval_release_value(context, values, owner);
        if result.is_ok() { result = released; }
    }
    result
}

/// Commits one copied child through every retained parent until persistent storage owns it.
fn propagate_replacement(
    mut replacement: Option<RuntimeCellHandle>, parents: &mut Vec<Parent>,
    mut data_source: Option<elephc_generics::Type>, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    while let Some(value) = replacement {
        let Some(parent) = parents.pop() else { break; };
        replacement = match parent {
            Parent::Discard => None,
            Parent::Variable(name) => {
                let origin = local_binding_origins::prepare_array_nested_write(
                    scope, &name, value, data_source.as_ref(), context, values,
                )?;
                validate_generic_local_contracts(scope, &name, value, context, values)?;
                write_back_owned_variable_ref_target(scope, &name, value, context, values)?;
                local_binding_origins::publish_array_local_write(scope, &name, origin, context, values)?;
                None
            }
            Parent::Static { class, property } => {
                eval_array_static_property_set_result(
                    &class, &property, value, data_source.as_ref(), context, values,
                )?;
                None
            }
            Parent::Property { object, property } => {
                eval_array_property_set_result(
                    object, &property, value, data_source.as_ref(), context, values,
                )?;
                None
            }
            Parent::Index { array, index, storage } => {
                if let Some(source) = data_source.take() {
                    data_source = Some(local_binding_origins::wrap_nested_array_source(
                        Some(index), source, context, values,
                    )?);
                }
                match storage {
                    ResolvedStorage::Snapshot => {
                        write_value(array, Some(index), None, value, owners, context, values)?
                    }
                    ResolvedStorage::Attached => {
                        write_attached_value(array, Some(index), None, value, owners, context, values)?
                    }
                    ResolvedStorage::Captured => {
                        write_captured_value(array, Some(index), None, value, owners, context, values)?
                    }
                }
            }
        };
    }
    Ok(())
}

/// Retains a traversal value and records the storage needed only if an array is replaced.
fn resolve(
    plan: PreparedWriteReceiver<'_>, parents: &mut Vec<Parent>, owners: &mut Vec<RuntimeCellHandle>,
    mode: DimensionFetchMode, dimension_container: bool,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<ResolvedValue, EvalStatus> {
    let source;
    let current = match plan {
        PreparedWriteReceiver::Guarded(plan, expr) => {
            let mut resolved = resolve(*plan, parents, owners, mode, dimension_container, context, scope, values)?;
            resolved.source = mixed_guards::projection_source(expr, context, scope).as_ref().map(crate::eval_ir::generic_type).or(resolved.source);
            return Ok(resolved);
        }
        PreparedWriteReceiver::Late(EvalExpr::LoadVar(name)) => {
            let current = match visible_scope_cell(context, scope, name) {
                Some(cell) => values.retain(cell)?,
                None => values.null()?,
            };
            source = local_binding_origins::source_view(&EvalExpr::LoadVar(name.clone()), context, scope).as_ref().map(crate::eval_ir::generic_type);
            parents.push(Parent::Variable(name.clone()));
            current
        }
        PreparedWriteReceiver::Late(EvalExpr::StaticPropertyGet { class_name, property }) => {
            let current = eval_static_property_get_result(class_name, property, context, values)?;
            source = local_binding_origins::source_view(&EvalExpr::StaticPropertyGet { class_name: class_name.clone(), property: property.clone() }, context, scope).as_ref().map(crate::eval_ir::generic_type);
            parents.push(Parent::Static { class: class_name.clone(), property: property.clone() });
            current
        }
        PreparedWriteReceiver::Static { class, property } => {
            let current = eval_static_property_get_result(&class, &property, context, values)?;
            source = local_binding_origins::source_view(&EvalExpr::StaticPropertyGet { class_name: class.clone(), property: property.clone() }, context, scope).as_ref().map(crate::eval_ir::generic_type);
            parents.push(Parent::Static { class, property });
            current
        }
        PreparedWriteReceiver::Late(expr) => {
            let (value, observed) = super::property_write_receivers::evaluate_receiver_source(expr, context, scope, values)?;
            source = observed; value
        }
        PreparedWriteReceiver::Value(value, observed) => { source = observed; values.retain(value)? },
        PreparedWriteReceiver::Property(parent, property) => {
            let receiver = resolve(*parent, parents, owners, mode, false, context, scope, values)?;
            let object = receiver.value;
            write_source_domains::object(receiver.source.as_ref(), context, values)?;
            source = nullsafe_chains::property_source(object, &property, context, values)?;
            let current = if dimension_container {
                selected_property(object, &property, mode, context, values)?
            } else { eval_write_property_get_result(object, &property, context, values)? };
            parents.push(Parent::Property { object, property });
            current
        }
        PreparedWriteReceiver::DynamicProperty(parent, property, name_first) => {
            let mut property = Some(property);
            let early = if name_first { Some(finish_name(property.take().unwrap(), owners, context, scope, values)?) } else { None };
            let receiver = resolve(*parent, parents, owners, mode, false, context, scope, values)?;
            let object = receiver.value;
            let property = if let Some(name) = early { name } else {
                finish_name(property.take().unwrap(), owners, context, scope, values)?
            };
            write_source_domains::object(receiver.source.as_ref(), context, values)?;
            source = nullsafe_chains::property_source(object, &property, context, values)?;
            let current = if dimension_container {
                selected_property(object, &property, mode, context, values)?
            } else { eval_write_property_get_result(object, &property, context, values)? };
            parents.push(Parent::Property { object, property });
            current
        }
        PreparedWriteReceiver::Index(parent, index) => {
            let resolved_array = resolve(*parent, parents, owners, mode, true, context, scope, values)?;
            let array = resolved_array.value;
            let (index, key_source) = finish_index_with_source(index, owners, context, scope, values)?;
            let key_source = write_source_domains::source_type(key_source.as_ref(), index, context, values)?;
            write_source_domains::offset(resolved_array.source.as_ref(), key_source.as_ref(), true, context, values)?;
            checked_array_keys::for_receiver(array, index, context, values)?;
            let source = resolved_array.source.as_ref().and_then(local_binding_origins::declared_offset_result);
            let native = super::super::runtime_ops::simplexml_properties::dimension(
                values, array.borrowed(), elephc_builtin_contract::ObjectDimension::WriteSelect,
                vec![index.borrowed()],
            )?;
            let attachment = if native.is_none() && values.type_tag(array)? == EVAL_TAG_OBJECT {
                overloaded_attachment(array, context, values)?
            } else { OverloadedAttachment::Copy };
            let (current, attached_spl) = match (native, &attachment) {
                (Some(current), _) => (current, false),
                (None, OverloadedAttachment::Spl) => {
                    let current = values.spl_storage_fetch_for_write(array, index)?;
                    mark_attached_spl_root_changed(parents, context, scope)?;
                    (current, true)
                }
                (None, _) => (quiet_child(array, index, owners, context, values)?, false),
            };
            let current = if current.is_borrowed() { values.retain(current)? } else { current };
            owners.push(current);
            let persistent_parent = !matches!(resolved_array.storage, ResolvedStorage::Snapshot)
                || parents.iter().rev()
                .find(|parent| !matches!(parent, Parent::Index { .. }))
                .is_some_and(|parent| matches!(parent,
                    Parent::Variable(_) | Parent::Static { .. } | Parent::Property { .. }));
            if matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)
                && values.type_tag(current)? == EVAL_TAG_NULL
                && persistent_parent
            {
                let empty = eval_array_autovivification(Some(current), false, context, values)?;
                owners.push(empty);
                let replacement = match resolved_array.storage {
                    ResolvedStorage::Snapshot => {
                        write_value(array, Some(index), None, empty, owners, context, values)?
                    }
                    ResolvedStorage::Attached => {
                        write_attached_value(array, Some(index), None, empty, owners, context, values)?
                    }
                    ResolvedStorage::Captured => {
                        write_captured_value(array, Some(index), None, empty, owners, context, values)?
                    }
                };
                propagate_replacement(replacement, parents, None, owners, context, scope, values)?;
                parents.clear();
                parents.push(Parent::Discard);
                return Ok(ResolvedValue { value: empty, storage: ResolvedStorage::Captured, source });
            }
            if attached_spl {
                // The protected fetch returned the cell already installed in internal SPL
                // storage. Its later replacement is observable through that cell, so a
                // public offsetSet replay would overwrite destructor reentry and setter hooks.
                parents.push(Parent::Discard);
            } else if values.type_tag(array)? == EVAL_TAG_OBJECT && values.type_tag(current)? != EVAL_TAG_OBJECT {
                if matches!(attachment, OverloadedAttachment::Reference) {
                    return eval_throw_error("Unsupported dimension write: by-reference offsetGet", context, values);
                }
                // Native write-select already emits its own indirect-modification notice.
                if native.is_none() {
                    let class = eval_dynamic_class_name(array, context, values)?;
                    eval_dispatch_php_error(
                        &format!("Indirect modification of overloaded element of {class} has no effect"),
                        8, context, values,
                    )?;
                }
                parents.push(Parent::Discard);
            } else {
                parents.push(Parent::Index {
                    array,
                    index,
                    storage: resolved_array.storage,
                });
            }
            let storage = if attached_spl { ResolvedStorage::Attached } else { ResolvedStorage::Snapshot };
            return Ok(ResolvedValue { value: current, storage, source });
        }
    };
    let current = if current.is_borrowed() { values.retain(current)? } else { current };
    owners.push(current);
    Ok(ResolvedValue { value: current, storage: ResolvedStorage::Snapshot, source })
}

/// Marks an imported root dirty once the attached fetch may have autovivified SPL storage.
fn mark_attached_spl_root_changed(
    parents: &[Parent],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
) -> Result<(), EvalStatus> {
    if let Some(name) = parents.iter().find_map(|parent| match parent {
        Parent::Variable(name) => Some(name.as_str()),
        _ => None,
    }) {
        mark_scope_binding_changed(context, scope, name)?;
    }
    Ok(())
}

/// Autovivification reads absent array children without an undefined-key warning.
fn quiet_child(
    array: RuntimeCellHandle, index: RuntimeCellHandle, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match values.type_tag(array)? {
        EVAL_TAG_OBJECT => eval_array_get_quiet_result(array, index, context, values),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let exists = values.array_key_exists(index, array)?;
            owners.push(exists);
            if values.truthy(exists)? { eval_array_get_quiet_result(array, index, context, values) }
            else { values.null() }
        }
        EVAL_TAG_NULL => values.null(),
        EVAL_TAG_BOOL if !values.truthy(array)? => values.null(),
        EVAL_TAG_STRING => eval_throw_error("Cannot use string offset as an array", context, values),
        _ => eval_throw_error("Cannot use a scalar value as an array", context, values),
    }
}

/// Distinguishes native writable storage, ordinary copies, and unsupported reference getters.
enum OverloadedAttachment { Spl, Copy, Reference }

/// Resolves inherited eval methods before consulting the first native ancestor's metadata.
fn overloaded_attachment(
    object: RuntimeCellHandle, context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<OverloadedAttachment, EvalStatus> {
    let runtime_class = eval_dynamic_class_name(object, context, values)?;
    if let Some((_, method)) = context.class_method(&runtime_class, "offsetGet") {
        return Ok(if method.returns_by_ref() { OverloadedAttachment::Reference }
            else { OverloadedAttachment::Copy });
    }
    let native_class = if context.class(&runtime_class).is_some() {
        context.class_native_parent_name(&runtime_class).unwrap_or(runtime_class)
    } else { runtime_class };
    let owner = values.reflection_method_declaring_class(&native_class, "offsetGet")?;
    if let Some(owner) = owner {
        if ["ArrayObject", "ArrayIterator"].iter().any(|class| same_eval_class_name(&owner, class)) {
            return Ok(OverloadedAttachment::Spl);
        }
    }
    let by_ref = values.reflection_method_flags(&native_class, "offsetGet")?
        .is_some_and(|flags| flags & EVAL_REFLECTION_METHOD_FLAG_RETURNS_REFERENCE != 0);
    Ok(if by_ref { OverloadedAttachment::Reference } else { OverloadedAttachment::Copy })
}
