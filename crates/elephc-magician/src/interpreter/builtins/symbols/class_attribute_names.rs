//! Purpose:
//! Eval registry entry and implementation for `class_attribute_names`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Shared class-attribute metadata logic for `class_attribute_args()` and
//!   `class_get_attributes()` lives here.

eval_builtin! {
    contract: "class_attribute_names",
    area: Symbols,
    direct: Symbols,
    values: Symbols,
}

use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};
use super::super::super::*;
use super::super::eval_class_metadata_name;

/// Dispatches direct eval calls for the `class_attribute_names` symbol builtin.
pub(in crate::interpreter) fn eval_class_attribute_names_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_class_attribute_metadata("class_attribute_names", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `class_attribute_names` symbol builtin.
pub(in crate::interpreter) fn eval_class_attribute_names_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_class_attribute_metadata_result("class_attribute_names", evaluated_args, context, values)
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
            let attributes = eval_class_like_attribute_metadata(context, &class_name);
            eval_class_attribute_names_result(&attributes, values)
        }
        ("class_get_attributes", [class_name]) => {
            let class_name = eval_class_metadata_name(*class_name, values)?;
            let attributes = eval_class_like_attribute_metadata(context, &class_name);
            eval_reflection_attribute_array_result(
                &attributes,
                EVAL_REFLECTION_ATTRIBUTE_TARGET_CLASS,
                context,
                values,
            )
        }
        ("class_attribute_args", [class_name, attribute_name]) => {
            let class_name = eval_class_metadata_name(*class_name, values)?;
            let attribute_name = eval_class_metadata_name(*attribute_name, values)?;
            let attributes = eval_class_like_attribute_metadata(context, &class_name);
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

/// Returns class-like attributes for eval declarations or generated AOT metadata.
fn eval_class_like_attribute_metadata(
    context: &ElephcEvalContext,
    name: &str,
) -> Vec<EvalAttribute> {
    if let Some(attributes) = eval_class_like_attributes(context, name) {
        return attributes.to_vec();
    }
    context.native_class_attributes(name)
}

/// Builds the indexed string array returned by `class_attribute_names()`.
fn eval_class_attribute_names_result(
    attributes: &[EvalAttribute],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::indexed(values, attributes.len())?;
    for (index, attribute) in attributes.iter().enumerate() {
        result.entry_key_first(|values| values.int(index as i64), |values| values.string(attribute.name()))?;
    }
    Ok(result.finish())
}

/// Builds an indexed `ReflectionAttribute` array from eval-retained attribute metadata.
pub(in crate::interpreter) fn eval_reflection_attribute_array_result(
    attributes: &[EvalAttribute],
    target: u64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::indexed(values, attributes.len())?;
    for (index, attribute) in attributes.iter().enumerate() {
        let args = attribute.args().ok_or(EvalStatus::RuntimeFatal)?;
        let repeated = eval_attribute_is_repeated(attributes, attribute.name());
        result.entry_key_first(|values| values.int(index as i64), |values| {
            let args = RuntimeOwnerSlot::new(eval_class_attribute_args_result(args, values)?);
            let reflector = RuntimeOwnerSlot::empty();
            with_runtime_owners(&(&args, &reflector), || {
                let operation = (|| {
                    let object = values.reflection_attribute_new(attribute.name(), args.borrowed(), target, repeated)?;
                    reflector.replace(Some(object));
                    let identity = values.object_identity(object)?;
                    context.register_eval_reflection_attribute(identity, attribute.clone(), target, repeated);
                    Ok(())
                })();
                let released = values.release(args.take());
                reflector.finish(operation.and(released), |cell| values.release(cell))
            })
        })?;
    }
    Ok(result.finish())
}

/// Returns true when an attribute name appears more than once on the same owner.
fn eval_attribute_is_repeated(attributes: &[EvalAttribute], name: &str) -> bool {
    attributes
        .iter()
        .filter(|attribute| eval_attribute_name_matches(attribute.name(), name))
        .nth(1)
        .is_some()
}

/// Builds the mixed PHP array returned by `class_attribute_args()`.
pub(in crate::interpreter) fn eval_class_attribute_args_result(
    args: &[EvalAttributeArg],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = if args.iter().any(|arg| arg.name().is_some()) {
        EvalArrayBuilder::assoc(values, args.len())?
    } else {
        EvalArrayBuilder::indexed(values, args.len())?
    };
    for (index, arg) in args.iter().enumerate() {
        result.entry_key_first(
            |values| match arg.name() {
                Some(name) => values.string(name),
                None => values.int(index as i64),
            },
            |values| eval_class_attribute_arg_value(arg.value(), values),
        )?;
    }
    Ok(result.finish())
}

/// Materializes one retained eval attribute argument as a runtime cell.
fn eval_class_attribute_arg_value(
    arg: &EvalAttributeArg,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match arg {
        EvalAttributeArg::String(value) => values.string_literal(value),
        EvalAttributeArg::Bytes(value) => values.string_literal_bytes(value),
        EvalAttributeArg::Int(value) => values.int(*value),
        EvalAttributeArg::Float(bits) => values.float(f64::from_bits(*bits)),
        EvalAttributeArg::Bool(value) => values.bool_value(*value),
        EvalAttributeArg::Null => values.null(),
        EvalAttributeArg::Array(elements) => eval_class_attribute_array_arg_value(elements, values),
        EvalAttributeArg::Named { value, .. } | EvalAttributeArg::IntKeyed { value, .. } => {
            eval_class_attribute_arg_value(value, values)
        }
    }
}

/// Materializes one retained attribute array literal as a runtime array cell.
fn eval_class_attribute_array_arg_value(
    elements: &[EvalAttributeArg],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = if elements
        .iter()
        .any(|element| element.name().is_some() || element.int_key().is_some()) {
        EvalArrayBuilder::assoc(values, elements.len())?
    } else {
        EvalArrayBuilder::indexed(values, elements.len())?
    };
    for (index, element) in elements.iter().enumerate() {
        result.entry_key_first(
            |values| match element.name() {
                Some(name) => values.string(name),
                None => values.int(element.int_key().unwrap_or(index as i64)),
            },
            |values| eval_class_attribute_arg_value(element.value(), values),
        )?;
    }
    Ok(result.finish())
}

/// Returns whether a query names the same PHP attribute class case-insensitively.
fn eval_attribute_name_matches(attribute_name: &str, query: &str) -> bool {
    attribute_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(query.trim_start_matches('\\'))
}
