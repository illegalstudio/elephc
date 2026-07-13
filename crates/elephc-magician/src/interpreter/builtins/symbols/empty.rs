//! Purpose:
//! Eval registry entry and implementation for `empty`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Direct calls stay source-sensitive so missing variables are not evaluated normally.

eval_builtin! {
    name: "empty",
    area: Symbols,
    params: [value],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates PHP's `empty(...)` language construct over eval-visible values.
pub(in crate::interpreter) fn eval_empty_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_empty(args, context, scope, values)
}

/// Evaluates callable `empty(...)` over one already materialized value.
pub(in crate::interpreter) fn eval_empty_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_empty_result(evaluated_args, values)
}

/// Evaluates PHP's `empty(...)` language construct over eval-visible values.
pub(in crate::interpreter) fn eval_builtin_empty(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let empty = eval_empty_arg(arg, context, scope, values)?;
    values.bool_value(empty)
}

/// Evaluates callable `empty(...)` over one already materialized value.
pub(in crate::interpreter) fn eval_empty_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let empty = !values.truthy(*value)?;
    values.bool_value(empty)
}

/// Evaluates one `empty` operand without warning or failing on missing variables.
pub(in crate::interpreter) fn eval_empty_arg(
    arg: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let EvalExpr::LoadVar(name) = arg {
        let Some(value) = visible_scope_cell(context, scope, name) else {
            return Ok(true);
        };
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::PropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        if !eval_property_isset_result(object, property, context, values)? {
            return Ok(true);
        }
        let value = eval_property_get_result(object, property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::DynamicPropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        let property = eval_dynamic_member_name(property, context, scope, values)?;
        if !eval_property_isset_result(object, &property, context, values)? {
            return Ok(true);
        }
        let value = eval_property_get_result(object, &property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::NullsafePropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        if values.is_null(object)? {
            return Ok(true);
        }
        if !eval_property_isset_result(object, property, context, values)? {
            return Ok(true);
        }
        let value = eval_property_get_result(object, property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::NullsafeDynamicPropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        if values.is_null(object)? {
            return Ok(true);
        }
        let property = eval_dynamic_member_name(property, context, scope, values)?;
        if !eval_property_isset_result(object, &property, context, values)? {
            return Ok(true);
        }
        let value = eval_property_get_result(object, &property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::StaticPropertyGet {
        class_name,
        property,
    } = arg
    {
        if !eval_static_property_isset_result(class_name, property, context, values)? {
            return Ok(true);
        }
        let value = eval_static_property_get_result(class_name, property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::DynamicStaticPropertyGet {
        class_name,
        property,
    } = arg
    {
        let class_name = eval_expr(class_name, context, scope, values)?;
        let class_name = eval_dynamic_class_name(class_name, context, values)?;
        if !eval_static_property_isset_result(&class_name, property, context, values)? {
            return Ok(true);
        }
        let value = eval_static_property_get_result(&class_name, property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::DynamicStaticPropertyNameGet {
        class_name,
        property,
    } = arg
    {
        let class_name = eval_expr(class_name, context, scope, values)?;
        let class_name = eval_dynamic_class_name(class_name, context, values)?;
        let property = eval_dynamic_member_name(property, context, scope, values)?;
        if !eval_static_property_isset_result(&class_name, &property, context, values)? {
            return Ok(true);
        }
        let value = eval_static_property_get_result(&class_name, &property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::ArrayGet { array, index } = arg {
        let array = eval_expr(array, context, scope, values)?;
        let index = eval_expr(index, context, scope, values)?;
        if values.type_tag(array)? == EVAL_TAG_OBJECT {
            return eval_array_access_empty_result(array, index, context, values);
        }
        let value = values.array_get(array, index)?;
        return Ok(!values.truthy(value)?);
    }
    let value = eval_expr(arg, context, scope, values)?;
    Ok(!values.truthy(value)?)
}

/// Evaluates `empty($object[$key])` through `ArrayAccess::offsetExists()` and `offsetGet()`.
fn eval_array_access_empty_result(
    object: RuntimeCellHandle,
    index: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if !super::isset::eval_array_access_isset_result(object, index, context, values)? {
        return Ok(true);
    }
    let value = eval_array_get_result(object, index, context, values)?;
    Ok(!values.truthy(value)?)
}
