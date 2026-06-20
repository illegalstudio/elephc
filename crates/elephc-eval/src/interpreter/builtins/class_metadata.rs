//! Purpose:
//! Implements eval class metadata and class-relation introspection builtins.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - Eval-declared class-like symbols carry parent, interface, direct trait-use,
//!   and class-level attribute metadata for literal positional args.
//! - Missing class-like relation targets return `false`, matching the main
//!   backend's unknown-target fallback.

use super::super::*;
use super::*;

mod oop_introspection;

pub(in crate::interpreter) use oop_introspection::*;

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
                eval_class_relation_names_result(context.class_interface_names(&target), values)
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
                eval_class_relation_names_result(context.interface_parent_names(&target), values)
            }
            "class_parents" | "class_uses" => values.assoc_new(0),
            _ => Err(EvalStatus::RuntimeFatal),
        };
    }
    values.assoc_new(0)
}

/// Evaluates class attribute metadata helpers.
pub(in crate::interpreter) fn eval_builtin_class_attribute_metadata(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = match (name, args) {
        ("class_attribute_names" | "class_get_attributes", [class_name]) => {
            vec![eval_expr(class_name, context, scope, values)?]
        }
        ("class_attribute_args", [class_name, attribute_name]) => vec![
            eval_expr(class_name, context, scope, values)?,
            eval_expr(attribute_name, context, scope, values)?,
        ],
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_class_attribute_metadata_result(name, &evaluated_args, context, values)
}

/// Evaluates materialized class attribute metadata arguments.
pub(in crate::interpreter) fn eval_class_attribute_metadata_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match (name, evaluated_args) {
        ("class_attribute_names", [class_name]) => {
            let class_name = eval_class_metadata_name(*class_name, values)?;
            let Some(attributes) = eval_class_like_attributes(context, &class_name) else {
                return values.array_new(0);
            };
            eval_class_attribute_names_result(attributes, values)
        }
        ("class_get_attributes", [class_name]) => {
            let class_name = eval_class_metadata_name(*class_name, values)?;
            let Some(attributes) = eval_class_like_attributes(context, &class_name) else {
                return values.array_new(0);
            };
            let attributes = attributes.to_vec();
            eval_reflection_attribute_array_result(&attributes, context, values)
        }
        ("class_attribute_args", [class_name, attribute_name]) => {
            let class_name = eval_class_metadata_name(*class_name, values)?;
            let attribute_name = eval_class_metadata_name(*attribute_name, values)?;
            let Some(attributes) = eval_class_like_attributes(context, &class_name) else {
                return values.array_new(0);
            };
            let Some(attribute) = attributes
                .iter()
                .find(|attribute| eval_attribute_name_matches(attribute.name(), &attribute_name))
            else {
                return values.array_new(0);
            };
            let Some(args) = attribute.args() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_class_attribute_args_result(args, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns class-like attributes for a dynamic eval class, interface, trait, or enum.
fn eval_class_like_attributes<'a>(
    context: &'a ElephcEvalContext,
    name: &str,
) -> Option<&'a [EvalAttribute]> {
    if let Some(class) = context.class(name) {
        return Some(class.attributes());
    }
    if let Some(interface) = context.interface(name) {
        return Some(interface.attributes());
    }
    if let Some(trait_decl) = context.trait_decl(name) {
        return Some(trait_decl.attributes());
    }
    context.enum_decl(name).map(EvalEnum::attributes)
}

/// Builds the indexed string array returned by `class_attribute_names()`.
fn eval_class_attribute_names_result(
    attributes: &[EvalAttribute],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(attributes.len())?;
    for (index, attribute) in attributes.iter().enumerate() {
        let key = values.int(index as i64)?;
        let value = values.string(attribute.name())?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Builds an indexed `ReflectionAttribute` array from eval-retained attribute metadata.
pub(in crate::interpreter) fn eval_reflection_attribute_array_result(
    attributes: &[EvalAttribute],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(attributes.len())?;
    for (index, attribute) in attributes.iter().enumerate() {
        let Some(args) = attribute.args() else {
            return Err(EvalStatus::RuntimeFatal);
        };
        let key = values.int(index as i64)?;
        let args = eval_class_attribute_args_result(args, values)?;
        let reflection_attribute = values.reflection_attribute_new(attribute.name(), args)?;
        let identity = values.object_identity(reflection_attribute)?;
        context.register_eval_reflection_attribute(identity, attribute.clone());
        values.release(args)?;
        result = values.array_set(result, key, reflection_attribute)?;
    }
    Ok(result)
}

/// Builds the indexed mixed array returned by `class_attribute_args()`.
fn eval_class_attribute_args_result(
    args: &[EvalAttributeArg],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(args.len())?;
    for (index, arg) in args.iter().enumerate() {
        let key = values.int(index as i64)?;
        let value = eval_class_attribute_arg_value(arg, values)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Materializes one retained eval attribute argument as a runtime cell.
fn eval_class_attribute_arg_value(
    arg: &EvalAttributeArg,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match arg {
        EvalAttributeArg::String(value) => values.string(value),
        EvalAttributeArg::Int(value) => values.int(*value),
        EvalAttributeArg::Bool(value) => values.bool_value(*value),
        EvalAttributeArg::Null => values.null(),
    }
}

/// Returns whether a query names the same PHP attribute class case-insensitively.
fn eval_attribute_name_matches(attribute_name: &str, query: &str) -> bool {
    attribute_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(query.trim_start_matches('\\'))
}

/// Returns whether a class-relation target refers to a known class-like symbol.
fn eval_class_relation_target_name(
    target: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    if values.type_tag(target)? == EVAL_TAG_OBJECT {
        let name = eval_get_class_result(target, context, values)?;
        let name = eval_class_metadata_name(name, values)?;
        return Ok(eval_class_relation_name_exists(&name, context, values)?.then_some(name));
    }
    let name = eval_class_metadata_name(target, values)?;
    Ok(eval_class_relation_name_exists(&name, context, values)?.then_some(name))
}

/// Returns whether one normalized class-like name exists in eval or runtime metadata.
fn eval_class_relation_name_exists(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.has_class(name)
        || context.has_interface(name)
        || context.has_trait(name)
        || context.has_enum(name)
        || values.class_exists(name)?
        || values.interface_exists(name)?
        || values.trait_exists(name)?
    {
        return Ok(true);
    }
    values.enum_exists(name)
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

/// Reads and normalizes one class metadata string argument.
fn eval_class_metadata_name(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(name.trim_start_matches('\\').to_string())
}
