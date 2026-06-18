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
            let mut ownership = ScopeCellOwnership::Owned;
            let array = if let Some(existing) =
                scope_entry(context, scope, name).filter(|entry| entry.flags().is_visible())
            {
                if values.is_array_like(existing.cell())? {
                    let tag = values.type_tag(existing.cell())?;
                    if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
                        return Err(EvalStatus::UnsupportedConstruct);
                    }
                    ownership = existing.flags().ownership;
                    existing.cell()
                } else {
                    values.array_new(1)?
                }
            } else {
                values.array_new(1)?
            };
            let index = eval_array_append_key(array, values)?;
            let value = eval_expr(value, context, scope, values)?;
            let array = values.array_set(array, index, value)?;
            for replaced in set_scope_cell(context, scope, name.clone(), array, ownership)? {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::ArraySetVar { name, index, value } => {
            let mut ownership = ScopeCellOwnership::Owned;
            let array = if let Some(existing) =
                scope_entry(context, scope, name).filter(|entry| entry.flags().is_visible())
            {
                if values.is_array_like(existing.cell())? {
                    ownership = existing.flags().ownership;
                    existing.cell()
                } else {
                    values.array_new(1)?
                }
            } else {
                values.array_new(1)?
            };
            let index = eval_expr(index, context, scope, values)?;
            let value = eval_expr(value, context, scope, values)?;
            let array = values.array_set(array, index, value)?;
            for replaced in set_scope_cell(context, scope, name.clone(), array, ownership)? {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Break => Ok(EvalControl::Break),
        EvalStmt::Continue => Ok(EvalControl::Continue),
        EvalStmt::DoWhile { body, condition } => {
            execute_do_while_stmt(body, condition, context, scope, values)
        }
        EvalStmt::Echo(expr) => {
            let value = eval_expr(expr, context, scope, values)?;
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
            execute_class_decl_stmt(class, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::InterfaceDecl(interface) => {
            execute_interface_decl_stmt(interface, context, values)?;
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
        EvalStmt::FunctionDecl { name, params, body } => {
            let key = name.to_ascii_lowercase();
            context
                .define_function(
                    key,
                    EvalFunction::new(name.clone(), params.clone(), body.clone()),
                )
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
        EvalStmt::Return(None) => Ok(EvalControl::Return(values.null()?)),
        EvalStmt::ReferenceAssign { target, source } => {
            for replaced in set_reference_alias(context, scope, target, source, values)? {
                values.release(replaced)?;
            }
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
            values.property_set(object, property, value)?;
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
                values.release(replaced)?;
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
        EvalStmt::UnsetVar { name } => {
            if let Some(replaced) = unset_scope_cell(scope, name.clone()) {
                values.release(replaced)?;
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
                    EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
                }
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Expr(expr) => {
            let _ = eval_expr(expr, context, scope, values)?;
            Ok(EvalControl::None)
        }
    }
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
        EvalControl::None | EvalControl::Break | EvalControl::Continue => Ok(()),
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

/// Registers an eval-declared class in the dynamic class table.
pub(in crate::interpreter) fn execute_class_decl_stmt(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let name = class.name().trim_start_matches('\\');
    if context.has_class(name)
        || context.has_interface(name)
        || values.class_exists(name)?
        || values.interface_exists(name)?
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(parent) = class.parent() {
        if context.class(parent).is_none() || context.class_is_a(parent, name, false) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    for interface in class.interfaces() {
        if context.has_interface(interface) {
            validate_class_implements_eval_interface(class, interface, context)?;
        } else if !values.interface_exists(interface)? {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    if context.define_class(class.clone()) {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Registers an eval-declared interface in the dynamic interface table.
pub(in crate::interpreter) fn execute_interface_decl_stmt(
    interface: &EvalInterface,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let name = interface.name().trim_start_matches('\\');
    if context.has_interface(name)
        || context.has_class(name)
        || values.interface_exists(name)?
        || values.class_exists(name)?
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    for parent in interface.parents() {
        if context.interface_parent_names(parent)
            .iter()
            .any(|ancestor| ancestor.eq_ignore_ascii_case(name))
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        if !context.has_interface(parent) && !values.interface_exists(parent)? {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    if context.define_interface(interface.clone()) {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Validates that one eval class provides methods required by one eval interface.
fn validate_class_implements_eval_interface(
    class: &EvalClass,
    interface_name: &str,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for requirement in context.interface_method_requirements(interface_name) {
        if !class_has_interface_method(class, &requirement, context) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns whether a class or its eval parents satisfy one interface method signature.
fn class_has_interface_method(
    class: &EvalClass,
    requirement: &EvalInterfaceMethod,
    context: &ElephcEvalContext,
) -> bool {
    if let Some(method) = class.method(requirement.name()) {
        return method.params().len() == requirement.params().len();
    }
    class
        .parent()
        .and_then(|parent| context.class_method(parent, requirement.name()))
        .is_some_and(|(_, method)| method.params().len() == requirement.params().len())
}

/// Creates a backing object for an eval-declared class and runs its constructor.
pub(in crate::interpreter) fn eval_dynamic_class_new_object(
    class: &EvalClass,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = values.new_object("stdClass")?;
    let identity = values.object_identity(object)?;
    context.register_dynamic_object(identity, class.name());
    let mut class_chain = context.class_chain(class.name());
    if class_chain.is_empty() {
        class_chain.push(class.clone());
    }
    for class in &class_chain {
        for property in class.properties() {
            let value = if let Some(default) = property.default() {
                eval_expr(default, context, caller_scope, values)?
            } else {
                values.null()?
            };
            values.property_set(object, property.name(), value)?;
        }
    }
    if let Some((constructor_class, constructor)) =
        context.class_method(class.name(), "__construct")
    {
        eval_dynamic_method_with_values(
            &constructor_class,
            &constructor,
            object,
            evaluated_args,
            context,
            values,
        )?;
    } else if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
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
    let Ok(identity) = values.object_identity(object) else {
        return values.method_call(object, method_name, evaluated_args);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        return values.method_call(object, method_name, evaluated_args);
    };
    let (class_name, method) = context
        .class_method(class.name(), method_name)
        .ok_or(EvalStatus::RuntimeFatal)?;
    eval_dynamic_method_with_values(
        &class_name,
        &method,
        object,
        evaluated_args,
        context,
        values,
    )
}

/// Executes one eval-declared class method with `$this` bound in method scope.
pub(in crate::interpreter) fn eval_dynamic_method_with_values(
    class_name: &str,
    method: &EvalClassMethod,
    object: RuntimeCellHandle,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args =
        bind_evaluated_function_args(method.params(), positional_args(evaluated_args))?;
    let mut method_scope = ElephcEvalScope::new();
    method_scope.set("this", object, ScopeCellOwnership::Borrowed);
    for (name, value) in method.params().iter().zip(evaluated_args) {
        method_scope.set(name.clone(), value, ScopeCellOwnership::Borrowed);
    }
    let qualified_method_name =
        format!("{}::{}", class_name.trim_start_matches('\\'), method.name());
    let static_names = static_var_names(method.body());
    context.push_function(qualified_method_name.clone());
    let result = execute_statements(method.body(), context, &mut method_scope, values);
    let persist_result = persist_static_locals(
        context,
        &qualified_method_name,
        &static_names,
        &method_scope,
        values,
    );
    context.pop_function();
    persist_result?;
    match result? {
        EvalControl::None => values.null(),
        EvalControl::Return(result) => Ok(result),
        EvalControl::Throw(result) => {
            context.set_pending_throw(result);
            Err(EvalStatus::UncaughtThrowable)
        }
        EvalControl::Break | EvalControl::Continue => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Wraps positional method arguments into the shared dynamic-call binding shape.
pub(in crate::interpreter) fn positional_args(
    args: Vec<RuntimeCellHandle>,
) -> Vec<EvaluatedCallArg> {
    args.into_iter()
        .map(|value| EvaluatedCallArg { name: None, value })
        .collect()
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
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
        match execute_statements(update, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `foreach` loop over eval array values.
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
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
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
