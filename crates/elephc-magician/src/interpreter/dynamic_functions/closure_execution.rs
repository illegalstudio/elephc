//! Purpose:
//! Executes eval-declared functions and closures with bound scope and captures.
//!
//! Called from:
//! - Dynamic function dispatch and callable/Closure invocation paths.
//!
//! Key details:
//! - Bound `$this`, class scope, reference captures, and static locals survive execution.

use super::*;

/// Evaluates an eval-declared function after its positional arguments are prepared.
pub(in crate::interpreter) fn eval_dynamic_function_with_values(
    function: &EvalFunction,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = evaluated_args
        .into_iter()
        .map(|value| EvaluatedCallArg {
            name: None,
            value,
            ref_target: None,
        })
        .collect();
    eval_dynamic_function_with_evaluated_args(function, evaluated_args, context, values)
}

/// Evaluates an eval-declared function after call arguments preserve names and ref targets.
pub(in crate::interpreter) fn eval_dynamic_function_with_evaluated_args(
    function: &EvalFunction,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_dynamic_function_with_evaluated_args_and_ref_flags(
        function,
        function.parameter_is_by_ref(),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates an eval-declared function with caller-selected by-ref binding flags.
pub(in crate::interpreter) fn eval_dynamic_function_with_evaluated_args_and_ref_flags(
    function: &EvalFunction,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_dynamic_function_with_evaluated_args_and_ref_mode(
        function,
        parameter_is_by_ref,
        evaluated_args,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Evaluates an eval-declared function with caller-selected by-ref mode.
pub(in crate::interpreter) fn eval_dynamic_function_with_evaluated_args_and_ref_mode(
    function: &EvalFunction,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let static_names = static_var_names(function.body());
    context.push_function(function.name());
    let evaluated_args = match bind_evaluated_method_args_with_ref_mode(
        function.params(),
        function.parameter_types(),
        function.parameter_defaults(),
        parameter_is_by_ref,
        function.parameter_is_variadic(),
        evaluated_args,
        by_ref_mode,
        context,
        values,
    ) {
        Ok(args) => args,
        Err(status) => {
            context.pop_function();
            return Err(status);
        }
    };
    let scope_parameter_is_by_ref =
        method_scope_parameter_ref_flags(parameter_is_by_ref, &evaluated_args, by_ref_mode);
    let mut function_scope = ElephcEvalScope::new();
    bind_method_scope_args(
        &mut function_scope,
        function.params(),
        &scope_parameter_is_by_ref,
        &evaluated_args,
    );
    let result = execute_statements(function.body(), context, &mut function_scope, values);
    let persist_result = persist_static_locals(
        context,
        function.name(),
        &static_names,
        &function_scope,
        values,
    );
    let writeback_result = write_back_method_ref_args(
        function.params(),
        &evaluated_args,
        &function_scope,
        context,
        values,
    );
    let return_result = match (persist_result, writeback_result, result) {
        (Err(status), _, _) | (_, Err(status), _) | (_, _, Err(status)) => Err(status),
        (Ok(()), Ok(()), Ok(control)) => eval_declared_return_control_value(
            function.return_type(),
            None,
            None,
            control,
            context,
            values,
        ),
    };
    context.pop_function();
    return_result
}

/// Evaluates one runtime eval closure after callback arguments preserve names and ref targets.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args(
    closure: &EvalClosure,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_closure_with_optional_binding(
        closure,
        function_ref_flags(closure),
        EvalByRefBindingMode::RequireTarget,
        None,
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates one runtime eval closure with `$this` and an optional binding scope.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args_and_bound_this_scope(
    closure: &EvalClosure,
    bound_this: RuntimeCellHandle,
    bound_scope: Option<String>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if closure.is_static() {
        values.warning("Cannot bind an instance to a static closure")?;
        return values.null();
    }
    let called_class = eval_closure_bound_object_class_name(bound_this, context, values)?;
    let class_scope = bound_scope.unwrap_or_else(|| called_class.clone());
    eval_closure_with_optional_binding(
        closure,
        function_ref_flags(closure),
        EvalByRefBindingMode::RequireTarget,
        Some(EvalClosureBinding {
            this_object: Some(bound_this),
            class_scope,
            called_class,
        }),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates a runtime eval closure with `$this`, scope, and caller-selected by-ref flags.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args_and_bound_this_scope_ref_flags(
    closure: &EvalClosure,
    bound_this: RuntimeCellHandle,
    bound_scope: Option<String>,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if closure.is_static() {
        values.warning("Cannot bind an instance to a static closure")?;
        return values.null();
    }
    let called_class = eval_closure_bound_object_class_name(bound_this, context, values)?;
    let class_scope = bound_scope.unwrap_or_else(|| called_class.clone());
    eval_closure_with_optional_binding(
        closure,
        parameter_is_by_ref,
        EvalByRefBindingMode::RequireTarget,
        Some(EvalClosureBinding {
            this_object: Some(bound_this),
            class_scope,
            called_class,
        }),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates a runtime eval closure with `$this`, scope, and caller-selected by-ref mode.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args_and_bound_this_scope_ref_mode(
    closure: &EvalClosure,
    bound_this: RuntimeCellHandle,
    bound_scope: Option<String>,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if closure.is_static() {
        values.warning("Cannot bind an instance to a static closure")?;
        return values.null();
    }
    let called_class = eval_closure_bound_object_class_name(bound_this, context, values)?;
    let class_scope = bound_scope.unwrap_or_else(|| called_class.clone());
    eval_closure_with_optional_binding(
        closure,
        parameter_is_by_ref,
        by_ref_mode,
        Some(EvalClosureBinding {
            this_object: Some(bound_this),
            class_scope,
            called_class,
        }),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates one runtime eval closure with a class scope but no `$this` binding.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args_and_bound_scope(
    closure: &EvalClosure,
    bound_scope: Option<String>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_scope) = bound_scope else {
        return eval_closure_with_evaluated_args(closure, evaluated_args, context, values);
    };
    eval_closure_with_optional_binding(
        closure,
        function_ref_flags(closure),
        EvalByRefBindingMode::RequireTarget,
        Some(EvalClosureBinding {
            this_object: None,
            called_class: class_scope.clone(),
            class_scope,
        }),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates a runtime eval closure with scope-only binding and caller-selected by-ref flags.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args_and_bound_scope_ref_flags(
    closure: &EvalClosure,
    bound_scope: Option<String>,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_scope) = bound_scope else {
        return eval_closure_with_optional_binding(
            closure,
            parameter_is_by_ref,
            EvalByRefBindingMode::RequireTarget,
            None,
            evaluated_args,
            context,
            values,
        );
    };
    eval_closure_with_optional_binding(
        closure,
        parameter_is_by_ref,
        EvalByRefBindingMode::RequireTarget,
        Some(EvalClosureBinding {
            this_object: None,
            called_class: class_scope.clone(),
            class_scope,
        }),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates a scope-only runtime eval closure with caller-selected by-ref mode.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args_and_bound_scope_ref_mode(
    closure: &EvalClosure,
    bound_scope: Option<String>,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_scope) = bound_scope else {
        return eval_closure_with_optional_binding(
            closure,
            parameter_is_by_ref,
            by_ref_mode,
            None,
            evaluated_args,
            context,
            values,
        );
    };
    eval_closure_with_optional_binding(
        closure,
        parameter_is_by_ref,
        by_ref_mode,
        Some(EvalClosureBinding {
            this_object: None,
            called_class: class_scope.clone(),
            class_scope,
        }),
        evaluated_args,
        context,
        values,
    )
}

/// Class binding metadata for a runtime eval closure invocation.
struct EvalClosureBinding {
    this_object: Option<RuntimeCellHandle>,
    class_scope: String,
    called_class: String,
}

/// Returns the closure function's declared by-reference parameter flags.
fn function_ref_flags(closure: &EvalClosure) -> &[bool] {
    closure.function().parameter_is_by_ref()
}

/// Evaluates one runtime eval closure with optional class and `$this` binding metadata.
fn eval_closure_with_optional_binding(
    closure: &EvalClosure,
    parameter_is_by_ref: &[bool],
    by_ref_mode: EvalByRefBindingMode<'_>,
    binding: Option<EvalClosureBinding>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let function = closure.function();
    let static_names = static_var_names(function.body());
    let bound_class_pushed = binding.is_some();
    context.push_function(function.name());
    if let Some(binding) = &binding {
        context.push_class_scope(binding.class_scope.clone());
        context.push_called_class_scope(binding.called_class.clone());
    }
    let evaluated_args = match bind_evaluated_method_args_with_ref_mode(
        function.params(),
        function.parameter_types(),
        function.parameter_defaults(),
        parameter_is_by_ref,
        function.parameter_is_variadic(),
        evaluated_args,
        by_ref_mode,
        context,
        values,
    ) {
        Ok(args) => args,
        Err(status) => {
            if bound_class_pushed {
                context.pop_called_class_scope();
                context.pop_class_scope();
            }
            context.pop_function();
            return Err(status);
        }
    };
    let mut function_scope = ElephcEvalScope::new();
    bind_closure_captures(&mut function_scope, closure.captures());
    if let Some(object) = binding.and_then(|binding| binding.this_object) {
        function_scope.set("this", object, ScopeCellOwnership::Borrowed);
    }
    let scope_parameter_is_by_ref =
        method_scope_parameter_ref_flags(parameter_is_by_ref, &evaluated_args, by_ref_mode);
    bind_method_scope_args(
        &mut function_scope,
        function.params(),
        &scope_parameter_is_by_ref,
        &evaluated_args,
    );
    let result = execute_statements(function.body(), context, &mut function_scope, values);
    let persist_result = persist_static_locals(
        context,
        function.name(),
        &static_names,
        &function_scope,
        values,
    );
    let capture_writeback_result =
        write_back_closure_ref_captures(closure.captures(), &function_scope, context, values);
    let arg_writeback_result = write_back_method_ref_args(
        function.params(),
        &evaluated_args,
        &function_scope,
        context,
        values,
    );
    let return_result = match (
        persist_result,
        capture_writeback_result,
        arg_writeback_result,
        result,
    ) {
        (Err(status), _, _, _)
        | (_, Err(status), _, _)
        | (_, _, Err(status), _)
        | (_, _, _, Err(status)) => Err(status),
        (Ok(()), Ok(()), Ok(()), Ok(control)) => eval_declared_return_control_value(
            function.return_type(),
            None,
            None,
            control,
            context,
            values,
        ),
    };
    if bound_class_pushed {
        context.pop_called_class_scope();
        context.pop_class_scope();
    }
    context.pop_function();
    return_result
}

/// Returns the PHP class name used as the bound scope for `Closure::call()`.
pub(in crate::interpreter) fn eval_closure_bound_object_class_name(
    object: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Ok(identity) = values.object_identity(object) {
        if let Some(class) = context.dynamic_object_class(identity) {
            return Ok(class.name().to_string());
        }
    }
    runtime_object_class_name(object, values)
}

/// Seeds one closure activation scope with values captured when the closure was created.
fn bind_closure_captures(
    function_scope: &mut ElephcEvalScope,
    captures: &[EvalClosureCaptureBinding],
) {
    for capture in captures {
        if let Some(target) = capture.by_ref_target().cloned() {
            function_scope.set_reference(
                capture.name().to_string(),
                capture.name().to_string(),
                capture.value(),
                ScopeCellOwnership::Borrowed,
            );
            function_scope.set_reference_target(capture.name().to_string(), target);
        } else {
            function_scope.set(
                capture.name().to_string(),
                capture.value(),
                ScopeCellOwnership::Borrowed,
            );
        }
    }
}

/// Writes modified by-reference closure captures back to their defining caller targets.
fn write_back_closure_ref_captures(
    captures: &[EvalClosureCaptureBinding],
    function_scope: &ElephcEvalScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for capture in captures {
        let Some(target) = capture.by_ref_target() else {
            continue;
        };
        let Some(entry) = function_scope
            .entry(capture.name())
            .filter(|entry| entry.flags().is_visible() && entry.flags().by_ref)
        else {
            continue;
        };
        write_back_method_ref_target(target, entry.cell(), context, values)?;
    }
    Ok(())
}

/// Persists static local variables from one eval-declared function activation.
pub(in crate::interpreter) fn persist_static_locals(
    context: &mut ElephcEvalContext,
    function_name: &str,
    names: &[String],
    scope: &ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for name in names {
        if let Some(cell) = scope.visible_cell(name) {
            if let Some(replaced) =
                context.set_static_local(function_name.to_string(), name.clone(), cell)
            {
                values.release(replaced)?;
            }
        }
    }
    Ok(())
}

/// One source-order static local declaration and its initializer expression.
#[derive(Clone)]
pub(in crate::interpreter) struct EvalStaticVarInitializer {
    pub name: String,
    pub init: EvalExpr,
}

/// Returns the distinct static local names declared anywhere in an eval function body.
pub(in crate::interpreter) fn static_var_names(body: &[EvalStmt]) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    visit_static_var_declarations(body, &mut seen, &mut |name, _| {
        names.push(name.to_string());
    });
    names
}

/// Returns static local declarations and initializers in first-seen source order.
pub(in crate::interpreter) fn static_var_initializers(
    body: &[EvalStmt],
) -> Vec<EvalStaticVarInitializer> {
    let mut vars = Vec::new();
    let mut seen = std::collections::HashSet::new();
    visit_static_var_declarations(body, &mut seen, &mut |name, init| {
        vars.push(EvalStaticVarInitializer {
            name: name.to_string(),
            init: init.clone(),
        });
    });
    vars
}

/// Visits distinct static local declarations in first-seen source order.
fn visit_static_var_declarations(
    body: &[EvalStmt],
    seen: &mut std::collections::HashSet<String>,
    visitor: &mut impl FnMut(&str, &EvalExpr),
) {
    for stmt in body {
        match stmt {
            EvalStmt::StaticVar { name, init } => {
                if seen.insert(name.clone()) {
                    visitor(name, init);
                }
            }
            EvalStmt::DoWhile { body, .. }
            | EvalStmt::Foreach { body, .. }
            | EvalStmt::For { body, .. }
            | EvalStmt::While { body, .. } => visit_static_var_declarations(body, seen, visitor),
            EvalStmt::FunctionDecl { .. } => {}
            EvalStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                visit_static_var_declarations(then_branch, seen, visitor);
                visit_static_var_declarations(else_branch, seen, visitor);
            }
            EvalStmt::Switch { cases, .. } => {
                for case in cases {
                    visit_static_var_declarations(&case.body, seen, visitor);
                }
            }
            EvalStmt::Try {
                body,
                catches,
                finally_body,
            } => {
                visit_static_var_declarations(body, seen, visitor);
                for catch in catches {
                    visit_static_var_declarations(&catch.body, seen, visitor);
                }
                visit_static_var_declarations(finally_body, seen, visitor);
            }
            EvalStmt::ArrayAppendVar { .. }
            | EvalStmt::ArraySetVar { .. }
            | EvalStmt::Break
            | EvalStmt::ClassDecl(_)
            | EvalStmt::Continue
            | EvalStmt::Echo(_)
            | EvalStmt::EnumDecl(_)
            | EvalStmt::Expr(_)
            | EvalStmt::Global { .. }
            | EvalStmt::InterfaceDecl(_)
            | EvalStmt::DynamicPropertyArrayAppend { .. }
            | EvalStmt::DynamicPropertyArraySet { .. }
            | EvalStmt::DynamicPropertyCompoundAssign { .. }
            | EvalStmt::DynamicPropertyIncDec { .. }
            | EvalStmt::DynamicPropertyReferenceBind { .. }
            | EvalStmt::DynamicPropertySet { .. }
            | EvalStmt::DynamicStaticPropertyArrayAppend { .. }
            | EvalStmt::DynamicStaticPropertyArraySet { .. }
            | EvalStmt::DynamicStaticPropertyIncDec { .. }
            | EvalStmt::DynamicStaticPropertyReferenceBind { .. }
            | EvalStmt::DynamicStaticPropertyNameArrayAppend { .. }
            | EvalStmt::DynamicStaticPropertyNameArraySet { .. }
            | EvalStmt::DynamicStaticPropertyNameIncDec { .. }
            | EvalStmt::DynamicStaticPropertyNameReferenceBind { .. }
            | EvalStmt::DynamicStaticPropertyNameSet { .. }
            | EvalStmt::DynamicStaticPropertySet { .. }
            | EvalStmt::PropertyReferenceBind { .. }
            | EvalStmt::PropertyArrayAppend { .. }
            | EvalStmt::PropertyArraySet { .. }
            | EvalStmt::PropertyCompoundAssign { .. }
            | EvalStmt::PropertyIncDec { .. }
            | EvalStmt::PropertySet { .. }
            | EvalStmt::ReferenceAssign { .. }
            | EvalStmt::Return(_)
            | EvalStmt::StaticPropertyArrayAppend { .. }
            | EvalStmt::StaticPropertyArraySet { .. }
            | EvalStmt::StaticPropertyIncDec { .. }
            | EvalStmt::StaticPropertyReferenceBind { .. }
            | EvalStmt::StaticPropertySet { .. }
            | EvalStmt::StoreVar { .. }
            | EvalStmt::Throw(_)
            | EvalStmt::TraitDecl(_)
            | EvalStmt::UnsetArrayElement { .. }
            | EvalStmt::UnsetDynamicProperty { .. }
            | EvalStmt::UnsetDynamicStaticProperty { .. }
            | EvalStmt::UnsetDynamicStaticPropertyName { .. }
            | EvalStmt::UnsetProperty { .. }
            | EvalStmt::UnsetStaticProperty { .. }
            | EvalStmt::UnsetVar { .. } => {}
        }
    }
}
