//! Purpose:
//! Checks exact scalar, nominal and structural membership in a declared runtime value contract.
//!
//! Called from:
//! - Declared returns and eval-owned instance/static property validation.
//!
//! Key details:
//! - Declaration owners resolve self and parent independently of the caller's lexical scope.
//! - Membership never coerces or consumes the borrowed input value.
//! - Existing union, intersection, generic and callable rules stay shared across boundaries.

use super::*;

/// Returns whether a value already satisfies one declaration type.
pub(in crate::interpreter) fn eval_declared_value_type_accepts_exact(
    declared_type: &EvalParameterType,
    declaring_class: Option<&str>,
    called_class_name: Option<&str>,
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let tag = values.type_tag(value)?;
    if tag == EVAL_TAG_NULL && declared_type_allows_null(declared_type) {
        return Ok(true);
    }
    if declared_type.is_intersection() {
        for variant in declared_type.variants() {
            if !declared_variant_accepts_exact(
                variant,
                declaring_class,
                called_class_name,
                value,
                tag,
                context,
                values,
            )? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    for variant in declared_type.variants() {
        if declared_variant_accepts_exact(
            variant,
            declaring_class,
            called_class_name,
            value,
            tag,
            context,
            values,
        )? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Returns whether one non-null declaration type atom accepts a runtime value exactly.
fn declared_variant_accepts_exact(
    variant: &EvalParameterTypeVariant,
    declaring_class: Option<&str>,
    called_class_name: Option<&str>,
    value: RuntimeCellHandle,
    tag: u64,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match variant {
        EvalParameterTypeVariant::Generic(ty) => generic_value_accepts(ty, value, context, values),
        EvalParameterTypeVariant::Array => Ok(matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)),
        EvalParameterTypeVariant::Bool => Ok(tag == EVAL_TAG_BOOL),
        EvalParameterTypeVariant::Callable => {
            if context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions) {
                eval_is_callable_value(value, None, context, values)
            } else {
                Ok(matches!(tag, EVAL_TAG_STRING | EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE))
            }
        }
        EvalParameterTypeVariant::Class(class_name) => declared_class_accepts(
            value,
            tag,
            class_name,
            declaring_class,
            called_class_name,
            context,
            values,
        ),
        EvalParameterTypeVariant::Float => Ok(tag == EVAL_TAG_FLOAT),
        EvalParameterTypeVariant::Int => Ok(tag == EVAL_TAG_INT),
        EvalParameterTypeVariant::Iterable => {
            if matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
                return Ok(true);
            }
            if declared_class_accepts(
                value,
                tag,
                "Traversable",
                declaring_class,
                called_class_name,
                context,
                values,
            )? {
                return Ok(true);
            }
            declared_class_accepts(
                value,
                tag,
                "Iterator",
                declaring_class,
                called_class_name,
                context,
                values,
            )
        }
        EvalParameterTypeVariant::Mixed => Ok(true),
        EvalParameterTypeVariant::Never | EvalParameterTypeVariant::Void => Ok(false),
        EvalParameterTypeVariant::Object => Ok(matches!(tag, EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE)),
        EvalParameterTypeVariant::String => Ok(tag == EVAL_TAG_STRING),
    }
}

/// Returns whether an object value satisfies one class-like declaration target.
fn declared_class_accepts(
    value: RuntimeCellHandle,
    tag: u64,
    class_name: &str,
    declaring_class: Option<&str>,
    called_class_name: Option<&str>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if !matches!(tag, EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE) {
        return Ok(false);
    }
    let target = eval_declared_type_runtime_class_name(
        class_name,
        declaring_class,
        called_class_name,
        context,
    )?;
    if tag == EVAL_TAG_CALLABLE {
        return Ok(target.eq_ignore_ascii_case("Closure"));
    }
    let identity = values.object_identity(value)?;
    if let Some(class) = context.dynamic_object_class(identity) {
        return Ok(declared_dynamic_object_is_a(
            class.name(),
            &target,
            context,
        ));
    }
    if values.object_is_a(value, &target, false)? {
        return Ok(true);
    }
    if target.eq_ignore_ascii_case("Traversable") {
        return Ok(values.object_is_a(value, "Iterator", false)?
            || values.object_is_a(value, "IteratorAggregate", false)?);
    }
    Ok(false)
}

/// Returns whether one eval-created object class satisfies a declaration target.
fn declared_dynamic_object_is_a(
    class_name: &str,
    target: &str,
    context: &ElephcEvalContext,
) -> bool {
    if context.class_is_a(class_name, target, false) {
        return true;
    }
    target.eq_ignore_ascii_case("Traversable")
        && (context.class_is_a(class_name, "Iterator", false)
            || context.class_is_a(class_name, "IteratorAggregate", false))
}

/// Resolves class keywords and aliases in a declaration type atom.
pub(in crate::interpreter) fn eval_declared_type_runtime_class_name(
    class_name: &str,
    declaring_class: Option<&str>,
    called_class_name: Option<&str>,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name
        .trim_start_matches('\\')
        .to_ascii_lowercase()
        .as_str()
    {
        "self" => declaring_class
            .map(|owner| owner.trim_start_matches('\\').to_string())
            .ok_or(EvalStatus::RuntimeFatal),
        "static" => called_class_name
            .or(declaring_class)
            .map(|owner| owner.trim_start_matches('\\').to_string())
            .ok_or(EvalStatus::RuntimeFatal),
        "parent" => {
            let owner = declaring_class.ok_or(EvalStatus::RuntimeFatal)?;
            context
                .class(owner)
                .and_then(EvalClass::parent)
                .map(|parent| parent.trim_start_matches('\\').to_string())
                .ok_or(EvalStatus::RuntimeFatal)
        }
        _ => Ok(context
            .resolve_class_like_name(class_name)
            .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string())),
    }
}

/// Returns whether a declaration type can accept PHP null.
fn declared_type_allows_null(declared_type: &EvalParameterType) -> bool {
    declared_type.allows_null()
        || (!declared_type.is_intersection()
            && declared_type
                .variants()
                .iter()
                .any(|variant| matches!(variant, EvalParameterTypeVariant::Mixed)))
}

