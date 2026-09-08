//! Purpose:
//! Projects native DateTime debug properties without calling user format or serialization hooks.
//!
//! Called from:
//! - The shared print_r/var_dump object-property collector.
//!
//! Key details:
//! - Native snapshots own boxed arrays; fetched fields are released after rendering.
//! - User properties keep declaration order and visibility before native fields are merged.

use super::super::super::*;
use super::print_r::{EvalDebugObjectProperty, EvalDebugPropertyVisibility, EvalDebugPropertyVisibilityKind};
use crate::interpreter::reflection::native_property_debug_visibility;
use std::collections::HashSet;

/// Collects a native date snapshot and user properties, or declines a non-date object.
pub(in crate::interpreter) fn eval_native_date_debug_properties(
    object: RuntimeCellHandle,
    identity: Option<u64>,
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Vec<EvalDebugObjectProperty>>, EvalStatus> {
    let base = if values.object_is_a(object, "DateTimeImmutable", false)? {
        "DateTimeImmutable"
    } else if values.object_is_a(object, "DateTime", false)? {
        "DateTime"
    } else {
        return Ok(None);
    };
    let snapshot = eval_native_method_with_evaluated_args_unchecked_bridge_scope(
        object, base, "__elephc_debug_properties", Vec::new(), Some(base), Some(class_name), context, values,
    )?;
    let result = (|| {
        let mut properties = if let Some(identity) = identity
            .filter(|identity| context.dynamic_object_class(*identity).is_some())
        {
            super::print_r::eval_debug_dynamic_object_properties(
                object, identity, class_name, context, values,
            )?
        } else {
            native_user_properties(object, class_name, base, context, values)?
        };
        let result = append_snapshot(snapshot, &mut properties, context, values);
        if let Err(status) = result {
            let _ = release_debug_properties(properties, context, values);
            return Err(status);
        }
        Ok(properties)
    })();
    let cleanup = values.release(snapshot);
    match result {
        Ok(properties) => match cleanup {
            Ok(()) => Ok(Some(properties)),
            Err(status) => {
                let _ = release_debug_properties(properties, context, values);
                Err(status)
            }
        },
        Err(status) => Err(status),
    }
}

/// Releases only the property references acquired by this debug projection.
pub(in crate::interpreter) fn release_debug_properties(
    properties: Vec<EvalDebugObjectProperty>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut result = Ok(());
    for property in properties {
        if property.owned_value {
            let released = eval_release_value(context, values, property.value);
            if result.is_ok() { result = released; }
        }
    }
    result
}

/// Copies and releases one owned runtime string cell, retaining the original conversion error.
fn consume_name(cell: RuntimeCellHandle, values: &mut impl RuntimeValueOps) -> Result<String, EvalStatus> {
    let result = values.string_bytes(cell)
        .and_then(|bytes| String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal));
    let cleanup = values.release(cell);
    result.and_then(|name| cleanup.map(|()| name))
}

/// Fetches an array value and releases the temporary iteration key on success and failure.
fn fetch_position(array: RuntimeCellHandle, position: usize, values: &mut impl RuntimeValueOps)
    -> Result<RuntimeCellHandle, EvalStatus>
{
    let key = values.array_iter_key(array, position)?;
    let result = values.array_get(array, key);
    let cleanup = values.release(key);
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(value), Err(status)) => { let _ = values.release(value); Err(status) }
        (Err(status), _) => Err(status),
    }
}

/// Converts an owned Reflection property-name array to Rust strings.
fn property_names(class_name: &str, values: &mut impl RuntimeValueOps) -> Result<Vec<String>, EvalStatus> {
    let array = values.reflection_property_names(class_name)?;
    let result = (|| {
        let mut names = Vec::new();
        for position in 0..values.array_len(array)? {
            let cell = fetch_position(array, position, values)?;
            names.push(consume_name(cell, values)?);
        }
        Ok(names)
    })();
    let cleanup = values.release(array);
    result.and_then(|names| cleanup.map(|()| names))
}

/// Reads declared native user properties in base-to-derived order, then dynamic public properties.
pub(super) fn native_user_properties(
    object: RuntimeCellHandle, class_name: &str, base: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalDebugObjectProperty>, EvalStatus> {
    let mut chain = Vec::new();
    let mut cursor = class_name.to_string();
    let mut visited = HashSet::new();
    while !cursor.eq_ignore_ascii_case(base) {
        if !visited.insert(cursor.to_ascii_lowercase()) { return Err(EvalStatus::RuntimeFatal); }
        chain.push(cursor.clone());
        let Some(parent) = context.native_class_parent(&cursor) else { break; };
        cursor = parent.to_string();
    }
    let mut properties = Vec::new();
    let result = (|| {
        for owner in chain.into_iter().rev() {
            for name in property_names(&owner, values)? {
                let Some(declaring) = values.reflection_property_declaring_class(&owner, &name)? else { continue; };
                if !declaring.eq_ignore_ascii_case(&owner) { continue; }
                let flags = values.reflection_property_flags(&owner, &name)?.unwrap_or(0);
                let Some(visibility) = native_property_debug_visibility(flags) else { continue; };
                context.push_class_scope(owner.clone());
                let value = (|| {
                    if values.property_is_initialized(object, &name)? {
                        values.property_get(object, &name).map(Some)
                    } else { Ok(None) }
                })();
                context.pop_class_scope();
                let Some(value) = value? else { continue; };
                let kind = match visibility {
                    EvalVisibility::Private => EvalDebugPropertyVisibilityKind::Private(owner.clone()),
                    EvalVisibility::Protected => EvalDebugPropertyVisibilityKind::Protected,
                    EvalVisibility::Public => EvalDebugPropertyVisibilityKind::Public,
                };
                merge_property(&mut properties, EvalDebugObjectProperty {
                    name, visibility: EvalDebugPropertyVisibility { kind }, value,
                    is_reference: false, owned_value: true, numeric_name: false,
                }, context, values)?;
            }
        }
        for position in 0..values.object_property_len(object)? {
            let key = values.object_property_iter_key(object, position)?;
            let name = consume_name(key, values)?;
            if properties.iter().any(|property| property.name == name) || name.contains('\0') { continue; }
            let value = values.property_get(object, &name)?;
            properties.push(EvalDebugObjectProperty {
                name, visibility: EvalDebugPropertyVisibility { kind: EvalDebugPropertyVisibilityKind::Public },
                value, is_reference: false, owned_value: true, numeric_name: false,
            });
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(properties),
        Err(status) => { let _ = release_debug_properties(properties, context, values); Err(status) }
    }
}

/// Replaces a same-slot non-private property without moving its position in the debug table.
fn merge_property(
    properties: &mut Vec<EvalDebugObjectProperty>, property: EvalDebugObjectProperty,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let existing = if matches!(property.visibility.kind, EvalDebugPropertyVisibilityKind::Private(_)) {
        None
    } else {
        properties.iter().position(|entry| entry.name == property.name
            && !matches!(entry.visibility.kind, EvalDebugPropertyVisibilityKind::Private(_)))
    };
    if let Some(index) = existing {
        let old = std::mem::replace(&mut properties[index], property);
        release_debug_properties(vec![old], context, values)
    } else {
        properties.push(property);
        Ok(())
    }
}

/// Appends native fields, overwriting colliding public slots while keeping their declaration order.
fn append_snapshot(
    snapshot: RuntimeCellHandle, properties: &mut Vec<EvalDebugObjectProperty>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for position in 0..values.array_len(snapshot)? {
        let key = values.array_iter_key(snapshot, position)?;
        let value = values.array_get(snapshot, key);
        let name = consume_name(key, values);
        let (value, name) = match (value, name) {
            (Ok(value), Ok(name)) => (value, name),
            (Ok(value), Err(status)) => { let _ = values.release(value); return Err(status); }
            (Err(status), _) => return Err(status),
        };
        merge_property(properties, EvalDebugObjectProperty {
            name, visibility: EvalDebugPropertyVisibility { kind: EvalDebugPropertyVisibilityKind::Public },
            value, is_reference: false, owned_value: true, numeric_name: false,
        }, context, values)?;
    }
    Ok(())
}
