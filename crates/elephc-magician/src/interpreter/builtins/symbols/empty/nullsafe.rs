//! Purpose:
//! Preserves a whole nullsafe postfix chain while evaluating an empty operand.
//!
//! Called from:
//! - Eval's empty language construct before individual property or offset handling.
//!
//! Key details:
//! - None means an upstream short circuit, distinct from a real null cell.
//! - Each intermediate receiver owns its storage until its successor has an owner.

use super::*;

/// Evaluates a terminal probe only after every upstream nullsafe guard has continued.
pub(super) fn evaluate(
    arg: &EvalExpr, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<bool>, EvalStatus> {
    if !has_nullsafe(arg) { return Ok(None); }
    if !matches!(arg, EvalExpr::PropertyGet { .. } | EvalExpr::DynamicPropertyGet { .. }
        | EvalExpr::NullsafePropertyGet { .. } | EvalExpr::NullsafeDynamicPropertyGet { .. }
        | EvalExpr::ArrayGet { .. }) { return Ok(None); }
    let (object, nullsafe) = receiver(arg).ok_or(EvalStatus::RuntimeFatal)?;
    let Some(object) = evaluate_receiver(object, context, scope, values)? else { return Ok(Some(true)); };
    let result = (|| {
        if nullsafe && values.is_null(object)? { return Ok(true); }
        match arg {
            EvalExpr::PropertyGet { property, .. } | EvalExpr::NullsafePropertyGet { property, .. } =>
                property_empty(object, property, context, values),
            EvalExpr::DynamicPropertyGet { property, .. } | EvalExpr::NullsafeDynamicPropertyGet { property, .. } => {
                let property = eval_dynamic_member_name(property, context, scope, values)?;
                property_empty(object, &property, context, values)
            }
            EvalExpr::ArrayGet { index, .. } => {
                let index = eval_owned_expr(index, context, scope, values)?;
                let result = if values.type_tag(object)? == EVAL_TAG_OBJECT {
                    super::eval_array_access_empty_result(object, index, context, values)
                } else {
                    values.array_get(object, index)
                        .and_then(|value| empty_value(value, context, values))
                };
                let cleanup = release_expr_result(index, context, values);
                result.and_then(|result| cleanup.map(|()| result))
            }
            _ => Err(EvalStatus::RuntimeFatal),
        }
    })();
    let cleanup = release_expr_result(object, context, values);
    result.and_then(|result| cleanup.map(|()| Some(result)))
}

/// Finds only postfix receivers, keeping keys and arguments outside the chain's structure.
fn receiver(expr: &EvalExpr) -> Option<(&EvalExpr, bool)> {
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
fn has_nullsafe(expr: &EvalExpr) -> bool {
    receiver(expr).is_some_and(|(object, nullsafe)| nullsafe || has_nullsafe(object))
}

/// Produces one owned receiver or propagates a prior nullsafe short circuit unchanged.
fn evaluate_receiver(
    expr: &EvalExpr, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((object, nullsafe)) = receiver(expr) else {
        return eval_owned_expr(expr, context, scope, values).map(Some);
    };
    let Some(object) = evaluate_receiver(object, context, scope, values)? else { return Ok(None); };
    let result = (|| {
        if nullsafe && values.is_null(object)? { return Ok(None); }
        let result = match expr {
            EvalExpr::PropertyGet { property, .. } | EvalExpr::NullsafePropertyGet { property, .. } =>
                eval_property_get_result(object, property, context, values),
            EvalExpr::DynamicPropertyGet { property, .. } | EvalExpr::NullsafeDynamicPropertyGet { property, .. } => {
                let property = eval_dynamic_member_name(property, context, scope, values)?;
                eval_property_get_result(object, &property, context, values)
            }
            EvalExpr::ArrayGet { index, .. } => {
                let index = eval_owned_expr(index, context, scope, values)?;
                let result = eval_array_get_result(object, index, context, values);
                finish_eval_argument_values(result, [index], context, values)
            }
            EvalExpr::MethodCall { method, args, .. } | EvalExpr::NullsafeMethodCall { method, args, .. } =>
                with_eval_method_arguments(args, context, scope, values, |args, context, _, values| {
                    eval_method_call_result_with_evaluated_args(object, method, args, context, values)
                }),
            EvalExpr::DynamicMethodCall { method, args, .. } | EvalExpr::NullsafeDynamicMethodCall { method, args, .. } => {
                let method = eval_dynamic_member_name(method, context, scope, values)?;
                with_eval_method_arguments(args, context, scope, values, |args, context, _, values| {
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

/// Applies native XML content checks before ordinary magic-property probing.
fn property_empty(
    object: RuntimeCellHandle, property: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let Some(empty) = runtime_ops::simplexml_properties::empty(values, object, Some(property), None)? {
        return Ok(empty);
    }
    if !eval_property_isset_result(object, property, context, values)? { return Ok(true); }
    let value = eval_property_get_result(object, property, context, values)?;
    empty_value(value, context, values)
}

/// Reads a probe's truthiness and releases only an owned returned cell.
fn empty_value(value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps)
    -> Result<bool, EvalStatus>
{
    let truthy = values.truthy(value);
    let cleanup = release_expr_result(value, context, values);
    truthy.and_then(|truthy| cleanup.map(|()| !truthy))
}
