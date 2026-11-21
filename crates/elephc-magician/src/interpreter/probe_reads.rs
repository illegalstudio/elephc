//! Purpose:
//! Reads intentional probe roots without weakening ordinary operand initialization checks.
//!
//! Called from:
//! - Shared probe operand preparation and null-coalescing array reads.
//!
//! Key details:
//! - Only the receiver chain is quiet; keys and other expressions retain ordinary evaluation.
//! - Existing operand owner frames protect every value across later source effects.

use super::*;

/// Produces an owned probe receiver while preserving missing roots and initialized null.
pub(super) fn eval_owned(expr: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !uses_quiet_roots(context) {
        return eval_owned_expr(expr, context, scope, values);
    }
    match expr {
        EvalExpr::LoadVar(name) => match visible_scope_cell(context, scope, name) {
            Some(value) => copy_scope_value(value, context, values),
            None => values.null(),
        },
        EvalExpr::ArrayGet { array, index } => with_eval_probe_operands(&[array, index], context, scope, values,
            |args, context, _, values| {
                let value = eval_array_get_quiet_result(args[0].borrowed(), args[1].borrowed(), context, values)?;
                if value.is_borrowed() { values.retain(value) } else { Ok(value) }
            }),
        EvalExpr::PropertyGet { .. } | EvalExpr::DynamicPropertyGet { .. }
        | EvalExpr::NullsafePropertyGet { .. } | EvalExpr::NullsafeDynamicPropertyGet { .. } =>
            super::expressions::eval_null_coalesce_value(expr, true, context, scope, values),
        _ => eval_owned_expr(expr, context, scope, values),
    }
}

/// Selects quiet root traversal without enabling another semantic guarantee.
pub(super) fn uses_quiet_roots(context: &ElephcEvalContext) -> bool {
    context.semantic_profile().contains(elephc_semantics::StrictFeature::Initialization)
        || context.semantic_profile().contains(elephc_semantics::StrictFeature::Indexing)
        || context.semantic_profile().contains(elephc_semantics::StrictFeature::Nullability)
}
