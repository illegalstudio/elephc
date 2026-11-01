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
) -> Option<(String, EvalClassMethodRef)> {
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
        "static" => {
            // php refuses `static::` inside a compile-time constant (a class constant
            // initializer, a property default, or an enum case value) as an uncatchable zend
            // compile error -- `"static::" is not allowed in compile-time constants` -- because
            // there is no live call frame to bind late. `current_class_scope()` (pushed by
            // `eval_class_like_member_default` so `self::`/`parent::` resolve there) would
            // otherwise make `static::` silently answer the SAME class as `self::`, which is a
            // wrong-value hole rather than a visible refusal.
            if context.in_compile_time_constant_context() {
                note_eval_runtime_failure(
                    "\"static::\" is not allowed in compile-time constants",
                    context,
                );
                return Err(EvalStatus::RuntimeFatal);
            }
            context
                .current_called_class_scope()
                .or_else(|| context.current_class_scope())
                .map(str::to_string)
                .ok_or(EvalStatus::RuntimeFatal)
        }
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
    let object = eval_dynamic_class_allocate_object(class, context, caller_scope, values).map_err(
        |status| trace_eval_class_new_error("allocation", class.name(), None, status, context),
    )?;
    let initialized = (|| {
        // PHP stamps a Throwable's file, line and trace when it is created, before any
        // constructor runs. The native `new` path does; an interpreted subclass of `Exception`
        // came out of here with `getFile()` empty and `getLine()` 0. Every Throwable has a native
        // root, so a class without a native parent skips the runtime `is_a` probe.
        if context.class_native_parent_name(class.name()).is_some() {
            crate::interpreter::expressions::initialize_native_throwable_source(
                object, context, values,
            )
            .map_err(|status| {
                trace_eval_class_new_error("throwable_source", class.name(), None, status, context)
            })?;
        }
        if let Some((constructor_class, constructor)) =
            context.class_method(class.name(), "__construct")
        {
            if validate_eval_member_access(&constructor_class, constructor.visibility(), context)
                .is_err()
            {
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
            )
            .map_err(|status| {
                trace_eval_class_new_error(
                    "constructor",
                    class.name(),
                    Some(constructor.name()),
                    status,
                    context,
                )
            })?;
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
                )
                .map_err(|status| {
                    trace_eval_class_new_error(
                        "native_parent_constructor",
                        class.name(),
                        Some("__construct"),
                        status,
                        context,
                    )
                })?;
            } else {
                // No constructor anywhere in the hierarchy: php evaluates the arguments and
                // ignores them. A container compiled against a newer library passes one to a
                // class that no longer declares a constructor (Symfony's
                // `ReflectionTypeResolver`), and refusing it killed every page that resolves a
                // type.
                for arg in evaluated_args {
                    if !arg.value.is_borrowed() {
                        eval_release_value(context, values, arg.value)?;
                    }
                }
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
                )
                .map_err(|status| {
                    trace_eval_class_new_error(
                        "native_parent_constructor",
                        class.name(),
                        Some("__construct"),
                        status,
                        context,
                    )
                })?;
            }
        }
        Ok(())
    })();
    if let Err(status) = initialized {
        discard_failed_dynamic_object(object, context, values);
        return Err(status);
    }
    Ok(object)
}

/// Releases a partial receiver while preserving its error and suppressing its destructor.
fn discard_failed_dynamic_object(object: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) {
    if let Ok(identity) = values.object_identity(object) {
        // An escaped `$this` retains class metadata, but PHP never runs its failed receiver's destructor.
        context.suppress_dynamic_object_destructor(identity);
    }
    let _ = values.release(object);
}

/// Emits the eval-class construction stage that failed under opt-in runtime tracing.
fn trace_eval_class_new_error(
    stage: &str,
    class_name: &str,
    member: Option<&str>,
    status: EvalStatus,
    context: &ElephcEvalContext,
) -> EvalStatus {
    if crate::eval_trace::enabled() {
        let call_site = context.call_site();
        eprintln!(
            "[elephc-eval-trace] phase=eval_class_new_error stage={stage} class={class_name:?} member={member:?} status={status:?} file={:?} line={}",
            call_site.0,
            call_site.2,
        );
    }
    status
}

/// Creates a PHP shallow clone and invokes an eval-declared `__clone()` hook when present.
pub(in crate::interpreter) fn eval_object_clone_result(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_object_clone_with_properties_result(object, None, None, context, values)
}

/// Creates a shallow clone, invokes `__clone()`, then applies PHP 8.5 property overrides.
pub(crate) fn eval_object_clone_with_properties_result(
    object: RuntimeCellHandle,
    with_properties: Option<RuntimeCellHandle>,
    scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let identity = values.object_identity(object)?;
    let dynamic_class_name = context
        .dynamic_object_class(identity)
        .map(|class| class.name().to_string());
    if elephc_builtin_contract::classes().iter().any(|contract| {
        !contract.cloneable && context.native_constructor_signature(contract.name).is_some()
    }) {
        let runtime_name = match &dynamic_class_name {
            Some(name) => name.clone(),
            None => eval_runtime_object_class_name(object, values)?,
        };
        let mut ancestry = context.class_parent_names(&runtime_name);
        ancestry.push(runtime_name.clone());
        if ancestry.iter().any(|name| elephc_builtin_contract::lookup_class(name).is_some_and(|contract| !contract.cloneable)) {
            return eval_throw_error(&format!("Trying to clone an uncloneable object of class {runtime_name}"), context, values);
        }
    }
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
    let dynamic_native_clone_hook = if clone_method.is_none() {
        if let Some(class_name) = dynamic_class_name.as_deref() {
            eval_dynamic_native_clone_hook_is_callable(class_name, context, values)?
        } else {
            None
        }
    } else {
        None
    };
    let aot_clone_hook = if dynamic_class_name.is_none() {
        eval_aot_clone_hook_is_callable(object, context, values)?
    } else {
        None
    };

    let clone = values.object_clone_shallow(object)?;
    let clone_identity = match values.object_identity(clone) {
        Ok(identity) => identity,
        Err(status) => {
            let _ = eval_release_value(context, values, clone);
            return Err(status);
        }
    };
    context.begin_clone_initialization(clone_identity);
    let initialization = (|| {
        if let Some(class_name) = dynamic_class_name {
            context.register_dynamic_object(clone_identity, &class_name);
            for (property, value) in context.dynamic_property_values_for_clone(identity) {
                // An array is COPIED (its elements stay shared, counted). Handing the clone the same
                // cell made `$clone->p[] = ...` write the original's array in place, since the cell's
                // own count says nothing about whether its payload is shared.
                if values.is_array_like(value)? {
                    let copy = values.copy_value(value)?;
                    if let Some(replaced) =
                        context.set_dynamic_property_value(clone_identity, &property, copy)
                    {
                        eval_release_value(context, values, replaced)?;
                    }
                    continue;
                }
                eval_store_dynamic_property_value(clone_identity, &property, value, context, values)?;
            }
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
            } else if let Some(hook) = dynamic_native_clone_hook {
                let result = eval_native_method_with_positional_values_unchecked_bridge_scope(
                    clone,
                    &hook.called_class,
                    "__clone",
                    Vec::new(),
                    Some(&hook.declaring_class),
                    Some(&hook.called_class),
                    context,
                    values,
                )?;
                values.release(result)?;
            }
        } else if let Some(hook) = aot_clone_hook {
            let result = eval_native_method_with_positional_values_unchecked_bridge_scope(
                clone,
                &hook.called_class,
                "__clone",
                Vec::new(),
                Some(&hook.declaring_class),
                Some(&hook.called_class),
                context,
                values,
            )?;
            values.release(result)?;
        }

        context.restart_clone_initialization(clone_identity);
        if let Some(properties) = with_properties {
            eval_apply_clone_properties(clone, properties, scope, context, values)?;
        }
        Ok(())
    })();
    context.end_clone_initialization(clone_identity);
    if let Err(status) = initialization {
        let _ = eval_release_value(context, values, clone);
        return Err(status);
    }
    Ok(clone)
}

/// Applies clone property overrides in PHP array iteration order.
fn eval_apply_clone_properties(
    clone: RuntimeCellHandle,
    properties: RuntimeCellHandle,
    scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let len = values.array_len(properties)?;
    for position in 0..len {
        let key = values.array_iter_key(properties, position)?;
        let mut owners = vec![key];
        let mut result = (|| {
            let value = values.array_get(properties, key)?;
            owners.push(value);
            let key_string = values.cast_string(key)?;
            owners.push(key_string);
            let key_bytes = values.string_bytes(key_string)?;
            let property_name =
                String::from_utf8(key_bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
            if property_name.starts_with('\0') {
                return eval_throw_error(
                    "Cannot access property starting with \"\\0\"",
                    context,
                    values,
                );
            }
            // Two independent sources can prove this entry is still part of a reference set.
            // The eval alias table describes targets a hash entry cannot (variables, nested
            // elements, object and static properties, cells, invoker slots), and it only knows
            // arrays eval itself built. An override array generated code built carries its
            // state in the native hash entry instead. Both are read for every entry, and
            // either one refusing throws at this exact point in php's iteration order.
            let alias_is_shared = eval_array_reference_key(key, values)?
                .and_then(|key| context.array_element_alias(properties, &key).cloned())
                .is_some_and(|reference| eval_clone_property_reference_is_shared(&reference, scope));
            let entry_is_shared = crate::runtime_hooks::array_entry_is_shared_reference(
                properties,
                &property_name,
                Some(value),
            );
            if alias_is_shared || entry_is_shared {
                return eval_throw_error(
                    "Cannot assign by reference when cloning with updated properties",
                    context,
                    values,
                );
            }
            eval_property_set_result(clone, &property_name, value, context, values)
        })();
        for owner in owners.into_iter().rev() {
            let released = eval_release_value(context, values, owner);
            if result.is_ok() {
                result = released;
            }
        }
        result?;
    }
    Ok(())
}

/// Returns whether a clone override still shares its array element with another storage slot.
fn eval_clone_property_reference_is_shared(target: &EvalReferenceTarget, scope: Option<&ElephcEvalScope>) -> bool {
    match target {
        EvalReferenceTarget::Variable { scope, name } => unsafe {
            scope.live_mut().is_some_and(|scope| scope.contains_visible(name))
        },
        EvalReferenceTarget::ArrayElement {
            scope, array_name, ..
        } => unsafe {
            scope
                .live_mut()
                .is_some_and(|scope| scope.contains_visible(array_name))
        },
        EvalReferenceTarget::NestedArrayElement { array_target, .. } => {
            eval_clone_property_reference_is_shared(array_target, scope)
        }
        EvalReferenceTarget::Cell { cell } => scope.map_or(true, |scope| scope.visible_entries()
            .iter().any(|(_, visible)| visible.as_ptr() == cell.as_ptr())),
        EvalReferenceTarget::ObjectProperty { .. }
        | EvalReferenceTarget::StaticProperty { .. }
        | EvalReferenceTarget::StaticLocal { .. }
        | EvalReferenceTarget::InvokerSlot { .. } => true,
    }
}

/// Generated clone-hook scopes needed by native method dispatch.
pub(super) struct EvalNativeCloneHook {
    declaring_class: String,
    called_class: String,
}

/// Returns the declaring and called scopes for an inherited generated/AOT `__clone()` hook.
pub(super) fn eval_dynamic_native_clone_hook_is_callable(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalNativeCloneHook>, EvalStatus> {
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
    Ok(Some(EvalNativeCloneHook {
        declaring_class,
        called_class: class_name.to_string(),
    }))
}

/// Calls one generated/AOT method with explicit PHP scope and caller-owned result storage.
pub(super) fn eval_native_method_call_with_scope_out(
    scope: &str,
    called_class_scope: Option<&str>,
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    result: &mut RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    context.push_class_scope(scope.to_string());
    if let Some(called_class) = called_class_scope {
        context.push_called_class_scope(called_class.to_string());
    }
    let _called_class_override = called_class_scope
        .map(|called_class| push_native_frame_called_class_override(context, scope, called_class));
    let call_result = values.method_call_out(object, method_name, evaluated_args, result);
    if called_class_scope.is_some() {
        context.pop_called_class_scope();
    }
    context.pop_class_scope();
    call_result
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
pub(in crate::interpreter) fn eval_with_native_bridge_scope<T>(
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
    eval_native_instance_property_metadata_for_access(
        called_class_name,
        &parent,
        property_name,
        context,
        values,
    )
}

/// Resolves native instance metadata while treating a strict ancestor's private name as absent.
pub(super) fn eval_native_instance_property_metadata_for_access(
    runtime_class_name: &str,
    metadata_class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility, EvalVisibility, bool)>, EvalStatus> {
    if let Some(scope) = context.current_class_scope() {
        if native_class_is_a(metadata_class_name, scope, context) {
            if let Some(metadata) =
                eval_reflection_aot_property_access_metadata(scope, property_name, values)?
            {
                if metadata.1 == EvalVisibility::Private
                    && !metadata.3
                    && same_eval_class_name(&metadata.0, scope)
                {
                    return Ok(Some(metadata));
                }
            }
        }
    }
    let metadata =
        eval_reflection_aot_property_access_metadata(metadata_class_name, property_name, values)?;
    Ok(metadata.filter(|(owner, visibility, _, _)| {
        *visibility != EvalVisibility::Private
            || same_eval_class_name(owner, runtime_class_name)
    }))
}

/// Accesses an already authorized eval property using its non-private native slot's declaring scope.
/// Eval visibility checks stay at the caller; private parent slots never become child overrides.
pub(super) fn eval_with_native_property_storage_scope<T, V: RuntimeValueOps>(
    class_name: &str,
    storage_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut V,
    operation: impl FnOnce(&mut V) -> Result<T, EvalStatus>,
) -> Result<T, EvalStatus> {
    let native = eval_dynamic_class_native_property_metadata(class_name, storage_name, context, values)?;
    if let Some((declaring_class, visibility, _, false)) = native {
        if visibility != EvalVisibility::Private {
            return eval_with_native_bridge_scope(&declaring_class, context, || operation(values));
        }
    }
    operation(values)
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

/// Returns the declaring and called scopes for an accessible instance AOT `__clone()` hook.
pub(super) fn eval_aot_clone_hook_is_callable(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalNativeCloneHook>, EvalStatus> {
    let class_name = eval_runtime_object_class_name(object, values)?;
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata(&class_name, "__clone", values)?
    else {
        return Ok(None);
    };
    if is_static || is_abstract {
        return Err(EvalStatus::RuntimeFatal);
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
        return eval_throw_clone_access_error(&declaring_class, visibility, context, values);
    }
    Ok(Some(EvalNativeCloneHook {
        declaring_class,
        called_class: class_name,
    }))
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
///
/// Widened from `pub(super)` to `pub(in crate::interpreter)` so `unserialize()`'s object
/// hydration (`builtins::core::unserialize`) can reuse the exact same "allocate + seed declared
/// defaults" primitive `ReflectionClass::newInstanceWithoutConstructor()` uses -- the two need
/// identical behavior (U20: an unserialized object with no constructor still carries its
/// declared property defaults for any property the payload does not mention).
pub(in crate::interpreter) fn eval_dynamic_class_allocate_object(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if class.is_abstract() || context.has_enum(class.name()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let declared_class_name = class.name().trim_start_matches('\\');
    let backing_class = if values.class_exists(declared_class_name)? {
        declared_class_name.to_string()
    } else {
        context
            .class_native_parent_name(class.name())
            .unwrap_or_else(|| String::from("stdClass"))
    };
    let object = values.new_object(&backing_class).map_err(|status| {
        trace_eval_class_new_error("backing_object", class.name(), None, status, context)
    })?;
    let initialized = (|| {
        let identity = values.object_identity(object).map_err(|status| {
            trace_eval_class_new_error("object_identity", class.name(), None, status, context)
        })?;
        context.register_dynamic_object(identity, class.name());
        let object_class_name = class.name().to_string();
        let mut class_chain = context.class_chain(class.name());
        if class_chain.is_empty() {
            class_chain.push(std::sync::Arc::new(class.clone()));
        }
        for class in &class_chain {
            for property in class
                .properties()
                .iter()
                .filter(|property| !property.is_static() && !property.is_abstract())
            {
                // A non-private redeclaration replaces the parent's slot and its default.
                // A typed child with no default must remain uninitialized.
                if property.visibility() != EvalVisibility::Private
                    && context.class_property(&object_class_name, property.name())
                        .is_some_and(|(owner, _)| !same_eval_class_name(&owner, class.name()))
                {
                    continue;
                }
                let value = if let Some(default) = property.default() {
                    Some(
                        eval_class_like_member_default(
                            class.name(),
                            property.trait_origin(),
                            default,
                            context,
                            caller_scope,
                            values,
                        )
                        .map_err(|status| {
                            trace_eval_class_new_error(
                                "property_default",
                                class.name(),
                                Some(property.name()),
                                status,
                                context,
                            )
                        })?,
                    )
                } else if property.property_type().is_none() {
                    Some(values.null()?)
                } else {
                    None
                };
                let storage_name = eval_instance_property_storage_name(class.name(), property);
                if let Some(value) = value {
                    // The overlay is where reads look, and the object's own slot is where the
                    // enumerators look: `print_r`, `var_dump`, `json_encode` and the `(array)`
                    // cast all walk the slots. Mirroring the default there is what keeps those
                    // agreeing with `$object->property`, instead of each enumerator learning about
                    // a second store. TWO INDEPENDENT CONDITIONS GOVERN THAT MIRROR -- whether to
                    // attempt it, and what a missing slot means.
                    //
                    // WHETHER: only a PUBLIC property is mirrored. `json_encode()` exports an
                    // object's public properties and walks the slots to find them, and this
                    // interpreter mangles a private storage name but leaves a protected one
                    // plain, so a protected property in the slots is indistinguishable from a
                    // public one and would be exported. `php -n` 8.5.6 encodes a class with a
                    // private, a protected, a public and a dynamic property as
                    // `{"c":"3","d":"4"}`. The readers that SHOULD see non-public properties --
                    // reflection, `get_object_vars()` from inside the class, `print_r` -- all take
                    // them from the class metadata plus the overlay, never from a raw slot walk,
                    // so nothing loses sight of them.
                    //
                    // A MISSING SLOT: the write is BEST-EFFORT and has to be. An eval class that
                    // extends an AOT class is backed by an instance of THAT class, whose slots
                    // were fixed at compile time, so a property the eval class declares and its
                    // native parent does not has no slot to receive it. Symfony's dumped container
                    // is exactly that shape -- `App_KernelDevDebugContainer extends Container`
                    // declaring `$targetDir`, `$parameters` and `$getService` -- and treating the
                    // missing slot as a failure aborted the allocation and killed every
                    // warm-container request. The overlay below still takes the value and
                    // `$object->property` still reads it.
                    //
                    // A property the COMPILED parent declares is the other case that must reach
                    // the slot, whatever its visibility: the parent's compiled methods read that
                    // slot and nothing else. `CheckTypeDeclarationsPass extends
                    // AbstractRecursivePass` redeclares `protected bool $skipScalars = true`, and
                    // the compiled `AbstractRecursivePass::processValue()` kept reading the
                    // parent's `false` -- recursing into every scalar argument until
                    // `lint:container` died on a TypeError. The slot exists by construction, so
                    // nothing new becomes enumerable. The compiled parent's slot is checked
                    // FIRST: a public redeclaration of a protected parent slot
                    // (`public int $guard = 15` over `protected int $guard`) is still a protected
                    // slot natively, so the unscoped public write below was refused and the
                    // parent's compiled methods kept reading the parent default.
                    if let Some(declaring_class) = eval_native_parent_inheritable_slot_owner(
                        class.name(),
                        property.name(),
                        context,
                        values,
                    )? {
                        // A non-public slot is only writable from its declaring class's scope,
                        // which is the same wrapper an interpreted `$this->p = ...` uses for such
                        // a slot.
                        let _ = eval_reflection_with_declaring_class_scope(
                            &declaring_class,
                            context,
                            |_| values.property_set(object, &storage_name, value),
                        );
                    } else if property.visibility() == EvalVisibility::Public {
                        let _ = values.property_set(object, &storage_name, value);
                    }
                    let stored = eval_store_dynamic_property_value(
                        identity,
                        &storage_name,
                        value,
                        context,
                        values,
                    );
                    // The overlay and the slot each took a reference of their own; the evaluated
                    // default is a temporary of this loop, so its reference goes back here.
                    let released = release_expr_result(value, context, values);
                    stored.and(released)?;
                    context.mark_dynamic_property_initialized(identity, &storage_name);
                }
            }
        }
        Ok(())
    })();
    if let Err(status) = initialized {
        discard_failed_dynamic_object(object, context, values);
        return Err(status);
    }
    Ok(object)
}

/// Returns the class declaring the compiled parent's non-private slot named `property`, if any.
///
/// A private parent slot is the parent's own and is never the one a redeclaration replaces.
fn eval_native_parent_inheritable_slot_owner(
    class_name: &str,
    property: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some(parent) = context.class_native_parent_name(class_name) else {
        return Ok(None);
    };
    Ok(
        match crate::interpreter::builtins::eval_runtime_property_access_metadata(
            &parent, property, values,
        )? {
            Some((declaring_class, visibility, _)) if visibility != EvalVisibility::Private => {
                Some(declaring_class)
            }
            _ => None,
        },
    )
}
