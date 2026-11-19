//! Purpose:
//! Evaluates complete nullsafe postfix chains without confusing short circuits with null values.
//!
//! Called from:
//! - Ordinary expressions, coalescing and the existing empty nullsafe adapter.
//!
//! Key details:
//! - None propagates an upstream short circuit without evaluating later selectors or arguments.
//! - Each intermediate receiver keeps its owner until its successor has an owner.

use super::*;

/// Finds only postfix receivers, keeping keys and arguments outside the chain's structure.
pub(super) fn receiver(expr: &EvalExpr) -> Option<(&EvalExpr, bool)> {
    match expr {
        EvalExpr::NullsafePropertyGet { object, .. } | EvalExpr::NullsafeDynamicPropertyGet { object, .. }
        | EvalExpr::NullsafeMethodCall { object, .. } | EvalExpr::NullsafeDynamicMethodCall { object, .. } => Some((object, true)),
        EvalExpr::PropertyGet { object, .. } | EvalExpr::DynamicPropertyGet { object, .. }
        | EvalExpr::MethodCall { object, .. } | EvalExpr::DynamicMethodCall { object, .. } => Some((object, false)),
        EvalExpr::ArrayGet { array, .. } => Some((array, false)),
        EvalExpr::DynamicCall { callee, .. } => Some((callee, false)),
        _ => None,
    }
}

/// Detects nullsafe hops without evaluating any part of the operand.
pub(super) fn has_nullsafe(expr: &EvalExpr) -> bool {
    receiver(expr).is_some_and(|(object, nullsafe)| nullsafe || has_nullsafe(object))
}

/// Produces one owned receiver or propagates a prior nullsafe short circuit unchanged.
pub(super) fn evaluate_receiver(
    expr: &EvalExpr, quiet: bool, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((object, nullsafe)) = receiver(expr) else {
        return eval_owned_expr(expr, context, scope, values).map(Some);
    };
    let Some(object) = evaluate_receiver(object, quiet, context, scope, values)? else { return Ok(None); };
    let result = (|| {
        if nullsafe && values.is_null(object)? { return Ok(None); }
        let result = match expr {
            EvalExpr::PropertyGet { property, .. } | EvalExpr::NullsafePropertyGet { property, .. } =>
                property_value(object, property, quiet, context, values),
            EvalExpr::DynamicPropertyGet { property, .. } | EvalExpr::NullsafeDynamicPropertyGet { property, .. } => {
                let property = eval_dynamic_member_name(property, context, scope, values)?;
                property_value(object, &property, quiet, context, values)
            }
            EvalExpr::ArrayGet { index, .. } => {
                let index = eval_owned_expr(index, context, scope, values)?;
                let result = if quiet { eval_array_get_quiet_result(object, index, context, values) }
                    else { eval_array_get_result(object, index, context, values) };
                finish_eval_argument_values(result, [index], context, values)
            }
            EvalExpr::MethodCall { method, args, .. } | EvalExpr::NullsafeMethodCall { method, args, .. } =>
                with_eval_object_method_arguments(args, object, method, context, scope, values, |args, context, _, values| {
                    eval_method_call_result_with_evaluated_args(object, method, args, context, values)
                }),
            EvalExpr::DynamicMethodCall { method, args, .. } | EvalExpr::NullsafeDynamicMethodCall { method, args, .. } => {
                let method = eval_dynamic_member_name(method, context, scope, values)?;
                with_eval_object_method_arguments(args, object, &method, context, scope, values, |args, context, _, values| {
                    eval_method_call_result_with_evaluated_args(object, &method, args, context, values)
                })
            }
            EvalExpr::DynamicCall { args, .. } => eval_dynamic_call_with_callback(object, args, context, scope, values),
            _ => Err(EvalStatus::RuntimeFatal),
        };
        result.and_then(|value| if value.is_borrowed() { values.retain(value) } else { Ok(value) }).map(Some)
    })();
    let cleanup = release_expr_result(object, context, values);
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            if let Some(value) = value { let _ = release_expr_result(value, context, values); }
            Err(status)
        }
    }
}

/// Reads one property under its ordinary or explicit quiet receiver contract.
fn property_value(object: RuntimeCellHandle, property: &str, quiet: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if quiet && context.semantic_profile().contains(elephc_semantics::StrictFeature::Nullability)
        && !initialization::coalesce_uses_getter(object, property, context, values)
        && !eval_property_isset_result(object, property, context, values)? {
        return values.null();
    }
    eval_property_get_result(object, property, context, values)
}
