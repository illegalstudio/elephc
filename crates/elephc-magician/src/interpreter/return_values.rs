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
        EvalControl::None | EvalControl::ReturnVoid => {
            let result = if matches!(control, EvalControl::None) { eval_declared_implicit_return_value(return_type, values) }
                else { eval_declared_void_return_value(return_type, values) };
            if result == Err(EvalStatus::RuntimeFatal) && context.current_function().is_some_and(|name| name.contains('<')) {
                return eval_throw_type_error("Missing return value for specialized return type", context, values);
            }
            result
        }
        EvalControl::Return(value) => {
            // A `return $var;` hands back the SCOPE's cell, and a borrowed handle carries no
            // owner of its own. `finish_activation_scope` releases every owned local right
            // after this, so returning the borrowed cell unchanged left the caller holding
            // freed storage: Symfony's `SodiumVault::list()` returned the list hash, the
            // activation released the same box, and `count()`/`foreach` then read the
            // allocator's free-list pointer. Take the same lease the throw path takes.
            let value = if value.is_borrowed() { values.retain(value)? } else { value };
            let result = eval_declared_explicit_return_value(
                return_type,
                return_owner,
                called_class_name,
                value,
                context,
                values,
            );
            finish_declared_return_value(value, result, context, values)
        }
        EvalControl::Throw(result) => {
            let thrown = if result.is_borrowed() { values.retain(result)? } else { result };
            context.set_pending_throw(thrown);
            Err(EvalStatus::UncaughtThrowable)
        }
        EvalControl::Break(_) | EvalControl::Continue(_) | EvalControl::Goto(_) => {
            Err(EvalStatus::UnsupportedConstruct)
        }
    }
}

/// Applies a registered native/AOT return type to an already materialized result.
pub(in crate::interpreter) fn eval_declared_native_return_value(
    return_type: Option<&EvalParameterType>,
    return_owner: Option<&str>,
    called_class_name: Option<&str>,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = (|| {
        let Some(return_type) = return_type else {
            return Ok(value);
        };
        if eval_declared_return_type_is_void(return_type) {
            return if values.type_tag(value)? == EVAL_TAG_NULL {
                Ok(value)
            } else {
                Err(EvalStatus::RuntimeFatal)
            };
        }
        if eval_declared_return_type_is_never(return_type) {
            return Err(EvalStatus::RuntimeFatal);
        }
        eval_declared_return_value(
            return_type, return_owner, called_class_name, value, context, values,
        )
    })();
    finish_declared_return_value(value, result, context, values)
}

/// Consumes an owned return cell when type validation rejects it or replaces it with a coerced value.
fn finish_declared_return_value(
    original: RuntimeCellHandle,
    result: Result<RuntimeCellHandle, EvalStatus>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(&result, Ok(value) if *value == original) {
        if let Err(status) = eval_release_value(context, values, original) {
            if let Ok(coerced) = result {
                eval_release_value(context, values, coerced)?;
            }
            return Err(status);
        }
    }
    result
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
        if context.current_function().is_some_and(|name| name.contains('<')) {
            return eval_throw_type_error("Return value does not satisfy the specialized intersection type", context, values);
        }
        return Err(EvalStatus::RuntimeFatal);
    }
    for variant in return_type.variants() {
        if !eval_scalar_coercion_is_allowed(variant, value, context, values)? {
            continue;
        }
        if let Some(coerced) =
            eval_method_parameter_scalar_coercion(variant, value, context, values)?
        {
            return Ok(coerced);
        }
    }
    if return_type.variants().iter().any(|ty| matches!(ty, EvalParameterTypeVariant::Generic(_)))
        || context.current_function().is_some_and(|name| name.contains('<')) {
        return eval_throw_type_error(&format!("Return value does not satisfy {}", crate::eval_ir::generic_type(return_type)), context, values);
    }
    // PHP names the callable, says `Return value`, and names both types. A `static` return
    // type is named by the class it resolved to (`must be of type Child`).
    let mut expected = eval_declared_type_spelling(return_type);
    if let Some(called) = called_class_name {
        expected = expected
            .split('|')
            .map(|part| match part {
                "static" => called.trim_start_matches('\\').to_string(),
                "?static" => format!("?{}", called.trim_start_matches('\\')),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join("|");
    }
    let message = format!(
        "{}(): Return value must be of type {}, {} returned",
        context.current_function().unwrap_or_default(),
        expected,
        eval_given_type_spelling(value, values)?
    );
    eval_throw_type_error(&message, context, values)
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
        EvalParameterTypeVariant::Generic(ty) => generic_value_accepts(ty, value, context, values),
        EvalParameterTypeVariant::Array => Ok(matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)),
        EvalParameterTypeVariant::Bool => Ok(tag == EVAL_TAG_BOOL),
        EvalParameterTypeVariant::Callable => Ok(matches!(
            tag,
            EVAL_TAG_STRING
                | EVAL_TAG_ARRAY
                | EVAL_TAG_ASSOC
                | EVAL_TAG_OBJECT
                | EVAL_TAG_CALLABLE
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
        // PHP 8.2 value types accept ONE boolean each, not either.
        EvalParameterTypeVariant::False => Ok(tag == EVAL_TAG_BOOL && !values.truthy(value)?),
        EvalParameterTypeVariant::True => Ok(tag == EVAL_TAG_BOOL && values.truthy(value)?),
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
    if tag != EVAL_TAG_OBJECT && tag != EVAL_TAG_CALLABLE {
        return Ok(false);
    }
    let target = eval_declared_return_runtime_class_name(
        class_name,
        return_owner,
        called_class_name,
        context,
    )?;
    // A closure built by COMPILED code crosses the bridge as a callable descriptor (tag 10), which
    // is its only representation there — there is no object to ask for a class name. It is still a
    // `Closure` to PHP, and returning one is ordinary: Symfony's
    // `HtmlErrorRenderer::isDebug(): \Closure` hands one to interpreted code on every error-handler
    // build, and refusing it took the whole request down.
    if tag == EVAL_TAG_CALLABLE {
        return Ok(target.eq_ignore_ascii_case("Closure"));
    }
    let identity = values.object_identity(value)?;
    if context.dynamic_object_is_a(identity, &target) {
        return Ok(true);
    }
    if values.object_is_a(value, &target, false)? {
        return Ok(true);
    }
    if crate::eval_trace::enabled() {
        let actual = values
            .object_class_name(value)
            .and_then(|name| {
                let bytes = values.string_bytes(name)?;
                values.release(name)?;
                String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
            });
        eprintln!(
            "[elephc-eval-trace] phase=return_object_mismatch actual={actual:?} target={target:?}",
        );
    }
    if target.eq_ignore_ascii_case("Traversable") {
        return Ok(values.object_is_a(value, "Iterator", false)?
            || values.object_is_a(value, "IteratorAggregate", false)?);
    }
    Ok(false)
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

/// Returns whether one scalar coercion is allowed under the mode in force.
///
/// `declare(strict_types=1)` turns every scalar coercion off with ONE exception php keeps: an
/// int where a float is declared still widens. Measured with `php -n` 8.5.6: `takesInt("5")`
/// throws under strict and returns 6 without it, while `takesFloat(5)` returns 5 in both.
pub(in crate::interpreter) fn eval_scalar_coercion_is_allowed(
    variant: &EvalParameterTypeVariant,
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if !context.strict_types() {
        return Ok(true);
    }
    Ok(matches!(variant, EvalParameterTypeVariant::Float)
        && values.type_tag(value)? == EVAL_TAG_INT)
}

/// Spells one declared type the way PHP spells it in a `TypeError`.
pub(in crate::interpreter) fn eval_declared_type_spelling(
    declared: &EvalParameterType,
) -> String {
    let separator = if declared.is_intersection() { "&" } else { "|" };
    let mut names: Vec<String> = declared
        .variants()
        .iter()
        .map(eval_declared_variant_spelling)
        .collect();
    if declared.allows_null() && !names.iter().any(|name| name == "null") {
        names.push("null".to_string());
    }
    names.join(separator)
}

/// Spells one declared type atom.
fn eval_declared_variant_spelling(variant: &EvalParameterTypeVariant) -> String {
    match variant {
        EvalParameterTypeVariant::Generic(ty) => ty.to_string(),
        EvalParameterTypeVariant::Array => "array".to_string(),
        EvalParameterTypeVariant::Bool => "bool".to_string(),
        EvalParameterTypeVariant::Callable => "callable".to_string(),
        EvalParameterTypeVariant::Class(name) => name.trim_start_matches('\\').to_string(),
        EvalParameterTypeVariant::False => "false".to_string(),
        EvalParameterTypeVariant::Float => "float".to_string(),
        EvalParameterTypeVariant::Int => "int".to_string(),
        EvalParameterTypeVariant::Iterable => "iterable".to_string(),
        EvalParameterTypeVariant::Mixed => "mixed".to_string(),
        EvalParameterTypeVariant::Never => "never".to_string(),
        EvalParameterTypeVariant::Object => "object".to_string(),
        EvalParameterTypeVariant::String => "string".to_string(),
        EvalParameterTypeVariant::True => "true".to_string(),
        EvalParameterTypeVariant::Void => "void".to_string(),
    }
}

/// Spells one runtime value's type the way PHP names it in a `TypeError`.
///
/// A boolean is named by its OWN VALUE, `true`/`false`, never the generic `bool` -- measured
/// against `php -n` 8.5.6 across every context this helper feeds (a declared parameter under
/// `strict_types`, a return-type coercion, an internal-function argument): `function ti(int $i)`
/// called `ti(true)` under `strict_types=1` says `must be of type int, true given`, and so does
/// every `float`/`string`/`array` sibling. `crates/elephc-magician/src/interpreter/tests` had no
/// fixture pinning the old `"bool"` spelling; the ONLY place that spelling was asserted is the
/// separate compiled-backend suite (`tests/error_tests/type_system.rs`), which is itself wrong
/// against the same measurement and out of this fix's scope.
pub(in crate::interpreter) fn eval_given_type_spelling(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    Ok(match values.type_tag(value)? {
        EVAL_TAG_NULL => "null".to_string(),
        EVAL_TAG_BOOL => if values.truthy(value)? { "true" } else { "false" }.to_string(),
        EVAL_TAG_INT => "int".to_string(),
        EVAL_TAG_FLOAT => "float".to_string(),
        EVAL_TAG_STRING => "string".to_string(),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array".to_string(),
        EVAL_TAG_RESOURCE => "resource".to_string(),
        EVAL_TAG_OBJECT => {
            // An eval-declared object is carried by a stdClass runtime object; its PHP class is
            // the one its owning context records. The carrier's name made a `static` return
            // mismatch read `..., stdClass returned` where php names the class.
            if let Some(name) = eval_owned_object_class_name(value, values)? {
                return Ok(name);
            }
            let name = values.object_class_name(value)?;
            let bytes = values.string_bytes(name)?;
            values.release(name)?;
            String::from_utf8(bytes).unwrap_or_else(|_| "object".to_string())
        }
        _ => "mixed".to_string(),
    })
}

/// Returns the eval class of an object some eval context owns, or `None` for a native object.
fn eval_owned_object_class_name(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let identity = values.object_identity(value)?;
    let Some(context) = crate::dynamic_destructors::dynamic_object_owner_context(identity) else {
        return Ok(None);
    };
    // SAFETY: the owner registry only hands out live contexts.
    Ok(unsafe { context.as_ref() }.and_then(|context| context.dynamic_object_class_name(identity)))
}
