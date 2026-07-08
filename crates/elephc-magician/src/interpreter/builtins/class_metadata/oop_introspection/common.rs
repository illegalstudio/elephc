//! Purpose:
//! Shared class-name resolution and reflection metadata helpers for eval OOP
//! introspection builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::class_metadata::oop_introspection` modules.
//!
//! Key details:
//! - Helpers normalize PHP class names case-insensitively while preserving
//!   runtime reflection ownership and visibility metadata.

use super::*;
use std::collections::HashSet;

pub(super) const EVAL_CLASS_METADATA_FLAG_STATIC: u64 = 1;
const EVAL_CLASS_METADATA_FLAG_PROTECTED: u64 = 4;
const EVAL_CLASS_METADATA_FLAG_PRIVATE: u64 = 8;

/// Resolves an object-or-class argument to a PHP class name and records whether it was an object.
pub(in crate::interpreter) fn eval_class_metadata_target_name(
    target: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(String, bool), EvalStatus> {
    match values.type_tag(target)? {
        EVAL_TAG_OBJECT => Ok((
            eval_object_class_metadata_name(target, context, values)?,
            true,
        )),
        EVAL_TAG_STRING => Ok((
            eval_resolved_class_metadata_name(target, context, values)?,
            false,
        )),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Resolves an object cell to its eval or runtime class name.
pub(in crate::interpreter) fn eval_object_class_metadata_name(
    object: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let identity = values.object_identity(object)?;
    if let Some(class_name) = context.dynamic_object_class_name(identity) {
        return Ok(class_name);
    }
    let class_name = values.object_class_name(object)?;
    let class_name_bytes = values.string_bytes(class_name);
    values.release(class_name)?;
    let class_name = String::from_utf8(class_name_bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(class_name.trim_start_matches('\\').to_string())
}

/// Reads a class-name cell and applies eval alias resolution.
pub(in crate::interpreter) fn eval_resolved_class_metadata_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = eval_class_metadata_name(name, values)?;
    Ok(context.resolve_class_name(&name).unwrap_or(name))
}

/// Returns whether one eval or generated/AOT class name is the same as or extends another.
pub(in crate::interpreter) fn eval_class_metadata_is_a(
    class_name: &str,
    target: &str,
    context: &ElephcEvalContext,
) -> bool {
    eval_same_class_metadata_name(class_name, target)
        || context.class_is_a(class_name, target, false)
        || eval_native_class_metadata_is_a(class_name, target, context)
}

/// Returns whether generated/AOT parent metadata proves one class extends another.
fn eval_native_class_metadata_is_a(
    class_name: &str,
    target: &str,
    context: &ElephcEvalContext,
) -> bool {
    let target = target.trim_start_matches('\\');
    let mut current = class_name.trim_start_matches('\\').to_string();
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current.to_ascii_lowercase()) {
            return false;
        }
        if eval_same_class_metadata_name(&current, target) {
            return true;
        }
        let Some(parent) = context.native_class_parent(&current) else {
            return false;
        };
        current = parent.to_string();
    }
}

/// Returns whether two PHP class names refer to the same normalized metadata name.
pub(in crate::interpreter) fn eval_same_class_metadata_name(left: &str, right: &str) -> bool {
    left.trim_start_matches('\\')
        .eq_ignore_ascii_case(right.trim_start_matches('\\'))
}

/// Returns access metadata for one generated/AOT method name, if reflection exposes it.
pub(in crate::interpreter) fn eval_runtime_method_access_metadata(
    class_name: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility)>, EvalStatus> {
    let Some(flags) = values.reflection_method_flags(class_name, method_name)? else {
        return Ok(None);
    };
    let declaring_class = values
        .reflection_method_declaring_class(class_name, method_name)?
        .unwrap_or_else(|| class_name.to_string());
    Ok(Some((
        declaring_class,
        eval_runtime_member_visibility(flags),
    )))
}

/// Returns access metadata for one generated/AOT property name, if reflection exposes it.
pub(in crate::interpreter) fn eval_runtime_property_access_metadata(
    class_name: &str,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility, bool)>, EvalStatus> {
    let Some(flags) = values.reflection_property_flags(class_name, property_name)? else {
        return Ok(None);
    };
    let declaring_class = values
        .reflection_property_declaring_class(class_name, property_name)?
        .unwrap_or_else(|| class_name.to_string());
    Ok(Some((
        declaring_class,
        eval_runtime_member_visibility(flags),
        flags & EVAL_CLASS_METADATA_FLAG_STATIC != 0,
    )))
}

/// Converts generated/AOT reflection member flags into eval visibility metadata.
fn eval_runtime_member_visibility(flags: u64) -> EvalVisibility {
    if flags & EVAL_CLASS_METADATA_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_CLASS_METADATA_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    }
}

/// Builds an indexed PHP array from owned Rust strings.
pub(in crate::interpreter) fn eval_indexed_string_array_result(
    names: &[String],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(names.len())?;
    for (index, name) in names.iter().enumerate() {
        let key = values.int(index as i64)?;
        let value = values.string(name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Copies a runtime string array into Rust-owned strings for class metadata helpers.
pub(in crate::interpreter) fn eval_runtime_string_array_to_vec(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.int(position as i64)?;
        let value = values.array_get(array, key)?;
        result.push(eval_class_metadata_name(value, values)?);
    }
    Ok(result)
}
