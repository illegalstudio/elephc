//! Purpose:
//! Implements eval class metadata and class-relation introspection builtins.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - Eval-declared classes currently have no parent, interface, trait, or
//!   attribute metadata, so known class-like targets return empty arrays.
//! - Missing class-like relation targets return `false`, matching the main
//!   backend's unknown-target fallback.

use super::super::*;
use super::*;

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
    if !eval_class_relation_target_exists(target, context, values)? {
        return values.bool_value(false);
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
    eval_class_attribute_metadata_result(name, &evaluated_args, values)
}

/// Evaluates materialized class attribute metadata arguments.
pub(in crate::interpreter) fn eval_class_attribute_metadata_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match (name, evaluated_args) {
        ("class_attribute_names" | "class_get_attributes", [class_name]) => {
            let _ = eval_class_metadata_name(*class_name, values)?;
            values.array_new(0)
        }
        ("class_attribute_args", [class_name, attribute_name]) => {
            let _ = eval_class_metadata_name(*class_name, values)?;
            let _ = eval_class_metadata_name(*attribute_name, values)?;
            values.array_new(0)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns whether a class-relation target refers to a known class-like symbol.
fn eval_class_relation_target_exists(
    target: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.type_tag(target)? == EVAL_TAG_OBJECT {
        let name = eval_get_class_result(target, context, values)?;
        let name = eval_class_metadata_name(name, values)?;
        return eval_class_relation_name_exists(&name, context, values);
    }
    let name = eval_class_metadata_name(target, values)?;
    eval_class_relation_name_exists(&name, context, values)
}

/// Returns whether one normalized class-like name exists in eval or runtime metadata.
fn eval_class_relation_name_exists(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.has_class(name)
        || values.class_exists(name)?
        || values.interface_exists(name)?
        || values.trait_exists(name)?
    {
        return Ok(true);
    }
    values.enum_exists(name)
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
