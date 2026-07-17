//! Purpose:
//! Dispatches each EvalIR statement variant to its focused execution helper and
//! propagates structured loop, throw, and return control flow.
//!
//! Called from:
//! - `crate::interpreter::execute_program_outcome_with_context()`.
//! - Dynamic eval function and method execution.
//!
//! Key details:
//! - The exhaustive match remains centralized so every new `EvalStmt` variant
//!   must explicitly define its runtime control-flow behavior.

use super::*;

/// Executes statements in source order and propagates the first eval `return`.
pub(in crate::interpreter) fn execute_statements(
    statements: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    for stmt in statements {
        match execute_stmt(stmt, context, scope, values)? {
            EvalControl::None => {}
            control => return Ok(control),
        }
    }
    Ok(EvalControl::None)
}

/// Executes one statement and returns `Some` only for eval `return`.
pub(in crate::interpreter) fn execute_stmt(
    stmt: &EvalStmt,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    match stmt {
        EvalStmt::ArrayAppendVar { name, value } => {
            eval_array_append_var_stmt(name, value, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::ArraySetVar { name, index, value } => {
            eval_array_set_var_stmt(name, index, value, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::Break => Ok(EvalControl::Break),
        EvalStmt::Continue => Ok(EvalControl::Continue),
        EvalStmt::DoWhile { body, condition } => {
            execute_do_while_stmt(body, condition, context, scope, values)
        }
        EvalStmt::Echo(expr) => {
            let value = eval_expr(expr, context, scope, values)?;
            let value = eval_string_context_value(value, context, values)?;
            values.echo(value)?;
            Ok(EvalControl::None)
        }
        EvalStmt::For {
            init,
            condition,
            update,
            body,
        } => execute_for_stmt(
            init,
            condition.as_ref(),
            update,
            body,
            context,
            scope,
            values,
        ),
        EvalStmt::ClassDecl(class) => {
            execute_class_decl_stmt(class, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::EnumDecl(enum_decl) => {
            execute_enum_decl_stmt(enum_decl, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::InterfaceDecl(interface) => {
            execute_interface_decl_stmt(interface, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::TraitDecl(trait_decl) => {
            execute_trait_decl_stmt(trait_decl, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::Foreach {
            array,
            key_name,
            value_name,
            body,
        } => execute_foreach_stmt(
            array,
            key_name.as_deref(),
            value_name,
            body,
            context,
            scope,
            values,
        ),
        EvalStmt::FunctionDecl {
            name,
            source_location,
            attributes,
            params,
            parameter_attributes,
            parameter_types,
            parameter_defaults,
            parameter_is_by_ref,
            parameter_is_variadic,
            return_type,
            body,
        } => {
            let key = name.to_ascii_lowercase();
            let mut function = EvalFunction::new(name.clone(), params.clone(), body.clone())
                .with_attributes(attributes.clone())
                .with_parameter_attributes(parameter_attributes.clone())
                .with_parameter_types(parameter_types.clone())
                .with_parameter_defaults(parameter_defaults.clone())
                .with_parameter_by_ref_flags(parameter_is_by_ref.clone())
                .with_parameter_variadic_flags(parameter_is_variadic.clone())
                .with_return_type(return_type.clone());
            if let Some(source_location) = source_location {
                function = function.with_source_location(*source_location);
            }
            context
                .define_function(key, function)
                .map_err(|_| EvalStatus::RuntimeFatal)?;
            Ok(EvalControl::None)
        }
        EvalStmt::Global { vars } => {
            execute_global_stmt(vars, context, scope)?;
            Ok(EvalControl::None)
        }
        EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = eval_expr(condition, context, scope, values)?;
            if values.truthy(condition)? {
                execute_statements(then_branch, context, scope, values)
            } else {
                execute_statements(else_branch, context, scope, values)
            }
        }
        EvalStmt::Return(Some(expr)) => Ok(EvalControl::Return(eval_expr(
            expr, context, scope, values,
        )?)),
        EvalStmt::Return(None) => Ok(EvalControl::ReturnVoid),
        EvalStmt::ReferenceAssign { target, source } => {
            for replaced in set_reference_alias(context, scope, target, source, values)? {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
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
        }
        EvalStmt::StaticVar { name, init } => {
            execute_static_var_stmt(name, init, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::PropertySet {
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
            let value = eval_expr(value, context, scope, values)?;
            eval_static_property_set_result(class_name, property, value, context, values)?;
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
            let value = eval_expr(value, context, scope, values)?;
            eval_static_property_set_result(&class_name, property, value, context, values)?;
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
            let value = eval_expr(value, context, scope, values)?;
            eval_static_property_set_result(&class_name, &property, value, context, values)?;
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
        }
        EvalStmt::StoreVar { name, value } => {
            let value = eval_expr(value, context, scope, values)?;
            for replaced in set_scope_cell(
                context,
                scope,
                name.clone(),
                value,
                ScopeCellOwnership::Owned,
            )? {
                eval_release_value(context, values, replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Switch { expr, cases } => {
            execute_switch_stmt(expr, cases, context, scope, values)
        }
        EvalStmt::Throw(expr) => {
            let thrown = eval_expr(expr, context, scope, values)?;
            if values.type_tag(thrown)? != EVAL_TAG_OBJECT {
                return Err(EvalStatus::RuntimeFatal);
            }
            Ok(EvalControl::Throw(thrown))
        }
        EvalStmt::Try {
            body,
            catches,
            finally_body,
        } => execute_try_stmt(body, catches, finally_body, context, scope, values),
        EvalStmt::UnsetArrayElement { array, index } => {
            eval_array_unset_element_stmt(array, index, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::UnsetProperty { object, property } => {
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
        EvalStmt::UnsetVar { name } => {
            if let Some(replaced) = unset_scope_cell(scope, name.clone()) {
                eval_release_value(context, values, replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::While { condition, body } => {
            while {
                let condition = eval_expr(condition, context, scope, values)?;
                values.truthy(condition)?
            } {
                match execute_statements(body, context, scope, values)? {
                    EvalControl::None | EvalControl::Continue => {}
                    EvalControl::Break => break,
                    EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
                    EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
                    EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
                }
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Expr(expr) => {
            let result = eval_expr(expr, context, scope, values)?;
            eval_release_value(context, values, result)?;
            Ok(EvalControl::None)
        }
    }
}
