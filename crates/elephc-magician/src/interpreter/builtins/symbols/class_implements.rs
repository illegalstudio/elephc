//! Purpose:
//! Eval registry entry and implementation for `class_implements`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Shared class-relation logic for `class_parents()` and `class_uses()` lives here.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "class_implements",
    area: Symbols,
    params: [object_or_class, autoload = EvalBuiltinDefaultValue::Bool(true)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;
use super::super::{eval_class_metadata_name, eval_class_relation_name_exists};

/// Dispatches direct eval calls for the `class_implements` symbol builtin.
pub(in crate::interpreter) fn eval_class_implements_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_class_relation("class_implements", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `class_implements` symbol builtin.
pub(in crate::interpreter) fn eval_class_implements_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_class_relation_result("class_implements", evaluated_args, context, values)
}

/// Evaluates `class_implements()`, `class_parents()`, or `class_uses()`.
pub(in crate::interpreter) fn eval_builtin_class_relation(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=2).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let target = eval_expr(&args[0], context, scope, values)?;
    if let Some(autoload) = args.get(1) {
        let _ = eval_expr(autoload, context, scope, values)?;
    }
    eval_class_relation_target_result(name, target, context, values)
}

/// Evaluates materialized class-relation builtin arguments.
pub(in crate::interpreter) fn eval_class_relation_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let target = match evaluated_args {
        [target] => *target,
        [target, _autoload] => *target,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_class_relation_target_result(name, target, context, values)
}

/// Resolves one class-relation target and returns an empty relation set or false.
pub(in crate::interpreter) fn eval_class_relation_target_result(
    name: &str,
    target: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(name, "class_implements" | "class_parents" | "class_uses") {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(target) = eval_class_relation_target_name(target, context, values)? else {
        return values.bool_value(false);
    };
    if context.class(&target).is_some() {
        return match name {
            "class_implements" => {
                let names =
                    eval_class_relation_eval_class_interface_names(&target, context, values)?;
                eval_class_relation_names_result(names, values)
            }
            "class_parents" => {
                eval_class_relation_names_result(context.class_parent_names(&target), values)
            }
            "class_uses" => {
                eval_class_relation_names_result(context.class_trait_names(&target), values)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        };
    }
    if context.interface(&target).is_some() {
        return match name {
            "class_implements" => {
                let names =
                    eval_class_relation_eval_interface_parent_names(&target, context, values)?;
                eval_class_relation_names_result(names, values)
            }
            "class_parents" | "class_uses" => values.assoc_new(0),
            _ => Err(EvalStatus::RuntimeFatal),
        };
    }
    if context.trait_decl(&target).is_some() {
        return match name {
            "class_uses" => {
                eval_class_relation_names_result(context.trait_trait_names(&target), values)
            }
            "class_implements" | "class_parents" => values.assoc_new(0),
            _ => Err(EvalStatus::RuntimeFatal),
        };
    }
    match name {
        "class_implements" => eval_runtime_class_interface_names_result(&target, values),
        "class_parents" => {
            eval_class_relation_names_result(
                eval_runtime_class_parent_names(&target, context),
                values,
            )
        }
        "class_uses" => eval_runtime_class_trait_names_result(&target, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds `class_implements()` data for generated/AOT class metadata.
fn eval_runtime_class_interface_names_result(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let names = eval_runtime_class_interface_names(class_name, values)?;
    eval_class_relation_names_result(names, values)
}

/// Returns generated/AOT interface names visible for one class-like symbol.
fn eval_runtime_class_interface_names(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let names_array = values.reflection_class_interface_names(class_name)?;
    let names = eval_class_relation_runtime_string_array_to_vec(names_array, values)?;
    values.release(names_array)?;
    Ok(names)
}

/// Builds `class_uses()` data for generated/AOT direct trait-use metadata.
fn eval_runtime_class_trait_names_result(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let names_array = values.reflection_class_trait_names(class_name)?;
    let names = eval_class_relation_runtime_string_array_to_vec(names_array, values)?;
    values.release(names_array)?;
    eval_class_relation_names_result(names, values)
}

/// Returns generated/AOT parent names in PHP's nearest-parent-first order.
fn eval_runtime_class_parent_names(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = context.native_class_parent(class_name).map(str::to_string);
    let mut seen = std::collections::HashSet::new();
    while let Some(parent) = current {
        let parent = parent.trim_start_matches('\\').to_string();
        if !seen.insert(parent.to_ascii_lowercase()) {
            break;
        }
        current = context.native_class_parent(&parent).map(str::to_string);
        names.push(parent);
    }
    names
}

/// Returns eval class interfaces plus interfaces inherited from generated/AOT parents.
fn eval_class_relation_eval_class_interface_names(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(parent) = context.class_native_parent_name(class_name) {
        for name in eval_runtime_class_interface_names(&parent, values)? {
            eval_class_relation_push_unique_name(name, &mut names, &mut seen);
        }
    }
    for name in context.class_interface_names(class_name) {
        eval_class_relation_push_unique_name(name.clone(), &mut names, &mut seen);
        if !context.has_interface(&name) && eval_runtime_interface_exists(&name, values)? {
            for parent in eval_runtime_class_interface_names(&name, values)? {
                eval_class_relation_push_unique_name(parent, &mut names, &mut seen);
            }
        }
    }
    Ok(names)
}

/// Returns eval interface parents plus inherited generated/AOT interface parents.
fn eval_class_relation_eval_interface_parent_names(
    interface_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in context.interface_parent_names(interface_name) {
        eval_class_relation_push_unique_name(name.clone(), &mut names, &mut seen);
        if !context.has_interface(&name) && eval_runtime_interface_exists(&name, values)? {
            for parent in eval_runtime_class_interface_names(&name, values)? {
                eval_class_relation_push_unique_name(parent, &mut names, &mut seen);
            }
        }
    }
    Ok(names)
}

/// Appends one class-like name while preserving PHP's case-insensitive uniqueness.
fn eval_class_relation_push_unique_name(
    name: String,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(name.to_ascii_lowercase()) {
        names.push(name);
    }
}

/// Copies a runtime string array into Rust-owned class/interface names.
fn eval_class_relation_runtime_string_array_to_vec(
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

/// Returns whether a class-relation target refers to a known class-like symbol.
fn eval_class_relation_target_name(
    target: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    if values.type_tag(target)? == EVAL_TAG_OBJECT {
        let name = super::get_class::eval_get_class_result(target, context, values)?;
        let name = eval_class_metadata_name(name, values)?;
        return Ok(eval_class_relation_name_exists(&name, context, values)?.then_some(name));
    }
    let name = eval_class_metadata_name(target, values)?;
    let name = context.resolve_class_like_name(&name).unwrap_or(name);
    Ok(eval_class_relation_name_exists(&name, context, values)?.then_some(name))
}

/// Builds a PHP associative class-name array keyed by class-name strings.
fn eval_class_relation_names_result(
    names: Vec<String>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(names.len())?;
    for name in names {
        let key = values.string(&name)?;
        let value = values.string(&name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}
