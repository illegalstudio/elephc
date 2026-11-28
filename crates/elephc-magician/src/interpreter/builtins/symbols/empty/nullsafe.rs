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
use crate::interpreter::nullsafe_chains::{has_nullsafe, receiver, evaluate_receiver};

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
    let Some(object) = evaluate_receiver(object, true, context, scope, values)? else { return Ok(Some(true)); };
    let result = (|| {
        if (nullsafe || context.semantic_profile().contains(elephc_semantics::StrictFeature::Nullability))
            && values.is_null(object)? { return Ok(true); }
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
