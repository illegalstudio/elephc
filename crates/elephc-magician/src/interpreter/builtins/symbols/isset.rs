//! Purpose:
//! Eval registry entry and implementation for `isset`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Direct calls stay source-sensitive so missing variables, object properties,
//!   array offsets, and ArrayAccess values keep PHP `isset()` semantics.

eval_builtin! {
    name: "isset",
    area: Symbols,
    params: [var],
    variadic: vars,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates PHP's `isset(...)` language construct over eval-visible values.
pub(in crate::interpreter) fn eval_isset_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_isset(args, context, scope, values)
}

/// Evaluates callable `isset(...)` over already materialized values.
pub(in crate::interpreter) fn eval_isset_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_isset_result(evaluated_args, values)
}

/// Evaluates PHP's `isset(...)` language construct over eval-visible values.
pub(in crate::interpreter) fn eval_builtin_isset(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for arg in args {
        if !eval_isset_arg(arg, context, scope, values)? {
            return values.bool_value(false);
        }
    }
    values.bool_value(true)
}

/// Evaluates callable `isset(...)` over already materialized values.
pub(in crate::interpreter) fn eval_isset_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for value in evaluated_args {
        if values.is_null(*value)? {
            return values.bool_value(false);
        }
    }
    values.bool_value(true)
}

/// Evaluates one `isset` operand without allocating a null cell for missing variables.
pub(in crate::interpreter) fn eval_isset_arg(
    arg: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let EvalExpr::LoadVar(name) = arg {
        let Some(value) = visible_scope_cell(context, scope, name) else {
            return Ok(false);
        };
        return Ok(!values.is_null(value)?);
    }
    if let EvalExpr::PropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        return eval_property_isset_result(object, property, context, values);
    }
    if let EvalExpr::DynamicPropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        let property = eval_dynamic_member_name(property, context, scope, values)?;
        return eval_property_isset_result(object, &property, context, values);
    }
    if let EvalExpr::NullsafePropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        if values.is_null(object)? {
            return Ok(false);
        }
        return eval_property_isset_result(object, property, context, values);
    }
    if let EvalExpr::NullsafeDynamicPropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        if values.is_null(object)? {
            return Ok(false);
        }
        let property = eval_dynamic_member_name(property, context, scope, values)?;
        return eval_property_isset_result(object, &property, context, values);
    }
    if let EvalExpr::StaticPropertyGet {
        class_name,
        property,
    } = arg
    {
        return eval_static_property_isset_result(class_name, property, context, values);
    }
    if let EvalExpr::DynamicStaticPropertyGet {
        class_name,
        property,
    } = arg
    {
        let class_name = eval_expr(class_name, context, scope, values)?;
        let class_name = eval_dynamic_class_name(class_name, context, values)?;
        return eval_static_property_isset_result(&class_name, property, context, values);
    }
    if let EvalExpr::DynamicStaticPropertyNameGet {
        class_name,
        property,
    } = arg
    {
        let class_name = eval_expr(class_name, context, scope, values)?;
        let class_name = eval_dynamic_class_name(class_name, context, values)?;
        let property = eval_dynamic_member_name(property, context, scope, values)?;
        return eval_static_property_isset_result(&class_name, &property, context, values);
    }
    if let EvalExpr::ArrayGet { array, index } = arg {
        let array = eval_expr(array, context, scope, values)?;
        let index = eval_expr(index, context, scope, values)?;
        if values.type_tag(array)? == EVAL_TAG_OBJECT {
            return eval_array_access_isset_result(array, index, context, values);
        }
        let value = values.array_get(array, index)?;
        return Ok(!values.is_null(value)?);
    }
    let value = eval_expr(arg, context, scope, values)?;
    Ok(!values.is_null(value)?)
}

/// Evaluates `isset($object[$key])` through `ArrayAccess::offsetExists()`.
pub(in crate::interpreter) fn eval_array_access_isset_result(
    object: RuntimeCellHandle,
    index: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if !eval_array_access_object_matches(object, context, values)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = eval_method_call_result(object, "offsetExists", vec![index], context, values)?;
    let exists = values.truthy(result)?;
    values.release(result)?;
    Ok(exists)
}

/// Returns whether an object operand implements the eval-visible `ArrayAccess` contract.
fn eval_array_access_object_matches(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let target_class = "ArrayAccess";
    super::is_a::dynamic_object_is_a(object, target_class, false, context, values)?
        .map_or_else(|| values.object_is_a(object, target_class, false), Ok)
}
