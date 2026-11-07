//! Purpose:
//! Rejects null at declared non-null object and iterable boundaries through Throwable.
//!
//! Called from:
//! - Argument binding, declared returns, and eval property writes.
//!
//! Key details:
//! - Nullable and mixed hints retain their existing behavior.
//! - Other type mismatches continue through the existing coercion and validation paths.

use super::*;

/// Recognizes hints for which a null object-like value must raise a catchable type error.
pub(in crate::interpreter) fn eval_nonnull_object_boundary(ty: &EvalParameterType) -> bool {
    !ty.allows_null()
        && !ty
            .variants()
            .iter()
            .any(|variant| matches!(variant, EvalParameterTypeVariant::Mixed))
        && ty.variants().iter().any(|variant| {
            matches!(
                variant,
                EvalParameterTypeVariant::Class(_)
                    | EvalParameterTypeVariant::Object
                    | EvalParameterTypeVariant::Iterable
            )
        })
}

/// Formats PHP error types, resolving lexical classes and expanding iterable into its two kinds.
pub(in crate::interpreter) fn eval_null_boundary_type_label(
    ty: &EvalParameterType,
    owner: Option<&str>,
    called: Option<&str>,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    let mut names = Vec::new();
    for variant in ty.variants() {
        match variant {
            EvalParameterTypeVariant::Class(name) => names.push(if owner.is_some() {
                eval_declared_return_runtime_class_name(name, owner, called, context)?
            } else {
                eval_method_parameter_runtime_class_name(name, context)?
            }),
            EvalParameterTypeVariant::Iterable => {
                names.extend(["Traversable".into(), "array".into()])
            }
            _ => names.push(
                eval_parameter_type_error_label(&EvalParameterType::new(
                    vec![variant.clone()],
                    false,
                ))
                .ok_or(EvalStatus::RuntimeFatal)?,
            ),
        }
    }
    Ok(if ty.is_intersection() {
        names.join("&")
    } else {
        elephc_builtin_contract::reflection_format::union(names, false)
    })
}

/// Uses PHP accessor labels instead of their generated method names in boundary errors.
pub(in crate::interpreter) fn eval_boundary_callable_label(callee: &str, context: &ElephcEvalContext) -> String {
    if let Some((class, method)) = callee.rsplit_once("::") {
        for (prefix, kind) in [("__propset_", "set"), ("__propget_", "get")] {
            if let Some(property) = method.strip_prefix(prefix) {
                if context.class_property(class, property)
                    .is_some_and(|(_, metadata)| metadata.matches_hook_method(method)) {
                    return format!("{class}::${property}::{kind}");
                }
            }
        }
    }
    callee.trim_start_matches('\\').to_string()
}

/// Refuses a null argument before bound arguments can enter the callable.
pub(in crate::interpreter) fn eval_reject_null_object_argument(
    callee: &str,
    number: usize,
    parameter: Option<&str>,
    ty: &EvalParameterType,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !eval_nonnull_object_boundary(ty) || values.type_tag(value)? != EVAL_TAG_NULL {
        return Ok(());
    }
    let callee = eval_boundary_callable_label(callee, context);
    let parameter = parameter
        .map(|name| format!(" (${name})"))
        .unwrap_or_default();
    let expected = eval_null_boundary_type_label(ty, None, None, context)?;
    eval_throw_type_error(
        &format!(
            "{callee}(): Argument #{number}{parameter} must be of type {expected}, null given"
        ),
        context,
        values,
    )
}

/// Validates an eval-declared property before changing its backing slot or a reference alias.
pub(in crate::interpreter) fn eval_reject_null_object_property(
    owner: &str,
    property: &EvalClassProperty,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(ty) = property.property_type() else {
        return Ok(());
    };
    if !eval_nonnull_object_boundary(ty) || values.type_tag(value)? != EVAL_TAG_NULL {
        return Ok(());
    }
    let expected = eval_null_boundary_type_label(ty, Some(owner), Some(owner), context)?;
    eval_throw_type_error(
        &format!(
            "Cannot assign null to property {}::${} of type {expected}",
            owner.trim_start_matches('\\'),
            property.name()
        ),
        context,
        values,
    )
}

/// Shares static-slot validation with Reflection before a borrowed write can replace its owner.
pub(in crate::interpreter) fn eval_reject_null_object_static_property(
    class: &str,
    property: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some((owner, metadata)) = context.class_property(class, property) {
        eval_reject_null_object_property(&owner, &metadata, value, context, values)?;
    }
    Ok(())
}

/// Checks a top-level source before preparation can retarget an already published hidden alias.
pub(in crate::interpreter) fn eval_preflight_property_reference_source(
    owner: &str, property: &EvalClassProperty, source: &str,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if context.current_function().is_some() || scope.reference_target(source).is_some()
        || !property.property_type().is_some_and(eval_nonnull_object_boundary) {
        return Ok(());
    }
    let value = match visible_scope_cell(context, scope, source) {
        Some(value) => value,
        None => {
            // PHP creates an absent source as null even when the property bind is refused.
            let value = values.null()?;
            for previous in set_owned_scope_cell(context, scope, source.to_string(), value, values)? {
                eval_release_value(context, values, previous)?;
            }
            value
        }
    };
    eval_reject_null_object_property(owner, property, value, context, values)
}
