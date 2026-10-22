//! Purpose:
//! Keeps compiler-generated deferred parameter default trees lazy in eval Reflection owners.
//!
//! Called from:
//! - ReflectionParameter metadata object construction and default placeholder selection.
//!
//! Key details:
//! - Capture-free native factories outlive the eval context that creates a parameter object.
//! - Only protected native Reflection getters invoke these factories, never Rust default binding.

use super::*;

/// Finds deferred Error data without evaluating prefixes or selecting lazy branches.
pub(super) fn parameter_default_is_deferred(expression: &EvalExpr) -> bool {
    match expression {
        EvalExpr::DeferredDefaultError(_) => true,
        EvalExpr::Array(entries) => entries.iter().any(|entry| match entry {
            EvalArrayElement::Value(value) | EvalArrayElement::Reference(value) =>
                parameter_default_is_deferred(value),
            EvalArrayElement::KeyValue { key, value }
            | EvalArrayElement::KeyReference { key, value } =>
                parameter_default_is_deferred(key) || parameter_default_is_deferred(value),
        }),
        EvalExpr::ArrayGet { array, index } =>
            parameter_default_is_deferred(array) || parameter_default_is_deferred(index),
        EvalExpr::Unary { expr, .. } | EvalExpr::Cast { expr, .. } =>
            parameter_default_is_deferred(expr),
        EvalExpr::Binary { left, right, .. } =>
            parameter_default_is_deferred(left) || parameter_default_is_deferred(right),
        EvalExpr::NullCoalesce { value, default } =>
            parameter_default_is_deferred(value) || parameter_default_is_deferred(default),
        EvalExpr::Ternary { condition, then_branch, else_branch } =>
            parameter_default_is_deferred(condition)
                || then_branch.as_deref().is_some_and(parameter_default_is_deferred)
                || parameter_default_is_deferred(else_branch),
        EvalExpr::Match { subject, arms, default } => parameter_default_is_deferred(subject)
            || arms.iter().any(|arm| arm.patterns.iter().any(parameter_default_is_deferred)
                || parameter_default_is_deferred(&arm.value))
            || default.as_deref().is_some_and(parameter_default_is_deferred),
        EvalExpr::NewObject { args, .. } =>
            args.iter().any(|arg| parameter_default_is_deferred(arg.value())),
        EvalExpr::Call { name, .. } if name == "\0elephc_native_default_array_spread" => true,
        _ => false,
    }
}

/// Attaches a context-independent native factory or a direct Error through private metadata slots.
pub(super) fn attach_parameter_default_factory(
    object: RuntimeCellHandle,
    parameter: &EvalReflectionParameterMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(default) = parameter.default_value.as_ref()
        .filter(|default| parameter_default_is_deferred(default)) else { return Ok(()); };
    let (property, factory) = if let Some(descriptor) = parameter.native_default_factory {
        ("__default_factory", values.native_callable_from_raw(descriptor)?)
    } else if let EvalExpr::DeferredDefaultError(message) = default {
        ("__default_error", values.string(message)?)
    } else {
        return Err(EvalStatus::UnsupportedConstruct);
    };
    let installed = eval_reflection_with_declaring_class_scope("ReflectionParameter", context,
        |_| values.property_set(object, property, factory));
    let retired = eval_release_value(context, values, factory);
    installed.and(retired)
}

/// Reflection detects deferred errors inside normalized spreads, not ordinary function names.
#[cfg(test)]
#[test]
fn native_default_spread_retains_reflection_error_detection() {
    let args = vec![crate::eval_ir::EvalCallArg::positional(
        EvalExpr::DeferredDefaultError("parent".into()),
    )];
    assert!(parameter_default_is_deferred(&EvalExpr::Array(vec![
        EvalArrayElement::Value(EvalExpr::Call {
            name: "\0elephc_native_default_array_spread".into(), args: args.clone(),
        }),
    ])));
    assert!(!parameter_default_is_deferred(&EvalExpr::Call {
        name: "__elephc_default_array_spread".into(), args,
    }));
}
