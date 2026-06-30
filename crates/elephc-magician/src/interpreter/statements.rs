//! Purpose:
//! Executes EvalIR statements, loops, exception handling, static locals, and eval-declared classes.
//!
//! Called from:
//! - `crate::interpreter::execute_program_outcome_with_context()` and dynamic function execution.
//!
//! Key details:
//! - Statement execution propagates `EvalControl` instead of flattening returns, throws, breaks, or continues.
//! - Scope writes flow through shared scope-cell helpers so global aliases and reference aliases stay coherent.

use super::*;
use crate::context::{
    NativeCallableArrayDefaultElement, NativeCallableArrayDefaultKey,
    NativeCallableObjectDefaultArg, NativeCallableSignature,
    push_native_frame_called_class_override,
};

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

/// Applies member increment/decrement to a runtime value using PHP numeric semantics.
fn eval_inc_dec_value(
    current: RuntimeCellHandle,
    increment: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let one = values.int(1)?;
    if increment {
        values.add(current, one)
    } else {
        values.sub(current, one)
    }
}

/// Reads, updates, and writes one object property after the receiver/name are evaluated.
fn eval_property_inc_dec_result(
    object: RuntimeCellHandle,
    property: &str,
    increment: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let current = eval_property_get_result(object, property, context, values)?;
    let value = eval_inc_dec_value(current, increment, values)?;
    eval_property_set_result(object, property, value, context, values)
}

/// Reads, updates, and writes one static property after the receiver/name are resolved.
fn eval_static_property_inc_dec_result(
    class_name: &str,
    property: &str,
    increment: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let current = eval_static_property_get_result(class_name, property, context, values)?;
    let value = eval_inc_dec_value(current, increment, values)?;
    eval_static_property_set_result(class_name, property, value, context, values)
}

/// Releases one eval-owned value after running an eval-declared dynamic destructor if needed.
fn eval_release_value(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    if let Some(identity) = values.final_object_identity_for_release(value)? {
        eval_dynamic_destructor_for_release(identity, value, context, values)?;
    }
    values.release(value)
}

/// Calls a dynamic eval `__destruct()` hook immediately before the runtime frees the object.
fn eval_dynamic_destructor_for_release(
    identity: u64,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_dynamic_destructor_for_object_cell(identity, object, context, values).map(|_| ())
}

/// Calls a dynamic eval `__destruct()` hook for an already-boxed object cell.
pub(crate) fn eval_dynamic_destructor_for_object_cell(
    identity: u64,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(class_name) = context
        .dynamic_object_class(identity)
        .map(|class| class.name().to_string())
    else {
        return Ok(false);
    };
    let Some((declaring_class, method)) = context.class_method(&class_name, "__destruct") else {
        return Ok(false);
    };
    if !context.begin_dynamic_object_destructor(identity) {
        return Ok(true);
    }
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &class_name,
        &method,
        object,
        Vec::new(),
        context,
        values,
    );
    let release_result = match result {
        Ok(result) => values.release(result),
        Err(status) => Err(status),
    };
    context.finish_dynamic_object_destructor(identity);
    release_result.map(|_| true)
}

/// Executes `unset($object[$key])` through `ArrayAccess::offsetUnset()`.
fn eval_array_unset_element_stmt(
    array: &EvalExpr,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match array {
        EvalExpr::LoadVar(name) => {
            let existing = scope_entry(context, scope, name)
                .filter(|entry| entry.flags().is_visible())
                .map(|entry| (entry.cell(), entry.flags().ownership));
            let Some((array, ownership)) = existing else {
                return Ok(());
            };
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                for replaced in set_scope_cell(context, scope, name.clone(), array, ownership)? {
                    values.release(replaced)?;
                }
            }
            return Ok(());
        }
        EvalExpr::PropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            let array = eval_property_get_result(object, property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_property_set_result(object, property, array, context, values)?;
            }
            return Ok(());
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            let array = eval_property_get_result(object, &property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_property_set_result(object, &property, array, context, values)?;
            }
            return Ok(());
        }
        EvalExpr::StaticPropertyGet {
            class_name,
            property,
        } => {
            let array = eval_static_property_get_result(class_name, property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_static_property_set_result(class_name, property, array, context, values)?;
            }
            return Ok(());
        }
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let array = eval_static_property_get_result(&class_name, property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_static_property_set_result(&class_name, property, array, context, values)?;
            }
            return Ok(());
        }
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            let array = eval_static_property_get_result(&class_name, &property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_static_property_set_result(&class_name, &property, array, context, values)?;
            }
            return Ok(());
        }
        _ => {}
    }
    let array = eval_expr(array, context, scope, values)?;
    eval_array_access_unset_result(array, index, context, scope, values)
}

/// Unsets one offset from an already-resolved array-like target and returns a replacement array.
fn eval_array_unset_target_result(
    array: RuntimeCellHandle,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        eval_array_access_unset_result(array, index, context, scope, values)?;
        return Ok(None);
    }
    let tag = values.type_tag(array)?;
    if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let index = eval_array_set_index(index, context, scope, values)?;
    eval_array_without_key_result(array, index, values).map(Some)
}

/// Executes `unset($object[$key])` through `ArrayAccess::offsetUnset()`.
fn eval_array_access_unset_result(
    array: RuntimeCellHandle,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let index = eval_expr(index, context, scope, values)?;
    if values.type_tag(array)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    if !eval_array_access_object_matches(array, context, values)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = eval_method_call_result(array, "offsetUnset", vec![index], context, values)?;
    values.release(result)?;
    Ok(())
}

/// Rebuilds an array without the strict-equal key requested by `unset($array[$key])`.
fn eval_array_without_key_result(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let tag = values.type_tag(array)?;
    let mut result = if tag == EVAL_TAG_ASSOC {
        values.assoc_new(len.saturating_sub(1))?
    } else {
        values.array_new(len.saturating_sub(1))?
    };
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let equal = values.compare(EvalBinOp::StrictEq, key, index)?;
        if values.truthy(equal)? {
            continue;
        }
        let value = values.array_get(array, key)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Executes `$var[] = value` and dispatches object writes through `ArrayAccess::offsetSet()`.
fn eval_array_append_var_stmt(
    name: &str,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let existing = scope_entry(context, scope, name)
        .filter(|entry| entry.flags().is_visible())
        .map(|entry| (entry.cell(), entry.flags().ownership));
    if let Some((object, _)) = existing {
        if values.type_tag(object)? != EVAL_TAG_OBJECT {
            return eval_non_object_array_append_var_stmt(
                name, value, existing, context, scope, values,
            );
        }
        let offset = values.null()?;
        let value = eval_expr(value, context, scope, values)?;
        if !eval_array_access_object_matches(object, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let result =
            eval_method_call_result(object, "offsetSet", vec![offset, value], context, values)?;
        values.release(result)?;
        return Ok(());
    }

    eval_non_object_array_append_var_stmt(name, value, existing, context, scope, values)
}

/// Executes the non-object `$var[] = value` path with the existing array semantics.
fn eval_non_object_array_append_var_stmt(
    name: &str,
    value: &EvalExpr,
    existing: Option<(RuntimeCellHandle, ScopeCellOwnership)>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut ownership = ScopeCellOwnership::Owned;
    let array = if let Some((cell, flags_ownership)) = existing {
        if values.is_array_like(cell)? {
            let tag = values.type_tag(cell)?;
            if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
                return Err(EvalStatus::UnsupportedConstruct);
            }
            ownership = flags_ownership;
            cell
        } else {
            values.array_new(1)?
        }
    } else {
        values.array_new(1)?
    };
    let index = eval_array_append_key(array, values)?;
    let value = eval_expr(value, context, scope, values)?;
    let array = values.array_set(array, index, value)?;
    for replaced in set_scope_cell(context, scope, name.to_string(), array, ownership)? {
        values.release(replaced)?;
    }
    Ok(())
}

/// Executes `$var[index] = value` and dispatches object writes through `ArrayAccess::offsetSet()`.
fn eval_array_set_var_stmt(
    name: &str,
    index: &EvalExpr,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let existing = scope_entry(context, scope, name)
        .filter(|entry| entry.flags().is_visible())
        .map(|entry| (entry.cell(), entry.flags().ownership));
    if let Some((object, _)) = existing {
        if values.type_tag(object)? != EVAL_TAG_OBJECT {
            return eval_non_object_array_set_var_stmt(
                name, index, value, existing, context, scope, values,
            );
        }
        let index = eval_expr(index, context, scope, values)?;
        let value = eval_expr(value, context, scope, values)?;
        if !eval_array_access_object_matches(object, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let result =
            eval_method_call_result(object, "offsetSet", vec![index, value], context, values)?;
        values.release(result)?;
        return Ok(());
    }

    eval_non_object_array_set_var_stmt(name, index, value, existing, context, scope, values)
}

/// Executes the non-object `$var[index] = value` path with the existing array semantics.
fn eval_non_object_array_set_var_stmt(
    name: &str,
    index: &EvalExpr,
    value: &EvalExpr,
    existing: Option<(RuntimeCellHandle, ScopeCellOwnership)>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut ownership = ScopeCellOwnership::Owned;
    let array = if let Some((cell, flags_ownership)) = existing {
        if values.is_array_like(cell)? {
            ownership = flags_ownership;
            cell
        } else {
            values.array_new(1)?
        }
    } else {
        values.array_new(1)?
    };
    let index = eval_array_set_index(index, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    let array = eval_array_set_target_for_index(array, index, values)?;
    let array = values.array_set(array, index, value)?;
    for replaced in set_scope_cell(context, scope, name.to_string(), array, ownership)? {
        values.release(replaced)?;
    }
    Ok(())
}

/// Executes `$object->property[] = value`, dispatching ArrayAccess property values when needed.
fn eval_property_array_append_result(
    object: RuntimeCellHandle,
    property: &str,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_property_get_result(object, property, context, values)?;
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        if !eval_array_access_object_matches(array, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let offset = values.null()?;
        let value = eval_expr(value, context, scope, values)?;
        let result =
            eval_method_call_result(array, "offsetSet", vec![offset, value], context, values)?;
        values.release(result)?;
        return Ok(());
    }
    let array = if values.is_array_like(array)? {
        let tag = values.type_tag(array)?;
        if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            return Err(EvalStatus::UnsupportedConstruct);
        }
        array
    } else {
        values.array_new(1)?
    };
    let index = eval_array_append_key(array, values)?;
    let value = eval_expr(value, context, scope, values)?;
    let array = values.array_set(array, index, value)?;
    eval_property_set_result(object, property, array, context, values)
}

/// Executes `$object->property[index] = value` and compound indexed property writes.
fn eval_property_array_set_result(
    object: RuntimeCellHandle,
    property: &str,
    index: &EvalExpr,
    op: Option<EvalBinOp>,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_property_get_result(object, property, context, values)?;
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        if !eval_array_access_object_matches(array, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let index = eval_expr(index, context, scope, values)?;
        let value = eval_property_array_set_value(array, index, op, value, context, scope, values)?;
        let result =
            eval_method_call_result(array, "offsetSet", vec![index, value], context, values)?;
        values.release(result)?;
        return Ok(());
    }
    let index = eval_array_set_index(index, context, scope, values)?;
    let array = if values.is_array_like(array)? {
        array
    } else {
        values.array_new(1)?
    };
    let array = eval_array_set_target_for_index(array, index, values)?;
    let value = eval_property_array_set_value(array, index, op, value, context, scope, values)?;
    let array = values.array_set(array, index, value)?;
    eval_property_set_result(object, property, array, context, values)
}

/// Computes the value written by a simple or compound property-array assignment.
fn eval_property_array_set_value(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    op: Option<EvalBinOp>,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(op) = op else {
        return eval_expr(value, context, scope, values);
    };
    let current = eval_array_get_result(array, index, context, values)?;
    let right = eval_expr(value, context, scope, values)?;
    eval_binary_result(op, current, right, context, values)
}

/// Executes `Class::$property[] = value`, including ArrayAccess static-property values.
fn eval_static_property_array_append_result(
    class_name: &str,
    property: &str,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_static_property_get_result(class_name, property, context, values)?;
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        if !eval_array_access_object_matches(array, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let offset = values.null()?;
        let value = eval_expr(value, context, scope, values)?;
        let result =
            eval_method_call_result(array, "offsetSet", vec![offset, value], context, values)?;
        values.release(result)?;
        return Ok(());
    }
    let array = if values.is_array_like(array)? {
        let tag = values.type_tag(array)?;
        if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            return Err(EvalStatus::UnsupportedConstruct);
        }
        array
    } else {
        values.array_new(1)?
    };
    let index = eval_array_append_key(array, values)?;
    let value = eval_expr(value, context, scope, values)?;
    let array = values.array_set(array, index, value)?;
    eval_static_property_set_result(class_name, property, array, context, values)
}

/// Executes `Class::$property[index] = value` and compound indexed static-property writes.
fn eval_static_property_array_set_result(
    class_name: &str,
    property: &str,
    index: &EvalExpr,
    op: Option<EvalBinOp>,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_static_property_get_result(class_name, property, context, values)?;
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        if !eval_array_access_object_matches(array, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let index = eval_expr(index, context, scope, values)?;
        let value = eval_property_array_set_value(array, index, op, value, context, scope, values)?;
        let result =
            eval_method_call_result(array, "offsetSet", vec![index, value], context, values)?;
        values.release(result)?;
        return Ok(());
    }
    let index = eval_array_set_index(index, context, scope, values)?;
    let array = if values.is_array_like(array)? {
        array
    } else {
        values.array_new(1)?
    };
    let array = eval_array_set_target_for_index(array, index, values)?;
    let value = eval_property_array_set_value(array, index, op, value, context, scope, values)?;
    let array = values.array_set(array, index, value)?;
    eval_static_property_set_result(class_name, property, array, context, values)
}

/// Evaluates an array-set index and normalizes PHP integer-string keys.
fn eval_array_set_index(
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let index = eval_expr(index, context, scope, values)?;
    if values.type_tag(index)? != EVAL_TAG_STRING {
        return Ok(index);
    }
    let bytes = values.string_bytes(index)?;
    match eval_numeric_string_array_key(&bytes) {
        Some(key) => values.int(key),
        None => Ok(index),
    }
}

/// Converts indexed arrays to associative arrays before writing a non-numeric string key.
fn eval_array_set_target_for_index(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(array)? != EVAL_TAG_ARRAY || values.type_tag(index)? != EVAL_TAG_STRING {
        return Ok(array);
    }
    let len = values.array_len(array)?;
    let mut assoc = values.assoc_new(len + 1)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        assoc = values.array_set(assoc, key, value)?;
    }
    Ok(assoc)
}

/// Executes an eval `try` body and handles supported `catch` clauses.
pub(in crate::interpreter) fn execute_try_stmt(
    body: &[EvalStmt],
    catches: &[EvalCatch],
    finally_body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let control = match execute_statements(body, context, scope, values) {
        Ok(EvalControl::Throw(thrown)) => {
            execute_matching_catch(thrown, catches, context, scope, values)?
        }
        Err(EvalStatus::UncaughtThrowable) => {
            let Some(thrown) = context.take_pending_throw() else {
                return Err(EvalStatus::UncaughtThrowable);
            };
            execute_matching_catch(thrown, catches, context, scope, values)?
        }
        Ok(control) => control,
        Err(status) => return Err(status),
    };
    if finally_body.is_empty() {
        return Ok(control);
    }
    match execute_statements(finally_body, context, scope, values) {
        Ok(EvalControl::None) => Ok(control),
        Ok(finally_control) => {
            release_overridden_control(control, values)?;
            Ok(finally_control)
        }
        Err(status) => {
            release_overridden_control(control, values)?;
            Err(status)
        }
    }
}

/// Releases a pending control-flow value when `finally` replaces that action.
pub(in crate::interpreter) fn release_overridden_control(
    control: EvalControl,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match control {
        EvalControl::Return(value) | EvalControl::Throw(value) => values.release(value),
        EvalControl::None
        | EvalControl::ReturnVoid
        | EvalControl::Break
        | EvalControl::Continue => Ok(()),
    }
}

/// Executes the first supported catch clause for a thrown eval object.
pub(in crate::interpreter) fn execute_matching_catch(
    thrown: RuntimeCellHandle,
    catches: &[EvalCatch],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let mut matched = None;
    for catch in catches {
        if catch_types_match_thrown(thrown, &catch.class_names, context, values)? {
            matched = Some(catch);
            break;
        }
    }
    let Some(catch) = matched else {
        return Ok(EvalControl::Throw(thrown));
    };
    if let Some(var_name) = &catch.var_name {
        for replaced in set_scope_cell(
            context,
            scope,
            var_name.clone(),
            thrown,
            ScopeCellOwnership::Owned,
        )? {
            values.release(replaced)?;
        }
    } else {
        values.release(thrown)?;
    }
    execute_statements(&catch.body, context, scope, values)
}

/// Returns true when any type in one catch clause accepts the thrown object.
pub(in crate::interpreter) fn catch_types_match_thrown(
    thrown: RuntimeCellHandle,
    class_names: &[String],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for class_name in class_names {
        let class_name = class_name.trim_start_matches('\\');
        if class_name.eq_ignore_ascii_case("Throwable") {
            return Ok(true);
        }
        if let Some(matched) = dynamic_object_is_a(thrown, class_name, false, context, values)? {
            if matched {
                return Ok(true);
            }
            continue;
        }
        if values.object_is_a(thrown, class_name, false)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Returns whether one name is a PHP native enum interface.
pub(in crate::interpreter) fn eval_builtin_enum_interface_name(name: &str) -> bool {
    let name = name.trim_start_matches('\\');
    name.eq_ignore_ascii_case("UnitEnum") || name.eq_ignore_ascii_case("BackedEnum")
}

/// Returns whether one name is PHP's native backed-enum interface.
fn eval_builtin_backed_enum_interface_name(name: &str) -> bool {
    name.trim_start_matches('\\')
        .eq_ignore_ascii_case("BackedEnum")
}

/// Returns whether one name is PHP's native Throwable interface.
fn eval_builtin_throwable_interface_name(name: &str) -> bool {
    name.trim_start_matches('\\')
        .eq_ignore_ascii_case("Throwable")
}

/// Returns whether one name is visible as a native/runtime interface to eval.
pub(in crate::interpreter) fn eval_runtime_interface_exists(
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(eval_builtin_enum_interface_name(name) || values.interface_exists(name)?)
}

/// Registers an eval-declared class in the dynamic class table.
pub(in crate::interpreter) fn execute_class_decl_stmt(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let name = class.name().trim_start_matches('\\');
    if context.has_class(name)
        || context.has_interface(name)
        || context.has_trait(name)
        || context.has_enum(name)
        || values.class_exists(name)?
        || eval_runtime_interface_exists(name, values)?
        || values.trait_exists(name)?
        || values.enum_exists(name)?
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let class = expand_eval_class_traits(class, context)?.with_readonly_properties();
    let class = &class;
    validate_eval_class_modifiers(class, context, values)?;
    let native_parent = validate_eval_class_parent(class, context, values)?;
    for interface in class.interfaces() {
        if !context.has_interface(interface) && !eval_runtime_interface_exists(interface, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    validate_eval_class_does_not_implement_throwable_interfaces(class, context)?;
    validate_eval_class_does_not_implement_enum_interfaces(class, context)?;
    validate_declared_class_interface_members(class, context)?;
    validate_declared_class_builtin_interface_members(class, context)?;
    validate_declared_class_aot_interface_members(class, context, values)?;
    if !class.is_abstract() {
        validate_concrete_class_requirements(class, context)?;
        validate_concrete_class_builtin_interface_requirements(class, context)?;
        validate_concrete_class_aot_parent_requirements(class, context, values)?;
        validate_concrete_class_aot_interface_requirements(class, context, values)?;
    }
    if context.define_class(class.clone()) {
        if let Some(parent) = native_parent.as_deref() {
            if !context.define_native_class_parent(class.name(), parent) {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        initialize_eval_declared_constants(
            class.name(),
            class.constants(),
            context,
            scope,
            values,
        )?;
        initialize_eval_static_properties(class, context, scope, values)
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Validates an eval class parent and returns an AOT parent name when the parent is runtime-backed.
fn validate_eval_class_parent(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some(parent) = class.parent() else {
        return Ok(None);
    };
    let parent = context
        .resolve_class_name(parent)
        .unwrap_or_else(|| parent.trim_start_matches('\\').to_string());
    if let Some(parent_class) = context.class(&parent) {
        if parent_class.is_final()
            || parent_class.is_readonly_class() != class.is_readonly_class()
            || context.class_is_a(&parent, class.name(), false)
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(None);
    }
    let Some((parent_is_final, parent_is_readonly)) =
        eval_reflection_aot_class_inheritance_modifiers(&parent, values)?
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if parent_is_final
        || parent_is_readonly != class.is_readonly_class()
        || native_class_is_a(&parent, class.name(), context)
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(Some(parent))
}

/// Registers one eval anonymous class expression if this execution has not seen it yet.
pub(in crate::interpreter) fn ensure_eval_anonymous_class_decl(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !class.is_anonymous() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(existing) = context.class(class.name()) {
        return if existing.is_anonymous() {
            Ok(())
        } else {
            Err(EvalStatus::RuntimeFatal)
        };
    }
    execute_class_decl_stmt(class, context, scope, values)
}

/// Registers an eval-declared enum and materializes its singleton cases.
pub(in crate::interpreter) fn execute_enum_decl_stmt(
    enum_decl: &EvalEnum,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let name = enum_decl.name().trim_start_matches('\\');
    if context.has_enum(name)
        || context.has_class(name)
        || context.has_interface(name)
        || context.has_trait(name)
        || values.enum_exists(name)?
        || values.class_exists(name)?
        || eval_runtime_interface_exists(name, values)?
        || values.trait_exists(name)?
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    validate_eval_enum_direct_method_declarations(enum_decl)?;
    let enum_decl = expand_eval_enum_traits(enum_decl, context)?;
    let enum_decl = &enum_decl;
    validate_eval_enum_decl(enum_decl, context, values)?;
    if context.define_enum(enum_decl.clone()) {
        initialize_eval_declared_constants(
            enum_decl.name(),
            enum_decl.constants(),
            context,
            scope,
            values,
        )?;
        initialize_eval_enum_cases(enum_decl, context, scope, values)
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Expands eval trait uses into enum metadata while rejecting imported properties.
fn expand_eval_enum_traits(
    enum_decl: &EvalEnum,
    context: &ElephcEvalContext,
) -> Result<EvalEnum, EvalStatus> {
    if enum_decl.traits().is_empty() {
        return Ok(enum_decl.clone());
    }
    let enum_class = enum_decl.as_class_metadata();
    validate_eval_trait_adaptations(&enum_class, context)?;
    let mut enum_method_names = class_method_name_set(&enum_class);
    insert_eval_enum_synthetic_method_names(enum_decl, &mut enum_method_names);
    let mut trait_method_names = std::collections::HashSet::new();
    let mut trait_properties = std::collections::HashMap::new();
    let mut trait_constants = std::collections::HashMap::new();
    let mut constants = Vec::new();
    let mut properties = Vec::new();
    let mut methods = Vec::new();
    for trait_name in enum_decl.traits() {
        let Some(trait_decl) = context.trait_decl(trait_name) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        append_eval_trait_constants(
            trait_decl,
            enum_decl.constants(),
            &mut trait_constants,
            &mut constants,
        )?;
        append_eval_trait_properties(
            trait_decl,
            &[],
            &mut trait_properties,
            &mut properties,
        )?;
        append_eval_trait_methods(
            trait_decl,
            enum_decl.trait_adaptations(),
            &enum_method_names,
            &mut trait_method_names,
            &mut methods,
        )?;
    }
    if !properties.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    constants.extend(enum_decl.constants().iter().cloned());
    methods.extend(enum_decl.methods().iter().cloned());
    let mut expanded = EvalEnum::with_members_traits_adaptations(
        enum_decl.name().to_string(),
        enum_decl.backing_type(),
        enum_decl.interfaces().to_vec(),
        enum_decl.cases().to_vec(),
        constants,
        methods,
        enum_decl.traits().to_vec(),
        enum_decl.trait_adaptations().to_vec(),
    )
    .with_attributes(enum_decl.attributes().to_vec());
    if let Some(source_location) = enum_decl.source_location() {
        expanded = expanded.with_source_location(source_location);
    }
    Ok(expanded)
}

/// Adds PHP's enum-provided method names to the set that hides trait imports.
fn insert_eval_enum_synthetic_method_names(
    enum_decl: &EvalEnum,
    method_names: &mut std::collections::HashSet<String>,
) {
    method_names.insert(String::from("cases"));
    if enum_decl.backing_type().is_some() {
        method_names.insert(String::from("from"));
        method_names.insert(String::from("tryfrom"));
    }
}

/// Validates enum metadata before it is inserted into the dynamic context.
fn validate_eval_enum_decl(
    enum_decl: &EvalEnum,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    validate_eval_enum_attribute_targets(enum_decl)?;
    validate_eval_declared_constants(enum_decl.constants())?;
    validate_eval_enum_case_declarations(enum_decl)?;
    validate_eval_enum_forbidden_magic_methods(enum_decl)?;
    let enum_class = enum_decl.as_class_metadata();
    validate_eval_class_modifiers(&enum_class, context, values)?;
    validate_eval_enum_interfaces(enum_decl, &enum_class, context, values)?;
    validate_declared_class_builtin_interface_members(&enum_class, context)?;
    validate_declared_class_aot_interface_members(&enum_class, context, values)?;
    validate_concrete_class_builtin_interface_requirements(&enum_class, context)?;
    validate_concrete_class_aot_interface_requirements(&enum_class, context, values)?;
    validate_concrete_class_requirements(&enum_class, context)
}

/// Validates PHP's special enum interface rules for one eval enum declaration.
fn validate_eval_enum_interfaces(
    enum_decl: &EvalEnum,
    enum_class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for interface in enum_decl.interfaces() {
        if eval_builtin_enum_interface_name(interface) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if !context.has_interface(interface) && !eval_runtime_interface_exists(interface, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    validate_eval_class_does_not_implement_throwable_interfaces(enum_class, context)?;
    if enum_decl.backing_type().is_none()
        && pending_class_interface_names(enum_class, context)
            .iter()
            .any(|interface| eval_builtin_backed_enum_interface_name(interface))
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Validates enum case names and pure/backed declaration shape.
fn validate_eval_enum_case_declarations(enum_decl: &EvalEnum) -> Result<(), EvalStatus> {
    let mut case_names = std::collections::HashSet::new();
    let constant_names = enum_decl
        .constants()
        .iter()
        .map(|constant| constant.name().to_string())
        .collect::<std::collections::HashSet<_>>();
    for case in enum_decl.cases() {
        validate_eval_non_method_attribute_targets(case.attributes())?;
        if !case_names.insert(case.name().to_string()) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if constant_names.contains(case.name()) {
            return Err(EvalStatus::RuntimeFatal);
        }
        match (enum_decl.backing_type(), case.value()) {
            (None, None) | (Some(_), Some(_)) => {}
            (None, Some(_)) | (Some(_), None) => return Err(EvalStatus::RuntimeFatal),
        }
    }
    Ok(())
}

/// Validates direct enum methods that PHP reserves on enum declarations.
fn validate_eval_enum_direct_method_declarations(enum_decl: &EvalEnum) -> Result<(), EvalStatus> {
    for method in enum_decl.methods() {
        if method.name().eq_ignore_ascii_case("cases") {
            return Err(EvalStatus::RuntimeFatal);
        }
        if enum_decl.backing_type().is_some()
            && (method.name().eq_ignore_ascii_case("from")
                || method.name().eq_ignore_ascii_case("tryFrom"))
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        if is_forbidden_eval_enum_magic_method(method.name()) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates enum methods, including trait imports, that PHP forbids by magic name.
fn validate_eval_enum_forbidden_magic_methods(enum_decl: &EvalEnum) -> Result<(), EvalStatus> {
    for method in enum_decl.methods() {
        if is_forbidden_eval_enum_magic_method(method.name()) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns whether PHP forbids this magic method name inside enum declarations.
fn is_forbidden_eval_enum_magic_method(name: &str) -> bool {
    [
        "__construct",
        "__destruct",
        "__clone",
        "__get",
        "__set",
        "__isset",
        "__unset",
        "__sleep",
        "__wakeup",
        "__serialize",
        "__unserialize",
        "__toString",
        "__debugInfo",
        "__set_state",
    ]
    .iter()
    .any(|method| name.eq_ignore_ascii_case(method))
}

/// Initializes enum singleton case objects for a newly declared eval enum.
fn initialize_eval_enum_cases(
    enum_decl: &EvalEnum,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut backing_values = Vec::new();
    for case in enum_decl.cases() {
        let backing_value = if let Some(value_expr) = case.value() {
            let value = eval_expr(value_expr, context, scope, values)?;
            validate_eval_enum_backing_value(enum_decl.backing_type(), value, values)?;
            for existing in &backing_values {
                let equal = values.compare(EvalBinOp::StrictEq, value, *existing)?;
                if values.truthy(equal)? {
                    return Err(EvalStatus::RuntimeFatal);
                }
            }
            backing_values.push(value);
            Some(value)
        } else {
            None
        };
        initialize_eval_enum_case(enum_decl, case, backing_value, context, values)?;
    }
    Ok(())
}

/// Validates that one evaluated enum backing value matches the declared backing type.
fn validate_eval_enum_backing_value(
    backing_type: Option<EvalEnumBackingType>,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(backing_type) = backing_type else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let tag = values.type_tag(value)?;
    match backing_type {
        EvalEnumBackingType::Int if tag == EVAL_TAG_INT => Ok(()),
        EvalEnumBackingType::String if tag == EVAL_TAG_STRING => Ok(()),
        EvalEnumBackingType::Int | EvalEnumBackingType::String => Err(EvalStatus::RuntimeFatal),
    }
}

/// Creates and stores one enum case singleton object.
fn initialize_eval_enum_case(
    enum_decl: &EvalEnum,
    case: &EvalEnumCase,
    backing_value: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let object = values.new_object("stdClass")?;
    let identity = values.object_identity(object)?;
    context.register_dynamic_object(identity, enum_decl.name());
    let name = values.string(case.name())?;
    values.property_set(object, "name", name)?;
    if let Some(value) = backing_value {
        values.property_set(object, "value", value)?;
        if let Some(replaced) = context.set_enum_case_value(enum_decl.name(), case.name(), value) {
            values.release(replaced)?;
        }
    }
    if let Some(replaced) = context.set_enum_case(enum_decl.name(), case.name(), object) {
        values.release(replaced)?;
    }
    Ok(())
}

/// Initializes class-like constant cells for a newly declared eval class-like.
fn initialize_eval_declared_constants(
    owner_name: &str,
    constants: &[EvalClassConstant],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for constant in constants {
        let value = eval_class_like_member_default(
            owner_name,
            constant.trait_origin(),
            constant.value(),
            context,
            scope,
            values,
        )?;
        if let Some(replaced) = context.set_class_constant_cell(owner_name, constant.name(), value)
        {
            values.release(replaced)?;
        }
    }
    Ok(())
}

/// Evaluates a class-like constant or property initializer with PHP magic scope.
fn eval_class_like_member_default(
    owner_name: &str,
    trait_origin: Option<&str>,
    default: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let trait_name = trait_origin.or_else(|| context.has_trait(owner_name).then_some(owner_name));
    context.push_class_like_member_magic_scope(owner_name, trait_name);
    let result = eval_expr(default, context, scope, values);
    context.pop_magic_scope();
    result
}

/// Initializes static property cells for a newly declared eval class.
fn initialize_eval_static_properties(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for property in class
        .properties()
        .iter()
        .filter(|property| property.is_static())
    {
        let value = if let Some(default) = property.default() {
            Some(eval_class_like_member_default(
                class.name(),
                property.trait_origin(),
                default,
                context,
                scope,
                values,
            )?)
        } else if property.property_type().is_none() {
            Some(values.null()?)
        } else {
            None
        };
        if let Some(value) = value {
            if let Some(replaced) =
                context.set_static_property(class.name(), property.name(), value)
            {
                values.release(replaced)?;
            }
        }
    }
    Ok(())
}

/// Registers an eval-declared interface in the dynamic interface table.
pub(in crate::interpreter) fn execute_interface_decl_stmt(
    interface: &EvalInterface,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let name = interface.name().trim_start_matches('\\');
    if context.has_interface(name)
        || context.has_class(name)
        || context.has_enum(name)
        || eval_runtime_interface_exists(name, values)?
        || values.class_exists(name)?
        || values.enum_exists(name)?
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    for parent in interface.parents() {
        if context
            .interface_parent_names(parent)
            .iter()
            .any(|ancestor| ancestor.eq_ignore_ascii_case(name))
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        if !context.has_interface(parent) && !eval_runtime_interface_exists(parent, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    validate_eval_interface_attribute_targets(interface)?;
    validate_eval_interface_override_attributes(interface, context, values)?;
    validate_eval_declared_constants(interface.constants())?;
    validate_eval_interface_constants(interface.constants())?;
    validate_interface_constant_parent_redeclarations(interface, context, values)?;
    if context.define_interface(interface.clone()) {
        initialize_eval_declared_constants(
            interface.name(),
            interface.constants(),
            context,
            scope,
            values,
        )
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Registers an eval-declared trait in the dynamic trait table.
pub(in crate::interpreter) fn execute_trait_decl_stmt(
    trait_decl: &EvalTrait,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let name = trait_decl.name().trim_start_matches('\\');
    if context.has_trait(name)
        || context.has_class(name)
        || context.has_interface(name)
        || context.has_enum(name)
        || values.trait_exists(name)?
        || values.class_exists(name)?
        || eval_runtime_interface_exists(name, values)?
        || values.enum_exists(name)?
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let trait_decl = expand_eval_trait_traits(trait_decl, context)?;
    validate_eval_trait_attribute_targets(&trait_decl)?;
    validate_eval_declared_constants(trait_decl.constants())?;
    validate_eval_magic_methods(trait_decl.methods())?;
    if context.define_trait(trait_decl.clone()) {
        initialize_eval_declared_constants(
            trait_decl.name(),
            trait_decl.constants(),
            context,
            scope,
            values,
        )
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Expands nested eval trait uses into the trait metadata registered by eval.
fn expand_eval_trait_traits(
    trait_decl: &EvalTrait,
    context: &ElephcEvalContext,
) -> Result<EvalTrait, EvalStatus> {
    if trait_decl.traits().is_empty() {
        return Ok(trait_decl.clone());
    }
    validate_eval_trait_decl_adaptations(trait_decl, context)?;
    let trait_method_names = trait_method_name_set(trait_decl);
    let mut imported_method_names = std::collections::HashSet::new();
    let mut imported_properties = std::collections::HashMap::new();
    let mut imported_constants = std::collections::HashMap::new();
    let mut constants = Vec::new();
    let mut properties = Vec::new();
    let mut methods = Vec::new();
    for used_trait_name in trait_decl.traits() {
        let Some(used_trait_decl) = context.trait_decl(used_trait_name) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        append_eval_trait_constants(
            used_trait_decl,
            trait_decl.constants(),
            &mut imported_constants,
            &mut constants,
        )?;
        append_eval_trait_properties(
            used_trait_decl,
            trait_decl.properties(),
            &mut imported_properties,
            &mut properties,
        )?;
        append_eval_trait_methods(
            used_trait_decl,
            trait_decl.trait_adaptations(),
            &trait_method_names,
            &mut imported_method_names,
            &mut methods,
        )?;
    }
    constants.extend(trait_decl.constants().iter().cloned());
    properties.extend(trait_decl.properties().iter().cloned());
    methods.extend(trait_decl.methods().iter().cloned());
    let mut expanded = EvalTrait::with_constants_traits_adaptations(
        trait_decl.name().to_string(),
        constants,
        properties,
        methods,
        trait_decl.traits().to_vec(),
        trait_decl.trait_adaptations().to_vec(),
    )
    .with_attributes(trait_decl.attributes().to_vec());
    if let Some(source_location) = trait_decl.source_location() {
        expanded = expanded.with_source_location(source_location);
    }
    Ok(expanded)
}

/// Validates that trait-level adaptations reference directly used traits and methods.
fn validate_eval_trait_decl_adaptations(
    trait_decl: &EvalTrait,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for adaptation in trait_decl.trait_adaptations() {
        match adaptation {
            EvalTraitAdaptation::Alias {
                trait_name, method, ..
            } => validate_eval_trait_decl_adaptation_method(
                trait_decl,
                context,
                trait_name.as_deref(),
                method,
            )?,
            EvalTraitAdaptation::InsteadOf {
                trait_name,
                method,
                instead_of,
            } => {
                let Some(trait_name) = trait_name.as_deref() else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                validate_eval_trait_decl_adaptation_method(
                    trait_decl,
                    context,
                    Some(trait_name),
                    method,
                )?;
                for suppressed in instead_of {
                    if eval_trait_used_trait_decl(trait_decl, context, suppressed).is_none() {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                    if same_eval_class_name(suppressed, trait_name) {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Validates one trait-level adaptation method target.
fn validate_eval_trait_decl_adaptation_method(
    trait_decl: &EvalTrait,
    context: &ElephcEvalContext,
    trait_name: Option<&str>,
    method: &str,
) -> Result<(), EvalStatus> {
    if let Some(trait_name) = trait_name {
        let Some(used_trait_decl) = eval_trait_used_trait_decl(trait_decl, context, trait_name)
        else {
            return Err(EvalStatus::RuntimeFatal);
        };
        return trait_has_method(used_trait_decl, method)
            .then_some(())
            .ok_or(EvalStatus::RuntimeFatal);
    }
    trait_decl
        .traits()
        .iter()
        .filter_map(|trait_name| context.trait_decl(trait_name))
        .any(|used_trait_decl| trait_has_method(used_trait_decl, method))
        .then_some(())
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns a trait declaration only when the pending trait directly uses that trait.
fn eval_trait_used_trait_decl<'a>(
    trait_decl: &EvalTrait,
    context: &'a ElephcEvalContext,
    trait_name: &str,
) -> Option<&'a EvalTrait> {
    trait_decl
        .traits()
        .iter()
        .any(|used_trait| same_eval_class_name(used_trait, trait_name))
        .then(|| context.trait_decl(trait_name))
        .flatten()
}

/// Expands eval trait uses into the class metadata used by dynamic dispatch.
fn expand_eval_class_traits(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<EvalClass, EvalStatus> {
    if class.traits().is_empty() {
        return Ok(class.clone());
    }
    validate_eval_trait_adaptations(class, context)?;
    let class_method_names = class_method_name_set(class);
    let mut trait_method_names = std::collections::HashSet::new();
    let mut trait_properties = std::collections::HashMap::new();
    let mut trait_constants = std::collections::HashMap::new();
    let mut constants = Vec::new();
    let mut properties = Vec::new();
    let mut methods = Vec::new();
    for trait_name in class.traits() {
        let Some(trait_decl) = context.trait_decl(trait_name) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        append_eval_trait_constants(
            trait_decl,
            class.constants(),
            &mut trait_constants,
            &mut constants,
        )?;
        append_eval_trait_properties(
            trait_decl,
            class.properties(),
            &mut trait_properties,
            &mut properties,
        )?;
        append_eval_trait_methods(
            trait_decl,
            class.trait_adaptations(),
            &class_method_names,
            &mut trait_method_names,
            &mut methods,
        )?;
    }
    constants.extend(class.constants().iter().cloned());
    properties.extend(class.properties().iter().cloned());
    methods.extend(class.methods().iter().cloned());
    let mut expanded = EvalClass::with_class_modifiers_traits_adaptations_and_constants(
        class.name().to_string(),
        class.is_abstract(),
        class.is_final(),
        class.is_readonly_class(),
        class.parent().map(str::to_string),
        class.interfaces().to_vec(),
        class.traits().to_vec(),
        class.trait_adaptations().to_vec(),
        constants,
        properties,
        methods,
    )
    .with_attributes(class.attributes().to_vec());
    if class.is_anonymous() {
        expanded = expanded.with_anonymous();
    }
    Ok(expanded)
}

/// Validates that trait adaptations reference used traits and existing methods.
fn validate_eval_trait_adaptations(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for adaptation in class.trait_adaptations() {
        match adaptation {
            EvalTraitAdaptation::Alias {
                trait_name, method, ..
            } => {
                validate_eval_trait_adaptation_method(class, context, trait_name.as_deref(), method)?
            }
            EvalTraitAdaptation::InsteadOf {
                trait_name,
                method,
                instead_of,
            } => {
                let Some(trait_name) = trait_name.as_deref() else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                validate_eval_trait_adaptation_method(class, context, Some(trait_name), method)?;
                for suppressed in instead_of {
                    if eval_used_trait_decl(class, context, suppressed).is_none() {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                    if same_eval_class_name(suppressed, trait_name) {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Validates one adaptation method target, allowing unqualified alias targets.
fn validate_eval_trait_adaptation_method(
    class: &EvalClass,
    context: &ElephcEvalContext,
    trait_name: Option<&str>,
    method: &str,
) -> Result<(), EvalStatus> {
    if let Some(trait_name) = trait_name {
        let Some(trait_decl) = eval_used_trait_decl(class, context, trait_name) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        return trait_has_method(trait_decl, method)
            .then_some(())
            .ok_or(EvalStatus::RuntimeFatal);
    }
    class
        .traits()
        .iter()
        .filter_map(|trait_name| context.trait_decl(trait_name))
        .any(|trait_decl| trait_has_method(trait_decl, method))
        .then_some(())
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns a trait declaration only when the pending class directly uses that trait.
fn eval_used_trait_decl<'a>(
    class: &EvalClass,
    context: &'a ElephcEvalContext,
    trait_name: &str,
) -> Option<&'a EvalTrait> {
    class
        .traits()
        .iter()
        .any(|used_trait| same_eval_class_name(used_trait, trait_name))
        .then(|| context.trait_decl(trait_name))
        .flatten()
}

/// Returns whether a trait declares a method by PHP case-insensitive method name.
fn trait_has_method(trait_decl: &EvalTrait, method: &str) -> bool {
    trait_decl
        .methods()
        .iter()
        .any(|trait_method| trait_method.name().eq_ignore_ascii_case(method))
}

/// Returns case-insensitive method names declared directly by a pending trait.
fn trait_method_name_set(trait_decl: &EvalTrait) -> std::collections::HashSet<String> {
    trait_decl
        .methods()
        .iter()
        .map(|method| method.name().to_ascii_lowercase())
        .collect()
}

/// Returns case-insensitive method names declared directly by a pending class.
fn class_method_name_set(class: &EvalClass) -> std::collections::HashSet<String> {
    class
        .methods()
        .iter()
        .map(|method| method.name().to_ascii_lowercase())
        .collect()
}

/// Appends trait constants while enforcing PHP-compatible same-name conflicts.
fn append_eval_trait_constants(
    trait_decl: &EvalTrait,
    class_constants: &[EvalClassConstant],
    trait_constants: &mut std::collections::HashMap<String, EvalClassConstant>,
    constants: &mut Vec<EvalClassConstant>,
) -> Result<(), EvalStatus> {
    for constant in trait_decl.constants() {
        if let Some(class_constant) = class_constants
            .iter()
            .find(|class_constant| class_constant.name() == constant.name())
        {
            validate_eval_trait_constant_compatibility(class_constant, constant)?;
            continue;
        }
        if let Some(existing) = trait_constants.get(constant.name()) {
            validate_eval_trait_constant_compatibility(existing, constant)?;
            continue;
        }
        let constant = constant
            .clone()
            .with_trait_origin(trait_decl.name().to_string());
        trait_constants.insert(constant.name().to_string(), constant.clone());
        constants.push(constant);
    }
    Ok(())
}

/// Validates that a same-name trait constant definition is compatible with PHP composition.
fn validate_eval_trait_constant_compatibility(
    existing: &EvalClassConstant,
    incoming: &EvalClassConstant,
) -> Result<(), EvalStatus> {
    if existing.visibility() == incoming.visibility()
        && existing.is_final() == incoming.is_final()
        && existing.value() == incoming.value()
    {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Appends trait properties while enforcing PHP-compatible same-name conflicts.
fn append_eval_trait_properties(
    trait_decl: &EvalTrait,
    class_properties: &[EvalClassProperty],
    trait_properties: &mut std::collections::HashMap<String, EvalClassProperty>,
    properties: &mut Vec<EvalClassProperty>,
) -> Result<(), EvalStatus> {
    for property in trait_decl.properties() {
        if let Some(class_property) = class_properties
            .iter()
            .find(|class_property| class_property.name() == property.name())
        {
            validate_eval_trait_property_compatibility(class_property, property)?;
            continue;
        }
        if let Some(existing) = trait_properties.get(property.name()) {
            let resolved = resolve_eval_trait_property_conflict(existing, property)?;
            if &resolved != existing {
                trait_properties.insert(property.name().to_string(), resolved.clone());
                if let Some(slot) = properties
                    .iter_mut()
                    .find(|candidate| candidate.name() == property.name())
                {
                    *slot = resolved;
                }
            }
            continue;
        }
        let property = property
            .clone()
            .with_trait_origin(trait_decl.name().to_string());
        trait_properties.insert(property.name().to_string(), property.clone());
        properties.push(property);
    }
    Ok(())
}

/// Validates that a same-name trait property definition is compatible with PHP composition.
fn validate_eval_trait_property_compatibility(
    existing: &EvalClassProperty,
    incoming: &EvalClassProperty,
) -> Result<(), EvalStatus> {
    resolve_eval_trait_property_conflict(existing, incoming).map(|_| ())
}

/// Resolves compatible same-name properties imported from classes and traits.
fn resolve_eval_trait_property_conflict(
    existing: &EvalClassProperty,
    incoming: &EvalClassProperty,
) -> Result<EvalClassProperty, EvalStatus> {
    if existing.is_abstract() && !incoming.is_abstract() {
        return class_property_satisfies_abstract_contract(incoming, existing)
            .then(|| incoming.clone())
            .ok_or(EvalStatus::RuntimeFatal);
    }
    if incoming.is_abstract() && !existing.is_abstract() {
        return class_property_satisfies_abstract_contract(existing, incoming)
            .then(|| existing.clone())
            .ok_or(EvalStatus::RuntimeFatal);
    }
    if existing.is_abstract() && incoming.is_abstract() {
        return eval_trait_abstract_properties_are_compatible(existing, incoming)
            .then(|| merge_abstract_property_contracts(existing, incoming))
            .ok_or(EvalStatus::RuntimeFatal);
    }
    if eval_trait_concrete_properties_are_compatible(existing, incoming) {
        Ok(existing.clone())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Returns whether two concrete same-name trait properties are identical enough to deduplicate.
fn eval_trait_concrete_properties_are_compatible(
    existing: &EvalClassProperty,
    incoming: &EvalClassProperty,
) -> bool {
    existing.visibility() == incoming.visibility()
        && existing.set_visibility() == incoming.set_visibility()
        && existing.is_static() == incoming.is_static()
        && existing.is_final() == incoming.is_final()
        && existing.is_readonly() == incoming.is_readonly()
        && existing.is_abstract() == incoming.is_abstract()
        && existing.has_get_hook() == incoming.has_get_hook()
        && existing.has_set_hook() == incoming.has_set_hook()
        && existing.requires_get_hook() == incoming.requires_get_hook()
        && existing.requires_set_hook() == incoming.requires_set_hook()
        && existing.is_virtual() == incoming.is_virtual()
        && existing.property_type() == incoming.property_type()
        && existing.set_hook_type() == incoming.set_hook_type()
        && existing.default() == incoming.default()
}

/// Returns whether two abstract trait property contracts can be merged.
fn eval_trait_abstract_properties_are_compatible(
    existing: &EvalClassProperty,
    incoming: &EvalClassProperty,
) -> bool {
    existing.visibility() == incoming.visibility()
        && existing.set_visibility() == incoming.set_visibility()
        && existing.is_static() == incoming.is_static()
        && existing.is_final() == incoming.is_final()
        && existing.is_readonly() == incoming.is_readonly()
        && existing.property_type() == incoming.property_type()
        && existing.set_hook_type() == incoming.set_hook_type()
        && existing.default() == incoming.default()
}

/// Appends trait methods unless the class provides a same-name method.
fn append_eval_trait_methods(
    trait_decl: &EvalTrait,
    trait_adaptations: &[EvalTraitAdaptation],
    class_method_names: &std::collections::HashSet<String>,
    trait_method_names: &mut std::collections::HashSet<String>,
    methods: &mut Vec<EvalClassMethod>,
) -> Result<(), EvalStatus> {
    for method in trait_decl.methods() {
        if trait_method_suppressed_by_insteadof(trait_decl.name(), method.name(), trait_adaptations)
        {
            continue;
        }
        let key = method.name().to_ascii_lowercase();
        if class_method_names.contains(&key) {
            continue;
        }
        let method = method
            .clone()
            .with_trait_origin(trait_decl.name().to_string());
        let method = apply_trait_visibility_adaptations(
            trait_decl.name(),
            &method,
            trait_adaptations,
        );
        if !trait_method_names.insert(key) {
            return Err(EvalStatus::RuntimeFatal);
        }
        methods.push(method);
    }
    append_eval_trait_method_aliases(
        trait_decl,
        trait_adaptations,
        class_method_names,
        trait_method_names,
        methods,
    )
}

/// Appends trait method aliases declared with `as`.
fn append_eval_trait_method_aliases(
    trait_decl: &EvalTrait,
    trait_adaptations: &[EvalTraitAdaptation],
    class_method_names: &std::collections::HashSet<String>,
    trait_method_names: &mut std::collections::HashSet<String>,
    methods: &mut Vec<EvalClassMethod>,
) -> Result<(), EvalStatus> {
    for adaptation in trait_adaptations {
        let EvalTraitAdaptation::Alias {
            trait_name,
            method,
            alias: Some(alias),
            visibility,
        } = adaptation
        else {
            continue;
        };
        if !trait_adaptation_target_matches(
            trait_name.as_deref(),
            method,
            trait_decl.name(),
            method,
        ) {
            continue;
        }
        let Some(source_method) = trait_decl
            .methods()
            .iter()
            .find(|trait_method| trait_method.name().eq_ignore_ascii_case(method))
        else {
            if trait_name.is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            continue;
        };
        let mut alias_method = source_method
            .clone()
            .with_trait_origin(trait_decl.name().to_string())
            .renamed(alias.clone());
        if let Some(visibility) = visibility {
            alias_method = alias_method.with_visibility_override(*visibility);
        }
        let key = alias_method.name().to_ascii_lowercase();
        if class_method_names.contains(&key) {
            continue;
        }
        if trait_method_names.contains(&key)
            && source_method.name().eq_ignore_ascii_case(alias)
            && alias_method.visibility() == source_method.visibility()
        {
            continue;
        }
        if !trait_method_names.insert(key) {
            return Err(EvalStatus::RuntimeFatal);
        }
        methods.push(alias_method);
    }
    Ok(())
}

/// Returns whether an `insteadof` adaptation suppresses this trait method import.
fn trait_method_suppressed_by_insteadof(
    trait_name: &str,
    method_name: &str,
    trait_adaptations: &[EvalTraitAdaptation],
) -> bool {
    trait_adaptations.iter().any(|adaptation| {
        let EvalTraitAdaptation::InsteadOf {
            trait_name: selected_trait,
            method,
            instead_of,
        } = adaptation
        else {
            return false;
        };
        method.eq_ignore_ascii_case(method_name)
            && instead_of
                .iter()
                .any(|suppressed| same_eval_class_name(suppressed, trait_name))
            && !selected_trait
                .as_deref()
                .is_some_and(|selected| same_eval_class_name(selected, trait_name))
    })
}

/// Applies visibility-only `as` adaptations to an imported trait method.
fn apply_trait_visibility_adaptations(
    trait_name: &str,
    method: &EvalClassMethod,
    trait_adaptations: &[EvalTraitAdaptation],
) -> EvalClassMethod {
    let mut method = method.clone();
    for adaptation in trait_adaptations {
        let EvalTraitAdaptation::Alias {
            trait_name: target_trait,
            method: target_method,
            alias: None,
            visibility: Some(visibility),
        } = adaptation
        else {
            continue;
        };
        if trait_adaptation_target_matches(
            target_trait.as_deref(),
            target_method,
            trait_name,
            method.name(),
        ) {
            method = method.with_visibility_override(*visibility);
        }
    }
    method
}

/// Returns whether an adaptation target selects one trait method.
fn trait_adaptation_target_matches(
    target_trait: Option<&str>,
    target_method: &str,
    trait_name: &str,
    method_name: &str,
) -> bool {
    target_method.eq_ignore_ascii_case(method_name)
        && target_trait.map_or(true, |target_trait| {
            same_eval_class_name(target_trait, trait_name)
        })
}

/// Rejects non-enum classes that implement PHP's native enum interfaces.
fn validate_eval_class_does_not_implement_enum_interfaces(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if pending_class_interface_names(class, context)
        .iter()
        .any(|interface| eval_builtin_enum_interface_name(interface))
    {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects eval classes and enums that directly implement PHP's Throwable contract.
fn validate_eval_class_does_not_implement_throwable_interfaces(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if pending_class_interface_names(class, context)
        .iter()
        .any(|interface| eval_builtin_throwable_interface_name(interface))
    {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Validates abstract/final modifiers on an eval-declared class and its methods.
fn validate_eval_class_modifiers(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if class.is_abstract() && class.is_final() {
        return Err(EvalStatus::RuntimeFatal);
    }
    validate_eval_class_attribute_targets(class.attributes())?;
    if class.is_readonly_class() && eval_class_has_allow_dynamic_properties_attribute(class) {
        return Err(EvalStatus::RuntimeFatal);
    }
    validate_eval_declared_constants(class.constants())?;
    for constant in class.constants() {
        validate_constant_parent_redeclaration(class, constant, context, values)?;
    }
    validate_eval_declared_properties(class, context)?;
    for property in class.properties() {
        validate_property_parent_redeclaration(class, property, context, values)?;
    }
    for method in class.methods() {
        validate_eval_method_attribute_targets(method.attributes())?;
        validate_eval_magic_method(method)?;
        if method.is_abstract() && method.is_final() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if method.is_abstract() && method.visibility() == EvalVisibility::Private {
            return Err(EvalStatus::RuntimeFatal);
        }
        if method.is_static() && method.name().eq_ignore_ascii_case("__construct") {
            return Err(EvalStatus::RuntimeFatal);
        }
        if method.is_abstract() && !class.is_abstract() {
            return Err(EvalStatus::RuntimeFatal);
        }
        validate_method_parent_override(class, method, context)?;
        validate_method_aot_parent_override(class, method, context, values)?;
        validate_eval_override_attribute(class, method, context, values)?;
    }
    Ok(())
}

/// Returns whether a class carries PHP's global `#[AllowDynamicProperties]` attribute.
fn eval_class_has_allow_dynamic_properties_attribute(class: &EvalClass) -> bool {
    eval_attributes_have_global_builtin_attribute(class.attributes(), "AllowDynamicProperties")
}

/// Bridge reflection flag for static generated/AOT members.
const EVAL_REFLECTION_MEMBER_FLAG_STATIC: u64 = 1;

/// Bridge reflection flag for public generated/AOT members.
const EVAL_REFLECTION_MEMBER_FLAG_PUBLIC: u64 = 2;

/// Bridge reflection flag for protected generated/AOT members.
const EVAL_REFLECTION_MEMBER_FLAG_PROTECTED: u64 = 4;

/// Bridge reflection flag for private generated/AOT members.
const EVAL_REFLECTION_MEMBER_FLAG_PRIVATE: u64 = 8;

/// Bridge reflection flag for final generated/AOT members.
const EVAL_REFLECTION_MEMBER_FLAG_FINAL: u64 = 16;

/// Bridge reflection flag for abstract generated/AOT members.
const EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT: u64 = 32;

/// Bridge reflection flag for readonly generated/AOT properties.
const EVAL_REFLECTION_MEMBER_FLAG_READONLY: u64 = 64;

/// Bridge reflection flag for protected-set generated/AOT properties.
const EVAL_REFLECTION_MEMBER_FLAG_PROTECTED_SET: u64 = 2048;

/// Bridge reflection flag for private-set generated/AOT properties.
const EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET: u64 = 4096;

/// Method requirement discovered from generated/AOT interface metadata.
struct EvalAotInterfaceMethodRequirement {
    owner: String,
    name: String,
    is_static: bool,
    signature: Option<EvalInterfaceMethod>,
}

/// Abstract method requirement discovered from generated/AOT parent metadata.
struct EvalAotAbstractMethodRequirement {
    owner: String,
    is_static: bool,
    visibility: EvalVisibility,
    signature: Option<EvalInterfaceMethod>,
}

/// Abstract property requirement discovered from generated/AOT parent metadata.
struct EvalAotAbstractPropertyRequirement {
    owner: String,
    property: EvalClassProperty,
}

/// Rejects builtin attributes that cannot target an eval-declared class.
fn validate_eval_class_attribute_targets(
    attributes: &[EvalAttribute],
) -> Result<(), EvalStatus> {
    if eval_attributes_have_global_builtin_attribute(attributes, "Override") {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects builtin attributes that cannot target eval-declared interfaces.
fn validate_eval_interface_attribute_targets(
    interface: &EvalInterface,
) -> Result<(), EvalStatus> {
    validate_eval_non_class_attribute_targets(interface.attributes())?;
    for property in interface.properties() {
        validate_eval_non_method_attribute_targets(property.attributes())?;
    }
    for method in interface.methods() {
        validate_eval_method_attribute_targets(method.attributes())?;
    }
    Ok(())
}

/// Validates PHP's global `#[Override]` marker on eval-declared interface methods.
fn validate_eval_interface_override_attributes(
    interface: &EvalInterface,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let parent_requirements = eval_interface_parent_method_requirements(interface, context);
    for method in interface.methods() {
        if !eval_interface_method_has_global_builtin_attribute(method, "Override") {
            continue;
        }
        if parent_requirements
            .iter()
            .any(|(_, requirement)| eval_interface_method_matches_requirement(method, requirement))
        {
            continue;
        }
        if eval_aot_interface_parent_method_matches(interface, method, values)? {
            continue;
        }
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Returns method requirements inherited by one eval interface declaration.
fn eval_interface_parent_method_requirements(
    interface: &EvalInterface,
    context: &ElephcEvalContext,
) -> Vec<(String, EvalInterfaceMethod)> {
    let mut requirements = Vec::new();
    for parent in interface.parents() {
        if context.has_interface(parent) {
            requirements.extend(context.interface_method_requirements_with_owners(parent));
        }
        requirements.extend(builtin_interface_method_requirements(parent));
    }
    requirements
}

/// Returns whether a generated/AOT parent interface exposes a matching method.
fn eval_aot_interface_parent_method_matches(
    interface: &EvalInterface,
    method: &EvalInterfaceMethod,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for parent in interface.parents() {
        if !values.interface_exists(parent)? {
            continue;
        }
        let parent = parent.trim_start_matches('\\');
        if let Some(flags) = values.reflection_method_flags(parent, method.name())? {
            let parent_method_is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
            return Ok(parent_method_is_static == method.is_static());
        }
    }
    Ok(false)
}

/// Returns whether an interface method matches one inherited requirement signature kind.
fn eval_interface_method_matches_requirement(
    method: &EvalInterfaceMethod,
    requirement: &EvalInterfaceMethod,
) -> bool {
    requirement.name().eq_ignore_ascii_case(method.name())
        && requirement.is_static() == method.is_static()
}

/// Rejects builtin attributes that cannot target eval-declared traits.
fn validate_eval_trait_attribute_targets(trait_decl: &EvalTrait) -> Result<(), EvalStatus> {
    validate_eval_non_class_attribute_targets(trait_decl.attributes())?;
    for property in trait_decl.properties() {
        validate_eval_non_method_attribute_targets(property.attributes())?;
    }
    for method in trait_decl.methods() {
        validate_eval_method_attribute_targets(method.attributes())?;
    }
    Ok(())
}

/// Rejects builtin attributes that cannot target eval-declared enums.
fn validate_eval_enum_attribute_targets(enum_decl: &EvalEnum) -> Result<(), EvalStatus> {
    validate_eval_non_class_attribute_targets(enum_decl.attributes())
}

/// Rejects class-only or method-only builtin attributes on non-class declarations.
fn validate_eval_non_class_attribute_targets(
    attributes: &[EvalAttribute],
) -> Result<(), EvalStatus> {
    if eval_attributes_have_global_builtin_attribute(attributes, "AllowDynamicProperties")
        || eval_attributes_have_global_builtin_attribute(attributes, "Override")
    {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects class-only or method-only builtin attributes on non-method members.
fn validate_eval_non_method_attribute_targets(
    attributes: &[EvalAttribute],
) -> Result<(), EvalStatus> {
    if eval_attributes_have_global_builtin_attribute(attributes, "AllowDynamicProperties")
        || eval_attributes_have_global_builtin_attribute(attributes, "Override")
    {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects class-only builtin attributes on method declarations.
fn validate_eval_method_attribute_targets(
    attributes: &[EvalAttribute],
) -> Result<(), EvalStatus> {
    if eval_attributes_have_global_builtin_attribute(attributes, "AllowDynamicProperties") {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Returns whether the attribute list contains one global builtin attribute.
fn eval_attributes_have_global_builtin_attribute(
    attributes: &[EvalAttribute],
    builtin: &str,
) -> bool {
    attributes
        .iter()
        .any(|attribute| eval_attribute_is_global_builtin(attribute, builtin))
}

/// Returns whether one attribute names a global builtin attribute class.
fn eval_attribute_is_global_builtin(attribute: &EvalAttribute, builtin: &str) -> bool {
    attribute
        .name()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(builtin)
}

/// Validates PHP's global `#[Override]` marker on one eval-declared method.
fn validate_eval_override_attribute(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !eval_method_has_global_builtin_attribute(method, "Override") {
        return Ok(());
    }
    if eval_method_overrides_parent(class, method, context)
        || eval_method_overrides_aot_parent(class, method, context, values)?
        || eval_method_implements_interface(class, method, context, values)?
    {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Returns whether a method has a global builtin marker attribute.
fn eval_method_has_global_builtin_attribute(method: &EvalClassMethod, builtin: &str) -> bool {
    eval_attributes_have_global_builtin_attribute(method.attributes(), builtin)
}

/// Returns whether an interface method has a global builtin marker attribute.
fn eval_interface_method_has_global_builtin_attribute(
    method: &EvalInterfaceMethod,
    builtin: &str,
) -> bool {
    eval_attributes_have_global_builtin_attribute(method.attributes(), builtin)
}

/// Returns whether one method overrides a non-private parent method.
fn eval_method_overrides_parent(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &ElephcEvalContext,
) -> bool {
    class
        .parent()
        .and_then(|parent| context.class_method(parent, method.name()))
        .is_some_and(|(_, parent_method)| {
            parent_method.visibility() != EvalVisibility::Private
                && parent_method.is_static() == method.is_static()
        })
}

/// Returns whether one method overrides a visible generated/AOT parent method.
fn eval_method_overrides_aot_parent(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(parent) = pending_class_native_parent_name(class, context) else {
        return Ok(false);
    };
    if !values.class_exists(&parent)? {
        return Ok(false);
    }
    let Some(flags) = values.reflection_method_flags(&parent, method.name())? else {
        return Ok(false);
    };
    let parent_method_is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    let parent_method_is_private = flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0;
    Ok(!parent_method_is_private && parent_method_is_static == method.is_static())
}

/// Returns the nearest generated/AOT parent for a class not yet registered in context.
fn pending_class_native_parent_name(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Option<String> {
    let mut current = class.parent()?.to_string();
    let mut seen = std::collections::HashSet::new();
    loop {
        let resolved = context
            .resolve_class_name(&current)
            .unwrap_or_else(|| current.trim_start_matches('\\').to_string());
        if !seen.insert(resolved.to_ascii_lowercase()) {
            return None;
        }
        let Some(parent_class) = context.class(&resolved) else {
            return Some(resolved.trim_start_matches('\\').to_string());
        };
        current = parent_class.parent()?.to_string();
    }
}

/// Returns whether one method implements a direct or inherited interface method.
fn eval_method_implements_interface(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if pending_class_interface_names(class, context)
        .iter()
        .filter(|interface| context.has_interface(interface))
        .any(|interface| {
            context
                .interface_method_requirements_with_owners(interface)
                .into_iter()
                .any(|(_, requirement)| {
                    requirement.name().eq_ignore_ascii_case(method.name())
                        && requirement.is_static() == method.is_static()
                })
        })
    {
        return Ok(true);
    }
    Ok(pending_class_aot_interface_method_requirements(class, context, values)?
        .iter()
        .any(|requirement| {
            requirement.name.eq_ignore_ascii_case(method.name())
                && requirement.is_static == method.is_static()
        }))
}

/// Validates PHP magic-method contracts for one eval class-like method list.
fn validate_eval_magic_methods(methods: &[EvalClassMethod]) -> Result<(), EvalStatus> {
    for method in methods {
        validate_eval_magic_method(method)?;
    }
    Ok(())
}

/// Validates staticness, visibility, arity, and declared return type for one eval magic method.
fn validate_eval_magic_method(method: &EvalClassMethod) -> Result<(), EvalStatus> {
    let name = method.name().to_ascii_lowercase();
    if validated_eval_magic_method_rejects_by_ref_params(&name) {
        validate_magic_no_by_ref_params(method)?;
    }
    match name.as_str() {
        "__tostring" => {
            validate_magic_non_static(method)?;
            validate_magic_public(method)?;
            validate_magic_arity(method, 0)?;
            validate_magic_declared_return_type(method, MagicReturnType::String)?;
        }
        "__get" | "__isset" | "__unset" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 1)?;
            validate_magic_declared_param_type(method, 0, MagicParamType::String)?;
            if method.name().eq_ignore_ascii_case("__isset") {
                validate_magic_declared_return_type(method, MagicReturnType::Bool)?;
            } else if method.name().eq_ignore_ascii_case("__unset") {
                validate_magic_declared_return_type(method, MagicReturnType::Void)?;
            }
        }
        "__set" | "__call" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 2)?;
            validate_magic_declared_param_type(method, 0, MagicParamType::String)?;
            if method.name().eq_ignore_ascii_case("__set") {
                validate_magic_declared_return_type(method, MagicReturnType::Void)?;
            } else {
                validate_magic_declared_param_type(method, 1, MagicParamType::Array)?;
            }
        }
        "__callstatic" => {
            validate_magic_static(method)?;
            validate_magic_arity(method, 2)?;
            validate_magic_declared_param_type(method, 0, MagicParamType::String)?;
            validate_magic_declared_param_type(method, 1, MagicParamType::Array)?;
        }
        "__sleep" | "__serialize" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 0)?;
            validate_magic_declared_return_type(method, MagicReturnType::Array)?;
        }
        "__wakeup" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 0)?;
            validate_magic_declared_return_type(method, MagicReturnType::Void)?;
        }
        "__unserialize" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 1)?;
            validate_magic_declared_param_type(method, 0, MagicParamType::Array)?;
            validate_magic_declared_return_type(method, MagicReturnType::Void)?;
        }
        "__debuginfo" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 0)?;
            validate_magic_declared_return_type(method, MagicReturnType::NullableArray)?;
        }
        "__set_state" => {
            validate_magic_static(method)?;
            validate_magic_arity(method, 1)?;
            validate_magic_declared_param_type(method, 0, MagicParamType::Array)?;
        }
        "__invoke" => {
            validate_magic_non_static(method)?;
        }
        "__clone" | "__destruct" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 0)?;
            if method.name().eq_ignore_ascii_case("__clone") {
                validate_magic_declared_return_type(method, MagicReturnType::Void)?;
            } else {
                validate_magic_no_declared_return_type(method)?;
            }
        }
        "__construct" => {
            if method.is_static() {
                return Err(EvalStatus::RuntimeFatal);
            }
            validate_magic_no_declared_return_type(method)?;
        }
        _ => {}
    }
    Ok(())
}

/// Returns whether PHP rejects by-reference parameters for this magic method.
fn validated_eval_magic_method_rejects_by_ref_params(name: &str) -> bool {
    is_validated_eval_magic_method(name) && !matches!(name, "__construct" | "__invoke")
}

/// Returns whether eval knows PHP declaration-time rules for this magic method.
fn is_validated_eval_magic_method(name: &str) -> bool {
    matches!(
        name,
        "__tostring"
            | "__get"
            | "__isset"
            | "__unset"
            | "__set"
            | "__call"
            | "__callstatic"
            | "__sleep"
            | "__serialize"
            | "__wakeup"
            | "__unserialize"
            | "__debuginfo"
            | "__set_state"
            | "__invoke"
            | "__clone"
            | "__destruct"
            | "__construct"
    )
}

/// Magic method return types that eval can validate from retained declarations.
#[derive(Clone, Copy)]
enum MagicReturnType {
    Array,
    Bool,
    NullableArray,
    String,
    Void,
}

/// Magic method parameter types that eval can validate from retained declarations.
#[derive(Clone, Copy)]
enum MagicParamType {
    Array,
    String,
}

/// Rejects static declarations for magic methods that must be instance methods.
fn validate_magic_non_static(method: &EvalClassMethod) -> Result<(), EvalStatus> {
    if method.is_static() {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects instance declarations for magic methods that must be static methods.
fn validate_magic_static(method: &EvalClassMethod) -> Result<(), EvalStatus> {
    if method.is_static() {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Rejects non-public declarations for public-only PHP magic methods.
fn validate_magic_public(method: &EvalClassMethod) -> Result<(), EvalStatus> {
    if method.visibility() == EvalVisibility::Public {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Rejects magic methods whose arity differs from PHP's required shape.
fn validate_magic_arity(method: &EvalClassMethod, expected: usize) -> Result<(), EvalStatus> {
    let has_variadic = method
        .parameter_is_variadic()
        .iter()
        .any(|is_variadic| *is_variadic);
    if method.params().len() == expected && !has_variadic {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Rejects by-reference parameters on PHP magic methods.
fn validate_magic_no_by_ref_params(method: &EvalClassMethod) -> Result<(), EvalStatus> {
    if method
        .parameter_is_by_ref()
        .iter()
        .any(|is_by_ref| *is_by_ref)
    {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects incompatible explicit parameter types on PHP magic methods.
fn validate_magic_declared_param_type(
    method: &EvalClassMethod,
    position: usize,
    expected: MagicParamType,
) -> Result<(), EvalStatus> {
    let Some(Some(parameter_type)) = method.parameter_types().get(position) else {
        return Ok(());
    };
    if magic_param_type_matches(parameter_type, expected) {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Returns whether one retained eval parameter type is exactly the expected magic atom.
fn magic_param_type_matches(
    parameter_type: &EvalParameterType,
    expected: MagicParamType,
) -> bool {
    if parameter_type.allows_null() || parameter_type.is_intersection() {
        return false;
    }
    let [variant] = parameter_type.variants() else {
        return false;
    };
    matches!(
        (expected, variant),
        (MagicParamType::Array, EvalParameterTypeVariant::Array)
            | (MagicParamType::String, EvalParameterTypeVariant::String)
    )
}

/// Rejects PHP magic methods that cannot declare any return type.
fn validate_magic_no_declared_return_type(method: &EvalClassMethod) -> Result<(), EvalStatus> {
    if method.return_type().is_some() {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects incompatible explicit return types on PHP magic methods.
fn validate_magic_declared_return_type(
    method: &EvalClassMethod,
    expected: MagicReturnType,
) -> Result<(), EvalStatus> {
    method.return_type().map_or(Ok(()), |return_type| {
        if magic_return_type_matches(return_type, expected) {
            Ok(())
        } else {
            Err(EvalStatus::RuntimeFatal)
        }
    })
}

/// Returns whether one retained eval return type is exactly the expected magic return atom.
fn magic_return_type_matches(
    return_type: &EvalParameterType,
    expected: MagicReturnType,
) -> bool {
    if return_type.is_intersection() {
        return false;
    }
    if return_type.allows_null() && !matches!(expected, MagicReturnType::NullableArray) {
        return false;
    }
    let [variant] = return_type.variants() else {
        return false;
    };
    matches!(
        (expected, variant),
        (MagicReturnType::Array, EvalParameterTypeVariant::Array)
            | (MagicReturnType::Bool, EvalParameterTypeVariant::Bool)
            | (MagicReturnType::NullableArray, EvalParameterTypeVariant::Array)
            | (MagicReturnType::String, EvalParameterTypeVariant::String)
            | (MagicReturnType::Void, EvalParameterTypeVariant::Void)
    )
}

/// Validates property declarations that can be checked before class registration.
fn validate_eval_declared_properties(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    let mut names = std::collections::HashSet::new();
    for property in class.properties() {
        validate_eval_non_method_attribute_targets(property.attributes())?;
        if !names.insert(property.name().to_string()) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if property.is_abstract()
            && (!class.is_abstract()
                || property.is_static()
                || property.is_final()
                || property.is_readonly()
                || property.default().is_some()
                || (!property.requires_get_hook() && !property.requires_set_hook()))
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        if property.is_static() && property.is_readonly() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if let Some(set_visibility) = property.set_visibility() {
            if property.is_static() || property.property_type().is_none() {
                return Err(EvalStatus::RuntimeFatal);
            }
            if property_visibility_rank(set_visibility)
                > property_visibility_rank(property.visibility())
            {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        if property.is_final() && property.visibility() == EvalVisibility::Private {
            return Err(EvalStatus::RuntimeFatal);
        }
        if property.is_readonly() && property.property_type().is_none() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if property.is_readonly() && property.default().is_some() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if (property.has_get_hook() || property.has_set_hook())
            && (property.is_static() || property.is_readonly() || property.default().is_some())
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        validate_eval_property_set_hook_parameter_type(class, property, context)?;
    }
    Ok(())
}

/// Validates that an explicit set-hook parameter type can accept every property value.
fn validate_eval_property_set_hook_parameter_type(
    class: &EvalClass,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    let Some(set_hook_type) = property.set_hook_type() else {
        return Ok(());
    };
    let Some(property_type) = property.property_type() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let set_hook_types = vec![Some(set_hook_type.clone())];
    let property_types = vec![Some(property_type.clone())];
    method_parameter_type_signature_accepts(
        &set_hook_types,
        &[],
        class.name(),
        &property_types,
        &[],
        class.name(),
        1,
        Some(class),
        context,
    )
    .then_some(())
    .ok_or(EvalStatus::RuntimeFatal)
}

/// Validates one property declaration against inherited eval property metadata.
fn validate_property_parent_redeclaration(
    class: &EvalClass,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(parent) = class.parent() else {
        return Ok(());
    };
    if let Some((parent_declaring_class, parent_property)) =
        context.class_property(parent, property.name())
    {
        if parent_property.visibility() == EvalVisibility::Private {
            return Ok(());
        }
        if parent_property.is_final()
            || parent_property.set_visibility() == Some(EvalVisibility::Private)
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        if parent_property.is_static() != property.is_static()
            || (parent_property.is_readonly() && !property.is_readonly())
            || property_visibility_rank(property.visibility())
                < property_visibility_rank(parent_property.visibility())
            || property_visibility_rank(property.write_visibility())
                < property_visibility_rank(parent_property.write_visibility())
            || !property_type_signature_matches(
                property.property_type(),
                class.name(),
                parent_property.property_type(),
                &parent_declaring_class,
                Some(class),
                context,
            )
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(());
    }
    validate_property_aot_parent_redeclaration(parent, class, property, context, values)
}

/// Validates one property declaration against inherited generated/AOT property metadata.
fn validate_property_aot_parent_redeclaration(
    parent: &str,
    class: &EvalClass,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if context.has_class(parent) || !values.class_exists(parent)? {
        return Ok(());
    }
    let parent = parent.trim_start_matches('\\');
    let Some(flags) = values.reflection_property_flags(parent, property.name())? else {
        return Ok(());
    };
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        return Ok(());
    }
    if flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0
        || flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET != 0
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let parent_is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    let parent_visibility = eval_aot_property_visibility(flags);
    let parent_write_visibility = eval_aot_property_write_visibility(flags, parent_visibility);
    let parent_is_readonly = flags & EVAL_REFLECTION_MEMBER_FLAG_READONLY != 0;
    let parent_declaring_class =
        eval_aot_property_declaring_class(parent, property.name(), values)?;
    let parent_property_type = context
        .native_property_type(&parent_declaring_class, property.name())
        .or_else(|| context.native_property_type(parent, property.name()));
    if parent_is_static != property.is_static()
        || (parent_is_readonly && !property.is_readonly())
        || property_visibility_rank(property.visibility())
            < property_visibility_rank(parent_visibility)
        || property_visibility_rank(property.write_visibility())
            < property_visibility_rank(parent_write_visibility)
        || !property_type_signature_matches(
            property.property_type(),
            class.name(),
            parent_property_type.as_ref(),
            &parent_declaring_class,
            Some(class),
            context,
        )
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Returns the eval visibility represented by generated/AOT property reflection flags.
fn eval_aot_property_visibility(flags: u64) -> EvalVisibility {
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    }
}

/// Returns the eval write visibility represented by generated/AOT property flags.
fn eval_aot_property_write_visibility(
    flags: u64,
    read_visibility: EvalVisibility,
) -> EvalVisibility {
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED_SET != 0 {
        EvalVisibility::Protected
    } else {
        read_visibility
    }
}

/// Returns the generated/AOT declaring class for one reflected property.
fn eval_aot_property_declaring_class(
    class_name: &str,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    values
        .reflection_property_declaring_class(class_name, property_name)
        .map(|declaring_class| declaring_class.unwrap_or_else(|| class_name.to_string()))
}

/// Validates constant declarations that can be checked before registration.
fn validate_eval_declared_constants(constants: &[EvalClassConstant]) -> Result<(), EvalStatus> {
    let mut names = std::collections::HashSet::new();
    for constant in constants {
        validate_eval_non_method_attribute_targets(constant.attributes())?;
        if !names.insert(constant.name().to_string()) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if constant.is_final() && constant.visibility() == EvalVisibility::Private {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates declarations that are specific to PHP interface constants.
fn validate_eval_interface_constants(constants: &[EvalClassConstant]) -> Result<(), EvalStatus> {
    for constant in constants {
        if constant.visibility() != EvalVisibility::Public {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates interface constants against inherited parent-interface constants.
fn validate_interface_constant_parent_redeclarations(
    interface: &EvalInterface,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for constant in interface.constants() {
        for parent in interface.parents() {
            if let Some((_, parent_constant)) = context.interface_constant(parent, constant.name())
            {
                if parent_constant.is_final() {
                    return Err(EvalStatus::RuntimeFatal);
                }
            }
            validate_aot_interface_constant_redeclaration(parent, constant, values)?;
        }
    }
    Ok(())
}

/// Validates one constant declaration against inherited eval constant metadata.
fn validate_constant_parent_redeclaration(
    class: &EvalClass,
    constant: &EvalClassConstant,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(parent) = class.parent() {
        if let Some((_, parent_constant)) = context.class_constant(parent, constant.name()) {
            if parent_constant.visibility() != EvalVisibility::Private
                && (parent_constant.is_final()
                    || constant_visibility_rank(constant.visibility())
                        < constant_visibility_rank(parent_constant.visibility()))
            {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        validate_aot_class_constant_redeclaration(parent, constant, values)?;
    }
    for interface in pending_class_interface_names(class, context) {
        if let Some((_, interface_constant)) =
            context.interface_constant(&interface, constant.name())
        {
            if interface_constant.is_final()
                || constant_visibility_rank(constant.visibility())
                    < constant_visibility_rank(interface_constant.visibility())
            {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        validate_aot_interface_constant_redeclaration(&interface, constant, values)?;
    }
    Ok(())
}

/// Validates a class constant redeclaration against a generated/AOT parent class constant.
fn validate_aot_class_constant_redeclaration(
    parent: &str,
    constant: &EvalClassConstant,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !values.class_exists(parent)? {
        return Ok(());
    }
    validate_aot_constant_redeclaration(parent, constant, false, values)
}

/// Validates a class/interface constant redeclaration against a generated/AOT interface constant.
fn validate_aot_interface_constant_redeclaration(
    interface: &str,
    constant: &EvalClassConstant,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !values.interface_exists(interface)? {
        return Ok(());
    }
    validate_aot_constant_redeclaration(interface, constant, true, values)
}

/// Applies PHP redeclaration rules to one generated/AOT constant metadata row.
fn validate_aot_constant_redeclaration(
    class_like: &str,
    constant: &EvalClassConstant,
    interface_context: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class_like = class_like.trim_start_matches('\\');
    let Some(flags) = values.reflection_constant_flags(class_like, constant.name())? else {
        return Ok(());
    };
    let inherited_visibility = eval_aot_constant_visibility(flags);
    if !interface_context && inherited_visibility == EvalVisibility::Private {
        return Ok(());
    }
    if flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0
        || constant_visibility_rank(constant.visibility())
            < constant_visibility_rank(inherited_visibility)
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Returns the eval visibility represented by generated/AOT constant reflection flags.
fn eval_aot_constant_visibility(flags: u64) -> EvalVisibility {
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    }
}

/// Returns a comparable rank where larger means less restrictive constant visibility.
fn constant_visibility_rank(visibility: EvalVisibility) -> u8 {
    match visibility {
        EvalVisibility::Private => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Public => 3,
    }
}

/// Validates declared or inherited class members that already cover eval interface contracts.
fn validate_declared_class_interface_members(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for interface in pending_class_interface_names(class, context) {
        if !context.has_interface(&interface) {
            continue;
        }
        validate_declared_class_interface_methods(class, &interface, context)?;
        validate_declared_class_interface_properties(class, &interface, context)?;
    }
    Ok(())
}

/// Validates declared class methods against PHP builtin runtime interface contracts.
fn validate_declared_class_builtin_interface_members(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for (requirement_owner, requirement) in
        pending_class_builtin_interface_method_requirements(class, context)
    {
        let Some((declaring_class, method)) =
            pending_class_method(class, requirement.name(), context)
        else {
            continue;
        };
        if method.visibility() != EvalVisibility::Public
            || method.is_static() != requirement.is_static()
            || !class_method_satisfies_builtin_interface_signature(
                &method,
                &declaring_class,
                &requirement,
                &requirement_owner,
                Some(class),
                context,
            )
        {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates declared class methods against generated/AOT interface contracts.
fn validate_declared_class_aot_interface_members(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for requirement in pending_class_aot_interface_method_requirements(class, context, values)? {
        let Some((declaring_class, method)) = pending_class_method(class, &requirement.name, context)
        else {
            continue;
        };
        if !class_method_satisfies_aot_interface_requirement(
            &method,
            &declaring_class,
            &requirement,
            Some(class),
            context,
            false,
        ) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    for (requirement_owner, requirement) in
        pending_class_aot_interface_property_requirements(class, context, values)?
    {
        let Some((declaring_class, property)) =
            pending_class_property_with_owner(class, requirement.name(), context)
        else {
            continue;
        };
        if !class_property_can_cover_interface_contract(
            &property,
            &declaring_class,
            &requirement,
            &requirement_owner,
            Some(class),
            context,
        ) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates class methods present for an eval interface, even on abstract classes.
fn validate_declared_class_interface_methods(
    class: &EvalClass,
    interface_name: &str,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for (requirement_owner, requirement) in
        context.interface_method_requirements_with_owners(interface_name)
    {
        let Some((declaring_class, method)) =
            pending_class_method(class, requirement.name(), context)
        else {
            continue;
        };
        if method.visibility() != EvalVisibility::Public
            || method.is_static() != requirement.is_static()
            || !class_method_satisfies_interface_signature(
                &method,
                &declaring_class,
                &requirement,
                &requirement_owner,
                Some(class),
                context,
            )
        {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates class properties present for an eval interface, even on abstract classes.
fn validate_declared_class_interface_properties(
    class: &EvalClass,
    interface_name: &str,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for (requirement_owner, requirement) in
        context.interface_property_requirements_with_owners(interface_name)
    {
        let Some((declaring_class, property)) =
            pending_class_property_with_owner(class, requirement.name(), context)
        else {
            continue;
        };
        if !class_property_can_cover_interface_contract(
            &property,
            &declaring_class,
            &requirement,
            &requirement_owner,
            Some(class),
            context,
        ) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns a method from a pending class or one of its already registered parents.
fn pending_class_method(
    class: &EvalClass,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassMethod)> {
    if let Some(method) = class.method(method_name) {
        return Some((class.name().to_string(), method.clone()));
    }
    class
        .parent()
        .and_then(|parent| context.class_method(parent, method_name))
}

/// Validates one method declaration against inherited eval method metadata.
fn validate_method_parent_override(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    let Some(parent) = class.parent() else {
        return Ok(());
    };
    let Some((parent_declaring_class, parent_method)) = context.class_method(parent, method.name())
    else {
        return Ok(());
    };
    if parent_method.visibility() == EvalVisibility::Private {
        return Ok(());
    }
    if parent_method.is_static() != method.is_static() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if method_visibility_rank(method.visibility())
        < method_visibility_rank(parent_method.visibility())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    if parent_method.is_final() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if method.is_abstract() && !parent_method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if !class_method_signature_accepts(
        method,
        class.name(),
        &parent_method,
        &parent_declaring_class,
        Some(class),
        context,
    ) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Validates one method declaration against inherited generated/AOT method metadata.
fn validate_method_aot_parent_override(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(parent) = pending_class_native_parent_name(class, context) else {
        return Ok(());
    };
    if !values.class_exists(&parent)? {
        return Ok(());
    }
    let Some(flags) = values.reflection_method_flags(&parent, method.name())? else {
        return Ok(());
    };
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        return Ok(());
    }
    let parent_is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    if parent_is_static != method.is_static() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let parent_visibility = eval_aot_method_visibility(flags);
    if method_visibility_rank(method.visibility()) < method_visibility_rank(parent_visibility) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    if method.is_abstract() && flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(required) = eval_aot_method_signature_requirement(
        &parent,
        method.name(),
        parent_is_static,
        context,
        values,
    )? else {
        return Ok(());
    };
    if !class_method_satisfies_interface_signature(
        method,
        class.name(),
        &required,
        &eval_aot_method_declaring_class(&parent, method.name(), values)?,
        Some(class),
        context,
    ) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Returns the eval visibility represented by generated/AOT reflection flags.
fn eval_aot_method_visibility(flags: u64) -> EvalVisibility {
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PUBLIC != 0 {
        EvalVisibility::Public
    } else {
        EvalVisibility::Public
    }
}

/// Returns the generated/AOT declaring class for one reflected method.
fn eval_aot_method_declaring_class(
    class_name: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    values
        .reflection_method_declaring_class(class_name, method_name)
        .map(|declaring_class| declaring_class.unwrap_or_else(|| class_name.to_string()))
}

/// Returns a generated/AOT parent method signature as an eval method requirement.
fn eval_aot_method_signature_requirement(
    class_name: &str,
    method_name: &str,
    is_static: bool,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalInterfaceMethod>, EvalStatus> {
    let declaring_class = eval_aot_method_declaring_class(class_name, method_name, values)?;
    let signature = if is_static {
        context.native_static_method_signature(&declaring_class, method_name)
    } else {
        context.native_method_signature(&declaring_class, method_name)
    };
    Ok(signature.map(|signature| {
        eval_native_signature_interface_method(method_name, is_static, &signature)
    }))
}

/// Returns whether one eval class method can accept every call accepted by its parent method.
fn class_method_signature_accepts(
    method: &EvalClassMethod,
    method_owner: &str,
    required: &EvalClassMethod,
    required_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    method_signature_accepts(
        method.params().len(),
        method.parameter_defaults(),
        method.parameter_is_by_ref(),
        method.parameter_is_variadic(),
        required.params().len(),
        required.parameter_defaults(),
        required.parameter_is_by_ref(),
        required.parameter_is_variadic(),
    ) && method_parameter_type_signature_accepts(
        method.parameter_types(),
        method.parameter_is_variadic(),
        method_owner,
        required.parameter_types(),
        required.parameter_is_variadic(),
        required_owner,
        required.params().len(),
        pending_class,
        context,
    ) && method_return_type_signature_accepts(
        method.return_type(),
        method_owner,
        required.return_type(),
        required_owner,
        pending_class,
        context,
    )
}

/// Returns a comparable rank where larger means less restrictive visibility.
fn method_visibility_rank(visibility: EvalVisibility) -> u8 {
    match visibility {
        EvalVisibility::Private => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Public => 3,
    }
}

/// Validates that a concrete class has satisfied inherited abstract and interface requirements.
fn validate_concrete_class_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if !pending_class_abstract_method_requirements(class, context).is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if !pending_class_abstract_property_requirements(class, context)?.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for interface in pending_class_interface_names(class, context) {
        if context.has_interface(&interface) {
            validate_class_implements_eval_interface(class, &interface, context)?;
        }
    }
    Ok(())
}

/// Validates concrete class methods required by PHP builtin runtime interfaces.
fn validate_concrete_class_builtin_interface_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for (requirement_owner, requirement) in
        pending_class_builtin_interface_method_requirements(class, context)
    {
        if !class_has_builtin_interface_method(class, &requirement_owner, &requirement, context) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates concrete class methods required by generated/AOT abstract parents.
fn validate_concrete_class_aot_parent_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !pending_class_aot_parent_abstract_method_requirements(class, context, values)?.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if !pending_class_aot_parent_abstract_property_requirements(class, context, values)?.is_empty()
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Validates concrete class methods required by generated/AOT runtime interfaces.
fn validate_concrete_class_aot_interface_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for requirement in pending_class_aot_interface_method_requirements(class, context, values)? {
        if !class_has_aot_interface_method(class, &requirement, context) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    for (requirement_owner, requirement) in
        pending_class_aot_interface_property_requirements(class, context, values)?
    {
        if !class_has_interface_property(class, &requirement_owner, &requirement, context) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns inherited abstract methods that the pending class has not concretized.
fn pending_class_abstract_method_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Vec<EvalClassMethod> {
    let mut requirements = std::collections::HashMap::new();
    if let Some(parent) = class.parent() {
        collect_class_abstract_method_requirements(parent, context, &mut requirements);
    }
    apply_class_abstract_method_requirements(class, &mut requirements);
    requirements.into_values().collect()
}

/// Returns inherited abstract properties that the pending class has not concretized.
fn pending_class_abstract_property_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<Vec<EvalClassProperty>, EvalStatus> {
    let mut requirements = std::collections::HashMap::new();
    if let Some(parent) = class.parent() {
        collect_class_abstract_property_requirements(parent, context, &mut requirements)?;
    }
    apply_class_abstract_property_requirements(class, &mut requirements)?;
    Ok(requirements.into_values().collect())
}

/// Returns generated/AOT abstract parent methods the pending class has not concretized.
fn pending_class_aot_parent_abstract_method_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalAotAbstractMethodRequirement>, EvalStatus> {
    let mut requirements = std::collections::HashMap::new();
    if let Some(parent) = class.parent() {
        collect_aot_parent_abstract_method_requirements(
            parent,
            context,
            values,
            &mut requirements,
        )?;
    }
    apply_class_aot_parent_abstract_method_requirements(class, context, &mut requirements)?;
    Ok(requirements.into_values().collect())
}

/// Returns generated/AOT abstract parent properties the pending class has not concretized.
fn pending_class_aot_parent_abstract_property_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalAotAbstractPropertyRequirement>, EvalStatus> {
    let mut requirements = std::collections::HashMap::new();
    if let Some(parent) = class.parent() {
        collect_aot_parent_abstract_property_requirements(
            parent,
            context,
            values,
            &mut requirements,
        )?;
    }
    apply_class_aot_parent_abstract_property_requirements(class, context, &mut requirements)?;
    Ok(requirements.into_values().collect())
}

/// Collects abstract method requirements from one declared eval class ancestry chain.
fn collect_class_abstract_method_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    requirements: &mut std::collections::HashMap<String, EvalClassMethod>,
) {
    let Some(class) = context.class(class_name) else {
        return;
    };
    if let Some(parent) = class.parent() {
        collect_class_abstract_method_requirements(parent, context, requirements);
    }
    apply_class_abstract_method_requirements(class, requirements);
}

/// Collects generated/AOT abstract method requirements through eval and AOT parents.
fn collect_aot_parent_abstract_method_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    requirements: &mut std::collections::HashMap<String, EvalAotAbstractMethodRequirement>,
) -> Result<(), EvalStatus> {
    let class_name = class_name.trim_start_matches('\\');
    if let Some(class) = context.class(class_name) {
        if let Some(parent) = class.parent() {
            collect_aot_parent_abstract_method_requirements(
                parent,
                context,
                values,
                requirements,
            )?;
        }
        apply_class_aot_parent_abstract_method_requirements(class, context, requirements)?;
        return Ok(());
    }
    if values.class_exists(class_name)? {
        collect_native_aot_abstract_method_requirements(
            class_name,
            context,
            values,
            requirements,
        )?;
    }
    Ok(())
}

/// Collects abstract methods exposed by one generated/AOT class reflection row.
fn collect_native_aot_abstract_method_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    requirements: &mut std::collections::HashMap<String, EvalAotAbstractMethodRequirement>,
) -> Result<(), EvalStatus> {
    for method_name in eval_aot_method_names(class_name, values)? {
        let Some(flags) = values.reflection_method_flags(class_name, &method_name)? else {
            continue;
        };
        let key = method_name.to_ascii_lowercase();
        if flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT == 0 {
            requirements.remove(&key);
            continue;
        }
        if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
            continue;
        }
        let requirement =
            eval_aot_abstract_method_requirement(class_name, &method_name, flags, context, values)?;
        requirements.insert(key, requirement);
    }
    Ok(())
}

/// Collects generated/AOT abstract property requirements through eval and AOT parents.
fn collect_aot_parent_abstract_property_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    requirements: &mut std::collections::HashMap<String, EvalAotAbstractPropertyRequirement>,
) -> Result<(), EvalStatus> {
    let class_name = class_name.trim_start_matches('\\');
    if let Some(class) = context.class(class_name) {
        if let Some(parent) = class.parent() {
            collect_aot_parent_abstract_property_requirements(
                parent,
                context,
                values,
                requirements,
            )?;
        }
        apply_class_aot_parent_abstract_property_requirements(class, context, requirements)?;
        return Ok(());
    }
    if values.class_exists(class_name)? {
        collect_native_aot_abstract_property_requirements(
            class_name,
            context,
            values,
            requirements,
        )?;
    }
    Ok(())
}

/// Collects abstract properties exposed by one generated/AOT class metadata row.
fn collect_native_aot_abstract_property_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    requirements: &mut std::collections::HashMap<String, EvalAotAbstractPropertyRequirement>,
) -> Result<(), EvalStatus> {
    for (owner, property) in context.native_abstract_property_requirements(class_name) {
        let Some(flags) = values.reflection_property_flags(class_name, property.name())? else {
            continue;
        };
        if flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT == 0
            || flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0
        {
            continue;
        }
        let visibility = eval_aot_property_visibility(flags);
        let write_visibility = eval_aot_property_write_visibility(flags, visibility);
        let set_visibility = (write_visibility != visibility).then_some(write_visibility);
        let requirement = EvalClassProperty::with_visibility(property.name(), visibility, None)
            .with_type(property.property_type().cloned())
            .with_set_visibility(set_visibility)
            .with_abstract_hook_contract(property.requires_get(), property.requires_set());
        requirements.insert(
            property.name().to_string(),
            EvalAotAbstractPropertyRequirement {
                owner,
                property: requirement,
            },
        );
    }
    Ok(())
}

/// Collects abstract property requirements from one declared eval class ancestry chain.
fn collect_class_abstract_property_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    requirements: &mut std::collections::HashMap<String, EvalClassProperty>,
) -> Result<(), EvalStatus> {
    let Some(class) = context.class(class_name) else {
        return Ok(());
    };
    if let Some(parent) = class.parent() {
        collect_class_abstract_property_requirements(parent, context, requirements)?;
    }
    apply_class_abstract_property_requirements(class, requirements)
}

/// Applies one class's methods to the open abstract-method requirement set.
fn apply_class_abstract_method_requirements(
    class: &EvalClass,
    requirements: &mut std::collections::HashMap<String, EvalClassMethod>,
) {
    for method in class.methods() {
        let key = method.name().to_ascii_lowercase();
        if method.is_abstract() {
            requirements.insert(key, method.clone());
        } else {
            requirements.remove(&key);
        }
    }
}

/// Applies one eval class's methods to the open AOT abstract-method requirement set.
fn apply_class_aot_parent_abstract_method_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    requirements: &mut std::collections::HashMap<String, EvalAotAbstractMethodRequirement>,
) -> Result<(), EvalStatus> {
    for method in class.methods() {
        let key = method.name().to_ascii_lowercase();
        let Some(requirement) = requirements.get(&key) else {
            continue;
        };
        if !class_method_satisfies_aot_abstract_parent_requirement(
            method,
            class.name(),
            requirement,
            Some(class),
            context,
            false,
        ) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if !method.is_abstract() {
            requirements.remove(&key);
        }
    }
    Ok(())
}

/// Applies one eval class's properties to the open AOT abstract-property requirement set.
fn apply_class_aot_parent_abstract_property_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    requirements: &mut std::collections::HashMap<String, EvalAotAbstractPropertyRequirement>,
) -> Result<(), EvalStatus> {
    for property in class.properties() {
        let key = property.name().to_string();
        let Some(requirement) = requirements.get(&key).map(|requirement| {
            EvalAotAbstractPropertyRequirement {
                owner: requirement.owner.clone(),
                property: requirement.property.clone(),
            }
        }) else {
            continue;
        };
        if !class_property_satisfies_aot_abstract_parent_requirement(
            property,
            class.name(),
            &requirement,
            Some(class),
            context,
        ) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if property.is_abstract() {
            requirements.insert(
                key,
                EvalAotAbstractPropertyRequirement {
                    owner: class.name().to_string(),
                    property: merge_abstract_property_contracts(
                        &requirement.property,
                        property,
                    ),
                },
            );
        } else {
            requirements.remove(&key);
        }
    }
    Ok(())
}

/// Applies one class's properties to the open abstract-property requirement set.
fn apply_class_abstract_property_requirements(
    class: &EvalClass,
    requirements: &mut std::collections::HashMap<String, EvalClassProperty>,
) -> Result<(), EvalStatus> {
    for property in class.properties() {
        let key = property.name().to_string();
        if property.is_abstract() {
            if let Some(existing) = requirements.get(&key) {
                (property_contract_visibility_allows(existing, property)
                    && property_contract_write_visibility_allows(existing, property))
                .then_some(())
                .ok_or(EvalStatus::RuntimeFatal)?;
                requirements.insert(key, merge_abstract_property_contracts(existing, property));
            } else {
                requirements.insert(key, property.clone());
            }
        } else if let Some(requirement) = requirements.get(&key) {
            class_property_satisfies_abstract_contract(property, requirement)
                .then_some(())
                .ok_or(EvalStatus::RuntimeFatal)?;
            requirements.remove(&key);
        }
    }
    Ok(())
}

/// Merges inherited and redeclared abstract property hook requirements.
fn merge_abstract_property_contracts(
    inherited: &EvalClassProperty,
    redeclared: &EvalClassProperty,
) -> EvalClassProperty {
    redeclared.clone().with_abstract_hook_contract(
        inherited.requires_get_hook() || redeclared.requires_get_hook(),
        inherited.requires_set_hook() || redeclared.requires_set_hook(),
    )
}

/// Returns whether a redeclared property keeps compatible visibility.
fn property_contract_visibility_allows(
    inherited: &EvalClassProperty,
    redeclared: &EvalClassProperty,
) -> bool {
    property_visibility_rank(redeclared.visibility())
        >= property_visibility_rank(inherited.visibility())
}

/// Returns whether a redeclared property keeps compatible write visibility.
fn property_contract_write_visibility_allows(
    inherited: &EvalClassProperty,
    redeclared: &EvalClassProperty,
) -> bool {
    !inherited.requires_set_hook()
        || property_visibility_rank(redeclared.write_visibility())
            >= property_visibility_rank(inherited.write_visibility())
}

/// Returns whether a concrete property satisfies an abstract hook contract.
fn class_property_satisfies_abstract_contract(
    property: &EvalClassProperty,
    requirement: &EvalClassProperty,
) -> bool {
    if property.is_abstract()
        || property.is_static()
        || property.property_type() != requirement.property_type()
        || !property_contract_visibility_allows(requirement, property)
    {
        return false;
    }
    if requirement.requires_set_hook() {
        return requirement.set_visibility() != Some(EvalVisibility::Private)
            && property_contract_write_visibility_allows(requirement, property)
            && (property.has_set_hook() || (!property.has_get_hook() && !property.is_readonly()));
    }
    requirement.requires_get_hook()
}

/// Returns whether one property satisfies a generated/AOT abstract parent contract.
fn class_property_satisfies_aot_abstract_parent_requirement(
    property: &EvalClassProperty,
    property_owner: &str,
    requirement: &EvalAotAbstractPropertyRequirement,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    let required = &requirement.property;
    if property.is_static() != required.is_static()
        || !property_contract_visibility_allows(required, property)
        || !property_type_signature_matches(
            property.property_type(),
            property_owner,
            required.property_type(),
            &requirement.owner,
            pending_class,
            context,
        )
    {
        return false;
    }
    if property.is_abstract() {
        return (!required.requires_get_hook() || property.requires_get_hook())
            && (!required.requires_set_hook()
                || (property.requires_set_hook()
                    && property_contract_write_visibility_allows(required, property)));
    }
    if required.requires_get_hook() && !class_property_supports_interface_get(property) {
        return false;
    }
    if required.requires_set_hook() {
        return required.set_visibility() != Some(EvalVisibility::Private)
            && property_contract_write_visibility_allows(required, property)
            && class_property_supports_interface_set(property);
    }
    required.requires_get_hook()
}

/// Returns a comparable rank where larger means less restrictive property visibility.
fn property_visibility_rank(visibility: EvalVisibility) -> u8 {
    match visibility {
        EvalVisibility::Private => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Public => 3,
    }
}

/// Returns interface names inherited or directly declared by a pending eval class.
fn pending_class_interface_names(class: &EvalClass, context: &ElephcEvalContext) -> Vec<String> {
    let mut interfaces = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(parent) = class.parent() {
        for interface in context.class_interface_names(parent) {
            push_pending_class_interface_name(&interface, &mut interfaces, &mut seen);
        }
    }
    for interface in class.interfaces() {
        push_pending_class_interface_tree(interface, context, &mut interfaces, &mut seen);
    }
    interfaces
}

/// Adds one interface and its eval-declared parent interfaces to a pending class list.
fn push_pending_class_interface_tree(
    interface: &str,
    context: &ElephcEvalContext,
    interfaces: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    push_pending_class_interface_name(interface, interfaces, seen);
    for parent in context.interface_parent_names(interface) {
        push_pending_class_interface_name(&parent, interfaces, seen);
    }
}

/// Adds one interface name once using PHP class-name case-insensitive matching.
fn push_pending_class_interface_name(
    interface: &str,
    interfaces: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    let interface = interface.trim_start_matches('\\');
    if seen.insert(interface.to_ascii_lowercase()) {
        interfaces.push(interface.to_string());
    }
}

/// Returns PHP builtin interface method requirements inherited by a pending class.
fn pending_class_builtin_interface_method_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Vec<(String, EvalInterfaceMethod)> {
    let mut requirements = Vec::new();
    for interface in pending_class_interface_names(class, context) {
        requirements.extend(builtin_interface_method_requirements(&interface));
    }
    requirements
}

/// Returns generated/AOT interface method requirements inherited by a pending class.
fn pending_class_aot_interface_method_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalAotInterfaceMethodRequirement>, EvalStatus> {
    let mut requirements = Vec::new();
    for interface in pending_class_interface_names(class, context) {
        if context.has_interface(&interface) || !values.interface_exists(&interface)? {
            continue;
        }
        requirements.extend(eval_aot_interface_method_requirements(
            &interface, context, values,
        )?);
    }
    Ok(requirements)
}

/// Returns generated/AOT interface property requirements inherited by a pending class.
fn pending_class_aot_interface_property_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<(String, EvalInterfaceProperty)>, EvalStatus> {
    let mut requirements = Vec::new();
    for interface in pending_class_interface_names(class, context) {
        if context.has_interface(&interface) || !values.interface_exists(&interface)? {
            continue;
        }
        requirements.extend(context.native_interface_property_requirements(&interface));
    }
    Ok(requirements)
}

/// Returns generated/AOT method requirements for one runtime interface.
fn eval_aot_interface_method_requirements(
    interface: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalAotInterfaceMethodRequirement>, EvalStatus> {
    let interface = interface.trim_start_matches('\\');
    let method_names = eval_aot_interface_method_names(interface, values)?;
    let mut requirements = Vec::new();
    for method_name in method_names {
        if let Some(requirement) =
            eval_aot_interface_method_requirement(interface, &method_name, context, values)?
        {
            requirements.push(requirement);
        }
    }
    Ok(requirements)
}

/// Returns generated/AOT method names for one runtime interface.
fn eval_aot_interface_method_names(
    interface: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    eval_aot_method_names(interface, values)
}

/// Returns generated/AOT method names for one runtime class-like symbol.
fn eval_aot_method_names(
    class_like: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let method_names = values.reflection_method_names(class_like)?;
    let names = eval_runtime_string_array_to_vec(method_names, values)?;
    values.release(method_names)?;
    Ok(names)
}

/// Builds one generated/AOT abstract parent method requirement from metadata.
fn eval_aot_abstract_method_requirement(
    class_name: &str,
    method_name: &str,
    flags: u64,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalAotAbstractMethodRequirement, EvalStatus> {
    let is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    let owner = eval_aot_method_declaring_class(class_name, method_name, values)?;
    let signature = eval_aot_method_signature_requirement(
        class_name,
        method_name,
        is_static,
        context,
        values,
    )?;
    Ok(EvalAotAbstractMethodRequirement {
        owner,
        is_static,
        visibility: eval_aot_method_visibility(flags),
        signature,
    })
}

/// Builds one generated/AOT interface method requirement from reflection and signature metadata.
fn eval_aot_interface_method_requirement(
    interface: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalAotInterfaceMethodRequirement>, EvalStatus> {
    let Some(flags) = values.reflection_method_flags(interface, method_name)? else {
        return Ok(None);
    };
    let is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    let owner = values
        .reflection_method_declaring_class(interface, method_name)?
        .unwrap_or_else(|| interface.to_string());
    let signature = if is_static {
        context.native_static_method_signature(&owner, method_name)
    } else {
        context.native_method_signature(&owner, method_name)
    };
    Ok(Some(EvalAotInterfaceMethodRequirement {
        owner: owner.clone(),
        name: method_name.to_string(),
        is_static,
        signature: signature.map(|signature| {
            eval_native_signature_interface_method(method_name, is_static, &signature)
        }),
    }))
}

/// Converts generated/AOT callable metadata into an eval interface method requirement.
fn eval_native_signature_interface_method(
    method_name: &str,
    is_static: bool,
    signature: &NativeCallableSignature,
) -> EvalInterfaceMethod {
    let param_count = signature.param_count();
    EvalInterfaceMethod::new(
        method_name,
        (0..param_count)
            .map(|index| {
                signature
                    .param_names()
                    .get(index)
                    .filter(|name| !name.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("arg{index}"))
            })
            .collect(),
    )
    .with_static(is_static)
    .with_parameter_types(
        (0..param_count)
            .map(|index| signature.param_type(index).cloned())
            .collect(),
    )
    .with_parameter_defaults(
        (0..param_count)
            .map(|index| {
                signature
                    .param_default(index)
                    .map(|_| EvalExpr::Const(EvalConst::Null))
            })
            .collect(),
    )
    .with_parameter_by_ref_flags(
        (0..param_count)
            .map(|index| signature.param_by_ref(index))
            .collect(),
    )
    .with_parameter_variadic_flags(
        (0..param_count)
            .map(|index| signature.param_variadic(index))
            .collect(),
    )
    .with_return_type(signature.return_type().cloned())
}

/// Copies a runtime string array into Rust-owned strings for declaration validation.
fn eval_runtime_string_array_to_vec(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.int(position as i64)?;
        let value = values.array_get(array, key)?;
        result.push(eval_runtime_string_value(value, values)?);
    }
    Ok(result)
}

/// Reads one runtime string cell as UTF-8 metadata.
fn eval_runtime_string_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Validates that one eval class provides methods required by one eval interface.
fn validate_class_implements_eval_interface(
    class: &EvalClass,
    interface_name: &str,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for (requirement_owner, requirement) in
        context.interface_method_requirements_with_owners(interface_name)
    {
        if !class_has_interface_method(class, &requirement_owner, &requirement, context) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    for (requirement_owner, requirement) in
        context.interface_property_requirements_with_owners(interface_name)
    {
        if !class_has_interface_property(class, &requirement_owner, &requirement, context) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns whether a class or its eval parents satisfy one builtin interface method signature.
fn class_has_builtin_interface_method(
    class: &EvalClass,
    requirement_owner: &str,
    requirement: &EvalInterfaceMethod,
    context: &ElephcEvalContext,
) -> bool {
    if let Some(method) = class.method(requirement.name()) {
        return method.visibility() == EvalVisibility::Public
            && method.is_static() == requirement.is_static()
            && !method.is_abstract()
            && class_method_satisfies_builtin_interface_signature(
                method,
                class.name(),
                requirement,
                requirement_owner,
                Some(class),
                context,
            );
    }
    class
        .parent()
        .and_then(|parent| context.class_method(parent, requirement.name()))
        .is_some_and(|(declaring_class, method)| {
            method.visibility() == EvalVisibility::Public
                && method.is_static() == requirement.is_static()
                && !method.is_abstract()
                && class_method_satisfies_builtin_interface_signature(
                    &method,
                    &declaring_class,
                    requirement,
                    requirement_owner,
                    Some(class),
                    context,
                )
        })
}

/// Returns whether a class or its eval parents satisfy one generated/AOT interface method.
fn class_has_aot_interface_method(
    class: &EvalClass,
    requirement: &EvalAotInterfaceMethodRequirement,
    context: &ElephcEvalContext,
) -> bool {
    if let Some((declaring_class, method)) = pending_class_method(class, &requirement.name, context)
    {
        return class_method_satisfies_aot_interface_requirement(
            &method,
            &declaring_class,
            requirement,
            Some(class),
            context,
            true,
        );
    }
    false
}

/// Returns whether a class or its eval parents satisfy one interface method signature.
fn class_has_interface_method(
    class: &EvalClass,
    requirement_owner: &str,
    requirement: &EvalInterfaceMethod,
    context: &ElephcEvalContext,
) -> bool {
    if let Some(method) = class.method(requirement.name()) {
        return method.visibility() == EvalVisibility::Public
            && method.is_static() == requirement.is_static()
            && !method.is_abstract()
            && class_method_satisfies_interface_signature(
                method,
                class.name(),
                requirement,
                requirement_owner,
                Some(class),
                context,
            );
    }
    class
        .parent()
        .and_then(|parent| context.class_method(parent, requirement.name()))
        .is_some_and(|(declaring_class, method)| {
            method.visibility() == EvalVisibility::Public
                && method.is_static() == requirement.is_static()
                && !method.is_abstract()
                && class_method_satisfies_interface_signature(
                    &method,
                    &declaring_class,
                    requirement,
                    requirement_owner,
                    Some(class),
                    context,
                )
        })
}

/// Returns whether one method satisfies a generated/AOT interface requirement.
fn class_method_satisfies_aot_interface_requirement(
    method: &EvalClassMethod,
    method_owner: &str,
    requirement: &EvalAotInterfaceMethodRequirement,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
    require_concrete: bool,
) -> bool {
    if method.visibility() != EvalVisibility::Public
        || method.is_static() != requirement.is_static
        || (require_concrete && method.is_abstract())
    {
        return false;
    }
    requirement.signature.as_ref().is_none_or(|signature| {
        class_method_satisfies_interface_signature(
            method,
            method_owner,
            signature,
            &requirement.owner,
            pending_class,
            context,
        )
    })
}

/// Returns whether one method satisfies a generated/AOT abstract parent requirement.
fn class_method_satisfies_aot_abstract_parent_requirement(
    method: &EvalClassMethod,
    method_owner: &str,
    requirement: &EvalAotAbstractMethodRequirement,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
    require_concrete: bool,
) -> bool {
    if method.is_static() != requirement.is_static
        || method_visibility_rank(method.visibility())
            < method_visibility_rank(requirement.visibility)
        || (require_concrete && method.is_abstract())
    {
        return false;
    }
    requirement.signature.as_ref().is_none_or(|signature| {
        class_method_satisfies_interface_signature(
            method,
            method_owner,
            signature,
            &requirement.owner,
            pending_class,
            context,
        )
    })
}

/// Returns whether one class method can accept every call required by an interface method.
fn class_method_satisfies_interface_signature(
    method: &EvalClassMethod,
    method_owner: &str,
    requirement: &EvalInterfaceMethod,
    requirement_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    class_method_satisfies_interface_signature_with_return_mode(
        method,
        method_owner,
        requirement,
        requirement_owner,
        pending_class,
        context,
        false,
    )
}

/// Returns whether one class method can satisfy a PHP builtin interface method contract.
fn class_method_satisfies_builtin_interface_signature(
    method: &EvalClassMethod,
    method_owner: &str,
    requirement: &EvalInterfaceMethod,
    requirement_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    class_method_satisfies_interface_signature_with_return_mode(
        method,
        method_owner,
        requirement,
        requirement_owner,
        pending_class,
        context,
        true,
    )
}

/// Returns whether one class method satisfies an interface signature with configurable return checks.
fn class_method_satisfies_interface_signature_with_return_mode(
    method: &EvalClassMethod,
    method_owner: &str,
    requirement: &EvalInterfaceMethod,
    requirement_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
    allow_missing_return_type: bool,
) -> bool {
    method_signature_accepts(
        method.params().len(),
        method.parameter_defaults(),
        method.parameter_is_by_ref(),
        method.parameter_is_variadic(),
        requirement.params().len(),
        requirement.parameter_defaults(),
        requirement.parameter_is_by_ref(),
        requirement.parameter_is_variadic(),
    ) && method_parameter_type_signature_accepts(
        method.parameter_types(),
        method.parameter_is_variadic(),
        method_owner,
        requirement.parameter_types(),
        requirement.parameter_is_variadic(),
        requirement_owner,
        requirement.params().len(),
        pending_class,
        context,
    ) && ((allow_missing_return_type && method.return_type().is_none())
        || method_return_type_signature_accepts(
            method.return_type(),
            method_owner,
            requirement.return_type(),
            requirement_owner,
            pending_class,
            context,
        ))
}

/// Returns whether one class property is compatible with one interface property contract.
fn class_property_can_cover_interface_contract(
    property: &EvalClassProperty,
    property_owner: &str,
    requirement: &EvalInterfaceProperty,
    requirement_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    if property.visibility() != EvalVisibility::Public || property.is_static() {
        return false;
    }
    if !class_property_type_satisfies_interface_contract(
        property.property_type(),
        property.settable_type(),
        property_owner,
        requirement,
        requirement_owner,
        pending_class,
        context,
    ) {
        return false;
    }
    if requirement.requires_get() && !class_property_supports_interface_get(property) {
        return false;
    }
    if requirement.requires_set() {
        return requirement.set_visibility() != Some(EvalVisibility::Private)
            && property_visibility_rank(property.write_visibility())
                >= property_visibility_rank(requirement.write_visibility())
            && class_property_supports_interface_set(property);
    }
    requirement.requires_get()
}

/// Returns whether one property type is compatible with interface get/set hook signatures.
fn class_property_type_satisfies_interface_contract(
    property_type: Option<&EvalParameterType>,
    settable_type: Option<&EvalParameterType>,
    property_owner: &str,
    requirement: &EvalInterfaceProperty,
    requirement_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    if requirement.requires_get()
        && !method_return_type_signature_accepts(
            property_type,
            property_owner,
            requirement.property_type(),
            requirement_owner,
            pending_class,
            context,
        )
    {
        return false;
    }
    if requirement.requires_set() {
        let property_types = vec![settable_type.cloned()];
        let requirement_types = vec![requirement.property_type().cloned()];
        return method_parameter_type_signature_accepts(
            &property_types,
            &[],
            property_owner,
            &requirement_types,
            &[],
            requirement_owner,
            1,
            pending_class,
            context,
        );
    }
    true
}

/// Returns whether one property can satisfy an interface `get` hook.
fn class_property_supports_interface_get(property: &EvalClassProperty) -> bool {
    property.has_get_hook() || property.requires_get_hook() || !property.is_virtual()
}

/// Returns whether one property can satisfy an interface `set` hook.
fn class_property_supports_interface_set(property: &EvalClassProperty) -> bool {
    property.has_set_hook()
        || property.requires_set_hook()
        || (!property.is_virtual() && !property.is_readonly())
}

/// Returns whether an implementing method accepts the full required arity range.
fn method_signature_accepts(
    implementation_param_count: usize,
    implementation_defaults: &[Option<EvalExpr>],
    implementation_by_refs: &[bool],
    implementation_variadics: &[bool],
    required_param_count: usize,
    required_defaults: &[Option<EvalExpr>],
    required_by_refs: &[bool],
    required_variadics: &[bool],
) -> bool {
    let implementation_min = method_signature_min_arity(
        implementation_param_count,
        implementation_defaults,
        implementation_variadics,
    );
    let required_min =
        method_signature_min_arity(required_param_count, required_defaults, required_variadics);
    if implementation_min > required_min {
        return false;
    }

    let implementation_max =
        method_signature_max_arity(implementation_param_count, implementation_variadics);
    let required_max = method_signature_max_arity(required_param_count, required_variadics);
    let arity_accepted = match (implementation_max, required_max) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(implementation_max), Some(required_max)) => implementation_max >= required_max,
    };
    arity_accepted
        && method_signature_by_refs_accept(
            implementation_by_refs,
            implementation_variadics,
            required_param_count,
            required_by_refs,
            required_variadics,
        )
}

/// Returns whether pass-by-reference requirements are compatible across accepted args.
fn method_signature_by_refs_accept(
    implementation_by_refs: &[bool],
    implementation_variadics: &[bool],
    required_param_count: usize,
    required_by_refs: &[bool],
    required_variadics: &[bool],
) -> bool {
    (0..required_param_count).all(|position| {
        method_signature_effective_by_ref(
            implementation_by_refs,
            implementation_variadics,
            position,
        ) == method_signature_effective_by_ref(required_by_refs, required_variadics, position)
    })
}

/// Returns the by-reference mode that one signature applies at an argument position.
fn method_signature_effective_by_ref(
    by_refs: &[bool],
    variadics: &[bool],
    position: usize,
) -> bool {
    if let Some(variadic_index) = variadics.iter().position(|is_variadic| *is_variadic) {
        if position >= variadic_index {
            return by_refs.get(variadic_index).copied().unwrap_or(false);
        }
    }
    by_refs.get(position).copied().unwrap_or(false)
}

/// Returns the minimum argument count accepted by one eval method signature.
fn method_signature_min_arity(
    param_count: usize,
    defaults: &[Option<EvalExpr>],
    variadics: &[bool],
) -> usize {
    let fixed_count = variadics
        .iter()
        .position(|is_variadic| *is_variadic)
        .unwrap_or(param_count);
    (0..fixed_count)
        .rfind(|position| !defaults.get(*position).is_some_and(Option::is_some))
        .map_or(0, |position| position + 1)
}

/// Returns the maximum argument count accepted by one eval method signature.
fn method_signature_max_arity(param_count: usize, variadics: &[bool]) -> Option<usize> {
    if variadics.iter().any(|is_variadic| *is_variadic) {
        None
    } else {
        Some(param_count)
    }
}

/// Returns whether a class or its eval parents satisfy one interface property contract.
fn class_has_interface_property(
    class: &EvalClass,
    requirement_owner: &str,
    requirement: &EvalInterfaceProperty,
    context: &ElephcEvalContext,
) -> bool {
    pending_class_property_with_owner(class, requirement.name(), context).is_some_and(
        |(declaring_class, property)| {
            !property.is_abstract()
                && class_property_can_cover_interface_contract(
                    &property,
                    &declaring_class,
                    requirement,
                    requirement_owner,
                    Some(class),
                    context,
                )
        },
    )
}

/// Returns a property from a pending class or one of its already registered parents.
fn pending_class_property_with_owner(
    class: &EvalClass,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassProperty)> {
    if let Some(property) = class
        .properties()
        .iter()
        .find(|property| property.name() == property_name)
    {
        return Some((class.name().to_string(), property.clone()));
    }
    class
        .parent()
        .and_then(|parent| context.class_property(parent, property_name))
}

/// Reads one object property while enforcing eval-declared member visibility.
pub(in crate::interpreter) fn eval_property_get_result(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return values.property_get(object, property_name);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = eval_runtime_object_class_name(object, values)?;
        if let Some((declaring_class, visibility, _, is_static)) =
            eval_reflection_aot_property_access_metadata(&class_name, property_name, values)?
        {
            if !is_static && validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                return eval_throw_property_access_error(
                    &declaring_class,
                    property_name,
                    visibility,
                    context,
                    values,
                );
            }
        }
        return values.property_get(object, property_name);
    };
    let object_class_name = class.name().to_string();
    let mut storage_property_name = property_name.to_string();
    let mut declared_property_found = false;
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        declared_property_found = true;
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_err() {
            if let Some(result) =
                eval_magic_property_get(object, &object_class_name, property_name, context, values)?
            {
                return Ok(result);
            }
            return eval_throw_property_access_error(
                &declaring_class,
                property.name(),
                property.visibility(),
                context,
                values,
            );
        }
        storage_property_name = eval_instance_property_storage_name(&declaring_class, &property);
        if property.has_get_hook()
            && !current_eval_property_hook_is(
                &declaring_class,
                property.name(),
                &property_hook_get_method(property.name()),
                context,
            )
        {
            let (hook_class, hook_method) = context
                .class_method(
                    &object_class_name,
                    &property_hook_get_method(property.name()),
                )
                .ok_or(EvalStatus::RuntimeFatal)?;
            return eval_dynamic_method_with_values(
                &hook_class,
                &object_class_name,
                &hook_method,
                object,
                Vec::new(),
                context,
                values,
            );
        }
        if property.property_type().is_some()
            && !context.dynamic_property_is_initialized(identity, &storage_property_name)
        {
            return eval_throw_uninitialized_property_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
    }
    if !declared_property_found
        && eval_object_public_property_exists(object, property_name, values)?
    {
        return values.property_get(object, property_name);
    }
    if !declared_property_found {
        if let Some((declaring_class, visibility, _, is_static)) =
            eval_dynamic_class_native_property_metadata(
                &object_class_name,
                property_name,
                context,
                values,
            )?
        {
            if !is_static {
                if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                    if let Some(result) = eval_magic_property_get(
                        object,
                        &object_class_name,
                        property_name,
                        context,
                        values,
                    )? {
                        return Ok(result);
                    }
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        visibility,
                        context,
                        values,
                    );
                }
                return eval_with_native_bridge_scope(&declaring_class, context, || {
                    values.property_get(object, property_name)
                });
            }
        }
    }
    if !declared_property_found {
        if let Some(result) =
            eval_magic_property_get(object, &object_class_name, property_name, context, values)?
        {
            return Ok(result);
        }
    }
    if let Some(target) = context
        .dynamic_property_alias(identity, &storage_property_name)
        .cloned()
    {
        return eval_reference_target_value(&target, context, values);
    }
    values.property_get(object, &storage_property_name)
}

/// Writes one object property while enforcing eval-declared member visibility.
pub(in crate::interpreter) fn eval_property_set_result(
    object: RuntimeCellHandle,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return values.property_set(object, property_name, value);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = eval_runtime_object_class_name(object, values)?;
        if let Some((declaring_class, _, write_visibility, is_static)) =
            eval_reflection_aot_property_access_metadata(&class_name, property_name, values)?
        {
            if !is_static
                && validate_eval_member_access(&declaring_class, write_visibility, context).is_err()
            {
                return eval_throw_property_access_error(
                    &declaring_class,
                    property_name,
                    write_visibility,
                    context,
                    values,
                );
            }
        }
        return values.property_set(object, property_name, value);
    };
    let object_class_name = class.name().to_string();
    if context.has_enum(&object_class_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let class_is_readonly = class.is_readonly_class();
    let mut storage_property_name = property_name.to_string();
    let mut declared_property_found = false;
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        declared_property_found = true;
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_err() {
            if eval_magic_property_set(
                object,
                &object_class_name,
                property_name,
                value,
                context,
                values,
            )? {
                return Ok(());
            }
            return eval_throw_property_access_error(
                &declaring_class,
                property.name(),
                property.visibility(),
                context,
                values,
            );
        }
        if validate_eval_property_write_access(&declaring_class, &property, context).is_err() {
            return eval_throw_property_write_access_error(
                &declaring_class,
                &property,
                context,
                values,
            );
        }
        if validate_eval_readonly_property_write(&declaring_class, &property, context).is_err() {
            return eval_throw_readonly_property_modification_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
        storage_property_name = eval_instance_property_storage_name(&declaring_class, &property);
        if property.has_set_hook() {
            if !current_eval_property_hook_is(
                &declaring_class,
                property.name(),
                &property_hook_set_method(property.name()),
                context,
            ) {
                let (hook_class, hook_method) = context
                    .class_method(
                        &object_class_name,
                        &property_hook_set_method(property.name()),
                    )
                    .ok_or(EvalStatus::RuntimeFatal)?;
                let hook_result = eval_dynamic_method_with_values(
                    &hook_class,
                    &object_class_name,
                    &hook_method,
                    object,
                    vec![EvaluatedCallArg {
                        name: None,
                        value,
                        ref_target: None,
                    }],
                    context,
                    values,
                )?;
                values.release(hook_result)?;
                return Ok(());
            }
        } else if property.has_get_hook() {
            return eval_throw_property_hook_readonly_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
    }
    if !declared_property_found {
        if let Some((declaring_class, _, write_visibility, is_static)) =
            eval_dynamic_class_native_property_metadata(
                &object_class_name,
                property_name,
                context,
                values,
            )?
        {
            if !is_static {
                if validate_eval_member_access(&declaring_class, write_visibility, context)
                    .is_err()
                {
                    if eval_magic_property_set(
                        object,
                        &object_class_name,
                        property_name,
                        value,
                        context,
                        values,
                    )? {
                        return Ok(());
                    }
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        write_visibility,
                        context,
                        values,
                    );
                }
                return eval_with_native_bridge_scope(&declaring_class, context, || {
                    values.property_set(object, property_name, value)
                });
            }
        }
    }
    if !declared_property_found
        && eval_magic_property_set(
            object,
            &object_class_name,
            property_name,
            value,
            context,
            values,
        )?
    {
        return Ok(());
    }
    if !declared_property_found && class_is_readonly {
        return eval_throw_dynamic_property_creation_error(
            &object_class_name,
            property_name,
            context,
            values,
        );
    }
    if let Some(target) = context
        .dynamic_property_alias(identity, &storage_property_name)
        .cloned()
    {
        eval_reference_target_write(
            identity,
            &storage_property_name,
            target,
            value,
            context,
            values,
        )?;
        context.mark_dynamic_property_initialized(identity, &storage_property_name);
        return values.property_set(object, &storage_property_name, value);
    }
    values.property_set(object, &storage_property_name, value)?;
    context.mark_dynamic_property_initialized(identity, &storage_property_name);
    Ok(())
}

/// Binds one eval object property to a by-reference source parameter.
fn eval_property_reference_bind_result(
    object: RuntimeCellHandle,
    property_name: &str,
    source_name: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let identity = values.object_identity(object)?;
    let class = context
        .dynamic_object_class(identity)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let object_class_name = class.name().to_string();
    if context.has_enum(&object_class_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (declaring_class, property) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?;
    validate_eval_property_write_access(&declaring_class, &property, context)?;
    if property.is_readonly() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let storage_property_name = eval_instance_property_storage_name(&declaring_class, &property);
    let target = eval_property_reference_target(
        identity,
        &storage_property_name,
        source_name,
        context,
        scope,
        values,
    )?;
    let value = eval_reference_target_value(&target, context, values)?;
    context.bind_dynamic_property_alias(identity, &storage_property_name, target);
    values.property_set(object, &storage_property_name, value)?;
    context.mark_dynamic_property_initialized(identity, &storage_property_name);
    Ok(())
}

/// Resolves a local by-reference source into a persistent property alias target.
fn eval_property_reference_target(
    object_identity: u64,
    storage_property_name: &str,
    source_name: &str,
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalReferenceTarget, EvalStatus> {
    if let Some(target) = scope.reference_target(source_name).cloned() {
        return Ok(target);
    }
    if context.current_function().is_some() {
        let cell =
            visible_scope_cell(context, scope, source_name).map_or_else(|| values.null(), Ok)?;
        return Ok(EvalReferenceTarget::Cell { cell });
    }
    let alias_name = eval_property_reference_alias_name(object_identity, storage_property_name);
    for replaced in set_reference_alias(context, scope, &alias_name, source_name, values)? {
        values.release(replaced)?;
    }
    Ok(EvalReferenceTarget::Variable {
        scope: scope as *mut ElephcEvalScope,
        name: alias_name,
    })
}

/// Builds the hidden scope variable name that backs one property reference alias.
fn eval_property_reference_alias_name(object_identity: u64, storage_property_name: &str) -> String {
    format!("\0elephc_property_ref:{object_identity}:{storage_property_name}")
}

/// Reads the current value from a persistent reference target.
pub(in crate::interpreter) fn eval_reference_target_value(
    target: &EvalReferenceTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match target {
        EvalReferenceTarget::Variable { scope, name } => {
            let Some(scope) = (unsafe { scope.as_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            visible_scope_cell(context, scope, name).map_or_else(|| values.null(), Ok)
        }
        EvalReferenceTarget::ArrayElement {
            scope,
            array_name,
            index,
        } => {
            let Some(scope) = (unsafe { scope.as_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let array =
                visible_scope_cell(context, scope, array_name).map_or_else(|| values.null(), Ok)?;
            values.array_get(array, *index)
        }
        EvalReferenceTarget::NestedArrayElement {
            array_target,
            index,
        } => {
            let array = eval_reference_target_value(array_target, context, values)?;
            values.array_get(array, *index)
        }
        EvalReferenceTarget::ObjectProperty {
            object,
            property,
            access_scope,
        } => {
            let previous_scope = context.replace_execution_scope(access_scope.clone());
            let result = eval_property_get_result(*object, property, context, values);
            context.replace_execution_scope(previous_scope);
            result
        }
        EvalReferenceTarget::StaticProperty {
            class_name,
            property,
            access_scope,
        } => {
            let previous_scope = context.replace_execution_scope(access_scope.clone());
            let result = eval_static_property_get_result(class_name, property, context, values);
            context.replace_execution_scope(previous_scope);
            result
        }
        EvalReferenceTarget::Cell { cell } => Ok(*cell),
    }
}

/// Writes a new value to a persistent reference target.
fn eval_reference_target_write(
    object_identity: u64,
    storage_property_name: &str,
    target: EvalReferenceTarget,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if matches!(target, EvalReferenceTarget::Cell { .. }) {
        context.bind_dynamic_property_alias(
            object_identity,
            storage_property_name,
            EvalReferenceTarget::Cell { cell: value },
        );
        return Ok(());
    }
    write_back_method_ref_target(&target, value, context, values)
}

/// Evaluates PHP `isset($object->property)` without forcing `__get()` first.
pub(in crate::interpreter) fn eval_property_isset_result(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        let value = values.property_get(object, property_name)?;
        return Ok(!values.is_null(value)?);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let value = values.property_get(object, property_name)?;
        return Ok(!values.is_null(value)?);
    };
    let object_class_name = class.name().to_string();
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_ok() {
            let storage_property_name =
                eval_instance_property_storage_name(&declaring_class, &property);
            if property.property_type().is_some()
                && !context.dynamic_property_is_initialized(identity, &storage_property_name)
            {
                return Ok(false);
            }
            let value = eval_property_get_result(object, property_name, context, values)?;
            return Ok(!values.is_null(value)?);
        }
        return eval_magic_property_isset(
            object,
            &object_class_name,
            property_name,
            context,
            values,
        )
        .map(|result| result.unwrap_or(false));
    }
    if eval_object_public_property_exists(object, property_name, values)? {
        let value = values.property_get(object, property_name)?;
        return Ok(!values.is_null(value)?);
    }
    if let Some((declaring_class, visibility, _, is_static)) =
        eval_dynamic_class_native_property_metadata(
            &object_class_name,
            property_name,
            context,
            values,
        )?
    {
        if !is_static {
            if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                return eval_magic_property_isset(
                    object,
                    &object_class_name,
                    property_name,
                    context,
                    values,
                )
                .map(|result| result.unwrap_or(false));
            }
            if !eval_with_native_bridge_scope(&declaring_class, context, || {
                values.property_is_initialized(object, property_name)
            })? {
                return Ok(false);
            }
            let value = eval_with_native_bridge_scope(&declaring_class, context, || {
                values.property_get(object, property_name)
            })?;
            return Ok(!values.is_null(value)?);
        }
    }
    eval_magic_property_isset(object, &object_class_name, property_name, context, values)
        .map(|result| result.unwrap_or(false))
}

/// Evaluates PHP `unset($object->property)` for eval-declared object receivers.
pub(in crate::interpreter) fn eval_property_unset_result(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(());
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        return Ok(());
    };
    let object_class_name = class.name().to_string();
    if context.has_enum(&object_class_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_ok() {
            if validate_eval_property_write_access(&declaring_class, &property, context).is_err() {
                return eval_throw_property_unset_access_error(
                    &declaring_class,
                    &property,
                    context,
                    values,
                );
            }
            if validate_eval_readonly_property_write(&declaring_class, &property, context).is_err() {
                return eval_throw_readonly_property_unset_error(
                    &declaring_class,
                    property.name(),
                    context,
                    values,
                );
            }
            let storage_property_name =
                eval_instance_property_storage_name(&declaring_class, &property);
            context.remove_dynamic_property_alias(identity, &storage_property_name);
            context.mark_dynamic_property_uninitialized(identity, &storage_property_name);
            let null = values.null()?;
            return values.property_set(object, &storage_property_name, null);
        }
        if eval_magic_property_unset(object, &object_class_name, property_name, context, values)? {
            return Ok(());
        }
        return Ok(());
    }
    if eval_object_public_property_exists(object, property_name, values)? {
        let null = values.null()?;
        return values.property_set(object, property_name, null);
    }
    let _ = eval_magic_property_unset(object, &object_class_name, property_name, context, values)?;
    Ok(())
}

/// Dispatches an undefined or inaccessible eval property read through `__get()`.
fn eval_magic_property_get(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(object_class_name, "__get") else {
        return Ok(None);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let property = values.string(property_name)?;
    eval_dynamic_method_with_values(
        &declaring_class,
        object_class_name,
        &method,
        object,
        positional_args(vec![property]),
        context,
        values,
    )
    .map(Some)
}

/// Dispatches an undefined or inaccessible eval property write through `__set()`.
fn eval_magic_property_set(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(object_class_name, "__set") else {
        return Ok(false);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let property = values.string(property_name)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        object_class_name,
        &method,
        object,
        positional_args(vec![property, value]),
        context,
        values,
    )?;
    values.release(result)?;
    Ok(true)
}

/// Dispatches an undefined or inaccessible eval property probe through `__isset()`.
fn eval_magic_property_isset(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<bool>, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(object_class_name, "__isset") else {
        return Ok(None);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let property = values.string(property_name)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        object_class_name,
        &method,
        object,
        positional_args(vec![property]),
        context,
        values,
    )?;
    let truthy = values.truthy(result)?;
    values.release(result)?;
    Ok(Some(truthy))
}

/// Dispatches an undefined or inaccessible eval property unset through `__unset()`.
fn eval_magic_property_unset(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(object_class_name, "__unset") else {
        return Ok(false);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let property = values.string(property_name)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        object_class_name,
        &method,
        object,
        positional_args(vec![property]),
        context,
        values,
    )?;
    values.release(result)?;
    Ok(true)
}

/// Returns whether the object already has a public dynamic property with this exact name.
fn eval_object_public_property_exists(
    object: RuntimeCellHandle,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let property_count = values.object_property_len(object)?;
    for position in 0..property_count {
        let key = values.object_property_iter_key(object, position)?;
        let key_bytes = values.string_bytes(key);
        values.release(key)?;
        if key_bytes? == property_name.as_bytes() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Validates that an object property may be used as a by-reference method argument.
pub(in crate::interpreter) fn validate_property_ref_target(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(());
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        return Ok(());
    };
    let object_class_name = class.name().to_string();
    if context.has_enum(&object_class_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        validate_eval_member_access(&declaring_class, property.visibility(), context)?;
        if property.is_readonly() {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns true while executing the named hook accessor for one property.
pub(in crate::interpreter) fn current_eval_property_hook_is(
    declaring_class: &str,
    property_name: &str,
    hook_method: &str,
    context: &ElephcEvalContext,
) -> bool {
    let Some(current_class) = context.current_class_scope() else {
        return false;
    };
    if !same_eval_class_name(current_class, declaring_class) {
        return false;
    }
    let Some((_, method)) = context
        .current_function()
        .and_then(|function| function.rsplit_once("::"))
    else {
        return false;
    };
    method.eq_ignore_ascii_case(hook_method)
        || method.eq_ignore_ascii_case(&property_hook_get_method(property_name))
        || method.eq_ignore_ascii_case(&property_hook_set_method(property_name))
}

/// Returns the synthetic get-hook method name for one property.
pub(in crate::interpreter) fn property_hook_get_method(property_name: &str) -> String {
    format!("__propget_{property_name}")
}

/// Returns the synthetic set-hook method name for one property.
pub(in crate::interpreter) fn property_hook_set_method(property_name: &str) -> String {
    format!("__propset_{property_name}")
}

/// Rejects writes to readonly eval-declared properties outside their declaring constructor.
fn validate_eval_readonly_property_write(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if !property.is_readonly() {
        return Ok(());
    }
    current_eval_method_is_declaring_constructor(declaring_class, context)
        .then_some(())
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns true while executing `__construct` for the property declaring class.
fn current_eval_method_is_declaring_constructor(
    declaring_class: &str,
    context: &ElephcEvalContext,
) -> bool {
    let Some(current_class) = context.current_class_scope() else {
        return false;
    };
    if !same_eval_class_name(current_class, declaring_class) {
        return false;
    }
    context
        .current_function()
        .and_then(|function| function.rsplit_once("::"))
        .is_some_and(|(_, method)| method.eq_ignore_ascii_case("__construct"))
}

/// Resolves the property metadata visible from the current class scope, if any.
fn eval_dynamic_property_for_access(
    object_class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassProperty)> {
    if let Some(current_class) = context.current_class_scope() {
        if context.class_is_a(object_class_name, current_class, false) {
            if let Some((declaring_class, property)) =
                context.class_own_property(current_class, property_name)
            {
                if property.visibility() == EvalVisibility::Private {
                    return Some((declaring_class, property));
                }
            }
        }
    }
    context.class_property(object_class_name, property_name)
}

/// Returns the physical storage name for an eval object property slot.
pub(in crate::interpreter) fn eval_instance_property_storage_name(
    declaring_class: &str,
    property: &EvalClassProperty,
) -> String {
    if property.visibility() == EvalVisibility::Private {
        format!(
            "\0{}\0{}",
            declaring_class.trim_start_matches('\\'),
            property.name()
        )
    } else {
        property.name().to_string()
    }
}

/// Validates the visibility that applies to property writes, including asymmetric `set` visibility.
fn validate_eval_property_write_access(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    validate_eval_member_access(declaring_class, property.write_visibility(), context)
}

/// Throws PHP's inaccessible property error for eval-declared properties.
fn eval_throw_property_access_error<T>(
    declaring_class: &str,
    property_name: &str,
    visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot access {} property {}::${}",
            eval_visibility_label(visibility),
            declaring_class.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's write access error for eval-declared properties.
fn eval_throw_property_write_access_error<T>(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    if let Some(set_visibility) = property.set_visibility() {
        return eval_throw_error(
            &format!(
                "Cannot modify {}(set) property {}::${} from {}",
                eval_visibility_label(set_visibility),
                declaring_class.trim_start_matches('\\'),
                property.name(),
                eval_native_constructor_scope_label(context)
            ),
            context,
            values,
        );
    }
    eval_throw_property_access_error(
        declaring_class,
        property.name(),
        property.write_visibility(),
        context,
        values,
    )
}

/// Throws PHP's unset access error for asymmetric eval-declared properties.
fn eval_throw_property_unset_access_error<T>(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    if let Some(set_visibility) = property.set_visibility() {
        return eval_throw_error(
            &format!(
                "Cannot unset {}(set) property {}::${} from {}",
                eval_visibility_label(set_visibility),
                declaring_class.trim_start_matches('\\'),
                property.name(),
                eval_native_constructor_scope_label(context)
            ),
            context,
            values,
        );
    }
    eval_throw_property_access_error(
        declaring_class,
        property.name(),
        property.write_visibility(),
        context,
        values,
    )
}

/// Throws PHP's read-only property-hook write error.
fn eval_throw_property_hook_readonly_error<T>(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Property {}::${} is read-only",
            declaring_class.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's readonly property assignment error for eval-declared properties.
fn eval_throw_readonly_property_modification_error<T>(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot modify readonly property {}::${}",
            declaring_class.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's readonly property unset error for eval-declared properties.
fn eval_throw_readonly_property_unset_error<T>(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot unset readonly property {}::${}",
            declaring_class.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's dynamic property creation error for readonly eval-declared classes.
fn eval_throw_dynamic_property_creation_error<T>(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot create dynamic property {}::${}",
            class_name.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's undeclared static property error for static property access.
fn eval_throw_undeclared_static_property_error<T>(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Access to undeclared static property {}::${}",
            class_name.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's uninitialized typed instance property error.
fn eval_throw_uninitialized_property_error<T>(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Typed property {}::${} must not be accessed before initialization",
            declaring_class.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's uninitialized typed static property error.
fn eval_throw_uninitialized_static_property_error<T>(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Typed static property {}::${} must not be accessed before initialization",
            declaring_class.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's class-not-found error for unresolved static member receivers.
fn eval_throw_class_not_found_error<T>(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!("Class \"{}\" not found", class_name.trim_start_matches('\\')),
        context,
        values,
    )
}

/// Throws PHP's inaccessible constant error for eval-declared class constants.
fn eval_throw_constant_access_error<T>(
    declaring_class: &str,
    constant_name: &str,
    visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot access {} constant {}::{}",
            eval_visibility_label(visibility),
            declaring_class.trim_start_matches('\\'),
            constant_name
        ),
        context,
        values,
    )
}

/// Throws PHP's inaccessible method error for eval-declared methods.
fn eval_throw_method_access_error<T>(
    declaring_class: &str,
    method_name: &str,
    visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Call to {} method {}::{}() from {}",
            eval_visibility_label(visibility),
            declaring_class.trim_start_matches('\\'),
            method_name,
            eval_native_constructor_scope_label(context)
        ),
        context,
        values,
    )
}

/// Throws PHP's inaccessible clone-expression error for `__clone()` hooks.
fn eval_throw_clone_access_error<T>(
    declaring_class: &str,
    visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Call to {} {}::__clone() from {}",
            eval_visibility_label(visibility),
            declaring_class.trim_start_matches('\\'),
            eval_native_constructor_scope_label(context)
        ),
        context,
        values,
    )
}

/// Throws PHP's error for calling an instance method through static syntax.
fn eval_throw_non_static_method_call_error<T>(
    declaring_class: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Non-static method {}::{}() cannot be called statically",
            declaring_class.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Throws PHP's error for calling an abstract method directly.
fn eval_throw_abstract_method_call_error<T>(
    declaring_class: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot call abstract method {}::{}()",
            declaring_class.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Throws PHP's undefined method error after static magic fallback misses.
fn eval_throw_undefined_method_call_error<T>(
    class_name: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Call to undefined method {}::{}()",
            class_name.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Throws PHP's error for invoking an object without `__invoke()`.
pub(in crate::interpreter) fn eval_throw_object_not_callable_error<T>(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Object of type {} is not callable",
            class_name.trim_start_matches('\\')
        ),
        context,
        values,
    )
}

/// Reads one eval-declared static property after resolving the class-like receiver.
pub(in crate::interpreter) fn eval_static_property_get_result(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if let Some((declaring_class, property)) = context.class_property(&class_name, property_name) {
        if !property.is_static() {
            return eval_throw_undeclared_static_property_error(
                &class_name,
                property_name,
                context,
                values,
            );
        }
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_err() {
            return eval_throw_property_access_error(
                &declaring_class,
                property.name(),
                property.visibility(),
                context,
                values,
            );
        }
        if let Some(target) = context
            .static_property_alias(&declaring_class, property.name())
            .cloned()
        {
            return eval_reference_target_value(&target, context, values);
        }
        if let Some(value) = context.static_property(&declaring_class, property.name()) {
            return Ok(value);
        }
        return eval_throw_uninitialized_static_property_error(
            &declaring_class,
            property.name(),
            context,
            values,
        );
    }
    if eval_static_member_context_owns_class(&class_name, context) {
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            if let Some((declaring_class, visibility, _, is_static)) =
                eval_reflection_aot_static_property_access_metadata(
                    &parent,
                    property_name,
                    context,
                    values,
                )?
            {
                if !is_static {
                    return eval_throw_undeclared_static_property_error(
                        &class_name,
                        property_name,
                        context,
                        values,
                    );
                }
                if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        visibility,
                        context,
                        values,
                    );
                }
                if let Some(target) = context
                    .static_property_alias(&declaring_class, property_name)
                    .cloned()
                {
                    return eval_reference_target_value(&target, context, values);
                }
                if !eval_with_native_bridge_scope(&declaring_class, context, || {
                    values.static_property_is_initialized(&declaring_class, property_name)
                })? {
                    return eval_throw_uninitialized_static_property_error(
                        &declaring_class,
                        property_name,
                        context,
                        values,
                    );
                }
                if let Some(value) = eval_with_native_bridge_scope(
                    &declaring_class,
                    context,
                    || values.static_property_get(&declaring_class, property_name),
                )? {
                    return Ok(value);
                }
            }
        }
        return eval_throw_undeclared_static_property_error(
            &class_name,
            property_name,
            context,
            values,
        );
    }
    if let Some((declaring_class, visibility, _, is_static)) =
        eval_reflection_aot_static_property_access_metadata(
            &class_name,
            property_name,
            context,
            values,
        )?
    {
        if is_static {
            if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                return eval_throw_property_access_error(
                    &declaring_class,
                    property_name,
                    visibility,
                    context,
                    values,
                );
            }
            if let Some(target) = context
                .static_property_alias(&declaring_class, property_name)
                .cloned()
            {
                return eval_reference_target_value(&target, context, values);
            }
            if !values.static_property_is_initialized(&declaring_class, property_name)? {
                return eval_throw_uninitialized_static_property_error(
                    &declaring_class,
                    property_name,
                    context,
                    values,
                );
            }
        }
    }
    if let Some(value) = values.static_property_get(&class_name, property_name)? {
        return Ok(value);
    }
    if eval_runtime_class_like_exists(&class_name, context, values)? {
        eval_throw_undeclared_static_property_error(&class_name, property_name, context, values)
    } else {
        eval_throw_class_not_found_error(&class_name, context, values)
    }
}

/// Returns whether a static property is PHP-`isset()` without throwing for missing properties.
pub(in crate::interpreter) fn eval_static_property_isset_result(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if let Some((declaring_class, property)) = context.class_property(&class_name, property_name) {
        if !property.is_static() {
            return Ok(false);
        }
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_err() {
            return Ok(false);
        }
        let value = if let Some(target) = context
            .static_property_alias(&declaring_class, property.name())
            .cloned()
        {
            eval_reference_target_value(&target, context, values)?
        } else {
            let Some(value) = context.static_property(&declaring_class, property.name()) else {
                return Ok(false);
            };
            value
        };
        return Ok(!values.is_null(value)?);
    }
    if eval_static_member_context_owns_class(&class_name, context) {
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            if let Some((declaring_class, visibility, _, is_static)) =
                eval_reflection_aot_static_property_access_metadata(
                    &parent,
                    property_name,
                    context,
                    values,
                )?
            {
                if !is_static {
                    return Ok(false);
                }
                if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                    return Ok(false);
                }
                if let Some(target) = context
                    .static_property_alias(&declaring_class, property_name)
                    .cloned()
                {
                    let value = eval_reference_target_value(&target, context, values)?;
                    return Ok(!values.is_null(value)?);
                }
                if !eval_with_native_bridge_scope(&declaring_class, context, || {
                    values.static_property_is_initialized(&declaring_class, property_name)
                })? {
                    return Ok(false);
                }
                if let Some(value) = eval_with_native_bridge_scope(
                    &declaring_class,
                    context,
                    || values.static_property_get(&declaring_class, property_name),
                )? {
                    return Ok(!values.is_null(value)?);
                }
            }
        }
        return Ok(false);
    }
    if let Some((declaring_class, visibility, _, is_static)) =
        eval_reflection_aot_static_property_access_metadata(
            &class_name,
            property_name,
            context,
            values,
        )?
    {
        if !is_static {
            return Ok(false);
        }
        if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
            return Ok(false);
        }
        if let Some(target) = context
            .static_property_alias(&declaring_class, property_name)
            .cloned()
        {
            let value = eval_reference_target_value(&target, context, values)?;
            return Ok(!values.is_null(value)?);
        }
        if !values.static_property_is_initialized(&declaring_class, property_name)? {
            return Ok(false);
        }
    } else if !eval_runtime_class_like_exists(&class_name, context, values)? {
        return eval_throw_class_not_found_error(&class_name, context, values);
    }
    if let Some(value) = values.static_property_get(&class_name, property_name)? {
        return Ok(!values.is_null(value)?);
    }
    Ok(false)
}

/// Throws PHP's catchable error for attempts to unset an existing static property target.
pub(in crate::interpreter) fn eval_static_property_unset_result(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if !eval_runtime_class_like_exists(&class_name, context, values)? {
        return eval_throw_class_not_found_error(&class_name, context, values);
    }
    eval_throw_error(
        &format!(
            "Attempt to unset static property {}::${}",
            class_name.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Reads one eval-declared class constant after resolving the class-like receiver.
pub(in crate::interpreter) fn eval_class_constant_fetch_result(
    class_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(value) = eval_builtin_reflection_class_constant(class_name, constant_name, values)?
    {
        return Ok(value);
    }
    if let Some(value) =
        eval_builtin_property_hook_type_case(class_name, constant_name, context, values)?
    {
        return Ok(value);
    }
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if let Some(case) = context.enum_case(&class_name, constant_name) {
        return Ok(case);
    }
    if let Some((declaring_class, constant)) = context.class_constant(&class_name, constant_name) {
        if validate_eval_member_access(&declaring_class, constant.visibility(), context).is_err() {
            return eval_throw_constant_access_error(
                &declaring_class,
                constant.name(),
                constant.visibility(),
                context,
                values,
            );
        }
        return context
            .class_constant_cell(&declaring_class, constant.name())
            .ok_or(EvalStatus::RuntimeFatal);
    }
    if eval_static_member_context_owns_class(&class_name, context) {
        if let Some((declaring_class, visibility)) =
            eval_dynamic_class_native_constant_metadata(
                &class_name,
                constant_name,
                context,
                values,
            )?
        {
            if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                return eval_throw_constant_access_error(
                    &declaring_class,
                    constant_name,
                    visibility,
                    context,
                    values,
                );
            }
            if let Some(value) = eval_with_native_bridge_scope(
                &declaring_class,
                context,
                || values.class_constant_get(&declaring_class, constant_name),
            )? {
                return Ok(value);
            }
        }
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(value) = values.class_constant_get(&class_name, constant_name)? {
        return Ok(value);
    }
    eval_throw_error(
        &format!(
            "Undefined constant {}::{}",
            class_name.trim_start_matches('\\'),
            constant_name
        ),
        context,
        values,
    )
}

/// Resolves eval-visible built-in Reflection class constants.
fn eval_builtin_reflection_class_constant(
    class_name: &str,
    constant_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let class_name = class_name.trim_start_matches('\\');
    let value = if class_name.eq_ignore_ascii_case("ReflectionClass") {
        match constant_name {
            "IS_IMPLICIT_ABSTRACT" => Some(16),
            "IS_FINAL" => Some(32),
            "IS_EXPLICIT_ABSTRACT" => Some(64),
            "IS_READONLY" => Some(65_536),
            _ => None,
        }
    } else if class_name.eq_ignore_ascii_case("ReflectionMethod") {
        match constant_name {
            "IS_PUBLIC" => Some(1),
            "IS_PROTECTED" => Some(2),
            "IS_PRIVATE" => Some(4),
            "IS_STATIC" => Some(16),
            "IS_FINAL" => Some(32),
            "IS_ABSTRACT" => Some(64),
            _ => None,
        }
    } else if class_name.eq_ignore_ascii_case("ReflectionProperty") {
        match constant_name {
            "IS_STATIC" => Some(16),
            "IS_READONLY" => Some(128),
            "IS_PUBLIC" => Some(1),
            "IS_PROTECTED" => Some(2),
            "IS_PRIVATE" => Some(4),
            "IS_ABSTRACT" => Some(64),
            "IS_PROTECTED_SET" => Some(2048),
            "IS_PRIVATE_SET" => Some(4096),
            "IS_VIRTUAL" => Some(512),
            "IS_FINAL" => Some(32),
            _ => None,
        }
    } else if class_name.eq_ignore_ascii_case("ReflectionClassConstant")
        || class_name.eq_ignore_ascii_case("ReflectionEnumUnitCase")
        || class_name.eq_ignore_ascii_case("ReflectionEnumBackedCase")
    {
        match constant_name {
            "IS_PUBLIC" => Some(1),
            "IS_PROTECTED" => Some(2),
            "IS_PRIVATE" => Some(4),
            "IS_FINAL" => Some(32),
            _ => None,
        }
    } else {
        None
    };
    value.map(|value| values.int(value)).transpose()
}

/// Resolves eval-visible `PropertyHookType` builtin enum cases.
fn eval_builtin_property_hook_type_case(
    class_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("PropertyHookType")
    {
        return Ok(None);
    }
    let Some((case_name, case_value)) = eval_property_hook_type_case_parts(constant_name) else {
        return Ok(None);
    };
    if let Some(case) = context.enum_case("PropertyHookType", case_name) {
        return Ok(Some(case));
    }
    let object = values.new_object("stdClass")?;
    let identity = values.object_identity(object)?;
    context.register_dynamic_object(identity, "PropertyHookType");
    let name = values.string(case_name)?;
    values.property_set(object, "name", name)?;
    let value = values.string(case_value)?;
    values.property_set(object, "value", value)?;
    if let Some(replaced) = context.set_enum_case_value("PropertyHookType", case_name, value) {
        values.release(replaced)?;
    }
    if let Some(replaced) = context.set_enum_case("PropertyHookType", case_name, object) {
        values.release(replaced)?;
    }
    Ok(Some(object))
}

/// Returns the PHP case name and backed value for a builtin property-hook case.
fn eval_property_hook_type_case_parts(constant_name: &str) -> Option<(&'static str, &'static str)> {
    match constant_name {
        "Get" => Some(("Get", "get")),
        "Set" => Some(("Set", "set")),
        _ => None,
    }
}

/// Returns the PHP class-name literal for `ClassName::class`-style eval expressions.
pub(in crate::interpreter) fn eval_class_name_fetch_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = resolve_eval_class_name_literal(class_name, context)?;
    values.string(&class_name)
}

/// Binds one eval-declared static property to a by-reference source variable.
fn eval_static_property_reference_bind_result(
    class_name: &str,
    property_name: &str,
    source_name: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if let Some((declaring_class, property)) = context.class_property(&class_name, property_name) {
        if !property.is_static() {
            return eval_throw_undeclared_static_property_error(
                &class_name,
                property_name,
                context,
                values,
            );
        }
        if validate_eval_property_write_access(&declaring_class, &property, context).is_err() {
            return eval_throw_property_write_access_error(
                &declaring_class,
                &property,
                context,
                values,
            );
        }
        if validate_eval_readonly_property_write(&declaring_class, &property, context).is_err() {
            return eval_throw_readonly_property_modification_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
        let target = eval_static_property_reference_target(
            &declaring_class,
            property.name(),
            source_name,
            context,
            scope,
            values,
        )?;
        let value = eval_reference_target_value(&target, context, values)?;
        context.bind_static_property_alias(&declaring_class, property.name(), target);
        if let Some(replaced) =
            context.set_static_property(&declaring_class, property.name(), value)
        {
            values.release(replaced)?;
        }
        return Ok(());
    }
    if eval_static_member_context_owns_class(&class_name, context) {
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            if let Some((declaring_class, _, write_visibility, is_static)) =
                eval_reflection_aot_static_property_access_metadata(
                    &parent,
                    property_name,
                    context,
                    values,
                )?
            {
                if !is_static {
                    return eval_throw_undeclared_static_property_error(
                        &class_name,
                        property_name,
                        context,
                        values,
                    );
                }
                if validate_eval_member_access(&declaring_class, write_visibility, context)
                    .is_err()
                {
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        write_visibility,
                        context,
                        values,
                    );
                }
                let target = eval_static_property_reference_target(
                    &declaring_class,
                    property_name,
                    source_name,
                    context,
                    scope,
                    values,
                )?;
                let value = eval_reference_target_value(&target, context, values)?;
                context.bind_static_property_alias(&declaring_class, property_name, target);
                if eval_with_native_bridge_scope(&declaring_class, context, || {
                    values.static_property_set(&declaring_class, property_name, value)
                })? {
                    return Ok(());
                }
            }
        }
        return eval_throw_undeclared_static_property_error(
            &class_name,
            property_name,
            context,
            values,
        );
    }
    if let Some((declaring_class, _, write_visibility, is_static)) =
        eval_reflection_aot_static_property_access_metadata(
            &class_name,
            property_name,
            context,
            values,
        )?
    {
        if !is_static {
            return eval_throw_undeclared_static_property_error(
                &class_name,
                property_name,
                context,
                values,
            );
        }
        if validate_eval_member_access(&declaring_class, write_visibility, context).is_err() {
            return eval_throw_property_access_error(
                &declaring_class,
                property_name,
                write_visibility,
                context,
                values,
            );
        }
        let target = eval_static_property_reference_target(
            &declaring_class,
            property_name,
            source_name,
            context,
            scope,
            values,
        )?;
        let value = eval_reference_target_value(&target, context, values)?;
        context.bind_static_property_alias(&declaring_class, property_name, target);
        if values.static_property_set(&class_name, property_name, value)? {
            return Ok(());
        }
        return eval_throw_undeclared_static_property_error(
            &class_name,
            property_name,
            context,
            values,
        );
    }
    if eval_runtime_class_like_exists(&class_name, context, values)? {
        eval_throw_undeclared_static_property_error(&class_name, property_name, context, values)
    } else {
        eval_throw_class_not_found_error(&class_name, context, values)
    }
}

/// Resolves a local by-reference source into a persistent static-property alias target.
fn eval_static_property_reference_target(
    class_name: &str,
    property_name: &str,
    source_name: &str,
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalReferenceTarget, EvalStatus> {
    if let Some(target) = scope.reference_target(source_name).cloned() {
        return Ok(target);
    }
    if context.current_function().is_some() {
        let cell =
            visible_scope_cell(context, scope, source_name).map_or_else(|| values.null(), Ok)?;
        return Ok(EvalReferenceTarget::Cell { cell });
    }
    let alias_name = eval_static_property_reference_alias_name(class_name, property_name);
    for replaced in set_reference_alias(context, scope, &alias_name, source_name, values)? {
        values.release(replaced)?;
    }
    Ok(EvalReferenceTarget::Variable {
        scope: scope as *mut ElephcEvalScope,
        name: alias_name,
    })
}

/// Builds the hidden scope variable name backing one static-property reference alias.
fn eval_static_property_reference_alias_name(class_name: &str, property_name: &str) -> String {
    format!("\0elephc_static_property_ref:{class_name}:{property_name}")
}

/// Writes one eval static-property assignment through its persistent reference target.
fn eval_static_reference_target_write(
    class_name: &str,
    property_name: &str,
    target: EvalReferenceTarget,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if matches!(target, EvalReferenceTarget::Cell { .. }) {
        context.bind_static_property_alias(
            class_name,
            property_name,
            EvalReferenceTarget::Cell { cell: value },
        );
        return Ok(());
    }
    write_back_method_ref_target(&target, value, context, values)
}

/// Writes one eval-declared static property after resolving the class-like receiver.
pub(in crate::interpreter) fn eval_static_property_set_result(
    class_name: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if let Some((declaring_class, property)) = context.class_property(&class_name, property_name) {
        if !property.is_static() {
            return eval_throw_undeclared_static_property_error(
                &class_name,
                property_name,
                context,
                values,
            );
        }
        if validate_eval_property_write_access(&declaring_class, &property, context).is_err() {
            return eval_throw_property_write_access_error(
                &declaring_class,
                &property,
                context,
                values,
            );
        }
        if validate_eval_readonly_property_write(&declaring_class, &property, context).is_err() {
            return eval_throw_readonly_property_modification_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
        if let Some(target) = context
            .static_property_alias(&declaring_class, property.name())
            .cloned()
        {
            eval_static_reference_target_write(
                &declaring_class,
                property.name(),
                target,
                value,
                context,
                values,
            )?;
        }
        if let Some(replaced) =
            context.set_static_property(&declaring_class, property.name(), value)
        {
            values.release(replaced)?;
        }
        return Ok(());
    }
    if eval_static_member_context_owns_class(&class_name, context) {
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            if let Some((declaring_class, _, write_visibility, is_static)) =
                eval_reflection_aot_static_property_access_metadata(
                    &parent,
                    property_name,
                    context,
                    values,
                )?
            {
                if !is_static {
                    return eval_throw_undeclared_static_property_error(
                        &class_name,
                        property_name,
                        context,
                        values,
                    );
                }
                if validate_eval_member_access(&declaring_class, write_visibility, context)
                    .is_err()
                {
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        write_visibility,
                        context,
                        values,
                    );
                }
                if let Some(target) = context
                    .static_property_alias(&declaring_class, property_name)
                    .cloned()
                {
                    eval_static_reference_target_write(
                        &declaring_class,
                        property_name,
                        target,
                        value,
                        context,
                        values,
                    )?;
                }
                if eval_with_native_bridge_scope(&declaring_class, context, || {
                    values.static_property_set(&declaring_class, property_name, value)
                })? {
                    return Ok(());
                }
            }
        }
        return eval_throw_undeclared_static_property_error(
            &class_name,
            property_name,
            context,
            values,
        );
    }
    if let Some((declaring_class, _, write_visibility, is_static)) =
        eval_reflection_aot_static_property_access_metadata(
            &class_name,
            property_name,
            context,
            values,
        )?
    {
        if is_static
            && validate_eval_member_access(&declaring_class, write_visibility, context).is_err()
        {
            return eval_throw_property_access_error(
                &declaring_class,
                property_name,
                write_visibility,
                context,
                values,
            );
        }
        if is_static {
            if let Some(target) = context
                .static_property_alias(&declaring_class, property_name)
                .cloned()
            {
                eval_static_reference_target_write(
                    &declaring_class,
                    property_name,
                    target,
                    value,
                    context,
                    values,
                )?;
            }
        }
    }
    if values.static_property_set(&class_name, property_name, value)? {
        return Ok(());
    }
    if eval_runtime_class_like_exists(&class_name, context, values)? {
        eval_throw_undeclared_static_property_error(&class_name, property_name, context, values)
    } else {
        eval_throw_class_not_found_error(&class_name, context, values)
    }
}

/// Dispatches a static method call to an eval-declared static method.
pub(in crate::interpreter) fn eval_static_method_call_result(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let receiver = resolve_eval_static_method_receiver(class_name, context)?;
    eval_static_method_call_result_resolved(
        receiver.dispatch_class,
        receiver.called_class,
        method_name,
        evaluated_args,
        None,
        context,
        values,
    )
}

/// Dispatches a static-syntax method call from an expression scope that may hold `$this`.
pub(in crate::interpreter) fn eval_static_method_call_result_from_scope(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    scope: &ElephcEvalScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let receiver = resolve_eval_static_method_receiver(class_name, context)?;
    eval_static_method_call_result_resolved(
        receiver.dispatch_class,
        receiver.called_class,
        method_name,
        evaluated_args,
        Some(scope),
        context,
        values,
    )
}

/// Dispatches a static method call using a first-class callable's captured called class.
pub(in crate::interpreter) fn eval_static_method_call_result_with_called_class(
    class_name: &str,
    called_class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    let called_class_name = context
        .resolve_class_name(called_class_name)
        .unwrap_or_else(|| called_class_name.trim_start_matches('\\').to_string());
    eval_static_method_call_result_resolved(
        class_name,
        called_class_name,
        method_name,
        evaluated_args,
        None,
        context,
        values,
    )
}

/// Dispatches a static method call after lookup and late-static names have been resolved.
fn eval_static_method_call_result_resolved(
    class_name: String,
    called_class_name: String,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_closure_static_method_result(
        &class_name,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_builtin_property_hook_type_static_method_result(
        &class_name,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_method_create_from_method_name_result(
        &class_name,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if eval_enum_static_builtin_applies(&class_name, method_name, context).is_some() {
        return eval_enum_builtin_static_method_result(
            &class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    }
    if let Some((declaring_class, method)) =
        eval_dynamic_static_method_for_call(&class_name, method_name, context)
    {
        if method.is_abstract() {
            return eval_throw_abstract_method_call_error(
                &declaring_class,
                method.name(),
                context,
                values,
            );
        }
        if validate_eval_member_access(&declaring_class, method.visibility(), context).is_err() {
            if let Some(result) = eval_magic_static_method_call(
                &class_name,
                &called_class_name,
                method_name,
                evaluated_args,
                context,
                values,
            )? {
                return Ok(result);
            }
            return eval_throw_method_access_error(
                &declaring_class,
                method.name(),
                method.visibility(),
                context,
                values,
            );
        }
        if !method.is_static() {
            if let Some(object) =
                eval_static_syntax_instance_receiver(&class_name, lexical_scope, context, values)?
            {
                return eval_dynamic_method_with_values(
                    &declaring_class,
                    &called_class_name,
                    &method,
                    object,
                    evaluated_args,
                    context,
                    values,
                );
            }
            return eval_throw_non_static_method_call_error(
                &declaring_class,
                method.name(),
                context,
                values,
            );
        }
        return eval_dynamic_static_method_with_values(
            &declaring_class,
            &called_class_name,
            &method,
            evaluated_args,
            context,
            values,
        );
    }
    if context.has_class(&class_name) {
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            if let Some(result) = eval_native_static_syntax_method_result(
                &parent,
                Some(&called_class_name),
                method_name,
                evaluated_args.clone(),
                lexical_scope,
                context,
                values,
            )?
            {
                return Ok(result);
            }
        }
    }
    if context.has_class(&class_name)
        || context.has_interface(&class_name)
        || context.has_trait(&class_name)
        || context.has_enum(&class_name)
    {
        if let Some(result) = eval_magic_static_method_call(
            &class_name,
            &called_class_name,
            method_name,
            evaluated_args,
            context,
            values,
        )? {
            return Ok(result);
        }
        return eval_throw_undefined_method_call_error(
            &class_name,
            method_name,
            context,
            values,
        );
    }
    if let Some(result) = eval_native_static_syntax_method_result(
        &class_name,
        None,
        method_name,
        evaluated_args.clone(),
        lexical_scope,
        context,
        values,
    )? {
        return Ok(result);
    }
    eval_native_static_method_with_evaluated_args(
        &class_name,
        method_name,
        evaluated_args,
        context,
        values,
    )
}

/// Dispatches static methods for eval's builtin `Closure` class slice.
fn eval_closure_static_method_result(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("Closure")
    {
        return Ok(None);
    }
    if method_name.eq_ignore_ascii_case("fromCallable") {
        return eval_closure_from_callable(evaluated_args, context, values).map(Some);
    }
    if method_name.eq_ignore_ascii_case("bind") {
        return eval_closure_bind_static(evaluated_args, context, values).map(Some);
    }
    Ok(None)
}

/// Materializes `Closure::fromCallable()` from one normalized eval callback.
fn eval_closure_from_callable(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut args = bind_evaluated_function_args(&[String::from("callback")], evaluated_args)?;
    let callback = args.pop().ok_or(EvalStatus::RuntimeFatal)?;
    let callable = eval_callable(callback, context, values)?;
    eval_validate_call_user_func_callback(&callable, "Closure::fromCallable", context, values)?;
    let target = eval_closure_object_target_from_callable(callable);
    eval_closure_object_from_target(target, context, values)
}

/// Converts a normalized callable target into the storage used by eval Closure objects.
fn eval_closure_object_target_from_callable(
    callable: EvaluatedCallable,
) -> EvalClosureObjectTarget {
    match callable {
        EvaluatedCallable::Named(name) => EvalClosureObjectTarget::Named(name),
        EvaluatedCallable::BoundClosure { name, bound_this } => {
            EvalClosureObjectTarget::BoundNamed { name, bound_this }
        }
        EvaluatedCallable::InvokableObject { object } => {
            EvalClosureObjectTarget::InvokableObject { object }
        }
        EvaluatedCallable::ObjectMethod {
            object,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => EvalClosureObjectTarget::ObjectMethod {
            object,
            method,
            called_class,
            native_class,
            bridge_scope,
        },
        EvaluatedCallable::StaticMethod {
            class_name,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => EvalClosureObjectTarget::StaticMethod {
            class_name,
            method,
            called_class,
            native_class,
            bridge_scope,
        },
    }
}

/// Allocates a PHP-visible eval Closure object for one retained callable target.
fn eval_closure_object_from_target(
    target: EvalClosureObjectTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = values.new_object("stdClass")?;
    let identity = values.object_identity(object)?;
    context.register_closure_object_target(identity, target);
    Ok(object)
}

/// Materializes `Closure::bind()` from a closure object and a persistent receiver.
fn eval_closure_bind_static(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (target, bound_this) = eval_closure_bind_static_args(evaluated_args, context, values)?;
    eval_closure_bind_target(target, bound_this, context, values)
}

/// Binds static `Closure::bind()` arguments to their PHP parameter slots.
fn eval_closure_bind_static_args(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(EvalClosureObjectTarget, Option<RuntimeCellHandle>), EvalStatus> {
    let bound = eval_closure_bind_args(
        &["closure", "newThis", "newScope"],
        2,
        evaluated_args,
    )?;
    let closure = required_closure_bind_arg(&bound, 0)?;
    let new_this = required_closure_bind_arg(&bound, 1)?;
    let target = eval_closure_target_arg(closure.value, context, values)?;
    let bound_this = eval_closure_bind_receiver_arg(new_this.value, values)?;
    Ok((target, bound_this))
}

/// Binds `Closure::bindTo()` arguments to their PHP parameter slots.
fn eval_closure_bind_to_args(
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let bound = eval_closure_bind_args(&["newThis", "newScope"], 1, evaluated_args)?;
    let new_this = required_closure_bind_arg(&bound, 0)?;
    eval_closure_bind_receiver_arg(new_this.value, values)
}

/// Binds positional and named Closure binding arguments while accepting optional scope.
fn eval_closure_bind_args(
    params: &[&str],
    required_count: usize,
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<Vec<Option<EvaluatedCallArg>>, EvalStatus> {
    let mut bound_args = vec![None; params.len()];
    let mut next_positional = 0;
    let mut saw_named = false;

    for arg in evaluated_args {
        if let Some(name) = arg.name.as_deref() {
            saw_named = true;
            let Some(index) = params
                .iter()
                .position(|param| param.eq_ignore_ascii_case(name))
            else {
                return Err(EvalStatus::RuntimeFatal);
            };
            if bound_args[index].is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            bound_args[index] = Some(arg);
            continue;
        }

        if saw_named || next_positional >= params.len() || bound_args[next_positional].is_some() {
            return Err(EvalStatus::RuntimeFatal);
        }
        bound_args[next_positional] = Some(arg);
        next_positional += 1;
    }

    if bound_args
        .iter()
        .take(required_count)
        .any(Option::is_none)
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(bound_args)
}

/// Returns one required Closure binding argument.
fn required_closure_bind_arg(
    bound_args: &[Option<EvaluatedCallArg>],
    index: usize,
) -> Result<&EvaluatedCallArg, EvalStatus> {
    bound_args
        .get(index)
        .and_then(Option::as_ref)
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Extracts a stored eval Closure object target from a runtime object.
fn eval_closure_target_arg(
    closure: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClosureObjectTarget, EvalStatus> {
    let identity = values.object_identity(closure)?;
    context
        .closure_object_target(identity)
        .cloned()
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Converts the `newThis` binding argument to an optional object receiver.
fn eval_closure_bind_receiver_arg(
    new_this: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if values.is_null(new_this)? {
        return Ok(None);
    }
    if values.type_tag(new_this)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(Some(new_this))
}

/// Creates a new Closure object with persistent binding metadata when supported.
fn eval_closure_bind_target(
    target: EvalClosureObjectTarget,
    bound_this: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(bound_this) = bound_this else {
        return eval_closure_unbind_target(target, context, values);
    };
    match target {
        EvalClosureObjectTarget::Named(name) | EvalClosureObjectTarget::BoundNamed { name, .. } => {
            let Some(closure) = context.closure(&name) else {
                return eval_closure_call_warning_null(
                    "Cannot rebind scope of closure created from function",
                    values,
                );
            };
            if closure.is_static() {
                return eval_closure_call_warning_null(
                    "Cannot bind an instance to a static closure",
                    values,
                );
            }
            eval_closure_object_from_target(
                EvalClosureObjectTarget::BoundNamed { name, bound_this },
                context,
                values,
            )
        }
        EvalClosureObjectTarget::InvokableObject { object } => {
            if !eval_closure_call_bound_class_matches(object, bound_this, context, values)? {
                return eval_closure_call_warning_null(
                    "Cannot rebind scope of closure created from method",
                    values,
                );
            }
            eval_closure_object_from_target(
                EvalClosureObjectTarget::InvokableObject { object: bound_this },
                context,
                values,
            )
        }
        EvalClosureObjectTarget::ObjectMethod {
            object,
            method,
            native_class,
            bridge_scope,
            ..
        } => {
            if !eval_closure_call_bound_class_matches(object, bound_this, context, values)? {
                return eval_closure_call_warning_null(
                    "Cannot rebind scope of closure created from method",
                    values,
                );
            }
            let called_class = Some(eval_closure_bound_object_class_name(
                bound_this, context, values,
            )?);
            eval_closure_object_from_target(
                EvalClosureObjectTarget::ObjectMethod {
                    object: bound_this,
                    method,
                    called_class,
                    native_class,
                    bridge_scope,
                },
                context,
                values,
            )
        }
        EvalClosureObjectTarget::StaticMethod { .. } => eval_closure_call_warning_null(
            "Cannot bind an instance to a static closure",
            values,
        ),
    }
}

/// Creates an unbound Closure object for targets that can drop `$this`.
fn eval_closure_unbind_target(
    target: EvalClosureObjectTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match target {
        EvalClosureObjectTarget::Named(name) | EvalClosureObjectTarget::BoundNamed { name, .. } => {
            eval_closure_object_from_target(EvalClosureObjectTarget::Named(name), context, values)
        }
        EvalClosureObjectTarget::InvokableObject { .. }
        | EvalClosureObjectTarget::ObjectMethod { .. } => {
            eval_closure_call_warning_null("Cannot unbind $this of method", values)
        }
        EvalClosureObjectTarget::StaticMethod { .. } => eval_closure_call_warning_null(
            "Cannot unbind $this of static method",
            values,
        ),
    }
}

/// Dispatches one generated/AOT method reached through PHP static-call syntax.
fn eval_native_static_syntax_method_result(
    class_name: &str,
    called_class_scope: Option<&str>,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?
    else {
        if eval_native_static_magic_method_available(class_name, context, values)? {
            return eval_native_static_method_with_evaluated_args(
                class_name,
                method_name,
                evaluated_args,
                context,
                values,
            )
            .map(Some);
        }
        return Ok(None);
    };
    if is_abstract {
        return eval_throw_abstract_method_call_error(
            &declaring_class,
            method_name,
            context,
            values,
        );
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
        if eval_native_static_magic_method_available(class_name, context, values)? {
            return eval_native_magic_static_method_call(
                class_name,
                method_name,
                evaluated_args,
                context,
                values,
            )
            .map(Some);
        }
        return eval_throw_method_access_error(
            &declaring_class,
            method_name,
            visibility,
            context,
            values,
        );
    }
    if !is_static {
        if let Some(object) =
            eval_static_syntax_instance_receiver(class_name, lexical_scope, context, values)?
        {
            return eval_native_method_with_evaluated_args_bridge_scope(
                object,
                class_name,
                method_name,
                evaluated_args,
                Some(&declaring_class),
                called_class_scope,
                context,
                values,
            )
            .map(Some);
        }
        return eval_throw_non_static_method_call_error(
            &declaring_class,
            method_name,
            context,
            values,
        );
    }
    eval_native_static_method_with_evaluated_args_bridge_scope(
        class_name,
        method_name,
        evaluated_args,
        Some(&declaring_class),
        called_class_scope,
        context,
        values,
    )
    .map(Some)
}

/// Returns `$this` when PHP permits static-call syntax to target an instance method.
fn eval_static_syntax_instance_receiver(
    class_name: &str,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(scope) = lexical_scope else {
        return Ok(None);
    };
    let Some(object) = visible_scope_cell(context, scope, "this") else {
        return Ok(None);
    };
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Ok(None);
    }
    let object_class_name = eval_static_syntax_object_class_name(object, context, values)?;
    if eval_static_syntax_object_matches_class(&object_class_name, class_name, context) {
        Ok(Some(object))
    } else {
        Ok(None)
    }
}

/// Resolves the PHP-visible class name for the current static-syntax `$this` object.
fn eval_static_syntax_object_class_name(
    object: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if let Ok(identity) = values.object_identity(object) {
        if let Some(class) = context.dynamic_object_class(identity) {
            return Ok(class.name().to_string());
        }
    }
    runtime_object_class_name(object, values)
}

/// Returns whether `$this` is an instance of the class named by static-call syntax.
fn eval_static_syntax_object_matches_class(
    object_class_name: &str,
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    same_eval_class_name(object_class_name, class_name)
        || context.class_is_a(object_class_name, class_name, false)
        || native_class_is_a(object_class_name, class_name, context)
}

/// Dispatches static methods for eval's builtin `PropertyHookType` enum slice.
fn eval_builtin_property_hook_type_static_method_result(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("PropertyHookType")
    {
        return Ok(None);
    }
    match eval_enum_static_builtin_name(method_name) {
        Some("cases") => {
            eval_builtin_property_hook_type_cases(evaluated_args, context, values).map(Some)
        }
        Some("from") => {
            eval_builtin_property_hook_type_from(evaluated_args, false, context, values).map(Some)
        }
        Some("tryFrom") => {
            eval_builtin_property_hook_type_from(evaluated_args, true, context, values).map(Some)
        }
        _ => Ok(None),
    }
}

/// Builds the indexed case array for eval's builtin `PropertyHookType` enum slice.
fn eval_builtin_property_hook_type_cases(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let case_names = ["Get", "Set"];
    let mut array = values.array_new(case_names.len())?;
    for (index, case_name) in case_names.iter().enumerate() {
        let key = values.int(index as i64)?;
        let case =
            eval_builtin_property_hook_type_case("PropertyHookType", case_name, context, values)?
                .ok_or(EvalStatus::RuntimeFatal)?;
        array = values.array_set(array, key, case)?;
    }
    Ok(array)
}

/// Evaluates builtin `PropertyHookType::from()` or `tryFrom()` inside eval.
fn eval_builtin_property_hook_type_from(
    evaluated_args: Vec<EvaluatedCallArg>,
    nullable_miss: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut args = bind_evaluated_function_args(&[String::from("value")], evaluated_args)?;
    let value = args.pop().ok_or(EvalStatus::RuntimeFatal)?;
    let bytes = values.string_bytes(value)?;
    let value_text = String::from_utf8_lossy(&bytes);
    for constant_name in ["Get", "Set"] {
        let Some((_, case_value)) = eval_property_hook_type_case_parts(constant_name) else {
            continue;
        };
        if value_text == case_value {
            return eval_builtin_property_hook_type_case(
                "PropertyHookType",
                constant_name,
                context,
                values,
            )?
            .ok_or(EvalStatus::RuntimeFatal);
        }
    }
    if nullable_miss {
        values.null()
    } else {
        let message = eval_enum_invalid_backing_value_message(
            "PropertyHookType",
            EvalEnumBackingType::String,
            value,
            values,
        )?;
        eval_throw_value_error(&message, context, values)
    }
}

/// Returns a recognized enum-provided static method name.
fn eval_enum_static_builtin_name(method_name: &str) -> Option<&'static str> {
    if method_name.eq_ignore_ascii_case("cases") {
        Some("cases")
    } else if method_name.eq_ignore_ascii_case("from") {
        Some("from")
    } else if method_name.eq_ignore_ascii_case("tryFrom") {
        Some("tryFrom")
    } else {
        None
    }
}

/// Returns a synthetic enum method only when that enum actually provides it.
pub(in crate::interpreter) fn eval_enum_static_builtin_applies(
    enum_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<&'static str> {
    let enum_decl = context.enum_decl(enum_name)?;
    match eval_enum_static_builtin_name(method_name)? {
        "cases" => Some("cases"),
        "from" if enum_decl.backing_type().is_some() => Some("from"),
        "tryFrom" if enum_decl.backing_type().is_some() => Some("tryFrom"),
        _ => None,
    }
}

/// Dispatches enum-provided static methods for eval-declared enums.
pub(in crate::interpreter) fn eval_enum_builtin_static_method_result(
    enum_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match eval_enum_static_builtin_name(method_name).ok_or(EvalStatus::RuntimeFatal)? {
        "cases" => eval_enum_cases_result(enum_name, evaluated_args, context, values),
        "from" => eval_enum_from_result(enum_name, evaluated_args, false, context, values),
        "tryFrom" => eval_enum_from_result(enum_name, evaluated_args, true, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds the indexed array returned by `EnumName::cases()`.
fn eval_enum_cases_result(
    enum_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let enum_decl = context
        .enum_decl(enum_name)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let case_names = enum_decl
        .cases()
        .iter()
        .map(|case| case.name().to_string())
        .collect::<Vec<_>>();
    let mut array = values.array_new(case_names.len())?;
    for (index, case_name) in case_names.iter().enumerate() {
        let key = values.int(index as i64)?;
        let case = context
            .enum_case(enum_name, case_name)
            .ok_or(EvalStatus::RuntimeFatal)?;
        array = values.array_set(array, key, case)?;
    }
    Ok(array)
}

/// Evaluates `EnumName::from()` or `EnumName::tryFrom()` for eval-backed enums.
fn eval_enum_from_result(
    enum_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    nullable_miss: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let enum_decl = context
        .enum_decl(enum_name)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let backing_type = enum_decl.backing_type().ok_or(EvalStatus::RuntimeFatal)?;
    let enum_display_name = enum_decl.name().trim_start_matches('\\').to_string();
    let case_names = enum_decl
        .cases()
        .iter()
        .map(|case| case.name().to_string())
        .collect::<Vec<_>>();
    let mut args = bind_evaluated_function_args(&[String::from("value")], evaluated_args)?;
    let value = args.pop().ok_or(EvalStatus::RuntimeFatal)?;
    for case_name in case_names {
        let case_value = context
            .enum_case_value(enum_name, &case_name)
            .ok_or(EvalStatus::RuntimeFatal)?;
        let equal = values.compare(EvalBinOp::StrictEq, value, case_value)?;
        if values.truthy(equal)? {
            return context
                .enum_case(enum_name, &case_name)
                .ok_or(EvalStatus::RuntimeFatal);
        }
    }
    if nullable_miss {
        values.null()
    } else {
        let message = eval_enum_invalid_backing_value_message(
            &enum_display_name,
            backing_type,
            value,
            values,
        )?;
        eval_throw_value_error(&message, context, values)
    }
}

/// Builds PHP's backed-enum `ValueError` message for an unmatched enum value.
fn eval_enum_invalid_backing_value_message(
    enum_name: &str,
    backing_type: EvalEnumBackingType,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let value = String::from_utf8_lossy(&bytes);
    let value = match backing_type {
        EvalEnumBackingType::Int => value.into_owned(),
        EvalEnumBackingType::String => format!("\"{}\"", value),
    };
    Ok(format!(
        "{} is not a valid backing value for enum {}",
        value, enum_name
    ))
}

/// Creates and schedules a `ValueError` through eval's normal Throwable channel.
fn eval_throw_value_error(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exception = values.new_object("ValueError")?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Creates and schedules a `ReflectionException` through eval's normal Throwable channel.
fn eval_throw_reflection_exception(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let exception = values.new_object("ReflectionException")?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Schedules the Throwable category required by one ReflectionClass instantiation error.
fn eval_throw_reflection_instantiation_error(
    error: EvalReflectionInstantiationError,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    match error {
        EvalReflectionInstantiationError::ThrowableError(message) => {
            eval_throw_error(&message, context, values)
        }
        EvalReflectionInstantiationError::ReflectionException(message) => {
            eval_throw_reflection_exception(&message, context, values)
        }
    }
}

/// Resolves a static method using private-method scope rules.
pub(in crate::interpreter) fn eval_dynamic_static_method_for_call(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassMethod)> {
    if let Some(current_class) = context.current_class_scope() {
        if eval_classes_are_related(current_class, class_name, context) {
            if let Some((declaring_class, method)) =
                context.class_own_method(current_class, method_name)
            {
                if method.visibility() == EvalVisibility::Private {
                    return Some((declaring_class, method));
                }
            }
        }
    }
    context.class_method(class_name, method_name)
}

/// Resolves `self`, `parent`, and `static` for eval static member access.
pub(in crate::interpreter) fn resolve_eval_static_class_name(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" => context
            .current_class_scope()
            .map(str::to_string)
            .ok_or(EvalStatus::RuntimeFatal),
        "static" => context
            .current_called_class_scope()
            .or_else(|| context.current_class_scope())
            .map(str::to_string)
            .ok_or(EvalStatus::RuntimeFatal),
        "parent" => {
            let current = context
                .current_class_scope()
                .ok_or(EvalStatus::RuntimeFatal)?;
            context
                .class(current)
                .and_then(EvalClass::parent)
                .map(|parent| {
                    context
                        .resolve_class_name(parent)
                        .unwrap_or_else(|| parent.trim_start_matches('\\').to_string())
                })
                .or_else(|| context.native_class_parent(current).map(str::to_string))
                .ok_or(EvalStatus::RuntimeFatal)
        }
        _ => context
            .resolve_class_name(class_name)
            .or_else(|| {
                context
                    .has_class(class_name)
                    .then(|| class_name.to_string())
            })
            .ok_or(EvalStatus::RuntimeFatal),
    }
}

/// Resolved static method dispatch metadata preserving PHP late-static forwarding.
pub(in crate::interpreter) struct EvalStaticMethodReceiver {
    pub(in crate::interpreter) dispatch_class: String,
    pub(in crate::interpreter) called_class: String,
}

/// Resolves static method receivers into lookup and late-static called-class names.
pub(in crate::interpreter) fn resolve_eval_static_method_receiver(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Result<EvalStaticMethodReceiver, EvalStatus> {
    let dispatch_class = resolve_eval_static_member_class_name(class_name, context)?;
    let called_class = match class_name.to_ascii_lowercase().as_str() {
        "self" | "parent" => context
            .current_called_class_scope()
            .or_else(|| context.current_class_scope())
            .map(str::to_string)
            .ok_or(EvalStatus::RuntimeFatal)?,
        "static" => dispatch_class.clone(),
        _ => dispatch_class.clone(),
    };
    Ok(EvalStaticMethodReceiver {
        dispatch_class,
        called_class,
    })
}

/// Resolves static member receivers while allowing non-eval class names to reach AOT lookup.
pub(in crate::interpreter) fn resolve_eval_static_member_class_name(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" | "parent" | "static" => resolve_eval_static_class_name(class_name, context),
        _ => Ok(context
            .resolve_class_name(class_name)
            .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string())),
    }
}

/// Returns true when an eval-declared class-like symbol should not fall through to AOT lookup.
fn eval_static_member_context_owns_class(
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    context.has_class(class_name)
        || context.has_interface(class_name)
        || context.has_trait(class_name)
        || context.has_enum(class_name)
}

/// Returns whether a static member receiver exists in eval metadata or generated metadata.
fn eval_runtime_class_like_exists(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(eval_static_member_context_owns_class(class_name, context)
        || values.class_exists(class_name)?
        || eval_runtime_interface_exists(class_name, values)?
        || values.trait_exists(class_name)?
        || values.enum_exists(class_name)?)
}

/// Resolves class-name literal receivers without requiring named classes to exist.
fn resolve_eval_class_name_literal(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" | "parent" | "static" => resolve_eval_static_class_name(class_name, context),
        _ => Ok(context
            .resolve_class_like_name(class_name)
            .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string())),
    }
}

/// Creates a backing object for an eval-declared class and runs its constructor.
pub(in crate::interpreter) fn eval_dynamic_class_new_object(
    class: &EvalClass,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = eval_dynamic_class_allocate_object(class, context, caller_scope, values)?;
    if let Some((constructor_class, constructor)) =
        context.class_method(class.name(), "__construct")
    {
        if validate_eval_member_access(&constructor_class, constructor.visibility(), context)
            .is_err()
        {
            let _ = values.release(object);
            return eval_throw_method_access_error(
                &constructor_class,
                constructor.name(),
                constructor.visibility(),
                context,
                values,
            );
        }
        let result = eval_dynamic_method_with_values(
            &constructor_class,
            class.name(),
            &constructor,
            object,
            evaluated_args,
            context,
            values,
        )?;
        eval_release_value(context, values, result)?;
    } else if !evaluated_args.is_empty() {
        if let Some(parent) = context.class_native_parent_name(class.name()) {
            eval_native_constructor_with_evaluated_args(
                &parent,
                object,
                evaluated_args,
                context,
                values,
            )?;
        } else {
            return Err(EvalStatus::RuntimeFatal);
        }
    } else if let Some(parent) = context.class_native_parent_name(class.name()) {
        if eval_aot_method_dispatch_metadata_in_hierarchy(
            &parent,
            "__construct",
            context,
            values,
        )?
        .is_some()
        {
            eval_native_constructor_with_evaluated_args(
                &parent,
                object,
                Vec::new(),
                context,
                values,
            )?;
        }
    }
    Ok(object)
}

/// Creates a PHP shallow clone and invokes an eval-declared `__clone()` hook when present.
pub(in crate::interpreter) fn eval_object_clone_result(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let identity = values.object_identity(object)?;
    let dynamic_class_name = context
        .dynamic_object_class(identity)
        .map(|class| class.name().to_string());
    let clone_method = dynamic_class_name
        .as_deref()
        .and_then(|class_name| context.class_method(class_name, "__clone"));
    if let Some((declaring_class, method)) = &clone_method {
        if validate_eval_member_access(declaring_class, method.visibility(), context).is_err() {
            return eval_throw_clone_access_error(
                declaring_class,
                method.visibility(),
                context,
                values,
            );
        }
    }
    let dynamic_native_clone_hook_scope = if clone_method.is_none() {
        if let Some(class_name) = dynamic_class_name.as_deref() {
            eval_dynamic_native_clone_hook_is_callable(class_name, context, values)?
        } else {
            None
        }
    } else {
        None
    };
    let should_call_aot_clone_hook = if dynamic_class_name.is_none() {
        eval_aot_clone_hook_is_callable(object, context, values)?
    } else {
        false
    };

    let clone = values.object_clone_shallow(object)?;
    if let Some(class_name) = dynamic_class_name {
        let clone_identity = values.object_identity(clone)?;
        context.register_dynamic_object(clone_identity, &class_name);
        context.clone_dynamic_property_aliases(identity, clone_identity);
        if let Some((declaring_class, method)) = clone_method {
            let result = eval_dynamic_method_with_values(
                &declaring_class,
                &class_name,
                &method,
                clone,
                Vec::new(),
                context,
                values,
            )?;
            eval_release_value(context, values, result)?;
        } else if let Some(scope) = dynamic_native_clone_hook_scope {
            let result = eval_native_method_call_with_scope(
                &scope,
                None,
                clone,
                "__clone",
                Vec::new(),
                context,
                values,
            )?;
            values.release(result)?;
        }
    } else if should_call_aot_clone_hook {
        let result = values.method_call(clone, "__clone", Vec::new())?;
        values.release(result)?;
    }
    Ok(clone)
}

/// Returns the declaring scope for an inherited generated/AOT `__clone()` hook.
fn eval_dynamic_native_clone_hook_is_callable(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_dynamic_class_native_method_metadata(class_name, "__clone", context, values)?
    else {
        return Ok(None);
    };
    if is_static || is_abstract {
        return Err(EvalStatus::RuntimeFatal);
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
        return eval_throw_clone_access_error(&declaring_class, visibility, context, values);
    }
    Ok(Some(declaring_class))
}

/// Calls one generated/AOT method while presenting an explicit PHP class scope to the bridge.
fn eval_native_method_call_with_scope(
    scope: &str,
    called_class_scope: Option<&str>,
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    context.push_class_scope(scope.to_string());
    if let Some(called_class) = called_class_scope {
        context.push_called_class_scope(called_class.to_string());
    }
    let _called_class_override = called_class_scope
        .map(|called_class| push_native_frame_called_class_override(context, scope, called_class));
    let result = values.method_call(object, method_name, evaluated_args);
    if called_class_scope.is_some() {
        context.pop_called_class_scope();
    }
    context.pop_class_scope();
    result
}

/// Calls one generated/AOT static method while presenting an explicit PHP class scope.
fn eval_native_static_method_call_with_scope(
    scope: &str,
    called_class_scope: Option<&str>,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    context.push_class_scope(scope.to_string());
    if let Some(called_class) = called_class_scope {
        context.push_called_class_scope(called_class.to_string());
    }
    let _called_class_override = called_class_scope
        .map(|called_class| push_native_frame_called_class_override(context, scope, called_class));
    let result = values.static_method_call(class_name, method_name, evaluated_args);
    if called_class_scope.is_some() {
        context.pop_called_class_scope();
    }
    context.pop_class_scope();
    result
}

/// Runs one generated/AOT bridge operation while exposing an explicit PHP class scope.
fn eval_with_native_bridge_scope<T>(
    scope: &str,
    context: &mut ElephcEvalContext,
    call: impl FnOnce() -> Result<T, EvalStatus>,
) -> Result<T, EvalStatus> {
    context.push_class_scope(scope.to_string());
    let result = call();
    context.pop_class_scope();
    result
}

/// Returns generated/AOT property metadata inherited by an eval-declared class.
fn eval_dynamic_class_native_property_metadata(
    called_class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility, EvalVisibility, bool)>, EvalStatus> {
    let Some(parent) = context.class_native_parent_name(called_class_name) else {
        return Ok(None);
    };
    eval_reflection_aot_property_access_metadata(&parent, property_name, values)
}

/// Returns generated/AOT class-constant metadata inherited by an eval-declared class.
fn eval_dynamic_class_native_constant_metadata(
    called_class_name: &str,
    constant_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility)>, EvalStatus> {
    let Some(parent) = context.class_native_parent_name(called_class_name) else {
        return Ok(None);
    };
    let Some(flags) = values.reflection_constant_flags(&parent, constant_name)? else {
        return Ok(None);
    };
    let declaring_class = values
        .reflection_constant_declaring_class(&parent, constant_name)?
        .unwrap_or(parent);
    let visibility = if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    };
    Ok(Some((declaring_class, visibility)))
}

/// Returns whether an accessible instance AOT `__clone()` hook should run.
fn eval_aot_clone_hook_is_callable(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let class_name = eval_runtime_object_class_name(object, values)?;
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata(&class_name, "__clone", values)?
    else {
        return Ok(false);
    };
    if is_static || is_abstract {
        return Err(EvalStatus::RuntimeFatal);
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
        return eval_throw_clone_access_error(&declaring_class, visibility, context, values);
    }
    Ok(true)
}

/// Reads the PHP-visible runtime class name for one AOT object handle.
fn eval_runtime_object_class_name(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let class_name = values.object_class_name(object)?;
    let bytes = values.string_bytes(class_name)?;
    values.release(class_name)?;
    String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Creates a backing object for an eval-declared class without running its constructor.
fn eval_dynamic_class_allocate_object(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if class.is_abstract() || context.has_enum(class.name()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let backing_class = context
        .class_native_parent_name(class.name())
        .unwrap_or_else(|| String::from("stdClass"));
    let object = values.new_object(&backing_class)?;
    let identity = values.object_identity(object)?;
    context.register_dynamic_object(identity, class.name());
    let mut class_chain = context.class_chain(class.name());
    if class_chain.is_empty() {
        class_chain.push(class.clone());
    }
    for class in &class_chain {
        for property in class
            .properties()
            .iter()
            .filter(|property| !property.is_static() && !property.is_abstract())
        {
            let value = if let Some(default) = property.default() {
                Some(eval_class_like_member_default(
                    class.name(),
                    property.trait_origin(),
                    default,
                    context,
                    caller_scope,
                    values,
                )?)
            } else if property.property_type().is_none() {
                Some(values.null()?)
            } else {
                None
            };
            let storage_name = eval_instance_property_storage_name(class.name(), property);
            if let Some(value) = value {
                values.property_set(object, &storage_name, value)?;
                context.mark_dynamic_property_initialized(identity, &storage_name);
            }
        }
    }
    Ok(object)
}

/// Dispatches a method call to an eval-declared class method or to the runtime hook.
pub(in crate::interpreter) fn eval_method_call_result(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_method_call_result_with_evaluated_args(
        object,
        method_name,
        positional_args(evaluated_args),
        context,
        values,
    )
}

/// Dispatches an object method call while preserving named-argument metadata for eval methods.
pub(in crate::interpreter) fn eval_method_call_result_with_evaluated_args(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        let evaluated_args = positional_evaluated_arg_values(evaluated_args)?;
        return values.method_call(object, method_name, evaluated_args);
    };
    if let Some(target) = context.closure_object_target(identity).cloned() {
        if let Some(result) =
            eval_closure_object_method_result(target, method_name, evaluated_args.clone(), context, values)?
        {
            return Ok(result);
        }
    }
    if let Some(attribute_metadata) = context.eval_reflection_attribute(identity).cloned() {
        if method_name.eq_ignore_ascii_case("newInstance") {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            return eval_reflection_attribute_new_instance_result(
                attribute_metadata.attribute(),
                context,
                values,
            );
        }
        if method_name.eq_ignore_ascii_case("getTarget") {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            return values.int(attribute_metadata.target() as i64);
        }
        if method_name.eq_ignore_ascii_case("isRepeated") {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            return values.bool_value(attribute_metadata.is_repeated());
        }
    }
    if let Some(result) = eval_reflection_parameter_legacy_type_predicate_result(
        object,
        method_name,
        evaluated_args.clone(),
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_parameter_to_string_result(
        object,
        method_name,
        evaluated_args.clone(),
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) =
        eval_reflection_type_to_string_result(object, method_name, evaluated_args.clone(), values)?
    {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_to_string_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_implements_interface_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_is_subclass_of_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_is_instance_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_source_location_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_basic_metadata_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_has_method_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_has_property_result(
        object,
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_has_constant_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_enum_methods_result(
        object,
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_relation_objects_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_trait_aliases_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_constant_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_constants_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_default_properties_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_static_properties_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_static_property_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_set_static_property_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_function_invoke_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_method_invoke_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_function_method_metadata_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_function_method_to_string_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_method_prototype_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_set_accessible_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_hooks_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_is_initialized_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_lazy_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_to_string_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_constant_to_string_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_enum_case_get_enum_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_get_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_raw_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_set_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_reflection_constant_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_reflection_constants_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_members_result(
        object,
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_member_result(
        object,
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(instance) = eval_reflection_class_new_instance_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(instance);
    }
    if let Some(instance) = eval_reflection_class_new_instance_without_constructor_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(instance);
    }
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = runtime_object_class_name(object, values)?;
        if method_name.eq_ignore_ascii_case("__clone") {
            if let Some((declaring_class, visibility, is_static, is_abstract)) =
                eval_aot_method_dispatch_metadata(&class_name, method_name, values)?
            {
                if is_static || is_abstract {
                    return Err(EvalStatus::RuntimeFatal);
                }
                if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                    return eval_throw_method_access_error(
                        &declaring_class,
                        method_name,
                        visibility,
                        context,
                        values,
                    );
                }
            }
        }
        return eval_native_method_with_evaluated_args(
            object,
            &class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    };
    let called_class_name = class.name().to_string();
    if eval_enum_static_builtin_applies(&called_class_name, method_name, context).is_some() {
        return eval_enum_builtin_static_method_result(
            &called_class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    }
    let mut inaccessible_method = None;
    if let Some((class_name, method)) =
        eval_dynamic_method_for_call(&called_class_name, method_name, context)
    {
        if method.is_abstract() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if validate_eval_member_access(&class_name, method.visibility(), context).is_ok() {
            if method.is_static() {
                return eval_dynamic_static_method_with_values(
                    &class_name,
                    &called_class_name,
                    &method,
                    evaluated_args,
                    context,
                    values,
                );
            }
            return eval_dynamic_method_with_values(
                &class_name,
                &called_class_name,
                &method,
                object,
                evaluated_args,
                context,
                values,
            );
        }
        inaccessible_method = Some((class_name, method));
    }
    if inaccessible_method.is_none() {
        if let Some(parent) = context.class_native_parent_name(&called_class_name) {
            if let Some((declaring_class, _, _, _)) =
                eval_aot_method_dispatch_metadata_in_hierarchy(
                    &parent,
                    method_name,
                    context,
                    values,
                )?
            {
                return eval_native_method_with_evaluated_args_bridge_scope(
                    object,
                    &parent,
                    method_name,
                    evaluated_args,
                    Some(&declaring_class),
                    Some(&called_class_name),
                    context,
                    values,
                );
            }
            if eval_native_instance_magic_method_available(&parent, context, values)? {
                return eval_native_method_with_evaluated_args(
                    object,
                    &parent,
                    method_name,
                    evaluated_args,
                    context,
                    values,
                );
            }
        }
    }
    if let Some(result) = eval_magic_instance_method_call(
        object,
        &called_class_name,
        method_name,
        evaluated_args,
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some((declaring_class, method)) = inaccessible_method {
        return eval_throw_method_access_error(
            &declaring_class,
            method.name(),
            method.visibility(),
            context,
            values,
        );
    }
    eval_throw_undefined_method_call_error(&called_class_name, method_name, context, values)
}

/// Dispatches PHP-visible methods on eval-backed `Closure` objects.
fn eval_closure_object_method_result(
    target: EvalClosureObjectTarget,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if method_name.eq_ignore_ascii_case("bindTo") {
        let bound_this = eval_closure_bind_to_args(evaluated_args, values)?;
        return eval_closure_bind_target(target, bound_this, context, values).map(Some);
    }
    if !method_name.eq_ignore_ascii_case("call") {
        return Ok(None);
    }
    let (bound_this, call_args) = eval_closure_call_split_args(evaluated_args)?;
    match target {
        EvalClosureObjectTarget::Named(name) => {
            if let Some(closure) = context.closure(&name).cloned() {
                return eval_closure_with_evaluated_args_and_bound_this(
                    &closure,
                    bound_this,
                    call_args,
                    context,
                    values,
                )
                .map(Some);
            }
            eval_closure_call_warning_null(
                "Cannot rebind scope of closure created from function",
                values,
            )
            .map(Some)
        }
        EvalClosureObjectTarget::BoundNamed { name, .. } => {
            if let Some(closure) = context.closure(&name).cloned() {
                return eval_closure_with_evaluated_args_and_bound_this(
                    &closure,
                    bound_this,
                    call_args,
                    context,
                    values,
                )
                .map(Some);
            }
            eval_closure_call_warning_null(
                "Cannot rebind scope of closure created from function",
                values,
            )
            .map(Some)
        }
        EvalClosureObjectTarget::InvokableObject { object } => {
            if !eval_closure_call_bound_class_matches(object, bound_this, context, values)? {
                return eval_closure_call_warning_null(
                    "Cannot rebind scope of closure created from method",
                    values,
                )
                .map(Some);
            }
            eval_invokable_object_call_result(bound_this, call_args, context, values).map(Some)
        }
        EvalClosureObjectTarget::ObjectMethod {
            object,
            method,
            called_class: _,
            native_class,
            bridge_scope,
        } => {
            if !eval_closure_call_bound_class_matches(object, bound_this, context, values)? {
                return eval_closure_call_warning_null(
                    "Cannot rebind scope of closure created from method",
                    values,
                )
                .map(Some);
            }
            let called_class = Some(eval_closure_bound_object_class_name(
                bound_this, context, values,
            )?);
            let callable = EvaluatedCallable::ObjectMethod {
                object: bound_this,
                method,
                called_class,
                native_class,
                bridge_scope,
            };
            eval_evaluated_callable_with_call_array_args(&callable, call_args, context, values)
                .map(Some)
        }
        EvalClosureObjectTarget::StaticMethod { .. } => eval_closure_call_warning_null(
            "Cannot bind an instance to a static closure",
            values,
        )
        .map(Some),
    }
}

/// Splits `Closure::call()` arguments into the bound object and forwarded closure args.
fn eval_closure_call_split_args(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<(RuntimeCellHandle, Vec<EvaluatedCallArg>), EvalStatus> {
    let mut bound_this = None;
    let mut consumed_positional_receiver = false;
    let mut call_args = Vec::with_capacity(evaluated_args.len().saturating_sub(1));

    for arg in evaluated_args {
        if arg
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("newThis"))
        {
            if bound_this.replace(arg.value).is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            continue;
        }
        if arg.name.is_none() && !consumed_positional_receiver && bound_this.is_none() {
            consumed_positional_receiver = true;
            bound_this = Some(arg.value);
            continue;
        }
        call_args.push(arg);
    }

    bound_this
        .map(|receiver| (receiver, call_args))
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns whether `Closure::call()` may bind a method closure to the new object.
fn eval_closure_call_bound_class_matches(
    original_object: RuntimeCellHandle,
    bound_this: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let original_class = eval_closure_bound_object_class_name(original_object, context, values)?;
    let bound_class = eval_closure_bound_object_class_name(bound_this, context, values)?;
    Ok(original_class.eq_ignore_ascii_case(&bound_class))
}

/// Emits PHP's `Closure::call()` warning and returns `null`.
fn eval_closure_call_warning_null(
    message: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.warning(message)?;
    values.null()
}

/// Dispatches an invokable object through `__invoke()` without enforcing hook visibility.
pub(in crate::interpreter) fn eval_invokable_object_call_result(
    object: RuntimeCellHandle,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        let evaluated_args = positional_evaluated_arg_values(evaluated_args)?;
        return values.method_call(object, "__invoke", evaluated_args);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = runtime_object_class_name(object, values)?;
        let Some((_, _, is_static, is_abstract)) =
            eval_aot_method_dispatch_metadata_in_hierarchy(
                &class_name,
                "__invoke",
                context,
                values,
            )?
        else {
            return eval_throw_object_not_callable_error(&class_name, context, values);
        };
        if is_static || is_abstract {
            return Err(EvalStatus::RuntimeFatal);
        }
        return eval_native_method_with_evaluated_args_unchecked(
            object,
            &class_name,
            "__invoke",
            evaluated_args,
            context,
            values,
        );
    };
    let called_class_name = class.name().to_string();
    let Some((declaring_class, method)) = context.class_method(&called_class_name, "__invoke")
    else {
        if let Some(native_class_name) =
            eval_dynamic_class_native_invokable_method_class(&called_class_name, context, values)?
        {
            return eval_native_method_with_evaluated_args_unchecked_bridge_scope(
                object,
                &native_class_name,
                "__invoke",
                evaluated_args,
                Some(&native_class_name),
                Some(&called_class_name),
                context,
                values,
            );
        }
        return eval_throw_object_not_callable_error(&called_class_name, context, values);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_dynamic_method_with_values(
        &declaring_class,
        &called_class_name,
        &method,
        object,
        evaluated_args,
        context,
        values,
    )
}

/// Rejects non-invokable eval-declared objects before dynamic-call arguments are evaluated.
pub(in crate::interpreter) fn eval_invokable_object_precheck(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(());
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = runtime_object_class_name(object, values)?;
        let Some((_, _, is_static, is_abstract)) =
            eval_aot_method_dispatch_metadata_in_hierarchy(
                &class_name,
                "__invoke",
                context,
                values,
            )?
        else {
            return eval_throw_object_not_callable_error(&class_name, context, values);
        };
        if is_static || is_abstract {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(());
    };
    let called_class_name = class.name().to_string();
    let Some((_, method)) = context.class_method(&called_class_name, "__invoke") else {
        if eval_dynamic_class_native_invokable_method_class(&called_class_name, context, values)?
            .is_some()
        {
            return Ok(());
        }
        return eval_throw_object_not_callable_error(&called_class_name, context, values);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Returns the generated/AOT class that can dispatch an inherited `__invoke()` hook.
pub(in crate::interpreter) fn eval_dynamic_class_native_invokable_method_class(
    called_class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some((declaring_class, _, is_static, is_abstract)) =
        eval_dynamic_class_native_method_metadata(called_class_name, "__invoke", context, values)?
    else {
        return Ok(None);
    };
    if is_static || is_abstract {
        return Ok(None);
    }
    Ok(Some(declaring_class))
}

/// Returns generated/AOT method metadata inherited by an eval-declared class.
pub(in crate::interpreter) fn eval_dynamic_class_native_method_metadata(
    called_class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility, bool, bool)>, EvalStatus> {
    let Some(parent) = context.class_native_parent_name(called_class_name) else {
        return Ok(None);
    };
    eval_aot_method_dispatch_metadata_in_hierarchy(&parent, method_name, context, values)
}

/// Dispatches a missing or inaccessible eval instance method through `__call()`.
fn eval_magic_instance_method_call(
    object: RuntimeCellHandle,
    called_class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(called_class_name, "__call") else {
        return Ok(None);
    };
    if method.is_static() || method.is_abstract() {
        return Ok(None);
    }
    let magic_args = eval_magic_call_args(method_name, evaluated_args, values)?;
    eval_dynamic_method_with_values(
        &declaring_class,
        called_class_name,
        &method,
        object,
        magic_args,
        context,
        values,
    )
    .map(Some)
}

/// Dispatches a missing or inaccessible eval static method through `__callStatic()`.
fn eval_magic_static_method_call(
    class_name: &str,
    called_class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(class_name, "__callStatic") else {
        return Ok(None);
    };
    if !method.is_static() || method.is_abstract() {
        return Ok(None);
    }
    let magic_args = eval_magic_call_args(method_name, evaluated_args, values)?;
    eval_dynamic_static_method_with_values(
        &declaring_class,
        called_class_name,
        &method,
        magic_args,
        context,
        values,
    )
    .map(Some)
}

/// Builds the two synthetic arguments passed to `__call()` and `__callStatic()`.
fn eval_magic_call_args(
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let method = values.string(method_name)?;
    let args = eval_magic_call_arg_array(evaluated_args, values)?;
    Ok(positional_args(vec![method, args]))
}

/// Materializes PHP's `$args` array for a magic method fallback.
fn eval_magic_call_arg_array(
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let contains_named = evaluated_args.iter().any(|arg| arg.name.is_some());
    let mut args = if contains_named {
        values.assoc_new(evaluated_args.len())?
    } else {
        values.array_new(evaluated_args.len())?
    };
    let mut next_positional = 0_i64;
    for arg in evaluated_args {
        let key = if let Some(name) = arg.name {
            values.string(&name)?
        } else {
            let key = values.int(next_positional)?;
            next_positional = next_positional
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            key
        };
        args = values.array_set(args, key, arg.value)?;
    }
    Ok(args)
}

/// Returns the runtime-visible class name for a non-eval object receiver.
pub(in crate::interpreter) fn runtime_object_class_name(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let class_name = values.object_class_name(object)?;
    let bytes = values.string_bytes(class_name);
    values.release(class_name)?;
    String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Instantiates the class named by a materialized eval `ReflectionClass` object.
fn eval_reflection_class_new_instance_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let constructor_args = if method_name.eq_ignore_ascii_case("newInstance") {
        evaluated_args
    } else if method_name.eq_ignore_ascii_case("newInstanceArgs") {
        eval_reflection_class_new_instance_args(evaluated_args, context, values)?
    } else {
        return Ok(None);
    };
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    if let Some(message) =
        eval_reflection_eval_instantiation_error_message(&reflected_name, context)
    {
        return eval_throw_error(&message, context, values);
    }
    if let Some(class) = context.class(&reflected_name).cloned() {
        if let Some((_, constructor)) = context.class_method(class.name(), "__construct") {
            if constructor.visibility() != EvalVisibility::Public {
                return eval_throw_reflection_exception(
                    &format!(
                        "Access to non-public constructor of class {}",
                        class.name()
                    ),
                    context,
                    values,
                );
            }
        }
        return eval_reflection_public_constructor_scope(context, values, |context, values| {
            let mut scope = ElephcEvalScope::new();
            eval_dynamic_class_new_object(&class, constructor_args, context, &mut scope, values)
                .map(Some)
        });
    }
    let class_name = context
        .resolve_class_name(&reflected_name)
        .unwrap_or(reflected_name);
    if let Some(error) = eval_reflection_aot_class_public_instantiation_error(&class_name, values)?
    {
        return eval_throw_reflection_instantiation_error(error, context, values);
    }
    eval_reflection_public_constructor_scope(context, values, |context, values| {
        let instance = values.new_object(&class_name)?;
        eval_native_constructor_with_evaluated_args(
            &class_name,
            instance,
            constructor_args,
            context,
            values,
        )?;
        Ok(Some(instance))
    })
}

/// Expands the single `ReflectionClass::newInstanceArgs()` array argument.
fn eval_reflection_class_new_instance_args(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("args")], evaluated_args)?;
    eval_array_call_arg_values(args[0], context, values)
}

/// Runs ReflectionClass construction with only public constructor visibility.
fn eval_reflection_public_constructor_scope<T, V: RuntimeValueOps>(
    context: &mut ElephcEvalContext,
    values: &mut V,
    action: impl FnOnce(&mut ElephcEvalContext, &mut V) -> Result<T, EvalStatus>,
) -> Result<T, EvalStatus> {
    context.push_class_scope(String::new());
    let result = action(context, values);
    context.pop_class_scope();
    result
}

/// Allocates the class named by a materialized eval `ReflectionClass` without running `__construct()`.
fn eval_reflection_class_new_instance_without_constructor_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("newInstanceWithoutConstructor") {
        return Ok(None);
    }
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    if let Some(message) =
        eval_reflection_eval_instantiation_error_message(&reflected_name, context)
    {
        return eval_throw_error(&message, context, values);
    }
    if let Some(class) = context.class(&reflected_name).cloned() {
        let mut scope = ElephcEvalScope::new();
        return eval_dynamic_class_allocate_object(&class, context, &mut scope, values).map(Some);
    }
    if context.has_interface(&reflected_name)
        || context.has_trait(&reflected_name)
        || context.has_enum(&reflected_name)
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let class_name = context
        .resolve_class_name(&reflected_name)
        .unwrap_or(reflected_name);
    if let Some(message) =
        eval_reflection_aot_class_without_constructor_error(&class_name, values)?
    {
        return eval_throw_error(&message, context, values);
    }
    values.new_object(&class_name).map(Some)
}

/// Builds PHP's reflection instantiation error for eval non-instantiable class-likes.
fn eval_reflection_eval_instantiation_error_message(
    reflected_name: &str,
    context: &ElephcEvalContext,
) -> Option<String> {
    if let Some(class) = context.class(reflected_name) {
        if class.is_abstract() {
            return Some(format!("Cannot instantiate abstract class {}", class.name()));
        }
        if let Some(enum_decl) = context.enum_decl(class.name()) {
            return Some(format!("Cannot instantiate enum {}", enum_decl.name()));
        }
        return None;
    }
    if let Some(interface) = context.interface(reflected_name) {
        return Some(format!("Cannot instantiate interface {}", interface.name()));
    }
    if let Some(trait_decl) = context.trait_decl(reflected_name) {
        return Some(format!("Cannot instantiate trait {}", trait_decl.name()));
    }
    context
        .enum_decl(reflected_name)
        .map(|enum_decl| format!("Cannot instantiate enum {}", enum_decl.name()))
}

/// Instantiates an attribute class for `ReflectionAttribute::newInstance()`.
fn eval_reflection_attribute_new_instance_result(
    attribute: &EvalAttribute,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = eval_reflection_attribute_evaluated_args(attribute, values)?;
    if let Some(class) = context.class(attribute.name()).cloned() {
        let mut scope = ElephcEvalScope::new();
        return eval_dynamic_class_new_object(&class, args, context, &mut scope, values);
    }
    let class_name = context
        .resolve_class_name(attribute.name())
        .unwrap_or_else(|| attribute.name().trim_start_matches('\\').to_string());
    if !values.class_exists(&class_name)? {
        return values.null();
    }
    let object = values.new_object(&class_name)?;
    if let Err(err) = eval_native_constructor_with_evaluated_args(
        &class_name,
        object,
        args,
        context,
        values,
    ) {
        let _ = values.release(object);
        return Err(err);
    }
    Ok(object)
}

/// Materializes eval attribute literal arguments as evaluated constructor args.
fn eval_reflection_attribute_evaluated_args(
    attribute: &EvalAttribute,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let Some(args) = attribute.args() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    args.iter()
        .map(|arg| {
            Ok(EvaluatedCallArg {
                name: arg.name().map(str::to_string),
                value: eval_reflection_attribute_arg_value(arg.value(), values)?,
                ref_target: None,
            })
        })
        .collect()
}

/// Materializes one eval attribute literal as a constructor argument cell.
fn eval_reflection_attribute_arg_value(
    arg: &EvalAttributeArg,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match arg {
        EvalAttributeArg::String(value) => values.string(value),
        EvalAttributeArg::Int(value) => values.int(*value),
        EvalAttributeArg::Float(bits) => values.float(f64::from_bits(*bits)),
        EvalAttributeArg::Bool(value) => values.bool_value(*value),
        EvalAttributeArg::Null => values.null(),
        EvalAttributeArg::Array(elements) => {
            eval_reflection_attribute_array_arg_value(elements, values)
        }
        EvalAttributeArg::Named { value, .. } | EvalAttributeArg::IntKeyed { value, .. } => {
            eval_reflection_attribute_arg_value(value, values)
        }
    }
}

/// Materializes one retained attribute array literal for constructor calls.
fn eval_reflection_attribute_array_arg_value(
    elements: &[EvalAttributeArg],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = if elements
        .iter()
        .any(|element| element.name().is_some() || element.int_key().is_some())
    {
        values.assoc_new(elements.len())?
    } else {
        values.array_new(elements.len())?
    };
    for (index, element) in elements.iter().enumerate() {
        let key = match element.name() {
            Some(name) => values.string(name)?,
            None => values.int(element.int_key().unwrap_or(index as i64))?,
        };
        let value = eval_reflection_attribute_arg_value(element.value(), values)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Resolves the method metadata visible from the current class scope.
pub(in crate::interpreter) fn eval_dynamic_method_for_call(
    object_class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassMethod)> {
    if let Some(current_class) = context.current_class_scope() {
        if context.class_is_a(object_class_name, current_class, false) {
            if let Some((declaring_class, method)) =
                context.class_own_method(current_class, method_name)
            {
                if method.visibility() == EvalVisibility::Private {
                    return Some((declaring_class, method));
                }
            }
        }
    }
    context.class_method(object_class_name, method_name)
}

/// Returns whether the current eval class scope can access one declared member.
pub(in crate::interpreter) fn validate_eval_member_access(
    declaring_class: &str,
    visibility: EvalVisibility,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if visibility == EvalVisibility::Public {
        return Ok(());
    }
    let Some(current_class) = context.current_class_scope() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    match visibility {
        EvalVisibility::Public => Ok(()),
        EvalVisibility::Private => same_eval_class_name(current_class, declaring_class)
            .then_some(())
            .ok_or(EvalStatus::RuntimeFatal),
        EvalVisibility::Protected => {
            eval_classes_are_related(current_class, declaring_class, context)
                .then_some(())
                .ok_or(EvalStatus::RuntimeFatal)
        }
    }
}

/// Returns true when two PHP class names refer to the same eval class.
fn same_eval_class_name(left: &str, right: &str) -> bool {
    left.trim_start_matches('\\')
        .eq_ignore_ascii_case(right.trim_start_matches('\\'))
}

/// Returns true when two eval or generated classes are in the same inheritance family.
fn eval_classes_are_related(left: &str, right: &str, context: &ElephcEvalContext) -> bool {
    same_eval_class_name(left, right)
        || context.class_is_a(left, right, false)
        || context.class_is_a(right, left, false)
        || native_class_is_a(left, right, context)
        || native_class_is_a(right, left, context)
}

/// Returns true when generated AOT parent metadata proves one class extends another.
fn native_class_is_a(class_name: &str, target: &str, context: &ElephcEvalContext) -> bool {
    let mut current = class_name.trim_start_matches('\\').to_string();
    let target = target.trim_start_matches('\\');
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(current.to_ascii_lowercase()) {
            return false;
        }
        if same_eval_class_name(&current, target) {
            return true;
        }
        let Some(parent) = context.native_class_parent(&current) else {
            return false;
        };
        current = parent.to_string();
    }
}

/// Binds method parameters into a fresh method scope and marks by-reference params as aliases.
pub(in crate::interpreter) fn bind_method_scope_args(
    method_scope: &mut ElephcEvalScope,
    params: &[String],
    parameter_is_by_ref: &[bool],
    bound_args: &[BoundMethodArg],
) {
    for (position, (name, bound_arg)) in params.iter().zip(bound_args.iter()).enumerate() {
        if parameter_is_by_ref.get(position).copied().unwrap_or(false) {
            method_scope.set_reference(
                name.clone(),
                name.clone(),
                bound_arg.value,
                ScopeCellOwnership::Borrowed,
            );
            if let Some(target) = bound_arg.ref_target.clone() {
                method_scope.set_reference_target(name.clone(), target);
            }
        } else {
            method_scope.set(name.clone(), bound_arg.value, ScopeCellOwnership::Borrowed);
        }
    }
    alias_duplicate_method_ref_args(method_scope, params, bound_args);
}

/// Creates local aliases when two by-reference method parameters point at the same caller variable.
fn alias_duplicate_method_ref_args(
    method_scope: &mut ElephcEvalScope,
    params: &[String],
    bound_args: &[BoundMethodArg],
) {
    for (position, bound_arg) in bound_args.iter().enumerate() {
        let Some(target) = bound_arg.ref_target.as_ref() else {
            continue;
        };
        let Some(param) = params.get(position) else {
            continue;
        };
        for previous_position in 0..position {
            let Some(previous_target) = bound_args[previous_position].ref_target.as_ref() else {
                continue;
            };
            if !same_method_ref_target(target, previous_target) {
                continue;
            }
            if let Some(previous_param) = params.get(previous_position) {
                method_scope.set_reference(
                    param.clone(),
                    previous_param.clone(),
                    bound_args[previous_position].value,
                    ScopeCellOwnership::Borrowed,
                );
            }
            break;
        }
    }
}

/// Returns true when two evaluated arguments target the same caller-side variable.
fn same_method_ref_target(left: &EvalReferenceTarget, right: &EvalReferenceTarget) -> bool {
    match (left, right) {
        (
            EvalReferenceTarget::Variable {
                scope: left_scope,
                name: left_name,
            },
            EvalReferenceTarget::Variable {
                scope: right_scope,
                name: right_name,
            },
        ) => left_scope == right_scope && left_name == right_name,
        (
            EvalReferenceTarget::ArrayElement {
                scope: left_scope,
                array_name: left_name,
                index: left_index,
            },
            EvalReferenceTarget::ArrayElement {
                scope: right_scope,
                array_name: right_name,
                index: right_index,
            },
        ) => left_scope == right_scope && left_name == right_name && left_index == right_index,
        (
            EvalReferenceTarget::NestedArrayElement {
                array_target: left_target,
                index: left_index,
            },
            EvalReferenceTarget::NestedArrayElement {
                array_target: right_target,
                index: right_index,
            },
        ) => left_index == right_index && same_method_ref_target(left_target, right_target),
        (
            EvalReferenceTarget::ObjectProperty {
                object: left_object,
                property: left_property,
                access_scope: left_access_scope,
            },
            EvalReferenceTarget::ObjectProperty {
                object: right_object,
                property: right_property,
                access_scope: right_access_scope,
            },
        ) => {
            left_object == right_object
                && left_property == right_property
                && left_access_scope == right_access_scope
        }
        (
            EvalReferenceTarget::Cell { cell: left_cell },
            EvalReferenceTarget::Cell { cell: right_cell },
        ) => left_cell == right_cell,
        (
            EvalReferenceTarget::StaticProperty {
                class_name: left_class_name,
                property: left_property,
                access_scope: left_access_scope,
            },
            EvalReferenceTarget::StaticProperty {
                class_name: right_class_name,
                property: right_property,
                access_scope: right_access_scope,
            },
        ) => {
            left_class_name == right_class_name
                && left_property == right_property
                && left_access_scope == right_access_scope
        }
        _ => false,
    }
}

/// Writes completed by-reference method parameter values back to their caller-side variables.
pub(in crate::interpreter) fn write_back_method_ref_args(
    params: &[String],
    bound_args: &[BoundMethodArg],
    method_scope: &ElephcEvalScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (position, bound_arg) in bound_args.iter().enumerate() {
        let Some(param) = params.get(position) else {
            continue;
        };
        if let Some(target) = bound_arg.ref_target.as_ref() {
            let Some(entry) = method_scope
                .entry(param)
                .filter(|entry| entry.flags().is_visible() && entry.flags().by_ref)
            else {
                continue;
            };
            write_back_method_ref_target(target, entry.cell(), context, values)?;
        }
        write_back_method_variadic_ref_args(param, bound_arg, method_scope, context, values)?;
    }
    Ok(())
}

/// Writes element-level changes from a by-reference variadic method parameter back to callers.
fn write_back_method_variadic_ref_args(
    param: &str,
    bound_arg: &BoundMethodArg,
    method_scope: &ElephcEvalScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if bound_arg.variadic_ref_targets.is_empty() {
        return Ok(());
    }
    let Some(entry) = method_scope
        .entry(param)
        .filter(|entry| entry.flags().is_visible() && entry.flags().by_ref)
    else {
        return Ok(());
    };
    if entry.cell() != bound_arg.value {
        return Ok(());
    }
    for (key, target) in &bound_arg.variadic_ref_targets {
        let value = values.array_get(entry.cell(), *key)?;
        write_back_method_ref_target(target, value, context, values)?;
    }
    Ok(())
}

/// Stores one by-reference result in the original caller-side target.
pub(in crate::interpreter) fn write_back_method_ref_target(
    target: &EvalReferenceTarget,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match target {
        EvalReferenceTarget::Variable { scope, name } => {
            let Some(scope) = (unsafe { scope.as_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            for replaced in set_scope_cell(
                context,
                scope,
                name.clone(),
                value,
                ScopeCellOwnership::Borrowed,
            )? {
                values.release(replaced)?;
            }
            Ok(())
        }
        EvalReferenceTarget::ArrayElement {
            scope,
            array_name,
            index,
        } => {
            let Some(scope) = (unsafe { scope.as_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            write_back_method_array_element_ref_target(
                scope, array_name, *index, value, context, values,
            )
        }
        EvalReferenceTarget::NestedArrayElement {
            array_target,
            index,
        } => write_back_method_nested_array_element_ref_target(
            array_target,
            *index,
            value,
            context,
            values,
        ),
        EvalReferenceTarget::ObjectProperty {
            object,
            property,
            access_scope,
        } => write_back_method_object_property_ref_target(
            *object,
            property,
            access_scope.clone(),
            value,
            context,
            values,
        ),
        EvalReferenceTarget::StaticProperty {
            class_name,
            property,
            access_scope,
        } => write_back_method_static_property_ref_target(
            class_name,
            property,
            access_scope.clone(),
            value,
            context,
            values,
        ),
        EvalReferenceTarget::Cell { .. } => Ok(()),
    }
}

/// Stores one by-reference method result in a caller-side array element.
fn write_back_method_array_element_ref_target(
    scope: &mut ElephcEvalScope,
    array_name: &str,
    index: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut ownership = ScopeCellOwnership::Owned;
    let array = if let Some(existing) =
        scope_entry(context, scope, array_name).filter(|entry| entry.flags().is_visible())
    {
        if values.is_array_like(existing.cell())? {
            ownership = existing.flags().ownership;
            values.array_clone_shallow(existing.cell())?
        } else {
            eval_new_array_for_index(index, values)?
        }
    } else {
        eval_new_array_for_index(index, values)?
    };
    let array = values.array_set(array, index, value)?;
    for replaced in set_scope_cell(context, scope, array_name.to_string(), array, ownership)? {
        values.release(replaced)?;
    }
    Ok(())
}

/// Stores one by-reference method result in an element of a nested caller-side array target.
fn write_back_method_nested_array_element_ref_target(
    array_target: &EvalReferenceTarget,
    index: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let current = eval_reference_target_value(array_target, context, values)?;
    let array = if values.is_array_like(current)? {
        values.array_clone_shallow(current)?
    } else {
        eval_new_array_for_index(index, values)?
    };
    let array = values.array_set(array, index, value)?;
    write_back_method_ref_target(array_target, array, context, values)
}

/// Stores one by-reference method result in a caller-side object property.
fn write_back_method_object_property_ref_target(
    object: RuntimeCellHandle,
    property: &str,
    access_scope: ElephcEvalExecutionScope,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let previous_scope = context.replace_execution_scope(access_scope);
    let result = eval_property_set_result(object, property, value, context, values);
    context.replace_execution_scope(previous_scope);
    result
}

/// Stores one by-reference method result in a caller-side static property.
fn write_back_method_static_property_ref_target(
    class_name: &str,
    property: &str,
    access_scope: ElephcEvalExecutionScope,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let previous_scope = context.replace_execution_scope(access_scope);
    let result = eval_static_property_set_result(class_name, property, value, context, values);
    context.replace_execution_scope(previous_scope);
    result
}

/// Creates an indexed or associative array according to the first write key.
fn eval_new_array_for_index(
    index: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(index)? == EVAL_TAG_STRING {
        values.assoc_new(1)
    } else {
        values.array_new(1)
    }
}

/// Executes one eval-declared class method with `$this` bound in method scope.
pub(in crate::interpreter) fn eval_dynamic_method_with_values(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    object: RuntimeCellHandle,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_dynamic_method_with_values_and_ref_flags(
        class_name,
        called_class_name,
        method,
        object,
        method.parameter_is_by_ref(),
        evaluated_args,
        context,
        values,
    )
}

/// Executes one eval-declared class method with caller-selected by-ref binding flags.
pub(in crate::interpreter) fn eval_dynamic_method_with_values_and_ref_flags(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    object: RuntimeCellHandle,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let qualified_method_name =
        format!("{}::{}", class_name.trim_start_matches('\\'), method.name());
    let static_names = static_var_names(method.body());
    context.push_function(qualified_method_name.clone());
    context.push_class_scope(class_name.to_string());
    context.push_called_class_scope(called_class_name.to_string());
    context.push_method_magic_scope(class_name, method);
    let evaluated_args = match bind_evaluated_method_args(
        method.params(),
        method.parameter_types(),
        method.parameter_defaults(),
        parameter_is_by_ref,
        method.parameter_is_variadic(),
        evaluated_args,
        context,
        values,
    ) {
        Ok(args) => args,
        Err(status) => {
            context.pop_magic_scope();
            context.pop_called_class_scope();
            context.pop_class_scope();
            context.pop_function();
            return Err(status);
        }
    };
    let mut method_scope = ElephcEvalScope::new();
    method_scope.set("this", object, ScopeCellOwnership::Borrowed);
    bind_method_scope_args(
        &mut method_scope,
        method.params(),
        parameter_is_by_ref,
        &evaluated_args,
    );
    let result = execute_statements(method.body(), context, &mut method_scope, values);
    let persist_result = persist_static_locals(
        context,
        &qualified_method_name,
        &static_names,
        &method_scope,
        values,
    );
    let writeback_result = write_back_method_ref_args(
        method.params(),
        &evaluated_args,
        &method_scope,
        context,
        values,
    );
    let return_result = match (persist_result, writeback_result, result) {
        (Err(status), _, _) | (_, Err(status), _) | (_, _, Err(status)) => Err(status),
        (Ok(()), Ok(()), Ok(control)) => eval_declared_return_control_value(
            method.return_type(),
            Some(class_name),
            Some(called_class_name),
            control,
            context,
            values,
        ),
    };
    context.pop_magic_scope();
    context.pop_called_class_scope();
    context.pop_class_scope();
    context.pop_function();
    return_result
}

/// Executes one eval-declared static class method without binding `$this`.
pub(in crate::interpreter) fn eval_dynamic_static_method_with_values(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_dynamic_static_method_with_values_and_ref_flags(
        class_name,
        called_class_name,
        method,
        method.parameter_is_by_ref(),
        evaluated_args,
        context,
        values,
    )
}

/// Executes one eval-declared static method with caller-selected by-ref binding flags.
pub(in crate::interpreter) fn eval_dynamic_static_method_with_values_and_ref_flags(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let qualified_method_name =
        format!("{}::{}", class_name.trim_start_matches('\\'), method.name());
    let static_names = static_var_names(method.body());
    context.push_function(qualified_method_name.clone());
    context.push_class_scope(class_name.to_string());
    context.push_called_class_scope(called_class_name.to_string());
    context.push_method_magic_scope(class_name, method);
    let evaluated_args = match bind_evaluated_method_args(
        method.params(),
        method.parameter_types(),
        method.parameter_defaults(),
        parameter_is_by_ref,
        method.parameter_is_variadic(),
        evaluated_args,
        context,
        values,
    ) {
        Ok(args) => args,
        Err(status) => {
            context.pop_magic_scope();
            context.pop_called_class_scope();
            context.pop_class_scope();
            context.pop_function();
            return Err(status);
        }
    };
    let mut method_scope = ElephcEvalScope::new();
    bind_method_scope_args(
        &mut method_scope,
        method.params(),
        parameter_is_by_ref,
        &evaluated_args,
    );
    let result = execute_statements(method.body(), context, &mut method_scope, values);
    let persist_result = persist_static_locals(
        context,
        &qualified_method_name,
        &static_names,
        &method_scope,
        values,
    );
    let writeback_result = write_back_method_ref_args(
        method.params(),
        &evaluated_args,
        &method_scope,
        context,
        values,
    );
    let return_result = match (persist_result, writeback_result, result) {
        (Err(status), _, _) | (_, Err(status), _) | (_, _, Err(status)) => Err(status),
        (Ok(()), Ok(()), Ok(control)) => eval_declared_return_control_value(
            method.return_type(),
            Some(class_name),
            Some(called_class_name),
            control,
            context,
            values,
        ),
    };
    context.pop_magic_scope();
    context.pop_called_class_scope();
    context.pop_class_scope();
    context.pop_function();
    return_result
}

/// Wraps positional method arguments into the shared dynamic-call binding shape.
pub(in crate::interpreter) fn positional_args(
    args: Vec<RuntimeCellHandle>,
) -> Vec<EvaluatedCallArg> {
    args.into_iter()
        .map(|value| EvaluatedCallArg {
            name: None,
            value,
            ref_target: None,
        })
        .collect()
}

/// Extracts positional runtime values and rejects named args before runtime method dispatch.
pub(in crate::interpreter) fn positional_evaluated_arg_values(
    args: Vec<EvaluatedCallArg>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if args.iter().any(|arg| arg.name.is_some()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(args.into_iter().map(|arg| arg.value).collect())
}

/// Binds native AOT callable args while preserving by-reference caller targets.
pub(in crate::interpreter) fn bind_native_callable_bound_args(
    signature: Option<NativeCallableSignature>,
    args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<BoundMethodArg>, EvalStatus> {
    bind_native_callable_bound_args_with_mode(
        signature,
        args,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Binds native AOT callable args using the selected by-reference degradation mode.
fn bind_native_callable_bound_args_with_mode(
    signature: Option<NativeCallableSignature>,
    args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<BoundMethodArg>, EvalStatus> {
    let Some(signature) = signature else {
        return positional_evaluated_bound_args(None, args, by_ref_mode, context, values);
    };
    if !signature.bridge_supported() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if signature.param_names().len() == signature.param_count() {
        bind_native_signature_args(&signature, args, by_ref_mode, context, values)
    } else {
        positional_evaluated_bound_args(Some(&signature), args, by_ref_mode, context, values)
    }
}

/// Binds positional-only native AOT args and validates registered by-reference slots.
fn positional_evaluated_bound_args(
    signature: Option<&NativeCallableSignature>,
    args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<BoundMethodArg>, EvalStatus> {
    if args.iter().any(|arg| arg.name.is_some()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut bound_args = args
        .into_iter()
        .enumerate()
        .map(|(index, arg)| {
            let ref_target = match signature {
                Some(signature) => native_parameter_ref_target(
                    signature,
                    Some(index),
                    arg.ref_target,
                    by_ref_mode,
                    values,
                )?,
                None => None,
            };
            Ok(BoundMethodArg {
                value: arg.value,
                ref_target,
                variadic_ref_targets: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(signature) = signature {
        apply_native_callable_bound_arg_types(signature, &mut bound_args, context, values)?;
        copy_native_call_user_func_by_value_ref_args(
            signature,
            &mut bound_args,
            by_ref_mode,
            values,
        )?;
    }
    Ok(bound_args)
}

/// Returns only runtime cell values from bound native AOT call arguments.
pub(in crate::interpreter) fn native_bound_arg_values(
    args: &[BoundMethodArg],
) -> Vec<RuntimeCellHandle> {
    args.iter().map(|arg| arg.value).collect()
}

/// Writes native AOT by-reference argument cells back to their eval caller targets.
pub(in crate::interpreter) fn write_back_native_callable_ref_args(
    bound_args: &[BoundMethodArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for bound_arg in bound_args {
        if let Some(target) = bound_arg.ref_target.as_ref() {
            write_back_method_ref_target(target, bound_arg.value, context, values)?;
        }
        for (key, target) in &bound_arg.variadic_ref_targets {
            let value = values.array_get(bound_arg.value, *key)?;
            write_back_method_ref_target(target, value, context, values)?;
        }
    }
    Ok(())
}

/// Binds native AOT callable args and fills omitted defaults from metadata.
fn bind_native_signature_args(
    signature: &NativeCallableSignature,
    args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<BoundMethodArg>, EvalStatus> {
    let mut bound_args = vec![None; signature.param_count()];
    let variadic_index = native_callable_variadic_index(signature);
    let mut next_positional = 0;
    let mut next_variadic_index = 0_i64;

    if let Some(index) = variadic_index {
        let array = values.array_new(args.len())?;
        bound_args[index] = Some(BoundMethodArg {
            value: array,
            ref_target: None,
            variadic_ref_targets: Vec::new(),
        });
    }

    for arg in args {
        if let Some(name) = arg.name {
            bind_native_named_signature_arg(
                signature,
                variadic_index,
                &mut bound_args,
                &name,
                arg.value,
                arg.ref_target,
                by_ref_mode,
                values,
            )?;
        } else {
            bind_native_positional_signature_arg(
                signature,
                &mut bound_args,
                variadic_index,
                &mut next_positional,
                &mut next_variadic_index,
                arg.value,
                arg.ref_target,
                by_ref_mode,
                values,
            )?;
        }
    }

    for (position, value) in bound_args.iter_mut().enumerate() {
        if Some(position) == variadic_index {
            continue;
        }
        if value.is_some() {
            continue;
        }
        if position < signature.required_param_count() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let Some(default) = signature.param_default(position) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        *value = Some(BoundMethodArg {
            value: materialize_native_callable_default(default, context, values)?,
            ref_target: None,
            variadic_ref_targets: Vec::new(),
        });
    }

    let mut bound_args = bound_args
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(EvalStatus::RuntimeFatal)?;
    apply_native_callable_bound_arg_types(signature, &mut bound_args, context, values)?;
    copy_native_call_user_func_by_value_ref_args(
        signature,
        &mut bound_args,
        by_ref_mode,
        values,
    )?;
    Ok(bound_args)
}

/// Applies registered native AOT parameter types after argument binding and default filling.
fn apply_native_callable_bound_arg_types(
    signature: &NativeCallableSignature,
    bound_args: &mut [BoundMethodArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (position, bound_arg) in bound_args.iter_mut().enumerate() {
        let Some(param_type) = signature.param_type(position) else {
            continue;
        };
        if signature.param_variadic(position) {
            apply_native_callable_variadic_arg_type(param_type, bound_arg, context, values)?;
        } else {
            bound_arg.value =
                eval_method_parameter_value(param_type, bound_arg.value, context, values)?;
        }
    }
    Ok(())
}

/// Applies one registered native variadic parameter type to each collected argument.
fn apply_native_callable_variadic_arg_type(
    param_type: &EvalParameterType,
    bound_arg: &mut BoundMethodArg,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let len = values.array_len(bound_arg.value)?;
    for position in 0..len {
        let key = values.array_iter_key(bound_arg.value, position)?;
        let value = values.array_get(bound_arg.value, key)?;
        let value = eval_method_parameter_value(param_type, value, context, values)?;
        bound_arg.value = values.array_set(bound_arg.value, key, value)?;
    }
    Ok(())
}

/// Copies by-value degraded by-ref native method args before the generated bridge mutates them.
fn copy_native_call_user_func_by_value_ref_args(
    signature: &NativeCallableSignature,
    bound_args: &mut [BoundMethodArg],
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !matches!(by_ref_mode, EvalByRefBindingMode::WarnByValue { .. }) {
        return Ok(());
    }
    let variadic_index = native_callable_variadic_index(signature);
    for (position, bound_arg) in bound_args.iter_mut().enumerate() {
        let param_index = if variadic_index.is_some_and(|index| position >= index) {
            variadic_index.ok_or(EvalStatus::RuntimeFatal)?
        } else {
            position
        };
        if !signature.param_by_ref(param_index) || bound_arg.ref_target.is_some() {
            continue;
        }
        bound_arg.value = copy_native_call_user_func_by_value_ref_arg(bound_arg.value, values)?;
    }
    Ok(())
}

/// Allocates a temporary runtime cell for one by-value degraded by-ref native method arg.
fn copy_native_call_user_func_by_value_ref_arg(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    match tag {
        EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_BOOL | EVAL_TAG_RESOURCE => {
            let word = values.raw_value_word(value)?;
            values.raw_word_value(tag, word)
        }
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(value)?;
            values.string_bytes_value(&bytes)
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => values.array_clone_shallow(value),
        EVAL_TAG_OBJECT => {
            let word = values.raw_value_word(value)?;
            let retained = values.retain_raw_heap_word(word)?;
            values.raw_heap_word_value(retained)
        }
        EVAL_TAG_NULL => values.null(),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the native callable variadic slot, if metadata registered one.
fn native_callable_variadic_index(signature: &NativeCallableSignature) -> Option<usize> {
    (0..signature.param_count()).find(|index| signature.param_variadic(*index))
}

/// Binds one positional native AOT argument to a fixed slot or variadic array.
fn bind_native_positional_signature_arg(
    signature: &NativeCallableSignature,
    bound_args: &mut [Option<BoundMethodArg>],
    variadic_index: Option<usize>,
    next_positional: &mut usize,
    next_variadic_index: &mut i64,
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if variadic_index.is_some_and(|index| *next_positional >= index) {
        let key = values.int(*next_variadic_index)?;
        *next_variadic_index = next_variadic_index
            .checked_add(1)
            .ok_or(EvalStatus::RuntimeFatal)?;
        let ref_target =
            native_parameter_ref_target(signature, variadic_index, ref_target, by_ref_mode, values)?;
        return bind_native_variadic_arg(bound_args, variadic_index, key, value, ref_target, values);
    }
    let param_index = *next_positional;
    if param_index >= bound_args.len() || bound_args[param_index].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let ref_target =
        native_parameter_ref_target(signature, Some(param_index), ref_target, by_ref_mode, values)?;
    bound_args[param_index] = Some(BoundMethodArg {
        value,
        ref_target,
        variadic_ref_targets: Vec::new(),
    });
    *next_positional += 1;
    Ok(())
}

/// Binds one named native AOT argument to a fixed non-variadic slot.
fn bind_native_named_signature_arg(
    signature: &NativeCallableSignature,
    variadic_index: Option<usize>,
    bound_args: &mut [Option<BoundMethodArg>],
    name: &str,
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(param_index) = native_regular_param_index(signature, variadic_index, name) {
        if bound_args[param_index].is_some() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let ref_target = native_parameter_ref_target(
            signature,
            Some(param_index),
            ref_target,
            by_ref_mode,
            values,
        )?;
        bound_args[param_index] = Some(BoundMethodArg {
            value,
            ref_target,
            variadic_ref_targets: Vec::new(),
        });
        return Ok(());
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Returns the caller writeback target required by a native by-reference parameter.
fn native_parameter_ref_target(
    signature: &NativeCallableSignature,
    param_index: Option<usize>,
    ref_target: Option<EvalReferenceTarget>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReferenceTarget>, EvalStatus> {
    let Some(param_index) = param_index else {
        return Ok(None);
    };
    if !signature.param_by_ref(param_index) {
        return Ok(None);
    }
    if let Some(ref_target) = ref_target {
        return Ok(Some(ref_target));
    }
    match by_ref_mode {
        EvalByRefBindingMode::RequireTarget => Err(EvalStatus::RuntimeFatal),
        EvalByRefBindingMode::WarnByValue { callable_name } => {
            let param_name = native_callable_param_warning_name(signature, param_index);
            values.warning(&format!(
                "{callable_name}(): Argument #{} (${param_name}) must be passed by reference, value given",
                param_index + 1
            ))?;
            Ok(None)
        }
    }
}

/// Returns the PHP parameter name used in native method by-reference warnings.
fn native_callable_param_warning_name(
    signature: &NativeCallableSignature,
    param_index: usize,
) -> String {
    signature
        .param_names()
        .get(param_index)
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("arg{}", param_index + 1))
}

/// Returns the matching non-variadic native parameter index for one named arg.
fn native_regular_param_index(
    signature: &NativeCallableSignature,
    variadic_index: Option<usize>,
    name: &str,
) -> Option<usize> {
    signature
        .param_names()
        .iter()
        .enumerate()
        .position(|(index, param)| Some(index) != variadic_index && param == name)
}

/// Appends one value into the native AOT variadic argument array.
fn bind_native_variadic_arg(
    bound_args: &mut [Option<BoundMethodArg>],
    variadic_index: Option<usize>,
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let index = variadic_index.ok_or(EvalStatus::RuntimeFatal)?;
    let bound = bound_args[index].as_mut().ok_or(EvalStatus::RuntimeFatal)?;
    let array = values.array_set(bound.value, key, value)?;
    bound.value = array;
    if let Some(ref_target) = ref_target {
        bound.variadic_ref_targets.push((key, ref_target));
    }
    Ok(())
}

/// Calls one generated/AOT instance method after native signature binding.
pub(in crate::interpreter) fn eval_native_method_with_evaluated_args(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_method_with_evaluated_args_bridge_scope(
        object,
        class_name,
        method_name,
        evaluated_args,
        None,
        None,
        context,
        values,
    )
}

/// Calls one generated/AOT instance method after validation with an optional bridge scope.
fn eval_native_method_with_evaluated_args_bridge_scope(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut resolved_bridge_scope = bridge_scope.map(str::to_string);
    let metadata =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?;
    if let Some((declaring_class, visibility, _, is_abstract)) = metadata {
        if resolved_bridge_scope.is_none() {
            resolved_bridge_scope = Some(declaring_class.clone());
        }
        if !is_abstract
            && validate_eval_member_access(&declaring_class, visibility, context).is_err()
        {
            if eval_native_instance_magic_method_available(class_name, context, values)? {
                return eval_native_magic_instance_method_call(
                    object,
                    class_name,
                    method_name,
                    evaluated_args,
                    context,
                    values,
                );
            }
            return eval_throw_method_access_error(
                &declaring_class,
                method_name,
                visibility,
                context,
                values,
            );
        }
    } else if eval_native_instance_magic_method_available(class_name, context, values)? {
        return eval_native_magic_instance_method_call(
            object,
            class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    }
    eval_native_method_with_evaluated_args_unchecked_bridge_scope(
        object,
        class_name,
        method_name,
        evaluated_args,
        resolved_bridge_scope.as_deref(),
        called_class_scope,
        context,
        values,
    )
}

/// Calls one generated/AOT instance method without enforcing member visibility.
fn eval_native_method_with_evaluated_args_unchecked(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_method_with_evaluated_args_unchecked_bridge_scope(
        object,
        class_name,
        method_name,
        evaluated_args,
        None,
        None,
        context,
        values,
    )
}

/// Calls one generated/AOT instance method without visibility checks using an optional bridge scope.
pub(in crate::interpreter) fn eval_native_method_with_evaluated_args_unchecked_bridge_scope(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
        object,
        class_name,
        method_name,
        evaluated_args,
        bridge_scope,
        called_class_scope,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Calls one generated/AOT instance method for `call_user_func()` by-value by-ref degradation.
pub(in crate::interpreter) fn eval_native_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signature_owner = bridge_scope.unwrap_or(class_name);
    let callable_name = format!("{}::{}", signature_owner.trim_start_matches('\\'), method_name);
    eval_native_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
        object,
        class_name,
        method_name,
        evaluated_args,
        bridge_scope,
        called_class_scope,
        EvalByRefBindingMode::WarnByValue {
            callable_name: &callable_name,
        },
        context,
        values,
    )
}

/// Calls one generated/AOT instance method with a selected by-reference binding mode.
fn eval_native_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signature_owner = bridge_scope.unwrap_or(class_name);
    let signature = context.native_method_signature(signature_owner, method_name);
    let return_type = signature.as_ref().and_then(|signature| signature.return_type().cloned());
    let bound_args =
        bind_native_callable_bound_args_with_mode(signature, evaluated_args, by_ref_mode, context, values)?;
    let result = if let Some(scope) = bridge_scope {
        eval_native_method_call_with_scope(
            scope,
            called_class_scope,
            object,
            method_name,
            native_bound_arg_values(&bound_args),
            context,
            values,
        )
    } else {
        values.method_call(object, method_name, native_bound_arg_values(&bound_args))
    };
    let writeback = write_back_native_callable_ref_args(&bound_args, context, values);
    match (result, writeback) {
        (Err(status), _) | (_, Err(status)) => Err(status),
        (Ok(result), Ok(())) => eval_declared_native_return_value(
            return_type.as_ref(),
            Some(signature_owner),
            called_class_scope.or(Some(class_name)),
            result,
            context,
            values,
        ),
    }
}

/// Calls one generated/AOT static method after native signature binding.
pub(in crate::interpreter) fn eval_native_static_method_with_evaluated_args(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_static_method_with_evaluated_args_bridge_scope(
        class_name,
        method_name,
        evaluated_args,
        None,
        None,
        context,
        values,
    )
}

/// Calls one generated/AOT static method after validation with an optional bridge scope.
fn eval_native_static_method_with_evaluated_args_bridge_scope(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut resolved_bridge_scope = bridge_scope.map(str::to_string);
    let metadata =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?;
    if let Some((declaring_class, visibility, is_static, is_abstract)) = metadata {
        if resolved_bridge_scope.is_none() {
            resolved_bridge_scope = Some(declaring_class.clone());
        }
        if is_static
            && !is_abstract
            && validate_eval_member_access(&declaring_class, visibility, context).is_err()
        {
            if eval_native_static_magic_method_available(class_name, context, values)? {
                return eval_native_magic_static_method_call(
                    class_name,
                    method_name,
                    evaluated_args,
                    context,
                    values,
                );
            }
            return eval_throw_method_access_error(
                &declaring_class,
                method_name,
                visibility,
                context,
                values,
            );
        }
    } else if eval_native_static_magic_method_available(class_name, context, values)? {
        return eval_native_magic_static_method_call(
            class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    }
    eval_native_static_method_with_evaluated_args_unchecked_bridge_scope(
        class_name,
        method_name,
        evaluated_args,
        resolved_bridge_scope.as_deref(),
        called_class_scope,
        context,
        values,
    )
}

/// Calls one generated/AOT static method without enforcing member visibility.
fn eval_native_static_method_with_evaluated_args_unchecked(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_static_method_with_evaluated_args_unchecked_bridge_scope(
        class_name,
        method_name,
        evaluated_args,
        None,
        None,
        context,
        values,
    )
}

/// Calls one generated/AOT static method without visibility checks using an optional bridge scope.
pub(in crate::interpreter) fn eval_native_static_method_with_evaluated_args_unchecked_bridge_scope(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_static_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
        class_name,
        method_name,
        evaluated_args,
        bridge_scope,
        called_class_scope,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Calls one generated/AOT static method for `call_user_func()` by-value by-ref degradation.
pub(in crate::interpreter) fn eval_native_static_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signature_owner = bridge_scope.unwrap_or(class_name);
    let callable_name = format!("{}::{}", signature_owner.trim_start_matches('\\'), method_name);
    eval_native_static_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
        class_name,
        method_name,
        evaluated_args,
        bridge_scope,
        called_class_scope,
        EvalByRefBindingMode::WarnByValue {
            callable_name: &callable_name,
        },
        context,
        values,
    )
}

/// Calls one generated/AOT static method with a selected by-reference binding mode.
fn eval_native_static_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signature_owner = bridge_scope.unwrap_or(class_name);
    let signature = context.native_static_method_signature(signature_owner, method_name);
    let return_type = signature.as_ref().and_then(|signature| signature.return_type().cloned());
    let bound_args =
        bind_native_callable_bound_args_with_mode(signature, evaluated_args, by_ref_mode, context, values)?;
    let result = if let Some(scope) = bridge_scope {
        eval_native_static_method_call_with_scope(
            scope,
            called_class_scope,
            class_name,
            method_name,
            native_bound_arg_values(&bound_args),
            context,
            values,
        )
    } else {
        values.static_method_call(class_name, method_name, native_bound_arg_values(&bound_args))
    };
    let writeback = write_back_native_callable_ref_args(&bound_args, context, values);
    match (result, writeback) {
        (Err(status), _) | (_, Err(status)) => Err(status),
        (Ok(result), Ok(())) => eval_declared_native_return_value(
            return_type.as_ref(),
            Some(signature_owner),
            called_class_scope.or(Some(class_name)),
            result,
            context,
            values,
        ),
    }
}

/// Returns whether a generated/AOT class has an instance `__call()` fallback.
fn eval_native_instance_magic_method_available(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(eval_aot_method_dispatch_metadata_in_hierarchy(class_name, "__call", context, values)?
        .is_some_and(|(_, _, is_static, is_abstract)| !is_static && !is_abstract))
}

/// Returns whether a generated/AOT class has a static `__callStatic()` fallback.
fn eval_native_static_magic_method_available(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(
        eval_aot_method_dispatch_metadata_in_hierarchy(
            class_name,
            "__callStatic",
            context,
            values,
        )?
        .is_some_and(|(_, _, is_static, is_abstract)| is_static && !is_abstract),
    )
}

/// Dispatches a missing or inaccessible generated/AOT instance method through `__call()`.
pub(in crate::interpreter) fn eval_native_magic_instance_method_call(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let magic_args = eval_magic_call_args(method_name, evaluated_args, values)?;
    eval_native_method_with_evaluated_args_unchecked(
        object,
        class_name,
        "__call",
        magic_args,
        context,
        values,
    )
}

/// Dispatches a missing or inaccessible generated/AOT static method through `__callStatic()`.
pub(in crate::interpreter) fn eval_native_magic_static_method_call(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let magic_args = eval_magic_call_args(method_name, evaluated_args, values)?;
    eval_native_static_method_with_evaluated_args_unchecked(
        class_name,
        "__callStatic",
        magic_args,
        context,
        values,
    )
}

/// Finds generated/AOT method metadata on a class or its native parent chain.
pub(in crate::interpreter) fn eval_aot_method_dispatch_metadata_in_hierarchy(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility, bool, bool)>, EvalStatus> {
    let mut current = class_name.trim_start_matches('\\').to_string();
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(current.to_ascii_lowercase()) {
            return Ok(None);
        }
        if let Some(metadata) = eval_aot_method_dispatch_metadata(&current, method_name, values)? {
            return Ok(Some(metadata));
        }
        let Some(parent) = context.native_class_parent(&current) else {
            return Ok(None);
        };
        current = parent.to_string();
    }
}

/// Runs one generated/AOT constructor after native signature binding.
pub(in crate::interpreter) fn eval_native_constructor_with_evaluated_args(
    class_name: &str,
    object: RuntimeCellHandle,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(message) = eval_native_constructor_access_error(class_name, context, values)? {
        return eval_throw_error(&message, context, values);
    }
    let bridge_scope =
        eval_native_constructor_bridge_scope(class_name, context, values)?;
    let signature = context.native_constructor_signature(class_name);
    let bound_args = bind_native_callable_bound_args(
        signature,
        evaluated_args,
        context,
        values,
    )?;
    let result = if let Some(scope) = bridge_scope.as_deref() {
        eval_with_native_bridge_scope(scope, context, || {
            values.construct_object(object, native_bound_arg_values(&bound_args))
        })
    } else {
        values.construct_object(object, native_bound_arg_values(&bound_args))
    };
    let writeback = write_back_native_callable_ref_args(&bound_args, context, values);
    match (result, writeback) {
        (Err(status), _) | (_, Err(status)) => Err(status),
        (Ok(()), Ok(())) => Ok(()),
    }
}

/// Returns the generated/AOT constructor scope that the runtime bridge can recognize.
fn eval_native_constructor_bridge_scope(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some((declaring_class, visibility)) =
        eval_reflection_aot_non_public_constructor(class_name, values)?
    else {
        return Ok(None);
    };
    if eval_native_constructor_access_allowed(&declaring_class, visibility, context) {
        Ok(Some(declaring_class))
    } else {
        Ok(None)
    }
}

/// Returns PHP's constructor access error for generated/AOT constructors, if inaccessible.
fn eval_native_constructor_access_error(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some((declaring_class, visibility)) =
        eval_reflection_aot_non_public_constructor(class_name, values)?
    else {
        return Ok(None);
    };
    if eval_native_constructor_access_allowed(&declaring_class, visibility, context) {
        return Ok(None);
    }
    Ok(Some(format!(
        "Call to {} {}::__construct() from {}",
        eval_visibility_label(visibility),
        declaring_class.trim_start_matches('\\'),
        eval_native_constructor_scope_label(context)
    )))
}

/// Returns whether the current eval scope may call one generated/AOT constructor.
fn eval_native_constructor_access_allowed(
    declaring_class: &str,
    visibility: EvalVisibility,
    context: &ElephcEvalContext,
) -> bool {
    match visibility {
        EvalVisibility::Public => true,
        EvalVisibility::Private => context
            .current_class_scope()
            .is_some_and(|current| same_eval_class_name(current, declaring_class)),
        EvalVisibility::Protected => context
            .current_class_scope()
            .is_some_and(|current| eval_classes_are_related(current, declaring_class, context)),
    }
}

/// Returns PHP's scope phrase for constructor access diagnostics.
fn eval_native_constructor_scope_label(context: &ElephcEvalContext) -> String {
    context.current_class_scope().map_or_else(
        || String::from("global scope"),
        |class_name| format!("scope {}", class_name.trim_start_matches('\\')),
    )
}

/// Returns PHP's lowercase visibility label.
fn eval_visibility_label(visibility: EvalVisibility) -> &'static str {
    match visibility {
        EvalVisibility::Public => "public",
        EvalVisibility::Protected => "protected",
        EvalVisibility::Private => "private",
    }
}

/// Allocates a fresh runtime cell for one invocation-safe native AOT default.
pub(in crate::interpreter) fn materialize_native_callable_default(
    default: &NativeCallableDefault,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match default {
        NativeCallableDefault::Null => values.null(),
        NativeCallableDefault::Bool(value) => values.bool_value(*value),
        NativeCallableDefault::Int(value) => values.int(*value),
        NativeCallableDefault::Float(value) => values.float(*value),
        NativeCallableDefault::String(value) => values.string(value),
        NativeCallableDefault::EmptyArray => values.array_new(0),
        NativeCallableDefault::Array(elements) => {
            materialize_native_callable_array_default(elements, context, values)
        }
        NativeCallableDefault::Object { class_name, args } => {
            materialize_native_callable_object_default(class_name, args, context, values)
        }
    }
}

/// Allocates one array-valued native AOT parameter default with fresh element cells.
fn materialize_native_callable_array_default(
    elements: &[NativeCallableArrayDefaultElement],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let has_string_key = elements.iter().any(|element| {
        matches!(
            element.key,
            Some(NativeCallableArrayDefaultKey::String(_))
        )
    });
    let mut array = if has_string_key {
        values.assoc_new(elements.len())?
    } else {
        values.array_new(elements.len())?
    };
    let mut next_auto_key = 0;
    for element in elements {
        let key = match &element.key {
            Some(NativeCallableArrayDefaultKey::Int(value)) => {
                if *value >= next_auto_key {
                    next_auto_key = value.saturating_add(1);
                }
                values.int(*value)?
            }
            Some(NativeCallableArrayDefaultKey::String(value)) => values.string(value)?,
            None => {
                let key = values.int(next_auto_key)?;
                next_auto_key = next_auto_key.saturating_add(1);
                key
            }
        };
        let value = materialize_native_callable_default(&element.value, context, values)?;
        array = values.array_set(array, key, value)?;
    }
    Ok(array)
}

/// Allocates and constructs one object-valued native AOT parameter default.
fn materialize_native_callable_object_default(
    class_name: &str,
    args: &[NativeCallableObjectDefaultArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = values.new_object(class_name)?;
    let mut constructor_args = Vec::with_capacity(args.len());
    for arg in args {
        constructor_args.push(EvaluatedCallArg {
            name: arg.name.clone(),
            value: materialize_native_callable_default(&arg.value, context, values)?,
            ref_target: None,
        });
    }
    if let Err(err) = eval_native_constructor_with_evaluated_args(
        class_name,
        object,
        constructor_args,
        context,
        values,
    ) {
        let _ = values.release(object);
        return Err(err);
    }
    Ok(object)
}

/// Executes a PHP `static $name = expr;` declaration in the current eval scope.
pub(in crate::interpreter) fn execute_static_var_stmt(
    name: &str,
    init: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(function_name) = context.current_function().map(str::to_string) else {
        let value = eval_expr(init, context, scope, values)?;
        if let Some(replaced) = scope.set(name.to_string(), value, ScopeCellOwnership::Owned) {
            values.release(replaced)?;
        }
        return Ok(());
    };
    if scope.contains_visible(name) {
        return Ok(());
    }
    let value = if let Some(value) = context.static_local(&function_name, name) {
        value
    } else {
        let value = eval_expr(init, context, scope, values)?;
        let _ = context.set_static_local(function_name.clone(), name.to_string(), value);
        value
    };
    if let Some(replaced) = scope.set(name.to_string(), value, ScopeCellOwnership::Borrowed) {
        values.release(replaced)?;
    }
    Ok(())
}

/// Executes a PHP switch with loose case matching, default fallback, and fallthrough.
pub(in crate::interpreter) fn execute_switch_stmt(
    expr: &EvalExpr,
    cases: &[EvalSwitchCase],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let subject = eval_expr(expr, context, scope, values)?;
    let mut default_index = None;
    let mut matched_index = None;
    for (index, case) in cases.iter().enumerate() {
        let Some(condition) = &case.condition else {
            if default_index.is_none() {
                default_index = Some(index);
            }
            continue;
        };
        let condition = eval_expr(condition, context, scope, values)?;
        let matches = values.compare(EvalBinOp::LooseEq, subject, condition)?;
        if values.truthy(matches)? {
            matched_index = Some(index);
            break;
        }
    }
    let Some(start_index) = matched_index.or(default_index) else {
        return Ok(EvalControl::None);
    };
    for case in &cases[start_index..] {
        match execute_statements(&case.body, context, scope, values)? {
            EvalControl::None => {}
            EvalControl::Break | EvalControl::Continue => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `do/while` loop, evaluating the condition after every body run.
pub(in crate::interpreter) fn execute_do_while_stmt(
    body: &[EvalStmt],
    condition: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    loop {
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
        let condition = eval_expr(condition, context, scope, values)?;
        if !values.truthy(condition)? {
            break;
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `for` loop while preserving update-on-continue semantics.
pub(in crate::interpreter) fn execute_for_stmt(
    init: &[EvalStmt],
    condition: Option<&EvalExpr>,
    update: &[EvalStmt],
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    match execute_statements(init, context, scope, values)? {
        EvalControl::None | EvalControl::Continue => {}
        EvalControl::Break => return Ok(EvalControl::None),
        EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
        EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
        EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
    }
    loop {
        if let Some(condition) = condition {
            let condition = eval_expr(condition, context, scope, values)?;
            if !values.truthy(condition)? {
                break;
            }
        }
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
        match execute_statements(update, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `foreach` loop over eval array and Traversable object values.
pub(in crate::interpreter) fn execute_foreach_stmt(
    array: &EvalExpr,
    key_name: Option<&str>,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let array = eval_expr(array, context, scope, values)?;
    match values.type_tag(array)? {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            execute_foreach_array_stmt(array, key_name, value_name, body, context, scope, values)
        }
        EVAL_TAG_OBJECT => {
            execute_foreach_object_stmt(array, key_name, value_name, body, context, scope, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Executes `foreach` over a PHP array value using insertion-order runtime hooks.
fn execute_foreach_array_stmt(
    array: RuntimeCellHandle,
    key_name: Option<&str>,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let len = values.array_len(array)?;
    for index in 0..len {
        let key = values.array_iter_key(array, index)?;
        let value = values.array_get(array, key)?;
        if let Some(key_name) = key_name {
            for replaced in set_scope_cell(
                context,
                scope,
                key_name.to_string(),
                key,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
        } else {
            values.release(key)?;
        }
        for replaced in set_scope_cell(
            context,
            scope,
            value_name.to_string(),
            value,
            ScopeCellOwnership::Owned,
        )? {
            values.release(replaced)?;
        }
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

/// Executes `foreach` over an Iterator or IteratorAggregate object.
fn execute_foreach_object_stmt(
    object: RuntimeCellHandle,
    key_name: Option<&str>,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    if eval_foreach_object_is_a(object, "Iterator", context, values)? {
        return execute_foreach_iterator_stmt(
            object, key_name, value_name, body, context, scope, values,
        );
    }
    if eval_foreach_object_is_a(object, "IteratorAggregate", context, values)? {
        let iterator = eval_method_call_result(object, "getIterator", Vec::new(), context, values)?;
        return match values.type_tag(iterator)? {
            EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => execute_foreach_array_stmt(
                iterator, key_name, value_name, body, context, scope, values,
            ),
            EVAL_TAG_OBJECT if eval_foreach_object_is_a(iterator, "Iterator", context, values)? => {
                execute_foreach_iterator_stmt(
                    iterator, key_name, value_name, body, context, scope, values,
                )
            }
            _ => Err(EvalStatus::RuntimeFatal),
        };
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Drives one Iterator object through PHP's `foreach` method-call sequence.
fn execute_foreach_iterator_stmt(
    iterator: RuntimeCellHandle,
    key_name: Option<&str>,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let result = eval_method_call_result(iterator, "rewind", Vec::new(), context, values)?;
    values.release(result)?;
    loop {
        let valid = eval_method_call_result(iterator, "valid", Vec::new(), context, values)?;
        let is_valid = values.truthy(valid)?;
        values.release(valid)?;
        if !is_valid {
            return Ok(EvalControl::None);
        }

        let value = eval_method_call_result(iterator, "current", Vec::new(), context, values)?;
        let key = if key_name.is_some() {
            Some(eval_method_call_result(
                iterator,
                "key",
                Vec::new(),
                context,
                values,
            )?)
        } else {
            None
        };
        if let Some((key_name, key)) = key_name.zip(key) {
            for replaced in set_scope_cell(
                context,
                scope,
                key_name.to_string(),
                key,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
        }
        for replaced in set_scope_cell(
            context,
            scope,
            value_name.to_string(),
            value,
            ScopeCellOwnership::Owned,
        )? {
            values.release(replaced)?;
        }

        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {
                let result =
                    eval_method_call_result(iterator, "next", Vec::new(), context, values)?;
                values.release(result)?;
            }
            EvalControl::Break => return Ok(EvalControl::None),
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
}

/// Returns whether a foreach object satisfies one iterator interface.
fn eval_foreach_object_is_a(
    object: RuntimeCellHandle,
    target: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    dynamic_object_is_a(object, target, false, context, values)?
        .map_or_else(|| values.object_is_a(object, target, false), Ok)
}

/// Returns PHP's next automatic integer key for `$array[]` append writes.
pub(in crate::interpreter) fn eval_array_append_key(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut next_key = None;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? != EVAL_TAG_INT {
            continue;
        }
        let one = values.int(1)?;
        let candidate = values.add(key, one)?;
        let replace = if let Some(current) = next_key {
            let is_greater = values.compare(EvalBinOp::Gt, candidate, current)?;
            values.truthy(is_greater)?
        } else {
            true
        };
        if replace {
            next_key = Some(candidate);
        }
    }
    next_key.map_or_else(|| values.int(0), Ok)
}
