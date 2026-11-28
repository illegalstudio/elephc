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
    callee: &str,
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
            let result = eval_declared_explicit_return_value(
                callee,
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
        EvalControl::Break | EvalControl::Continue | EvalControl::Goto(_) => {
            Err(EvalStatus::UnsupportedConstruct)
        }
    }
}

/// Applies a registered native/AOT return type to an already materialized result.
pub(in crate::interpreter) fn eval_declared_native_return_value(
    callee: Option<&str>,
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
            callee, false, return_type, return_owner, called_class_name, value, context, values,
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
    callee: &str,
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
    if eval_nonnull_object_boundary(return_type) && values.type_tag(value)? == EVAL_TAG_NULL {
        let expected = eval_null_boundary_type_label(return_type, return_owner, called_class_name, context)?;
        let callee = eval_boundary_callable_label(callee, context);
        return eval_throw_type_error(&format!("{callee}(): Return value must be of type {expected}, null returned"), context, values);
    }
    eval_declared_return_value(
        Some(callee),
        context.strict_types(),
        return_type,
        return_owner,
        called_class_name,
        value,
        context,
        values,
    )
}

/// Applies a non-void declared return type to one returned runtime value.
///
/// `strict` selects PHP's strict return rules; compiled AOT callables pass `false` because
/// their declared return values are already checked when the host is compiled.
fn eval_declared_return_value(
    callee: Option<&str>,
    strict: bool,
    return_type: &EvalParameterType,
    return_owner: Option<&str>,
    called_class_name: Option<&str>,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    checked_nullability::declared(value, return_type, context, values)?;
    if eval_declared_value_type_accepts_exact(
        return_type,
        return_owner,
        called_class_name,
        value,
        context,
        values,
    )? {
        return Ok(value);
    }
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions) {
        return eval_throw_type_error("--strict-coercions forbids implicit return conversion", context, values);
    }
    if return_type.is_intersection() {
        if context.current_function().is_some_and(|name| name.contains('<')) {
            return eval_throw_type_error("Return value does not satisfy the specialized intersection type", context, values);
        }
        return Err(EvalStatus::RuntimeFatal);
    }
    if strict {
        // A strict fragment allows only exact return types plus int-to-float widening.
        if values.type_tag(value)? == EVAL_TAG_INT
            && return_type
                .variants()
                .iter()
                .any(|variant| matches!(variant, EvalParameterTypeVariant::Float))
        {
            return values.cast_float(value);
        }
        if let Some(callee) = callee {
            return eval_throw_return_type_error(
                callee,
                return_owner,
                called_class_name,
                return_type,
                value,
                context,
                values,
            );
        }
    } else {
        for variant in return_type.variants() {
            if let Some(coerced) =
                eval_method_parameter_scalar_coercion(variant, value, context, values)?
            {
                return Ok(coerced);
            }
        }
    }
    if return_type.variants().iter().any(|ty| matches!(ty, EvalParameterTypeVariant::Generic(_)))
        || context.current_function().is_some_and(|name| name.contains('<')) {
        return eval_throw_type_error(&format!("Return value does not satisfy {}", crate::eval_ir::php_type_label(return_type)), context, values);
    }
    Err(EvalStatus::RuntimeFatal)
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

/// Gives each interpreted body its own return declaration without inheriting a caller's contract.
pub(in crate::interpreter) fn with_source_return_contract<T>(return_type: Option<&EvalParameterType>,
    declaring_class: Option<&str>, called_class: Option<&str>, context: &mut ElephcEvalContext,
    body: impl FnOnce(&mut ElephcEvalContext) -> T,
) -> T {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) { return body(context); }
    let contract = return_type.map(|ty| crate::context::ReturnSourceContract {
        ty: ty.clone(), declaring_class: declaring_class.map(str::to_string),
        called_class: called_class.map(str::to_string),
    });
    context.with_return_source_contract(contract, body)
}

/// Checks a logical return source where the function's own catch/finally regions can observe failure.
pub(in crate::interpreter) fn eval_source_return_expression(expr: &EvalExpr,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (value, source) = mixed_eval::evaluate(expr, context, scope, values)?;
    let result = (|| {
        if let Some(contract) = context.return_source_contract().cloned() {
            let target = local_binding_origins::resolve_declared_source_type(&crate::eval_ir::generic_type(&contract.ty),
                contract.declaring_class.as_deref(), contract.called_class.as_deref(), context)?;
            let expected = crate::eval_ir::parameter_type(&target);
            let source = source.as_ref().map(|source| ArgumentSource::capture(crate::eval_ir::parameter_type(source), context));
            mixed_source_proofs::require_argument(&expected, source.as_ref(), value, false, context, values)?;
        }
        Ok(value)
    })();
    finish_declared_return_value(value, result, context, values)
}
