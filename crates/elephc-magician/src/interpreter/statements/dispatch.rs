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
//! - Only program-level generic templates are hoisted; nested declarations execute in source order.

use super::*;

/// Checks program provenance, seeds CLI globals, and hoists templates before eval/include execution.
pub(in crate::interpreter) fn execute_program_statements(
    program: &EvalProgram,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    if program.native_generic_syntax() && crate::strict_php_mode::strict_php_mode() {
        return Err(EvalStatus::ParseError);
    }
    seed_named_cli_superglobals(program, context, scope, values)?;
    let statements = program.statements();
    let mut registered: Vec<(String, bool)> = Vec::new();
    for stmt in statements {
        if let EvalStmt::GenericDecl(template) = stmt {
            if !context.define_generic_template(template.as_ref().clone()) {
                for (name, is_function) in &registered { context.remove_generic_template(name, *is_function); }
                return Err(EvalStatus::RuntimeFatal);
            }
            registered.push((template.name.clone(), matches!(template.prototype.as_ref(), EvalStmt::FunctionDecl { .. })));
        }
    }
    for stmt in statements {
        if let EvalStmt::GenericDecl(template) = stmt {
            if let Err(error) = verify_generic_variance(template, context, values) {
                for (name, is_function) in &registered { context.remove_generic_template(name, *is_function); }
                return Err(error);
            }
        }
    }
    execute_statement_sequence(statements, true, context, scope, values)
}

/// Executes nested statements in source order without registering unreachable declarations.
pub(in crate::interpreter) fn execute_statements(
    statements: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    execute_statement_sequence(statements, false, context, scope, values)
}

/// Propagates control flow while skipping only templates already hoisted at program entry.
fn execute_statement_sequence(
    statements: &[EvalStmt],
    templates_registered: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    for stmt in statements {
        eval_pcntl_maybe_dispatch(context, values)?;
        if templates_registered && matches!(stmt, EvalStmt::GenericDecl(_)) { continue; }
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
    let specialized = specialize_static_storage_statement(stmt, context, scope, values)?;
    let stmt = specialized.as_ref().unwrap_or(stmt);
    match stmt {
        EvalStmt::GenericDecl(template) => {
            if !context.define_generic_template(template.as_ref().clone()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            if let Err(error) = verify_generic_variance(template, context, values) {
                context.remove_generic_template(&template.name, matches!(template.prototype.as_ref(), EvalStmt::FunctionDecl { .. }));
                return Err(error);
            }
            Ok(EvalControl::None)
        },
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
            eval_echo_expr(expr, context, scope, values)?;
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
            if context.generic_function_template(name).is_some() { return Err(EvalStatus::RuntimeFatal); }
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
            if eval_condition(condition, context, scope, values)? {
                execute_statements(then_branch, context, scope, values)
            } else {
                execute_statements(else_branch, context, scope, values)
            }
        }
        EvalStmt::Return(Some(expr)) => Ok(EvalControl::Return(eval_owned_expr(
            expr, context, scope, values,
        )?)),
        EvalStmt::Return(None) => Ok(EvalControl::ReturnVoid),
        EvalStmt::ReferenceAssign { target, source } => {
            for replaced in set_reference_alias(context, scope, target, source, values)? {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        stmt @ (
            EvalStmt::PropertyReferenceBind { .. }
            | EvalStmt::DynamicPropertyReferenceBind { .. }
            | EvalStmt::DynamicPropertySet { .. }
            | EvalStmt::DynamicPropertyArrayAppend { .. }
            | EvalStmt::DynamicPropertyArraySet { .. }
            | EvalStmt::DynamicPropertyCompoundAssign { .. }
            | EvalStmt::DynamicPropertyIncDec { .. }
            | EvalStmt::PropertySet { .. }
            | EvalStmt::PropertyArrayAppend { .. }
            | EvalStmt::PropertyArraySet { .. }
            | EvalStmt::PropertyCompoundAssign { .. }
            | EvalStmt::PropertyIncDec { .. }
            | EvalStmt::StaticPropertySet { .. }
            | EvalStmt::StaticPropertyReferenceBind { .. }
            | EvalStmt::StaticPropertyArrayAppend { .. }
            | EvalStmt::StaticPropertyArraySet { .. }
            | EvalStmt::StaticPropertyIncDec { .. }
            | EvalStmt::DynamicStaticPropertySet { .. }
            | EvalStmt::DynamicStaticPropertyReferenceBind { .. }
            | EvalStmt::DynamicStaticPropertyArrayAppend { .. }
            | EvalStmt::DynamicStaticPropertyArraySet { .. }
            | EvalStmt::DynamicStaticPropertyIncDec { .. }
            | EvalStmt::DynamicStaticPropertyNameSet { .. }
            | EvalStmt::DynamicStaticPropertyNameReferenceBind { .. }
            | EvalStmt::DynamicStaticPropertyNameArrayAppend { .. }
            | EvalStmt::DynamicStaticPropertyNameArraySet { .. }
            | EvalStmt::DynamicStaticPropertyNameIncDec { .. }
            | EvalStmt::UnsetProperty { .. }
            | EvalStmt::UnsetDynamicProperty { .. }
            | EvalStmt::UnsetStaticProperty { .. }
            | EvalStmt::UnsetDynamicStaticProperty { .. }
            | EvalStmt::UnsetDynamicStaticPropertyName { .. }
        ) => execute_property_stmt(stmt, context, scope, values),
        EvalStmt::StaticVar { name, init } => {
            execute_static_var_stmt(name, init, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::TypedVar { name, declared, value } => {
            let value = eval_owned_expr(value, context, scope, values)?;
            let checked = validate_generic_local(declared, name, value, context, values);
            if let Err(error) = checked { let _ = eval_release_value(context, values, value); return Err(error); }
            let previous = scope.local_type(name).cloned();
            scope.declare_local_type(name.clone(), declared.clone());
            let replaced = match set_owned_scope_cell(context, scope, name.clone(), value, values) {
                Ok(replaced) => replaced,
                Err(status) => {
                    if let Some(previous) = previous { scope.declare_local_type(name.clone(), previous); }
                    else { scope.clear_local_type(name); }
                    return Err(status);
                }
            };
            for replaced in replaced {
                eval_release_value(context, values, replaced)?;
            }
            if let Some(cell) = visible_scope_cell(context, scope, name) {
                if values.is_reference(cell)? { context.bind_local_reference_types(cell, &[declared.clone()]); }
            }
            Ok(EvalControl::None)
        }
        EvalStmt::StoreVar { name, value } => {
            let value = eval_owned_expr(value, context, scope, values)?;
            for replaced in set_owned_scope_cell(
                context,
                scope,
                name.clone(),
                value,
                values,
            )? {
                eval_release_value(context, values, replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Switch { expr, cases } => {
            execute_switch_stmt(expr, cases, context, scope, values)
        }
        EvalStmt::Throw(expr) => {
            let thrown = eval_owned_expr(expr, context, scope, values)?;
            let tag = values.type_tag(thrown);
            if tag != Ok(EVAL_TAG_OBJECT) {
                let _ = eval_release_value(context, values, thrown);
                tag?;
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
        EvalStmt::UnsetVar { name } => {
            if let Some(replaced) = unset_scope_cell(context, scope, name.clone()) {
                eval_release_value(context, values, replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::GcCollect => {
            values.collect_cycles()?;
            Ok(EvalControl::None)
        }
        EvalStmt::While { condition, body } => {
            while eval_condition(condition, context, scope, values)? {
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
            release_expr_result(result, context, values)?;
            Ok(EvalControl::None)
        }
    }
}
