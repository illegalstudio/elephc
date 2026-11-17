//! Purpose:
//! Returns the values of eval property compound assignments and increments.
//!
//! Called from:
//! - Expression dispatch and the property mutation statement wrappers.
//!
//! Key details:
//! - Receiver, name and RHS staging follows the shared write-fetch planner.
//! - Native XML updates retain their specialized prefix and postfix behavior.
//! - Computed values remain owned through property stores and operand cleanup.

use super::*;

/// Executes an expression compound write after evaluating its receiver, name and RHS once.
pub(in crate::interpreter) fn eval_property_compound_expression(
    object: &EvalExpr, property: &EvalExpr, dynamic: bool, op: EvalBinOp, value: &EvalExpr,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let property = expression_property_name(property, dynamic);
    with_eval_named_property_write_result(object, property, &[value], context, scope, values,
        |args, property, context, _, values|
            eval_property_compound_assign_value(args[0], property, op, args[1], context, values))
}

/// Returns an increment's selected old or new value without replaying a property getter.
pub(in crate::interpreter) fn eval_property_inc_dec_expression(
    object: &EvalExpr, property: &EvalExpr, dynamic: bool, increment: bool, prefix: bool,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let property = expression_property_name(property, dynamic);
    with_eval_named_property_write_result(object, property, &[], context, scope, values,
        |args, property, context, _, values|
            eval_property_inc_dec_value(args[0], property, increment, prefix, context, values))
}

/// Reuses literal names directly while preserving delayed conversion of computed names.
fn expression_property_name(property: &EvalExpr, dynamic: bool) -> DimensionProperty<'_> {
    match (property, dynamic) {
        (EvalExpr::Const(EvalConst::String(name)), false) => DimensionProperty::Literal(name),
        (property, _) => DimensionProperty::Dynamic(property),
    }
}

/// Produces one owned compound result while balancing the old property value on every exit.
pub(super) fn eval_property_compound_assign_value(
    object: RuntimeCellHandle, property: &str, op: EvalBinOp, right: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    validate_property_write_receiver(object, property, "assign", context, values)?;
    if let Some(result) = super::super::runtime_ops::simplexml_properties::update(
        values, object, property, op, right, false,
    )? { return Ok(result); }
    let current = eval_write_property_get_result(object, property, context, values)?;
    let current = if current.is_borrowed() { values.retain(current)? } else { current };
    let result = (|| {
        let value = eval_binary_result(op, current, right, context, values)?;
        let value = normalize_update_storage(object, property, value, context, values)?;
        if let Err(status) = eval_property_set_result(object, property, value, context, values) {
            let _ = release_expr_result(value, context, values);
            return Err(status);
        }
        Ok(value)
    })();
    finish_synthesized_method_arguments(result, vec![current], context, values)
}

/// Stores the updated property while handing the requested old or new owner to the caller.
pub(super) fn eval_property_inc_dec_value(
    object: RuntimeCellHandle, property: &str, increment: bool, prefix: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    validate_property_write_receiver(object, property, "increment/decrement", context, values)?;
    if super::super::runtime_ops::simplexml_properties::is_native(values, object, property)? {
        let one = values.int(1)?;
        let operation = if increment { EvalBinOp::Add } else { EvalBinOp::Sub };
        let result = super::super::runtime_ops::simplexml_properties::update(
            values, object, property, operation, one, !prefix,
        ).and_then(|result| result.ok_or(EvalStatus::RuntimeFatal));
        return finish_synthesized_method_arguments(result, vec![one], context, values);
    }
    let current = eval_write_property_get_result(object, property, context, values)?;
    let current = if current.is_borrowed() { values.retain(current)? } else { current };
    let updated = match eval_inc_dec_value(current, increment, values) {
        Ok(updated) => updated,
        Err(status) => return finish_synthesized_method_arguments(Err(status), vec![current], context, values),
    };
    let updated = match normalize_update_storage(object, property, updated, context, values) {
        Ok(updated) => updated,
        Err(status) => return finish_synthesized_method_arguments(Err(status), vec![current], context, values),
    };
    if let Err(status) = eval_property_set_result(object, property, updated, context, values) {
        return finish_synthesized_method_arguments(Err(status), vec![current, updated], context, values);
    }
    let (result, retired) = if prefix { (updated, current) } else { (current, updated) };
    finish_synthesized_method_arguments(Ok(result), vec![retired], context, values)
}

/// Keeps plain storage and its expression result identical while hooks retain computed results.
fn normalize_update_storage(
    object: RuntimeCellHandle, property: &str, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let normalized = (|| {
        let Some((owner, ty)) = property_dimension_guards::update_storage_contract(object, property, context, values)?
            else { return Ok(value); };
        if ty.is_intersection() || !ty.variants().iter().any(|variant| matches!(variant,
            EvalParameterTypeVariant::Int | EvalParameterTypeVariant::Float
                | EvalParameterTypeVariant::Bool | EvalParameterTypeVariant::String))
            || eval_method_parameter_type_accepts_exact(&ty, value, context, values)?
        { return Ok(value); }
        let tag = values.type_tag(value)?;
        for variant in ty.variants() {
            if matches!(variant, EvalParameterTypeVariant::Int) && tag == EVAL_TAG_FLOAT {
                let bytes = values.string_bytes(value)?;
                let number = std::str::from_utf8(&bytes).ok().and_then(|text| text.parse::<f64>().ok())
                    .ok_or(EvalStatus::RuntimeFatal)?;
                if !number.is_finite() || number < i64::MIN as f64 || number >= -(i64::MIN as f64) { continue; }
                let integer = eval_int_value(value, values)?;
                return values.int(integer);
            }
            if let Some(coerced) = eval_method_parameter_scalar_coercion(variant, value, context, values)? {
                return Ok(coerced);
            }
        }
        let actual = match tag {
            EVAL_TAG_OBJECT => {
                let identity = values.object_identity(value)?;
                match context.dynamic_object_class_name(identity) {
                    Some(class) => class.to_string(), None => eval_runtime_object_class_name(value, values)?,
                }
            }
            EVAL_TAG_INT => "int".into(), EVAL_TAG_FLOAT => "float".into(), EVAL_TAG_BOOL => "bool".into(),
            EVAL_TAG_STRING => "string".into(), EVAL_TAG_NULL => "null".into(), _ => eval_gettype_name(tag).into(),
        };
        let expected = property_dimension_guards::type_label(&ty, &owner, context)?;
        eval_throw_type_error(&format!("Cannot assign {actual} to property {owner}::${property} of type {expected}"), context, values)
    })();
    match normalized {
        Ok(normalized) if normalized == value => Ok(value),
        normalized => finish_synthesized_method_arguments(normalized, vec![value], context, values),
    }
}
