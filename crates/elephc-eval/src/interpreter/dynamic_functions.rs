//! Purpose:
//! Evaluates user-declared and native dynamic functions, including named/spread argument binding.
//!
//! Called from:
//! - `crate::interpreter::eval_call()` and dynamic callable dispatch helpers.
//!
//! Key details:
//! - PHP source evaluation order is preserved before argument binding.
//! - Static locals are persisted through `ElephcEvalContext` after function execution.

use super::*;

/// Evaluates an eval-declared user function with PHP-style argument binding.
pub(in crate::interpreter) fn eval_dynamic_function(
    function: &EvalFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args =
        eval_function_call_args(function.params(), args, context, caller_scope, values)?;
    eval_dynamic_function_with_values(function, evaluated_args, context, values)
}

/// Evaluates and binds function-like arguments to parameter order.
pub(in crate::interpreter) fn eval_function_call_args(
    params: &[String],
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, caller_scope, values)?;
    bind_evaluated_function_args(params, evaluated_args)
}

/// Evaluates source-order call arguments while preserving named-argument metadata.
pub(in crate::interpreter) fn eval_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let spread = eval_expr(arg.value(), context, caller_scope, values)?;
            if !values.is_array_like(spread)? {
                return Err(EvalStatus::RuntimeFatal);
            }
            append_unpacked_call_arg_values(spread, &mut evaluated_args, &mut saw_named, values)?;
            continue;
        }

        if let Some(name) = arg.name() {
            saw_named = true;
            let value = eval_expr(arg.value(), context, caller_scope, values)?;
            evaluated_args.push(EvaluatedCallArg {
                name: Some(name.to_string()),
                value,
            });
            continue;
        }

        if saw_named {
            return Err(EvalStatus::RuntimeFatal);
        }
        let value = eval_expr(arg.value(), context, caller_scope, values)?;
        evaluated_args.push(EvaluatedCallArg { name: None, value });
    }

    Ok(evaluated_args)
}

/// Converts a `call_user_func_array` argument array into ordered call arguments.
pub(in crate::interpreter) fn eval_array_call_arg_values(
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let len = values.array_len(arg_array)?;
    let mut evaluated_args = Vec::with_capacity(len);
    let mut saw_named = false;
    append_unpacked_call_arg_values(arg_array, &mut evaluated_args, &mut saw_named, values)?;
    Ok(evaluated_args)
}

/// Appends one unpacked array's values using PHP named-argument key semantics.
pub(in crate::interpreter) fn append_unpacked_call_arg_values(
    array: RuntimeCellHandle,
    evaluated_args: &mut Vec<EvaluatedCallArg>,
    saw_named: &mut bool,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        match values.type_tag(key)? {
            EVAL_TAG_INT => {
                if *saw_named {
                    return Err(EvalStatus::RuntimeFatal);
                }
                evaluated_args.push(EvaluatedCallArg { name: None, value });
            }
            EVAL_TAG_STRING => {
                *saw_named = true;
                let name = values.string_bytes(key)?;
                let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
                evaluated_args.push(EvaluatedCallArg {
                    name: Some(name),
                    value,
                });
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
    Ok(())
}

/// Binds evaluated positional and named values to declared parameter order.
pub(in crate::interpreter) fn bind_evaluated_function_args(
    params: &[String],
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let mut bound_args = vec![None; params.len()];
    let mut next_positional = 0;

    for arg in evaluated_args {
        if let Some(name) = arg.name {
            bind_dynamic_named_arg(params, &mut bound_args, &name, arg.value)?;
        } else {
            bind_dynamic_positional_arg(&mut bound_args, &mut next_positional, arg.value)?;
        }
    }

    bound_args
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Binds one positional dynamic-call value to the next declared parameter slot.
pub(in crate::interpreter) fn bind_dynamic_positional_arg(
    bound_args: &mut [Option<RuntimeCellHandle>],
    next_positional: &mut usize,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    if *next_positional >= bound_args.len() || bound_args[*next_positional].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    bound_args[*next_positional] = Some(value);
    *next_positional += 1;
    Ok(())
}

/// Binds one named dynamic-call value to the matching declared parameter slot.
pub(in crate::interpreter) fn bind_dynamic_named_arg(
    params: &[String],
    bound_args: &mut [Option<RuntimeCellHandle>],
    name: &str,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    let Some(param_index) = params.iter().position(|param| param == name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if bound_args[param_index].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    bound_args[param_index] = Some(value);
    Ok(())
}

/// Evaluates an eval-declared function after its positional arguments are prepared.
pub(super) fn eval_dynamic_function_with_values(
    function: &EvalFunction,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut function_scope = ElephcEvalScope::new();
    for (name, value) in function.params().iter().zip(evaluated_args) {
        function_scope.set(name.clone(), value, ScopeCellOwnership::Borrowed);
    }
    let static_names = static_var_names(function.body());
    context.push_function(function.name());
    let result = execute_statements(function.body(), context, &mut function_scope, values);
    let persist_result = persist_static_locals(
        context,
        function.name(),
        &static_names,
        &function_scope,
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

/// Persists static local variables from one eval-declared function activation.
pub(super) fn persist_static_locals(
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

/// Returns the distinct static local names declared anywhere in an eval function body.
pub(in crate::interpreter) fn static_var_names(body: &[EvalStmt]) -> Vec<String> {
    let mut names = std::collections::HashSet::new();
    collect_static_var_names(body, &mut names);
    names.into_iter().collect()
}

/// Recursively collects static local declaration names from eval statements.
fn collect_static_var_names(body: &[EvalStmt], names: &mut std::collections::HashSet<String>) {
    for stmt in body {
        match stmt {
            EvalStmt::StaticVar { name, .. } => {
                names.insert(name.clone());
            }
            EvalStmt::DoWhile { body, .. }
            | EvalStmt::Foreach { body, .. }
            | EvalStmt::For { body, .. }
            | EvalStmt::While { body, .. } => collect_static_var_names(body, names),
            EvalStmt::FunctionDecl { .. } => {}
            EvalStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_static_var_names(then_branch, names);
                collect_static_var_names(else_branch, names);
            }
            EvalStmt::Switch { cases, .. } => {
                for case in cases {
                    collect_static_var_names(&case.body, names);
                }
            }
            EvalStmt::Try {
                body,
                catches,
                finally_body,
            } => {
                collect_static_var_names(body, names);
                for catch in catches {
                    collect_static_var_names(&catch.body, names);
                }
                collect_static_var_names(finally_body, names);
            }
            EvalStmt::ArrayAppendVar { .. }
            | EvalStmt::ArraySetVar { .. }
            | EvalStmt::Break
            | EvalStmt::ClassDecl(_)
            | EvalStmt::Continue
            | EvalStmt::Echo(_)
            | EvalStmt::Expr(_)
            | EvalStmt::Global { .. }
            | EvalStmt::InterfaceDecl(_)
            | EvalStmt::PropertySet { .. }
            | EvalStmt::ReferenceAssign { .. }
            | EvalStmt::Return(_)
            | EvalStmt::StoreVar { .. }
            | EvalStmt::Throw(_)
            | EvalStmt::UnsetVar { .. } => {}
        }
    }
}

/// Evaluates a registered AOT function through its descriptor-compatible invoker.
pub(super) fn eval_native_function(
    function: NativeFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = if function.param_names().len() == function.param_count() {
        eval_function_call_args(function.param_names(), args, context, caller_scope, values)?
    } else {
        eval_positional_call_arg_values(args, context, caller_scope, values)?
    };
    eval_native_function_with_values(function, evaluated_args, values)
}

/// Invokes a registered AOT function after its positional arguments are prepared.
pub(super) fn eval_native_function_with_values(
    function: NativeFunction,
    evaluated_args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.len() != function.param_count() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let arg_array = values.array_new(evaluated_args.len())?;
    for (index, value) in evaluated_args.into_iter().enumerate() {
        let index = values.int(index as i64)?;
        let _ = values.array_set(arg_array, index, value)?;
    }
    let result = unsafe { function.call(arg_array) };
    values.release(arg_array)?;
    if result.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(result)
}
