//! Purpose:
//! Binds evaluated arguments for eval-declared and native functions.
//!
//! Called from:
//! - Dynamic function and callable dispatch after source-order argument evaluation.
//!
//! Key details:
//! - Named, variadic, by-reference, and raw native arguments preserve PHP binding rules.
//! - A native AOT container carries only what the caller actually supplied. Holes before the
//!   greatest supplied regular slot are still filled with that parameter's default, exactly as
//!   PHP does, but the trailing run of untouched optionals is TRUNCATED. The descriptor invoker
//!   applies those defaults itself and derives `func_num_args()` / `func_get_args()` metadata
//!   from the container length, so padding the container would report arguments the caller
//!   never passed.
//! - PHP accepts surplus positional arguments for a userland function that declares no
//!   variadic, and `func_get_args()` still reports them. Those arguments are appended to the
//!   container past the declared parameters instead of being rejected; the invoker routes them
//!   into the hidden collector, or ignores them when the callee never introspects its frame.
//! - An unknown NAMED argument is legal only for a source-declared variadic, where PHP keeps its
//!   string key in the collected array. The binder records that key, which turns the container
//!   associative; the invoker's associative entry already validates, binds and collects it.
//! - A by-reference source variadic (`&...$rest`) stages its tail PER ELEMENT, exactly like any
//!   other by-reference parameter. That is the representation the callee already expects: an AOT
//!   caller lowers a by-reference variadic tail through `lower_invoker_ref_arg_marker`, so the
//!   collected array holds reference MARKER cells rather than plain values, and the descriptor
//!   invoker's tail copy loop moves each container entry into it verbatim. Element writes inside
//!   the callee therefore reach the eval caller's variables through those markers.

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
    mut evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<BoundNativeFunctionArgs, EvalStatus> {
    // Source operands belong to the caller; only defaults and coercions belong here.
    for arg in &mut evaluated_args { arg.value = arg.value.borrowed(); }
    if native_function_variadic_index(function).is_some() {
        return bind_evaluated_native_variadic_function_args(
            function,
            evaluated_args,
            by_ref_mode,
            context,
            values,
        );
    }
    let mut bound_args = vec![None; function.param_count()];
    let mut surplus_args = Vec::new();
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
        } else if next_positional < bound_args.len() {
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
        } else {
            // Surplus positional arguments are by-value in PHP and bind to no parameter.
            surplus_args.push(BoundMethodArg {
                value: arg.value,
                ref_target: None,
                variadic_ref_targets: Vec::new(),
            });
        }
    }

    let supplied = supplied_regular_len(&bound_args);
    if supplied < function.required_param_count() {
        release_partial_native_bindings(&mut bound_args, context, values);
        release_native_bound_arg_owners(surplus_args, context, values);
        return Err(EvalStatus::RuntimeFatal);
    }
    // Regulars may only shrink when nothing follows them in the container.
    if surplus_args.is_empty() {
        truncate_unsupplied_native_tail(&mut bound_args, supplied);
    }
    if let Err(status) = fill_native_function_defaults(function, &mut bound_args, context, values) {
        // `fill_native_function_defaults` already reclaimed the regular slots it held.
        release_native_bound_arg_owners(surplus_args, context, values);
        return Err(status);
    }
    // A hole left here means an earlier required regular slot stayed unbound, as a named-only
    // call such as `f(b: 1)` against `f($a, $b)` leaves it. The already bound regular slots own
    // their cells just like the surplus ones, so reclaim BOTH before refusing the call; dropping
    // the slot vector would strand every owner it still holds.
    if bound_args.iter().any(Option::is_none) {
        release_partial_native_bindings(&mut bound_args, context, values);
        release_native_bound_arg_owners(surplus_args, context, values);
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut bound_args = bound_args.into_iter().flatten().collect::<Vec<_>>();
    bound_args.extend(surplus_args);
    let named_keys = vec![None; bound_args.len()];
    finish_native_function_binding(
        function,
        None,
        bound_args,
        named_keys,
        by_ref_mode,
        context,
        values,
    )
}

/// Reclaims already bound argument owners when the call fails before staging can transfer them.
///
/// Mirrors `release_native_bound_args`: the collected-key owners go back first, then the value.
/// A `ref_target` is borrowed caller metadata and owns nothing, so it is simply dropped.
fn release_native_bound_arg_owners(
    bound_args: impl IntoIterator<Item = BoundMethodArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) {
    for bound in bound_args {
        for (key, _) in &bound.variadic_ref_targets {
            let _ = release_expr_result(*key, context, values);
        }
        let _ = release_expr_result(bound.value, context, values);
    }
}

/// Returns how many leading regular slots the call actually reaches.
///
/// PHP fills the gap before the greatest supplied parameter with defaults and counts that
/// slot's index plus one, so the answer is the greatest bound index plus one, not the number
/// of bound slots.
fn supplied_regular_len(bound_args: &[Option<BoundMethodArg>]) -> usize {
    bound_args
        .iter()
        .rposition(Option::is_some)
        .map_or(0, |index| index + 1)
}

/// Drops the trailing regular slots the caller never reached so the container stays sparse.
///
/// `supplied` comes from `supplied_regular_len`, which is the greatest BOUND index plus one, so
/// every entry this removes is unbound by construction and owns nothing. The assertion states
/// that invariant instead of a release loop that could never run.
fn truncate_unsupplied_native_tail(bound_args: &mut Vec<Option<BoundMethodArg>>, supplied: usize) {
    debug_assert!(
        bound_args[supplied.min(bound_args.len())..]
            .iter()
            .all(Option::is_none),
        "truncating the unsupplied tail must not discard a bound argument owner"
    );
    bound_args.truncate(supplied);
}

/// Reclaims every already bound argument when binding fails before staging can transfer them.
fn release_partial_native_bindings(
    bound_args: &mut [Option<BoundMethodArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) {
    let taken = bound_args
        .iter_mut()
        .filter_map(Option::take)
        .collect::<Vec<_>>();
    release_native_bound_arg_owners(taken, context, values);
}

/// Binds a native AOT variadic function while keeping the raw invoker argument layout.
///
/// Only a SOURCE-declared variadic reaches here: the hidden `func_args` collector is never
/// registered as an eval-visible parameter, so a function that merely introspects its frame
/// takes the non-variadic path above and its surplus arguments are appended there.
fn bind_evaluated_native_variadic_function_args(
    function: &NativeFunction,
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<BoundNativeFunctionArgs, EvalStatus> {
    let variadic_index = native_function_variadic_index(function).ok_or(EvalStatus::RuntimeFatal)?;
    let has_param_names = function.param_names().len() == function.param_count();
    let mut regular_args = vec![None; variadic_index];
    let mut variadic_args: Vec<BoundMethodArg> = Vec::new();
    let mut named_variadic_args: Vec<(String, BoundMethodArg)> = Vec::new();
    let mut next_positional = 0;

    let collected = (|| {
        for arg in evaluated_args {
            if let Some(name) = arg.name {
                if !has_param_names {
                    return Err(EvalStatus::RuntimeFatal);
                }
                if native_function_regular_param_index(function, variadic_index, &name).is_none() {
                    // An unknown name is a tail ENTRY, keyed by its own string in PHP's collected
                    // array. Two of the same name would collide on that key, which PHP reports as
                    // a duplicate argument rather than silently keeping the last one.
                    if named_variadic_args
                        .iter()
                        .any(|(bound_name, _)| *bound_name == name)
                    {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                    // A tail entry binds the variadic parameter, so a by-reference variadic
                    // claims the caller's writeback target for this entry exactly as a
                    // positional tail element does.
                    let ref_target = native_function_parameter_ref_target(
                        function,
                        Some(variadic_index),
                        arg.ref_target,
                        by_ref_mode,
                        values,
                    )?;
                    named_variadic_args.push((
                        name,
                        BoundMethodArg {
                            value: arg.value,
                            ref_target,
                            variadic_ref_targets: Vec::new(),
                        },
                    ));
                    continue;
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
                // A positional tail element binds the variadic parameter. When the source
                // declares it by reference, staging turns this entry into a marker cell and the
                // callee's element writes travel back through `ref_target`.
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
        Ok(())
    })();
    if let Err(status) = collected {
        release_partial_native_bindings(&mut regular_args, context, values);
        release_native_bound_arg_owners(variadic_args, context, values);
        release_native_bound_arg_owners(
            named_variadic_args.into_iter().map(|(_, bound)| bound),
            context,
            values,
        );
        return Err(status);
    }

    // Regulars may only shrink when no POSITIONAL tail follows them: a positional tail argument
    // occupies the container slot right after the regulars, so each one must be materialized to
    // reach it. Unknown named entries carry their own string keys and impose no such ordering,
    // which is what keeps `func_num_args()` right for a call that omits a trailing optional and
    // still passes a named tail entry.
    if variadic_args.is_empty() {
        let supplied = supplied_regular_len(&regular_args);
        // `required_param_count()` already stops at the variadic slot for a variadic signature.
        if supplied < function.required_param_count() {
            release_partial_native_bindings(&mut regular_args, context, values);
            release_native_bound_arg_owners(
                named_variadic_args.into_iter().map(|(_, bound)| bound),
                context,
                values,
            );
            return Err(EvalStatus::RuntimeFatal);
        }
        truncate_unsupplied_native_tail(&mut regular_args, supplied);
    }
    if let Err(status) = fill_native_function_defaults(function, &mut regular_args, context, values)
    {
        release_native_bound_arg_owners(variadic_args, context, values);
        release_native_bound_arg_owners(
            named_variadic_args.into_iter().map(|(_, bound)| bound),
            context,
            values,
        );
        return Err(status);
    }

    if regular_args.iter().any(Option::is_none) {
        release_partial_native_bindings(&mut regular_args, context, values);
        release_native_bound_arg_owners(variadic_args, context, values);
        release_native_bound_arg_owners(
            named_variadic_args.into_iter().map(|(_, bound)| bound),
            context,
            values,
        );
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut bound_args = regular_args.into_iter().flatten().collect::<Vec<_>>();
    bound_args.extend(variadic_args);
    // Positional entries keep their own container index, so every string key lands after them.
    // That order is exactly what the invoker's one-pass container validation accepts: an integer
    // key after a string key is the `Cannot use positional argument after named argument` error.
    let mut named_keys = vec![None; bound_args.len()];
    for (name, bound) in named_variadic_args {
        named_keys.push(Some(name));
        bound_args.push(bound);
    }
    finish_native_function_binding(
        function,
        Some(variadic_index),
        bound_args,
        named_keys,
        by_ref_mode,
        context,
        values,
    )
}

/// Materializes omitted parameters and reclaims earlier defaults if a later default fails.
fn fill_native_function_defaults(
    function: &NativeFunction,
    bound_args: &mut [Option<BoundMethodArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let filled = (|| {
        for (position, bound) in bound_args.iter_mut().enumerate() {
            if bound.is_some() { continue; }
            if position < function.required_param_count() { return Err(EvalStatus::RuntimeFatal); }
            let default = function.param_default(position).ok_or(EvalStatus::RuntimeFatal)?;
            *bound = Some(BoundMethodArg {
                value: materialize_native_callable_default(default, context, values)?,
                ref_target: None,
                variadic_ref_targets: Vec::new(),
            });
        }
        Ok(())
    })();
    if filled.is_err() {
        for bound in bound_args.iter_mut().filter_map(Option::take) {
            let _ = release_expr_result(bound.value, context, values);
        }
    }
    filled
}

/// Coerces and stages parameters, reclaiming every untransferred binding owner on failure.
fn finish_native_function_binding(
    function: &NativeFunction,
    variadic_index: Option<usize>,
    mut bound_args: Vec<BoundMethodArg>,
    named_keys: Vec<Option<String>>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<BoundNativeFunctionArgs, EvalStatus> {
    let result = apply_native_function_arg_types(function, variadic_index, &mut bound_args, context, values)
        .and_then(|()| stage_native_function_invoker_args(
            function, variadic_index, &mut bound_args, named_keys, by_ref_mode, context, values,
        ));
    if result.is_err() {
        let _ = release_native_bound_args(&bound_args, context, values);
    }
    result
}

/// Applies registered native AOT function parameter types after argument binding.
fn apply_native_function_arg_types(
    function: &NativeFunction,
    variadic_index: Option<usize>,
    bound_args: &mut [BoundMethodArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
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
        let original = bound_arg.value;
        bound_arg.value = eval_method_parameter_value(param_type, original, context, values)?;
        if bound_arg.value != original { release_expr_result(original, context, values)?; }
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
