//! Purpose:
//! Applies shared Core attribute targets and NoDiscard diagnostics to interpreted declarations.
//!
//! Called from:
//! - Declaration validators and expression-statement execution.
//!
//! Key details:
//! - DelayedTargetValidation affects targets only; repeated non-repeatable markers still fail.
//! - Warning handlers run while the discarded return value remains owned by its caller.

use super::*;

#[cfg(test)]
mod repeatability_tests {
    use super::*;

    /// The global-constant target bit never permits repeated predefined attributes.
    #[test]
    fn php85_constant_target_does_not_make_core_attributes_repeatable() {
        for name in ["DelayedTargetValidation", "Deprecated"] {
            let attributes = [EvalAttribute::new(name, None), EvalAttribute::new(name, None)];
            assert_eq!(validate_eval_core_attribute_targets(&attributes, 2), Err(EvalStatus::RuntimeFatal));
            assert!(validate_eval_core_attribute_targets(&attributes[..1], 2).is_ok());
        }
    }
}

/// Validates predefined attribute targets using the same masks as the native checker.
pub(super) fn validate_eval_core_attribute_targets(attributes: &[EvalAttribute], target: i64) -> Result<(), EvalStatus> {
    let delayed = eval_attributes_have_global_builtin_attribute(attributes, "DelayedTargetValidation");
    for (index, attribute) in attributes.iter().enumerate() {
        let Some(flags) = elephc_builtin_contract::core_attribute_flags(attribute.name()) else { continue };
        if flags & elephc_builtin_contract::CORE_ATTRIBUTE_IS_REPEATABLE == 0 && attributes[..index].iter().any(|other| other.name().eq_ignore_ascii_case(attribute.name())) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if !delayed && flags & target == 0 { return Err(EvalStatus::RuntimeFatal); }
    }
    Ok(())
}

/// Requires a marked property to override a visible parent or interface property.
pub(super) fn validate_eval_property_override(
    class: &EvalClass,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !eval_attributes_have_global_builtin_attribute(property.attributes(), "Override") {
        return Ok(());
    }
    if class.parent().and_then(|parent| context.class_property(parent, property.name()))
        .is_some_and(|(_, inherited)| inherited.visibility() != EvalVisibility::Private
            && inherited.is_static() == property.is_static())
    {
        return Ok(());
    }
    if let Some(parent) = pending_class_native_parent_name(class, context) {
        if values.class_exists(&parent)? {
            if let Some(flags) = values.reflection_property_flags(&parent, property.name())? {
                if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE == 0
                    && (flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0) == property.is_static()
                {
                    return Ok(());
                }
            }
        }
    }
    if !property.is_static() {
        if pending_class_interface_names(class, context).iter().any(|interface| {
            context.interface_property_requirements(interface).iter()
                .any(|requirement| requirement.name() == property.name())
        }) || pending_class_aot_interface_property_requirements(class, context, values)?.iter()
            .any(|(_, requirement)| requirement.name() == property.name())
        {
            return Ok(());
        }
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Evaluates discarded instance calls once while retaining their receiver through diagnostics.
pub(super) fn eval_discarded_core_expr(
    expression: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match expression {
        EvalExpr::MethodCall { object, method, args }
        | EvalExpr::NullsafeMethodCall { object, method, args } => {
            with_eval_operands(&[object], context, scope, values, |receiver, context, scope, values| {
                if matches!(expression, EvalExpr::NullsafeMethodCall { .. }) && values.is_null(receiver[0])? {
                    return values.null();
                }
                with_eval_method_arguments(args, context, scope, values, |arguments, context, _, values| {
                    let result = eval_method_call_result_with_evaluated_args(receiver[0], method, arguments, context, values)?;
                    warn_eval_discarded_instance_result(receiver[0], method, result, context, values)
                })
            })
        }
        EvalExpr::DynamicMethodCall { object, method, args }
        | EvalExpr::NullsafeDynamicMethodCall { object, method, args } => {
            with_eval_operands(&[object], context, scope, values, |receiver, context, scope, values| {
                if matches!(expression, EvalExpr::NullsafeDynamicMethodCall { .. }) && values.is_null(receiver[0])? {
                    return values.null();
                }
                let method = eval_dynamic_member_name(method, context, scope, values)?;
                with_eval_method_arguments(args, context, scope, values, |arguments, context, _, values| {
                    let result = eval_method_call_result_with_evaluated_args(receiver[0], &method, arguments, context, values)?;
                    warn_eval_discarded_instance_result(receiver[0], &method, result, context, values)
                })
            })
        }
        _ => eval_expr(expression, context, scope, values),
    }
}

/// Warns for an eval method's discarded result and releases it if a warning handler throws.
fn warn_eval_discarded_instance_result(
    object: RuntimeCellHandle,
    method: &str,
    result: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let warned = (|| {
        if values.type_tag(object)? != EVAL_TAG_OBJECT { return Ok(()); }
        let identity = values.object_identity(object)?;
        let Some(class) = context.dynamic_object_class(identity) else { return Ok(()); };
        let Some((_, method)) = context.class_method(class.name(), method) else { return Ok(()); };
        if !eval_attributes_have_global_builtin_attribute(method.attributes(), "NoDiscard") { return Ok(()); }
        let name = format!("{}::{}", class.name(), method.name());
        eval_dispatch_php_error(
            &format!("The return value of {name}() should either be used or explicitly ignored"),
            E_WARNING, context, values,
        ).map(|_| ())
    })();
    if let Err(status) = warned {
        release_expr_result(result, context, values)?;
        return Err(status);
    }
    Ok(result)
}

/// Emits the PHP warning for an ignored result from an interpreted marked declaration.
pub(super) fn warn_eval_discarded_call(
    expression: &EvalExpr,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let selected = match expression {
        EvalExpr::Call { name, .. } => context.function(name).map(|function| (name.clone(), function.attributes().to_vec())),
        EvalExpr::NamespacedCall { name, fallback_name, .. } => context.function(name).or_else(|| context.function(fallback_name))
            .map(|function| (function.name().to_string(), function.attributes().to_vec())),
        EvalExpr::StaticMethodCall { class_name, method, .. } => context.class_method(class_name, method)
            .map(|(_, method)| (format!("{class_name}::{}", method.name()), method.attributes().to_vec())),
        _ => None,
    };
    if let Some((name, attributes)) = selected {
        if eval_attributes_have_global_builtin_attribute(&attributes, "NoDiscard") {
            eval_dispatch_php_error(
                &format!("The return value of {name}() should either be used or explicitly ignored"),
                E_WARNING, context, values,
            )?;
        }
    }
    Ok(())
}
