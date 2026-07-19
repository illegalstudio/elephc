//! Purpose:
//! Eval registry entry and implementation for `get_class_methods`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Method lists are filtered through the current eval scope and PHP visibility
//!   rules before materialization.

eval_builtin! {
    name: "get_class_methods",
    area: Symbols,
    params: [object_or_class],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;
use super::super::{
    eval_class_metadata_is_a, eval_class_metadata_target_name, eval_class_relation_name_exists,
    eval_indexed_string_array_result, eval_runtime_method_access_metadata,
    eval_runtime_string_array_to_vec, eval_same_class_metadata_name,
};
use std::collections::HashSet;

/// Dispatches direct eval calls for the `get_class_methods` symbol builtin.
pub(in crate::interpreter) fn eval_get_class_methods_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_get_class_methods(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `get_class_methods` symbol builtin.
pub(in crate::interpreter) fn eval_get_class_methods_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_get_class_methods_result(evaluated_args, context, values)
}

/// Evaluates `get_class_methods()` from eval expressions.
pub(in crate::interpreter) fn eval_builtin_get_class_methods(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [target] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let target = eval_expr(target, context, scope, values)?;
    eval_get_class_methods_result(&[target], context, values)
}

/// Evaluates materialized `get_class_methods()` arguments.
pub(in crate::interpreter) fn eval_get_class_methods_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [target] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let (class_name, target_is_object) =
        eval_class_metadata_target_name(*target, context, values)?;
    if !target_is_object && !eval_class_relation_name_exists(&class_name, context, values)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let names = eval_class_method_names_for_scope(&class_name, context, values)?;
    eval_indexed_string_array_result(&names, values)
}

/// Collects PHP-visible methods for `get_class_methods()` in the current eval scope.
fn eval_class_method_names_for_scope(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        for name in context.class_method_names(class_name) {
            let Some((declaring_class, method)) = context.class_method(class_name, &name) else {
                eval_push_unique_method_name(&mut names, &mut seen, name);
                continue;
            };
            if validate_eval_member_access(&declaring_class, method.visibility(), context).is_ok() {
                eval_push_unique_method_name(&mut names, &mut seen, name);
            }
        }
        eval_add_current_scope_private_method_names(
            &mut names, &mut seen, class_name, context, values,
        )?;
        eval_add_native_parent_method_names(&mut names, &mut seen, class_name, context, values)?;
        return Ok(names);
    }
    if context.has_interface(class_name) {
        return Ok(context.interface_method_names(class_name));
    }
    if let Some(trait_decl) = context.trait_decl(class_name) {
        return Ok(trait_decl
            .methods()
            .iter()
            .filter(|method| method.visibility() == EvalVisibility::Public)
            .map(|method| method.name().to_string())
            .collect());
    }
    let method_names = values.reflection_method_names(class_name)?;
    let names = eval_runtime_string_array_to_vec(method_names, values)?;
    values.release(method_names)?;
    let mut names = eval_visible_runtime_method_names(class_name, names, context, values)?;
    let mut seen = names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    eval_add_current_scope_private_method_names(&mut names, &mut seen, class_name, context, values)?;
    Ok(names)
}

/// Filters generated runtime methods to the surface visible from the current eval scope.
fn eval_visible_runtime_method_names(
    lookup_class_name: &str,
    names: Vec<String>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut result = Vec::new();
    for name in names {
        let Some((declaring_class, visibility)) =
            eval_runtime_method_access_metadata(lookup_class_name, &name, values)?
        else {
            continue;
        };
        if validate_eval_member_access(&declaring_class, visibility, context).is_ok() {
            result.push(name);
        }
    }
    Ok(result)
}

/// Adds generated/AOT parent method names inherited by one eval class.
fn eval_add_native_parent_method_names(
    names: &mut Vec<String>,
    seen: &mut HashSet<String>,
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(parent) = context.class_native_parent_name(class_name) else {
        return Ok(());
    };
    let method_names = values.reflection_method_names(&parent)?;
    let parent_names = eval_runtime_string_array_to_vec(method_names, values)?;
    values.release(method_names)?;
    let parent_names = eval_visible_runtime_method_names(&parent, parent_names, context, values)?;
    for name in parent_names {
        eval_push_unique_method_name(names, seen, name);
    }
    Ok(())
}

/// Adds private methods declared by the current eval scope when PHP would expose them.
fn eval_add_current_scope_private_method_names(
    names: &mut Vec<String>,
    seen: &mut HashSet<String>,
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(current_class) = context.current_class_scope() else {
        return Ok(());
    };
    if !eval_class_metadata_is_a(class_name, current_class, context) {
        return Ok(());
    }
    if let Some(class) = context.class(current_class) {
        for method in class.methods() {
            if method.visibility() == EvalVisibility::Private {
                eval_push_unique_method_name(names, seen, method.name().to_string());
            }
        }
        return Ok(());
    }
    if context.has_interface(current_class) || context.has_trait(current_class) {
        return Ok(());
    }
    if !eval_class_relation_name_exists(current_class, context, values)? {
        return Ok(());
    }
    let method_names = values.reflection_method_names(current_class)?;
    let current_names = eval_runtime_string_array_to_vec(method_names, values)?;
    values.release(method_names)?;
    for name in current_names {
        let Some((declaring_class, visibility)) =
            eval_runtime_method_access_metadata(current_class, &name, values)?
        else {
            continue;
        };
        if visibility == EvalVisibility::Private
            && eval_same_class_metadata_name(&declaring_class, current_class)
        {
            eval_push_unique_method_name(names, seen, name);
        }
    }
    Ok(())
}

/// Appends one method name while preserving PHP's case-insensitive uniqueness rule.
fn eval_push_unique_method_name(
    names: &mut Vec<String>,
    seen: &mut HashSet<String>,
    name: String,
) {
    if seen.insert(name.to_ascii_lowercase()) {
        names.push(name);
    }
}
