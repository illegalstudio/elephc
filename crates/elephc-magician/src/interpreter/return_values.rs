//! Purpose:
//! Enforces declared eval function and method return values at runtime.
//! This keeps return-value checks separate from argument binding and statement dispatch.
//!
//! Called from:
//! - `crate::interpreter::dynamic_functions`
//! - `crate::interpreter::statements`
//!
//! Key details:
//! - `self` resolves to the declaring owner, while `static` resolves to the called class.
//! - Return values use weak scalar coercions like parameter binding, with dedicated handling for `void` and `never`.

use super::*;

/// Applies one declared function or method return type to a completed control result.
pub(in crate::interpreter) fn eval_declared_return_control_value(
    return_type: Option<&EvalParameterType>,
    return_owner: Option<&str>,
    called_class_name: Option<&str>,
    control: EvalControl,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match control {
        EvalControl::None => eval_declared_implicit_return_value(return_type, values),
        EvalControl::ReturnVoid => eval_declared_void_return_value(return_type, values),
        EvalControl::Return(result) => eval_declared_explicit_return_value(
            return_type,
            return_owner,
            called_class_name,
            result,
            context,
            values,
        ),
        EvalControl::Throw(result) => {
            context.set_pending_throw(result);
            Err(EvalStatus::UncaughtThrowable)
        }
        EvalControl::Break | EvalControl::Continue => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Materializes an implicit return according to the declared return type.
fn eval_declared_implicit_return_value(
    return_type: Option<&EvalParameterType>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(return_type) = return_type else {
        return values.null();
    };
    if eval_declared_return_type_is_void(return_type) {
        return values.null();
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Materializes `return;` according to the declared return type.
fn eval_declared_void_return_value(
    return_type: Option<&EvalParameterType>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(return_type) = return_type else {
        return values.null();
    };
    if eval_declared_return_type_is_void(return_type) {
        return values.null();
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Validates or coerces an explicit returned value according to a declared return type.
fn eval_declared_explicit_return_value(
    return_type: Option<&EvalParameterType>,
    return_owner: Option<&str>,
    called_class_name: Option<&str>,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(return_type) = return_type else {
        return Ok(value);
    };
    if eval_declared_return_type_is_void(return_type)
        || eval_declared_return_type_is_never(return_type)
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_declared_return_value(
        return_type,
        return_owner,
        called_class_name,
        value,
        context,
        values,
    )
}

/// Applies a non-void declared return type to one returned runtime value.
fn eval_declared_return_value(
    return_type: &EvalParameterType,
    return_owner: Option<&str>,
    called_class_name: Option<&str>,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_declared_return_type_accepts_exact(
        return_type,
        return_owner,
        called_class_name,
        value,
        context,
        values,
    )? {
        return Ok(value);
    }
    if return_type.is_intersection() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for variant in return_type.variants() {
        if let Some(coerced) =
            eval_method_parameter_scalar_coercion(variant, value, context, values)?
        {
            return Ok(coerced);
        }
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Returns whether a value already satisfies one declared return type.
fn eval_declared_return_type_accepts_exact(
    return_type: &EvalParameterType,
    return_owner: Option<&str>,
    called_class_name: Option<&str>,
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let tag = values.type_tag(value)?;
    if tag == EVAL_TAG_NULL && eval_declared_return_type_allows_null(return_type) {
        return Ok(true);
    }
    if return_type.is_intersection() {
        for variant in return_type.variants() {
            if !eval_declared_return_variant_accepts_exact(
                variant,
                return_owner,
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
    for variant in return_type.variants() {
        if eval_declared_return_variant_accepts_exact(
            variant,
            return_owner,
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

/// Returns whether one non-null return type atom accepts a runtime value exactly.
fn eval_declared_return_variant_accepts_exact(
    variant: &EvalParameterTypeVariant,
    return_owner: Option<&str>,
    called_class_name: Option<&str>,
    value: RuntimeCellHandle,
    tag: u64,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match variant {
        EvalParameterTypeVariant::Array => Ok(matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)),
        EvalParameterTypeVariant::Bool => Ok(tag == EVAL_TAG_BOOL),
        EvalParameterTypeVariant::Callable => Ok(matches!(
            tag,
            EVAL_TAG_STRING | EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT
        )),
        EvalParameterTypeVariant::Class(class_name) => eval_declared_return_class_accepts(
            value,
            tag,
            class_name,
            return_owner,
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
            if eval_declared_return_class_accepts(
                value,
                tag,
                "Traversable",
                return_owner,
                called_class_name,
                context,
                values,
            )? {
                return Ok(true);
            }
            eval_declared_return_class_accepts(
                value,
                tag,
                "Iterator",
                return_owner,
                called_class_name,
                context,
                values,
            )
        }
        EvalParameterTypeVariant::Mixed => Ok(true),
        EvalParameterTypeVariant::Never | EvalParameterTypeVariant::Void => Ok(false),
        EvalParameterTypeVariant::Object => Ok(tag == EVAL_TAG_OBJECT),
        EvalParameterTypeVariant::String => Ok(tag == EVAL_TAG_STRING),
    }
}

/// Returns whether an object value satisfies one class-like declared return target.
fn eval_declared_return_class_accepts(
    value: RuntimeCellHandle,
    tag: u64,
    class_name: &str,
    return_owner: Option<&str>,
    called_class_name: Option<&str>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if tag != EVAL_TAG_OBJECT {
        return Ok(false);
    }
    let target = eval_declared_return_runtime_class_name(
        class_name,
        return_owner,
        called_class_name,
        context,
    )?;
    let identity = values.object_identity(value)?;
    if let Some(class) = context.dynamic_object_class(identity) {
        return Ok(eval_declared_dynamic_object_is_a(
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

/// Returns whether one eval-created object class satisfies a declared return target.
fn eval_declared_dynamic_object_is_a(
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

/// Resolves class keywords and aliases in a declared return type atom.
fn eval_declared_return_runtime_class_name(
    class_name: &str,
    return_owner: Option<&str>,
    called_class_name: Option<&str>,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name
        .trim_start_matches('\\')
        .to_ascii_lowercase()
        .as_str()
    {
        "self" => return_owner
            .map(|owner| owner.trim_start_matches('\\').to_string())
            .ok_or(EvalStatus::RuntimeFatal),
        "static" => called_class_name
            .or(return_owner)
            .map(|owner| owner.trim_start_matches('\\').to_string())
            .ok_or(EvalStatus::RuntimeFatal),
        "parent" => {
            let owner = return_owner.ok_or(EvalStatus::RuntimeFatal)?;
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

/// Returns whether a declared return type can accept PHP null.
fn eval_declared_return_type_allows_null(return_type: &EvalParameterType) -> bool {
    return_type.allows_null()
        || (!return_type.is_intersection()
            && return_type
                .variants()
                .iter()
                .any(|variant| matches!(variant, EvalParameterTypeVariant::Mixed)))
}

/// Returns whether a declared return type is exactly PHP `never`.
fn eval_declared_return_type_is_never(return_type: &EvalParameterType) -> bool {
    !return_type.allows_null()
        && !return_type.is_intersection()
        && matches!(return_type.variants(), [EvalParameterTypeVariant::Never])
}

/// Returns whether a declared return type is exactly PHP `void`.
fn eval_declared_return_type_is_void(return_type: &EvalParameterType) -> bool {
    !return_type.allows_null()
        && !return_type.is_intersection()
        && matches!(return_type.variants(), [EvalParameterTypeVariant::Void])
}
