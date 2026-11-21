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
    eval_boundary_type_label(ty, owner, called, context)
}

/// Formats one declared argument or return type the way PHP prints it in a `TypeError`.
///
/// Class atoms resolve through the active class scope (`self`, `static`, `parent`) or the
/// lexical class map, while built-in atoms reuse Reflection's canonical type labels. A single
/// type prints as `?T` when nullable; a union prints its members in PHP's order with `null`
/// appended.
pub(in crate::interpreter) fn eval_boundary_type_label(
    ty: &EvalParameterType,
    owner: Option<&str>,
    called: Option<&str>,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    let mut names = Vec::new();
    for variant in ty.variants() {
        match variant {
            EvalParameterTypeVariant::Class(name) => names.push(if owner.is_some() {
                eval_declared_type_runtime_class_name(name, owner, called, context)?
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
                .unwrap_or_else(|| crate::eval_ir::generic_type(ty).to_string()),
            ),
        }
    }
    Ok(if ty.is_intersection() {
        let intersection = names.join("&");
        if ty.allows_null() {
            format!("({intersection})|null")
        } else {
            intersection
        }
    } else if names.len() == 1 {
        elephc_builtin_contract::reflection_format::named_type(&names[0], ty.allows_null())
    } else {
        elephc_builtin_contract::reflection_format::union(names, ty.allows_null())
    })
}

/// Raises PHP's catchable `TypeError` for a rejected declared argument type.
pub(in crate::interpreter) fn eval_throw_argument_type_error<T>(
    callee: &str,
    number: usize,
    parameter: Option<&str>,
    ty: &EvalParameterType,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let callee = eval_boundary_callable_label(callee, context);
    let parameter = parameter
        .map(|name| format!(" (${name})"))
        .unwrap_or_default();
    let expected = eval_boundary_type_label(ty, None, None, context)?;
    let given = eval_given_type_name(value, context, values)?;
    eval_throw_type_error(
        &format!(
            "{callee}(): Argument #{number}{parameter} must be of type {expected}, {given} given"
        ),
        context,
        values,
    )
}

/// Raises PHP's catchable `TypeError` for a rejected declared return type.
pub(in crate::interpreter) fn eval_throw_return_type_error<T>(
    callee: &str,
    return_owner: Option<&str>,
    called_class_name: Option<&str>,
    ty: &EvalParameterType,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let callee = eval_boundary_callable_label(callee, context);
    let expected = eval_boundary_type_label(ty, return_owner, called_class_name, context)?;
    let given = eval_given_type_name(value, context, values)?;
    eval_throw_type_error(
        &format!(
            "{callee}(): Return value must be of type {expected}, {given} returned"
        ),
        context,
        values,
    )
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
