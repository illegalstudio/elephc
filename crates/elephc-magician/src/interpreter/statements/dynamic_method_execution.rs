//! Purpose:
//! Executes eval-declared instance and static methods from prepared argument values.
//!
//! Called from:
//! - Method dispatch and callable invocation after target resolution.
//!
//! Key details:
//! - Called-class scope, reference modes, positional extraction, and writeback stay paired.

use super::*;

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
    eval_dynamic_method_with_values_and_ref_mode(
        class_name,
        called_class_name,
        method,
        object,
        parameter_is_by_ref,
        evaluated_args,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Executes one eval-declared class method with caller-selected by-ref mode.
pub(in crate::interpreter) fn eval_dynamic_method_with_values_and_ref_mode(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    object: RuntimeCellHandle,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
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
    let binding = match bind_evaluated_function_args_with_ref_mode(
        method.params(),
        method.parameter_types(),
        method.parameter_defaults(),
        parameter_is_by_ref,
        method.parameter_is_variadic(),
        evaluated_args,
        by_ref_mode,
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
    let BoundEvalFunctionArgs {
        params: binding_params,
        parameter_is_by_ref: binding_by_ref,
        args: evaluated_args,
        mut frame,
    } = binding;
    let mut method_scope = ElephcEvalScope::new();
    method_scope.set("this", object, ScopeCellOwnership::Borrowed);
    let scope_parameter_is_by_ref =
        method_scope_parameter_ref_flags(&binding_by_ref, &evaluated_args, by_ref_mode);
    bind_method_scope_args(
        &mut method_scope,
        &binding_params,
        &scope_parameter_is_by_ref,
        &evaluated_args,
    );
    frame.bind_scope(&method_scope);
    context.push_function_args_with_backtrace(
        frame,
        Some(class_name.trim_start_matches('\\').to_string()),
        Some(object),
        false,
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
    let return_result = retain_static_local_return(
        return_result,
        &static_names,
        &method_scope,
        values,
    );
    context.pop_function_args();
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
    eval_dynamic_static_method_with_values_and_ref_mode(
        class_name,
        called_class_name,
        method,
        parameter_is_by_ref,
        evaluated_args,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Executes one eval-declared static method with caller-selected by-ref mode.
pub(in crate::interpreter) fn eval_dynamic_static_method_with_values_and_ref_mode(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
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
    let binding = match bind_evaluated_function_args_with_ref_mode(
        method.params(),
        method.parameter_types(),
        method.parameter_defaults(),
        parameter_is_by_ref,
        method.parameter_is_variadic(),
        evaluated_args,
        by_ref_mode,
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
    let BoundEvalFunctionArgs {
        params: binding_params,
        parameter_is_by_ref: binding_by_ref,
        args: evaluated_args,
        mut frame,
    } = binding;
    let mut method_scope = ElephcEvalScope::new();
    let scope_parameter_is_by_ref =
        method_scope_parameter_ref_flags(&binding_by_ref, &evaluated_args, by_ref_mode);
    bind_method_scope_args(
        &mut method_scope,
        &binding_params,
        &scope_parameter_is_by_ref,
        &evaluated_args,
    );
    frame.bind_scope(&method_scope);
    context.push_function_args_with_backtrace(
        frame,
        Some(class_name.trim_start_matches('\\').to_string()),
        None,
        true,
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
    let return_result = retain_static_local_return(
        return_result,
        &static_names,
        &method_scope,
        values,
    );
    context.pop_function_args();
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
