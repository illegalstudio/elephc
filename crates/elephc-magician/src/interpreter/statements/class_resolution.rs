//! Purpose:
//! Resolves class-like names and allocates or clones dynamic objects.
//!
//! Called from:
//! - New-object, clone, and static-member dispatch paths.
//!
//! Key details:
//! - self/parent/static scope, native parents, constructor modes, and clone hooks are resolved here.

use super::*;

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
pub(super) fn eval_static_member_context_owns_class(
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    context.has_class(class_name)
        || context.has_interface(class_name)
        || context.has_trait(class_name)
        || context.has_enum(class_name)
}

/// Returns whether a static member receiver exists in eval metadata or generated metadata.
pub(super) fn eval_runtime_class_like_exists(
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
pub(super) fn resolve_eval_class_name_literal(
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
    eval_dynamic_class_new_object_with_ref_mode(
        class,
        evaluated_args,
        EvalByRefBindingMode::RequireTarget,
        context,
        caller_scope,
        values,
    )
}

/// Creates an eval-declared object while using the selected constructor by-ref mode.
pub(super) fn eval_dynamic_class_new_object_with_ref_mode(
    class: &EvalClass,
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
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
        let result = eval_dynamic_method_with_values_and_ref_mode(
            &constructor_class,
            class.name(),
            &constructor,
            object,
            constructor.parameter_is_by_ref(),
            evaluated_args,
            by_ref_mode,
            context,
            values,
        )?;
        eval_release_value(context, values, result)?;
    } else if !evaluated_args.is_empty() {
        if let Some(parent) = context.class_native_parent_name(class.name()) {
            eval_native_constructor_with_evaluated_args_and_ref_mode(
                &parent,
                object,
                evaluated_args,
                by_ref_mode,
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
            eval_native_constructor_with_evaluated_args_and_ref_mode(
                &parent,
                object,
                Vec::new(),
                by_ref_mode,
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
pub(super) fn eval_dynamic_native_clone_hook_is_callable(
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
pub(super) fn eval_native_method_call_with_scope(
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
pub(super) fn eval_native_static_method_call_with_scope(
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
pub(super) fn eval_with_native_bridge_scope<T>(
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
pub(super) fn eval_dynamic_class_native_property_metadata(
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
pub(super) fn eval_dynamic_class_native_constant_metadata(
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
pub(super) fn eval_aot_clone_hook_is_callable(
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
pub(super) fn eval_runtime_object_class_name(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let class_name = values.object_class_name(object)?;
    let bytes = values.string_bytes(class_name)?;
    values.release(class_name)?;
    String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Creates a backing object for an eval-declared class without running its constructor.
pub(super) fn eval_dynamic_class_allocate_object(
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
