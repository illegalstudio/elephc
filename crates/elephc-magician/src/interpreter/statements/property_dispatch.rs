//! Purpose:
//! Executes EvalIR instance, static, and dynamic property mutation statements.
//!
//! Called from:
//! - `crate::interpreter::statements::dispatch::execute_stmt()`.
//!
//! Key details:
//! - Object/class/member expressions retain their original source evaluation order.

use super::*;

/// Executes one property-oriented statement selected by the exhaustive statement dispatcher.
pub(super) fn execute_property_stmt(
    stmt: &EvalStmt,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    match stmt {
        EvalStmt::PropertyReferenceBind {
            object,
            property,
            source,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            eval_property_reference_bind_result(object, property, source, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicPropertyReferenceBind {
            object,
            property,
            source,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_property_reference_bind_result(
                object,
                &property,
                source,
                context,
                scope,
                values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicPropertySet {
            object,
            property,
            value,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            let value = eval_expr(value, context, scope, values)?;
            eval_property_set_result(object, &property, value, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicPropertyArrayAppend {
            object,
            property,
            value,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_property_array_append_result(object, &property, value, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicPropertyArraySet {
            object,
            property,
            index,
            op,
            value,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_property_array_set_result(
                object, &property, index, *op, value, context, scope, values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicPropertyCompoundAssign {
            object,
            property,
            op,
            value,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            let current = eval_property_get_result(object, &property, context, values)?;
            let right = eval_expr(value, context, scope, values)?;
            let value = eval_binary_result(*op, current, right, context, values)?;
            eval_property_set_result(object, &property, value, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicPropertyIncDec {
            object,
            property,
            increment,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_property_inc_dec_result(object, &property, *increment, context, values)?;
            Ok(EvalControl::None)
        }        EvalStmt::PropertySet {
            object,
            property,
            value,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let value = eval_expr(value, context, scope, values)?;
            eval_property_set_result(object, property, value, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::PropertyArrayAppend {
            object,
            property,
            value,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            eval_property_array_append_result(object, property, value, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::PropertyArraySet {
            object,
            property,
            index,
            op,
            value,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            eval_property_array_set_result(
                object, property, index, *op, value, context, scope, values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::PropertyCompoundAssign {
            object,
            property,
            op,
            value,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let current = eval_property_get_result(object, property, context, values)?;
            let right = eval_expr(value, context, scope, values)?;
            let value = eval_binary_result(*op, current, right, context, values)?;
            eval_property_set_result(object, property, value, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::PropertyIncDec {
            object,
            property,
            increment,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            eval_property_inc_dec_result(object, property, *increment, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::StaticPropertySet {
            class_name,
            property,
            value,
        } => {
            let value = eval_owned_expr(value, context, scope, values)?;
            let result = eval_static_property_set_result(class_name, property, value, context, values);
            eval_release_value(context, values, value)?;
            result?;
            Ok(EvalControl::None)
        }
        EvalStmt::StaticPropertyReferenceBind {
            class_name,
            property,
            source,
        } => {
            eval_static_property_reference_bind_result(
                class_name, property, source, context, scope, values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::StaticPropertyArrayAppend {
            class_name,
            property,
            value,
        } => {
            eval_static_property_array_append_result(
                class_name, property, value, context, scope, values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::StaticPropertyArraySet {
            class_name,
            property,
            index,
            op,
            value,
        } => {
            eval_static_property_array_set_result(
                class_name, property, index, *op, value, context, scope, values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::StaticPropertyIncDec {
            class_name,
            property,
            increment,
        } => {
            eval_static_property_inc_dec_result(
                class_name, property, *increment, context, values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicStaticPropertySet {
            class_name,
            property,
            value,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let value = eval_owned_expr(value, context, scope, values)?;
            let result = eval_static_property_set_result(&class_name, property, value, context, values);
            eval_release_value(context, values, value)?;
            result?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicStaticPropertyReferenceBind {
            class_name,
            property,
            source,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            eval_static_property_reference_bind_result(
                &class_name,
                property,
                source,
                context,
                scope,
                values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicStaticPropertyArrayAppend {
            class_name,
            property,
            value,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            eval_static_property_array_append_result(
                &class_name,
                property,
                value,
                context,
                scope,
                values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicStaticPropertyArraySet {
            class_name,
            property,
            index,
            op,
            value,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            eval_static_property_array_set_result(
                &class_name,
                property,
                index,
                *op,
                value,
                context,
                scope,
                values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicStaticPropertyIncDec {
            class_name,
            property,
            increment,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            eval_static_property_inc_dec_result(
                &class_name,
                property,
                *increment,
                context,
                values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicStaticPropertyNameSet {
            class_name,
            property,
            value,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            let value = eval_owned_expr(value, context, scope, values)?;
            let result = eval_static_property_set_result(&class_name, &property, value, context, values);
            eval_release_value(context, values, value)?;
            result?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicStaticPropertyNameReferenceBind {
            class_name,
            property,
            source,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_static_property_reference_bind_result(
                &class_name,
                &property,
                source,
                context,
                scope,
                values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicStaticPropertyNameArrayAppend {
            class_name,
            property,
            value,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_static_property_array_append_result(
                &class_name,
                &property,
                value,
                context,
                scope,
                values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicStaticPropertyNameArraySet {
            class_name,
            property,
            index,
            op,
            value,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_static_property_array_set_result(
                &class_name,
                &property,
                index,
                *op,
                value,
                context,
                scope,
                values,
            )?;
            Ok(EvalControl::None)
        }
        EvalStmt::DynamicStaticPropertyNameIncDec {
            class_name,
            property,
            increment,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_static_property_inc_dec_result(
                &class_name,
                &property,
                *increment,
                context,
                values,
            )?;
            Ok(EvalControl::None)
        }        EvalStmt::UnsetProperty { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            eval_property_unset_result(object, property, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::UnsetDynamicProperty { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_property_unset_result(object, &property, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::UnsetStaticProperty {
            class_name,
            property,
        } => {
            eval_static_property_unset_result(class_name, property, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::UnsetDynamicStaticProperty {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            eval_static_property_unset_result(&class_name, property, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::UnsetDynamicStaticPropertyName {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_static_property_unset_result(&class_name, &property, context, values)?;
            Ok(EvalControl::None)
        }
        _ => unreachable!("property dispatcher received a non-property statement"),
    }
}
