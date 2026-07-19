//! Purpose:
//! Implements builtin Closure static methods and binding transformations.
//!
//! Called from:
//! - Static method dispatch for the eval Closure class surface.
//!
//! Key details:
//! - Receiver, scope, called class, and unbound targets are validated before cloning closures.

use super::*;

/// Dispatches static methods for eval's builtin `Closure` class slice.
pub(super) fn eval_closure_static_method_result(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    lexical_scope: Option<&ElephcEvalScope>,
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
        return eval_closure_from_callable(evaluated_args, lexical_scope, context, values)
            .map(Some);
    }
    if method_name.eq_ignore_ascii_case("bind") {
        return eval_closure_bind_static(evaluated_args, context, values).map(Some);
    }
    Ok(None)
}

/// Materializes `Closure::fromCallable()` from one normalized eval callback.
pub(super) fn eval_closure_from_callable(
    evaluated_args: Vec<EvaluatedCallArg>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut args = bind_evaluated_function_args(&[String::from("callback")], evaluated_args)?;
    let callback = args.pop().ok_or(EvalStatus::RuntimeFatal)?;
    let callable = match lexical_scope {
        Some(scope) => eval_callable_from_scope(callback, context, scope, values),
        None => eval_callable(callback, context, values),
    };
    let callable = match callable {
        Ok(callable) => callable,
        Err(EvalStatus::UnsupportedConstruct) if values.type_tag(callback)? == EVAL_TAG_OBJECT => {
            return eval_closure_from_callable_type_error(
                "no array or string given",
                context,
                values,
            );
        }
        Err(status) => return Err(status),
    };
    eval_validate_closure_from_callable_callback(&callable, context, values)?;
    let target = eval_closure_object_target_from_callable(callable);
    eval_closure_object_from_target(target, context, values)
}

/// Converts a normalized callable target into the storage used by eval Closure objects.
pub(super) fn eval_closure_object_target_from_callable(
    callable: EvaluatedCallable,
) -> EvalClosureObjectTarget {
    match callable {
        EvaluatedCallable::Named { name, .. } => EvalClosureObjectTarget::Named(name),
        EvaluatedCallable::BoundClosure {
            name,
            bound_this,
            bound_scope,
        } => EvalClosureObjectTarget::BoundNamed {
            name,
            bound_this,
            bound_scope,
        },
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
pub(super) fn eval_closure_object_from_target(
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
pub(super) fn eval_closure_bind_static(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (target, bound_this, bound_scope, rebinds_function_scope) =
        eval_closure_bind_static_args(evaluated_args, context, values)?;
    eval_closure_bind_target(
        target,
        bound_this,
        bound_scope,
        rebinds_function_scope,
        context,
        values,
    )
}

/// Binds static `Closure::bind()` arguments to their PHP parameter slots.
pub(super) fn eval_closure_bind_static_args(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<
    (
        EvalClosureObjectTarget,
        Option<RuntimeCellHandle>,
        Option<String>,
        bool,
    ),
    EvalStatus,
> {
    let bound = eval_closure_bind_args(
        &["closure", "newThis", "newScope"],
        2,
        evaluated_args,
    )?;
    let closure = required_closure_bind_arg(&bound, 0)?;
    let new_this = required_closure_bind_arg(&bound, 1)?;
    let target = eval_closure_target_arg(closure.value, context, values)?;
    let bound_this = eval_closure_bind_receiver_arg(new_this.value, values)?;
    let (bound_scope, rebinds_function_scope) =
        eval_closure_bind_scope_arg(bound.get(2), bound_this, context, values)?;
    Ok((target, bound_this, bound_scope, rebinds_function_scope))
}

/// Binds `Closure::bindTo()` arguments to their PHP parameter slots.
pub(super) fn eval_closure_bind_to_args(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(Option<RuntimeCellHandle>, Option<String>, bool), EvalStatus> {
    let bound = eval_closure_bind_args(&["newThis", "newScope"], 1, evaluated_args)?;
    let new_this = required_closure_bind_arg(&bound, 0)?;
    let bound_this = eval_closure_bind_receiver_arg(new_this.value, values)?;
    let (bound_scope, rebinds_function_scope) =
        eval_closure_bind_scope_arg(bound.get(1), bound_this, context, values)?;
    Ok((bound_this, bound_scope, rebinds_function_scope))
}

/// Binds positional and named Closure binding arguments while accepting optional scope.
pub(super) fn eval_closure_bind_args(
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
pub(super) fn required_closure_bind_arg(
    bound_args: &[Option<EvaluatedCallArg>],
    index: usize,
) -> Result<&EvaluatedCallArg, EvalStatus> {
    bound_args
        .get(index)
        .and_then(Option::as_ref)
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Extracts a stored eval Closure object target from a runtime object.
pub(super) fn eval_closure_target_arg(
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
pub(super) fn eval_closure_bind_receiver_arg(
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

/// Converts `newScope` into class scope plus whether function scope was rebound.
pub(super) fn eval_closure_bind_scope_arg(
    new_scope: Option<&Option<EvaluatedCallArg>>,
    bound_this: Option<RuntimeCellHandle>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(Option<String>, bool), EvalStatus> {
    let Some(new_scope) = new_scope.and_then(Option::as_ref) else {
        return Ok((None, false));
    };
    if values.is_null(new_scope.value)? {
        return Ok((None, false));
    }
    if values.type_tag(new_scope.value)? == EVAL_TAG_OBJECT {
        return eval_closure_bound_object_class_name(new_scope.value, context, values)
            .map(|scope| (Some(scope), true));
    }
    let bytes = values.string_bytes(new_scope.value)?;
    let scope = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    if scope.eq_ignore_ascii_case("static") {
        let Some(bound_this) = bound_this else {
            return Ok((None, false));
        };
        return eval_closure_bound_object_class_name(bound_this, context, values)
            .map(|scope| (Some(scope), false));
    }
    Ok((
        Some(
            context
                .resolve_class_name(&scope)
                .unwrap_or_else(|| scope.trim_start_matches('\\').to_string()),
        ),
        true,
    ))
}

/// Creates a new Closure object with persistent binding metadata when supported.
pub(super) fn eval_closure_bind_target(
    target: EvalClosureObjectTarget,
    bound_this: Option<RuntimeCellHandle>,
    bound_scope: Option<String>,
    rebinds_function_scope: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(bound_this) = bound_this else {
        return eval_closure_unbind_target(
            target,
            bound_scope,
            rebinds_function_scope,
            context,
            values,
        );
    };
    match target {
        EvalClosureObjectTarget::Named(name) | EvalClosureObjectTarget::BoundNamed { name, .. } => {
            let Some(closure) = context.closure(&name) else {
                if eval_function_probe_exists(context, &name) {
                    if rebinds_function_scope {
                        return eval_closure_call_warning_null(
                            "Cannot rebind scope of closure created from function",
                            values,
                        );
                    }
                    return eval_closure_object_from_target(
                        EvalClosureObjectTarget::BoundNamed {
                            name,
                            bound_this: Some(bound_this),
                            bound_scope,
                        },
                        context,
                        values,
                    );
                }
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
                EvalClosureObjectTarget::BoundNamed {
                    name,
                    bound_this: Some(bound_this),
                    bound_scope,
                },
                context,
                values,
            )
        }
        EvalClosureObjectTarget::InvokableObject { object } => {
            if !eval_closure_bind_bound_class_matches_method(
                object, "__invoke", bound_this, context, values,
            )? {
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
            if !eval_closure_bind_bound_class_matches_method(
                object, &method, bound_this, context, values,
            )? {
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
pub(super) fn eval_closure_unbind_target(
    target: EvalClosureObjectTarget,
    bound_scope: Option<String>,
    rebinds_function_scope: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match target {
        EvalClosureObjectTarget::Named(name) | EvalClosureObjectTarget::BoundNamed { name, .. } => {
            if rebinds_function_scope
                && context.closure(&name).is_none()
                && eval_function_probe_exists(context, &name)
            {
                return eval_closure_call_warning_null(
                    "Cannot rebind scope of closure created from function",
                    values,
                );
            }
            let target = match bound_scope {
                Some(bound_scope) => EvalClosureObjectTarget::BoundNamed {
                    name,
                    bound_this: None,
                    bound_scope: Some(bound_scope),
                },
                None => EvalClosureObjectTarget::Named(name),
            };
            eval_closure_object_from_target(target, context, values)
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
