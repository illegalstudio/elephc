//! Purpose:
//! Eval registry entry and implementation for `get_class_vars`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Eval-declared defaults are materialized in the declaring class scope.
//! - Generated/AOT defaults use native callable default metadata when present.

eval_builtin! {
    name: "get_class_vars",
    area: Symbols,
    params: [r#class],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;
use super::super::{
    eval_class_relation_name_exists, eval_resolved_class_metadata_name,
    eval_runtime_property_access_metadata, eval_runtime_string_array_to_vec,
};
use std::collections::HashSet;

/// Dispatches direct eval calls for the `get_class_vars` symbol builtin.
pub(in crate::interpreter) fn eval_get_class_vars_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_get_class_vars(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `get_class_vars` symbol builtin.
pub(in crate::interpreter) fn eval_get_class_vars_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_get_class_vars_result(evaluated_args, context, values)
}

/// Evaluates `get_class_vars()` from eval expressions.
pub(in crate::interpreter) fn eval_builtin_get_class_vars(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [target] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let target = eval_expr(target, context, scope, values)?;
    eval_get_class_vars_result(&[target], context, values)
}

/// Evaluates materialized `get_class_vars()` arguments.
pub(in crate::interpreter) fn eval_get_class_vars_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [target] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let class_name = eval_resolved_class_metadata_name(*target, context, values)?;
    if context.has_class(&class_name) || context.has_enum(&class_name) {
        return eval_dynamic_class_vars_result(&class_name, context, values);
    }
    if context.has_trait(&class_name) {
        return eval_dynamic_trait_vars_result(&class_name, context, values);
    }
    if context.has_interface(&class_name) {
        return values.assoc_new(0);
    }
    if eval_class_relation_name_exists(&class_name, context, values)? {
        return eval_runtime_class_vars_result(&class_name, context, values);
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Builds `get_class_vars()` for an eval-declared class or enum.
fn eval_dynamic_class_vars_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(0)?;
    let mut emitted_keys = HashSet::new();
    if let Some(enum_decl) = context.enum_decl(class_name) {
        let name_value = values.null()?;
        result = eval_add_class_var_entry(result, "name", name_value, values)?;
        emitted_keys.insert(String::from("name"));
        if enum_decl.backing_type().is_some() {
            let value_value = values.null()?;
            result = eval_add_class_var_entry(result, "value", value_value, values)?;
            emitted_keys.insert(String::from("value"));
        }
    }
    for class in context.class_chain(class_name).into_iter().rev() {
        for property in class.properties() {
            if emitted_keys.contains(property.name())
                || validate_eval_member_access(class.name(), property.visibility(), context)
                    .is_err()
            {
                continue;
            }
            let value =
                eval_class_vars_property_default_value(class.name(), property, context, values)?;
            result = eval_add_class_var_entry(result, property.name(), value, values)?;
            emitted_keys.insert(property.name().to_string());
        }
    }
    Ok(result)
}

/// Builds `get_class_vars()` for an eval-declared trait.
fn eval_dynamic_trait_vars_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(trait_decl) = context.trait_decl(class_name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let trait_name = trait_decl.name().to_string();
    let properties = trait_decl.properties().to_vec();
    let mut result = values.assoc_new(properties.len())?;
    let mut emitted_keys = HashSet::new();
    for property in properties {
        if emitted_keys.contains(property.name())
            || validate_eval_member_access(&trait_name, property.visibility(), context).is_err()
        {
            continue;
        }
        let value = eval_class_vars_property_default_value(&trait_name, &property, context, values)?;
        result = eval_add_class_var_entry(result, property.name(), value, values)?;
        emitted_keys.insert(property.name().to_string());
    }
    Ok(result)
}

/// Builds `get_class_vars()` data for generated/AOT class metadata.
fn eval_runtime_class_vars_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let property_names = values.reflection_property_names(class_name)?;
    let declared_names = eval_runtime_string_array_to_vec(property_names, values)?;
    values.release(property_names)?;
    let mut result = values.assoc_new(declared_names.len())?;
    let mut emitted_keys = HashSet::new();
    for property_name in declared_names {
        if emitted_keys.contains(&property_name) {
            continue;
        }
        let Some((declaring_class, visibility, _is_static)) =
            eval_runtime_property_access_metadata(class_name, &property_name, values)?
        else {
            continue;
        };
        if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
            continue;
        }
        let value = eval_runtime_class_var_default_value(
            class_name,
            &declaring_class,
            &property_name,
            context,
            values,
        )?;
        result = eval_add_class_var_entry(result, &property_name, value, values)?;
        emitted_keys.insert(property_name);
    }
    Ok(result)
}

/// Materializes one eval-declared property default for `get_class_vars()`.
fn eval_class_vars_property_default_value(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(default) = property.default() else {
        return values.null();
    };
    context.push_class_scope(declaring_class.to_string());
    context.push_called_class_scope(declaring_class.to_string());
    context.push_class_like_member_magic_scope(declaring_class, property.trait_origin());
    let result = eval_method_parameter_default(default, context, values);
    context.pop_magic_scope();
    context.pop_called_class_scope();
    context.pop_class_scope();
    result
}

/// Materializes one generated/AOT property default for `get_class_vars()`.
fn eval_runtime_class_var_default_value(
    runtime_class: &str,
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(default) = context
        .native_property_default(declaring_class, property_name)
        .or_else(|| context.native_property_default(runtime_class, property_name))
    {
        return materialize_native_callable_default(&default, context, values);
    }
    values.null()
}

/// Adds one string-keyed class variable value to an associative result array.
fn eval_add_class_var_entry(
    result: RuntimeCellHandle,
    property_name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(property_name)?;
    values.array_set(result, key, value)
}
