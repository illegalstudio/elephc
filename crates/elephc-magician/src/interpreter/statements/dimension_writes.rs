//! Purpose:
//! Mutates attached nested array paths and native object dimensions in opaque eval.
//!
//! Called from:
//! - The DimensionWrite statement dispatcher.
//!
//! Key details:
//! - Computed roots and keys are prepared before the RHS, with live storage resolved afterward.
//! - Array replacements propagate through retained parents; object mutations keep shared identity.
//! - Every intermediate is released in reverse order on success and failure.

use super::*;
use super::property_dimension_writes::{stage_operands, write_value};
use super::property_write_receivers::{PreparedWriteReceiver, finish_index, finish_name};

/// One attached parent which can receive a copied child after a nested array mutation.
enum Parent {
    Variable(String),
    Discard,
    Static { class: String, property: String },
    Property { object: RuntimeCellHandle, property: String },
    Index { array: RuntimeCellHandle, index: RuntimeCellHandle },
    Spl { array: RuntimeCellHandle, index: RuntimeCellHandle, owner: String },
}

/// Prepares source operands once and commits array replacements from the leaf toward its root.
pub(super) fn eval_dimension_write(
    array: &EvalExpr, index: Option<&EvalExpr>, op: Option<EvalBinOp>, value: &EvalExpr,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut owners = Vec::new();
    let mut result = (|| {
        let plan = prepare_property_write_receiver(array, &mut owners, context, scope, values)?;
        let (key, assigned) = stage_operands(index, value, &mut owners, context, scope, values)?;
        let mut parents = Vec::new();
        let current = resolve(plan, &mut parents, &mut owners, context, scope, values)?;
        if values.type_tag(current)? != EVAL_TAG_OBJECT
            && !parents.iter().any(|parent| !matches!(parent, Parent::Index { .. })) {
            return eval_throw_error("Unsupported dimension write: temporary arrays have no attached storage", context, values);
        }
        let mut replacement = write_value(current, key, op, assigned, &mut owners, context, values)?;
        while let Some(value) = replacement {
            let Some(parent) = parents.pop() else { break; };
            replacement = match parent {
                Parent::Discard => None,
                Parent::Variable(name) => {
                    validate_generic_local_contracts(scope, &name, value, context, values)?;
                    write_back_owned_variable_ref_target(scope, &name, value, context, values)?;
                    None
                }
                Parent::Static { class, property } => {
                    eval_static_property_set_result(&class, &property, value, context, values)?;
                    None
                }
                Parent::Property { object, property } => {
                    eval_property_set_result(object, &property, value, context, values)?;
                    None
                }
                Parent::Spl { array, index, owner } => {
                    // An eval subclass uses the builtin backing class, so this bypasses its
                    // virtual override. A native subclass needs a separate non-virtual ABI.
                    let backing = eval_runtime_object_class_name(array, values)?;
                    if values.reflection_method_declaring_class(&backing, "offsetSet")?
                        .is_some_and(|setter| !same_eval_class_name(&setter, &owner)) {
                        return eval_throw_error("Unsupported dimension write: native SPL offsetSet override", context, values);
                    }
                    let result = eval_native_method_with_positional_values_unchecked_bridge_scope(
                        array.borrowed(), &owner, "offsetSet", vec![index.borrowed(), value.borrowed()],
                        Some(&owner), None, context, values,
                    )?;
                    owners.push(result);
                    None
                }
                Parent::Index { array, index } => write_value(array, Some(index), None, value,
                    &mut owners, context, values)?,
            };
        }
        Ok(())
    })();
    for owner in owners.into_iter().rev() {
        let released = eval_release_value(context, values, owner);
        if result.is_ok() { result = released; }
    }
    result
}

/// Retains a traversal value and records the storage needed only if an array is replaced.
fn resolve(
    plan: PreparedWriteReceiver<'_>, parents: &mut Vec<Parent>, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let current = match plan {
        PreparedWriteReceiver::Late(EvalExpr::LoadVar(name)) => {
            let current = match visible_scope_cell(context, scope, name) {
                Some(cell) => values.retain(cell)?,
                None => values.null()?,
            };
            parents.push(Parent::Variable(name.clone()));
            current
        }
        PreparedWriteReceiver::Late(EvalExpr::StaticPropertyGet { class_name, property }) => {
            let current = eval_static_property_get_result(class_name, property, context, values)?;
            parents.push(Parent::Static { class: class_name.clone(), property: property.clone() });
            current
        }
        PreparedWriteReceiver::Static { class, property } => {
            let current = eval_static_property_get_result(&class, &property, context, values)?;
            parents.push(Parent::Static { class, property });
            current
        }
        PreparedWriteReceiver::Late(expr) => eval_owned_expr(expr, context, scope, values)?,
        PreparedWriteReceiver::Value(value) => values.retain(value)?,
        PreparedWriteReceiver::Property(parent, property) => {
            let object = resolve(*parent, parents, owners, context, scope, values)?;
            let current = eval_write_property_get_result(object, &property, context, values)?;
            parents.push(Parent::Property { object, property });
            current
        }
        PreparedWriteReceiver::DynamicProperty(parent, property, name_first) => {
            let early = if name_first { Some(finish_name(property, owners, context, scope, values)?) } else { None };
            let object = resolve(*parent, parents, owners, context, scope, values)?;
            let property = if let Some(name) = early { name } else {
                finish_name(property, owners, context, scope, values)?
            };
            let current = eval_write_property_get_result(object, &property, context, values)?;
            parents.push(Parent::Property { object, property });
            current
        }
        PreparedWriteReceiver::Index(parent, index) => {
            let array = resolve(*parent, parents, owners, context, scope, values)?;
            let index = finish_index(index, owners, context, scope, values)?;
            let native = super::super::runtime_ops::simplexml_properties::dimension(
                values, array.borrowed(), elephc_builtin_contract::ObjectDimension::WriteSelect,
                vec![index.borrowed()],
            )?;
            let attachment = if native.is_none() && values.type_tag(array)? == EVAL_TAG_OBJECT {
                let attachment = overloaded_attachment(array, context, values)?;
                if matches!(attachment, OverloadedAttachment::Reference) {
                    return eval_throw_error("Unsupported dimension write: by-reference offsetGet", context, values);
                }
                attachment
            } else { OverloadedAttachment::Copy };
            let current = match native {
                Some(current) => current,
                None => quiet_child(array, index, owners, context, values)?,
            };
            let current = if current.is_borrowed() { values.retain(current)? } else { current };
            owners.push(current);
            if values.type_tag(array)? == EVAL_TAG_OBJECT && values.type_tag(current)? != EVAL_TAG_OBJECT {
                if let OverloadedAttachment::Spl(owner) = attachment {
                    parents.push(Parent::Spl { array, index, owner });
                } else {
                    // Native write-select already emits its own indirect-modification notice.
                    if native.is_none() {
                        let class = eval_dynamic_class_name(array, context, values)?;
                        eval_dispatch_php_error(
                            &format!("Indirect modification of overloaded element of {class} has no effect"),
                            8, context, values,
                        )?;
                    }
                    parents.push(Parent::Discard);
                }
            } else {
                parents.push(Parent::Index { array, index });
            }
            return Ok(current);
        }
    };
    let current = if current.is_borrowed() { values.retain(current)? } else { current };
    owners.push(current);
    Ok(current)
}

/// Autovivification reads absent array children without an undefined-key warning.
fn quiet_child(
    array: RuntimeCellHandle, index: RuntimeCellHandle, owners: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match values.type_tag(array)? {
        EVAL_TAG_OBJECT => eval_array_get_result(array, index, context, values),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let exists = values.array_key_exists(index, array)?;
            owners.push(exists);
            if values.truthy(exists)? { eval_array_get_result(array, index, context, values) }
            else { values.null() }
        }
        EVAL_TAG_NULL => values.null(),
        EVAL_TAG_BOOL if !values.truthy(array)? => values.null(),
        EVAL_TAG_STRING => eval_throw_error("Cannot use string offset as an array", context, values),
        _ => eval_throw_error("Cannot use a scalar value as an array", context, values),
    }
}

/// Distinguishes native writable storage, ordinary copies, and unsupported reference getters.
enum OverloadedAttachment { Spl(String), Copy, Reference }

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
            return Ok(OverloadedAttachment::Spl(owner));
        }
    }
    let by_ref = values.reflection_method_flags(&native_class, "offsetGet")?
        .is_some_and(|flags| flags & EVAL_REFLECTION_METHOD_FLAG_RETURNS_REFERENCE != 0);
    Ok(if by_ref { OverloadedAttachment::Reference } else { OverloadedAttachment::Copy })
}
