//! Purpose:
//! Preserve non-null lazy XML views along eval property write receiver chains.
//!
//! Called from:
//! - Property assignment, updates, and dimension writes.
//!
//! Key details:
//! - Ordinary receiver expressions retain their read semantics.
//! - Every intermediate owner survives subsequent operands and is released on failure.

use super::*;

/// Evaluates a write receiver without turning an absent intermediate XML view into null.
pub(super) fn eval_property_write_receiver(
    expr: &EvalExpr, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut operands = Vec::new();
    let result = (|| {
        let (object, property) = match expr {
            EvalExpr::PropertyGet { object, property } => {
                let object = eval_property_write_receiver(object, context, scope, values)?;
                operands.push(object);
                (object, property.clone())
            }
            EvalExpr::DynamicPropertyGet { object, property } => {
                let object = eval_property_write_receiver(object, context, scope, values)?;
                operands.push(object);
                (object, eval_dynamic_member_name(property, context, scope, values)?)
            }
            EvalExpr::ArrayGet { array, index } => {
                let array = eval_property_write_receiver(array, context, scope, values)?;
                operands.push(array);
                let index = eval_owned_expr(index, context, scope, values)?;
                operands.push(index);
                let value = eval_array_get_result(array, index, context, values)?;
                return if value.is_borrowed() { values.retain(value) } else { Ok(value) };
            }
            _ => return eval_owned_expr(expr, context, scope, values),
        };
        let value = eval_write_property_get_result(object, &property, context, values)?;
        if value.is_borrowed() { values.retain(value) } else { Ok(value) }
    })();
    let cleanup = release_receivers(operands, context, values);
    match (result, cleanup) {
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = eval_release_value(context, values, value);
            Err(status)
        }
        (Ok(value), Ok(())) => Ok(value),
    }
}

/// Reads an update target as a lazy native selection, with the ordinary getter as fallback.
pub(super) fn eval_write_property_get_result(
    object: RuntimeCellHandle, property: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match super::super::runtime_ops::simplexml_properties::invoke(
        values, object, property, elephc_builtin_contract::ObjectProperty::WriteSelect, None,
    )? {
        Some(value) => Ok(value),
        None => eval_property_get_result(object, property, context, values),
    }
}

/// Stages a write-context receiver followed by ordinary operands, preserving existing ordering.
pub(super) fn with_eval_property_write_operands<V: RuntimeValueOps>(
    args: &[&EvalExpr], context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let mut operands = Vec::new();
    let result = (|| {
        for (index, arg) in args.iter().enumerate() {
            operands.push(if index == 0 { eval_property_write_receiver(arg, context, scope, values)? }
                else { eval_owned_expr(arg, context, scope, values)? });
        }
        consume(&operands, context, scope, values)
    })();
    let cleanup = release_receivers(operands, context, values);
    result.and(cleanup)
}

/// Releases all captured values even if an earlier receiver destructor throws.
fn release_receivers(
    operands: Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut result = Ok(());
    for operand in operands.into_iter().rev() {
        let released = eval_release_value(context, values, operand);
        if result.is_ok() { result = released; }
    }
    result
}
