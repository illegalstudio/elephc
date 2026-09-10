//! Purpose:
//! Binds evaluated arguments for eval-declared and native functions.
//!
//! Called from:
//! - Dynamic function and callable dispatch after source-order argument evaluation.
//!
//! Key details:
//! - Named, variadic, by-reference, and raw native arguments preserve PHP binding rules.

use super::*;

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

/// Binds already evaluated native AOT function args and fills omitted defaults.
pub(in crate::interpreter) fn bind_evaluated_native_function_args(
    function: &NativeFunction,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<BoundNativeFunctionArgs, EvalStatus> {
    bind_evaluated_native_function_args_with_mode(
        function,
        evaluated_args,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Binds native AOT function args for `call_user_func()` by-value by-ref degradation.
pub(in crate::interpreter) fn bind_evaluated_native_function_args_for_call_user_func(
    callable_name: &str,
    function: &NativeFunction,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<BoundNativeFunctionArgs, EvalStatus> {
    bind_evaluated_native_function_args_with_mode(
        function,
        evaluated_args,
        EvalByRefBindingMode::WarnByValue { callable_name },
        context,
        values,
    )
}

/// Binds already evaluated native AOT function args using the selected by-reference mode.
fn bind_evaluated_native_function_args_with_mode(
    function: &NativeFunction,
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<BoundNativeFunctionArgs, EvalStatus> {
    let mut staged = BoundNativeFunctionArgs {
        values: Vec::new(), ref_slots: Vec::new(), owners: Vec::new(),
    };
    let result = bind_native_function_into(
        function, evaluated_args, by_ref_mode, context, values, &mut staged,
    );
    match result {
        Ok(()) => Ok(staged),
        Err(status) => {
            let _ = cleanup_native_function_ref_args(&staged, values);
            let _ = release_native_argument_owners(staged.owners, values);
            Err(status)
        }
    }
}

/// Binds borrowed inputs while registering each newly allocated cell in the call's cleanup ledger.
fn bind_native_function_into(
    function: &NativeFunction,
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    staged: &mut BoundNativeFunctionArgs,
) -> Result<(), EvalStatus> {
    if !function.bridge_supported() { return Err(EvalStatus::RuntimeFatal); }
    if native_function_variadic_index(function).is_some() {
        return bind_evaluated_native_variadic_function_args(
            function,
            evaluated_args,
            by_ref_mode,
            context,
            values,
            staged,
        );
    }
    let mut bound_args = vec![None; function.param_count()];
    let has_param_names = function.param_names().len() == function.param_count();
    let mut next_positional = 0;

    for arg in evaluated_args {
        if let Some(name) = arg.name {
            if !has_param_names {
                return Err(EvalStatus::RuntimeFatal);
            }
            bind_native_function_named_arg(
                function,
                None,
                &mut bound_args,
                &name,
                arg.value,
                arg.ref_target,
                by_ref_mode,
                values,
            )?;
        } else {
            bind_native_function_positional_arg(
                function,
                &mut bound_args,
                None,
                &mut next_positional,
                arg.value,
                arg.ref_target,
                by_ref_mode,
                values,
            )?;
        }
    }

    for (position, bound) in bound_args.iter_mut().enumerate() {
        if bound.is_some() {
            continue;
        }
        if position < function.required_param_count() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let Some(default) = function.param_default(position) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        let value = materialize_native_callable_default(default, context, values)?;
        staged.owners.push(value);
        *bound = Some(BoundMethodArg {
            value,
            ref_target: None,
            variadic_ref_targets: Vec::new(),
        });
    }

    let mut bound_args = bound_args
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(EvalStatus::RuntimeFatal)?;
    apply_native_function_arg_types(function, None, &mut bound_args, context, values, &mut staged.owners)?;
    stage_native_function_invoker_args(function, None, bound_args, by_ref_mode, values, staged)
}

/// Binds a native AOT variadic function while keeping the raw invoker argument layout.
fn bind_evaluated_native_variadic_function_args(
    function: &NativeFunction,
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    staged: &mut BoundNativeFunctionArgs,
) -> Result<(), EvalStatus> {
    let variadic_index = native_function_variadic_index(function).ok_or(EvalStatus::RuntimeFatal)?;
    let has_param_names = function.param_names().len() == function.param_count();
    let mut regular_args = vec![None; variadic_index];
    let mut variadic_args = Vec::new();
    let mut next_positional = 0;

    for arg in evaluated_args {
        if let Some(name) = arg.name {
            if !has_param_names {
                return Err(EvalStatus::RuntimeFatal);
            }
            if native_function_regular_param_index(function, variadic_index, &name).is_none() {
                return Err(EvalStatus::RuntimeFatal);
            }
            bind_native_function_named_arg(
                function,
                Some(variadic_index),
                &mut regular_args,
                &name,
                arg.value,
                arg.ref_target,
                by_ref_mode,
                values,
            )?;
        } else if next_positional < variadic_index {
            bind_native_function_positional_arg(
                function,
                &mut regular_args,
                Some(variadic_index),
                &mut next_positional,
                arg.value,
                arg.ref_target,
                by_ref_mode,
                values,
            )?;
        } else {
            let ref_target = native_function_parameter_ref_target(
                function,
                Some(variadic_index),
                arg.ref_target,
                by_ref_mode,
                values,
            )?;
            variadic_args.push(BoundMethodArg {
                value: arg.value,
                ref_target,
                variadic_ref_targets: Vec::new(),
            });
        }
    }

    for (position, bound) in regular_args.iter_mut().enumerate() {
        if bound.is_some() {
            continue;
        }
        if position < function.required_param_count() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let Some(default) = function.param_default(position) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        let value = materialize_native_callable_default(default, context, values)?;
        staged.owners.push(value);
        *bound = Some(BoundMethodArg {
            value,
            ref_target: None,
            variadic_ref_targets: Vec::new(),
        });
    }

    let mut bound_args = regular_args
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(EvalStatus::RuntimeFatal)?;
    bound_args.extend(variadic_args);
    apply_native_function_arg_types(
        function,
        Some(variadic_index),
        &mut bound_args,
        context,
        values,
        &mut staged.owners,
    )?;
    stage_native_function_invoker_args(
        function,
        Some(variadic_index),
        bound_args,
        by_ref_mode,
        values,
        staged,
    )
}

/// Applies parameter types and publishes by-reference entry coercions before invocation.
fn apply_native_function_arg_types(
    function: &NativeFunction,
    variadic_index: Option<usize>,
    bound_args: &mut [BoundMethodArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    owners: &mut Vec<RuntimeCellHandle>,
) -> Result<(), EvalStatus> {
    for (position, bound_arg) in bound_args.iter_mut().enumerate() {
        let param_index = if variadic_index.is_some_and(|index| position >= index) {
            variadic_index.ok_or(EvalStatus::RuntimeFatal)?
        } else {
            position
        };
        let Some(param_type) = function.param_type(param_index) else {
            continue;
        };
        let value = eval_method_parameter_value(param_type, bound_arg.value, context, values)?;
        if value != bound_arg.value {
            owners.push(value);
            if function.param_by_ref(param_index) {
                if let Some(target) = bound_arg.ref_target.as_ref() {
                    write_back_method_ref_target(target, value, context, values)?;
                }
            }
        }
        bound_arg.value = value;
    }
    Ok(())
}

/// Binds one named native AOT function argument to a non-variadic parameter slot.
fn bind_native_function_named_arg(
    function: &NativeFunction,
    variadic_index: Option<usize>,
    bound_args: &mut [Option<BoundMethodArg>],
    name: &str,
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(param_index) = native_function_named_param_index(function, variadic_index, name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if bound_args[param_index].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let ref_target =
        native_function_parameter_ref_target(function, Some(param_index), ref_target, by_ref_mode, values)?;
    bound_args[param_index] = Some(BoundMethodArg {
        value,
        ref_target,
        variadic_ref_targets: Vec::new(),
    });
    Ok(())
}

/// Binds one positional native AOT function argument to the next fixed parameter.
fn bind_native_function_positional_arg(
    function: &NativeFunction,
    bound_args: &mut [Option<BoundMethodArg>],
    variadic_index: Option<usize>,
    next_positional: &mut usize,
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let param_index = *next_positional;
    if variadic_index.is_some_and(|index| param_index >= index)
        || param_index >= bound_args.len()
        || bound_args[param_index].is_some()
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let ref_target =
        native_function_parameter_ref_target(function, Some(param_index), ref_target, by_ref_mode, values)?;
    bound_args[param_index] = Some(BoundMethodArg {
        value,
        ref_target,
        variadic_ref_targets: Vec::new(),
    });
    *next_positional += 1;
    Ok(())
}

/// Returns the caller writeback target required by a native function by-reference parameter.
fn native_function_parameter_ref_target(
    function: &NativeFunction,
    param_index: Option<usize>,
    ref_target: Option<EvalReferenceTarget>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReferenceTarget>, EvalStatus> {
    let Some(param_index) = param_index else {
        return Ok(None);
    };
    if !function.param_by_ref(param_index) {
        return Ok(None);
    }
    if let Some(ref_target) = ref_target {
        return Ok(Some(ref_target));
    }
    match by_ref_mode {
        EvalByRefBindingMode::RequireTarget => Err(EvalStatus::RuntimeFatal),
        EvalByRefBindingMode::WarnByValue { callable_name } => {
            let param_name = native_function_param_warning_name(function, param_index);
            values.warning(&format!(
                "{callable_name}(): Argument #{} (${param_name}) must be passed by reference, value given",
                param_index + 1
            ))?;
            Ok(None)
        }
    }
}

/// Returns the PHP parameter name used in by-reference warning diagnostics.
fn native_function_param_warning_name(function: &NativeFunction, param_index: usize) -> String {
    function
        .param_names()
        .get(param_index)
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("arg{}", param_index + 1))
}

/// Returns the variadic parameter index for a native AOT function, if registered.
pub(super) fn native_function_variadic_index(function: &NativeFunction) -> Option<usize> {
    (0..function.param_count()).find(|index| function.param_variadic(*index))
}

/// Returns the native function parameter index for one named argument.
fn native_function_named_param_index(
    function: &NativeFunction,
    variadic_index: Option<usize>,
    name: &str,
) -> Option<usize> {
    function
        .param_names()
        .iter()
        .enumerate()
        .position(|(index, param)| Some(index) != variadic_index && param == name)
}

/// Returns the non-variadic native function parameter index for one named argument.
fn native_function_regular_param_index(
    function: &NativeFunction,
    variadic_index: usize,
    name: &str,
) -> Option<usize> {
    function
        .param_names()
        .iter()
        .enumerate()
        .position(|(index, param)| index < variadic_index && param == name)
}
