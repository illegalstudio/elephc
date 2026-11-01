//! Purpose:
//! Dispatches eval and native instance methods with dynamic visibility and scope.
//!
//! Called from:
//! - Method-call expression evaluation and reflected invocation.
//!
//! Key details:
//! - Private shadows, native bridges, closures, magic methods, and reference args remain ordered.

use super::*;
use crate::interpreter::builtins::spl::array_iterator::{
    eval_array_iterator_method_result, eval_object_is_array_iterator,
};

/// Dispatches a method call to an eval-declared class method or to the runtime hook.
pub(in crate::interpreter) fn eval_method_call_result(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_method_call_result_with_evaluated_args(
        object,
        method_name,
        positional_args(evaluated_args),
        context,
        values,
    )
}

/// `bindTo()` on a COMPILED closure (a callable descriptor, not an interpreter object).
///
/// Laravel's `Macroable::__call` runs `$macro->bindTo($this, static::class)` on macros that
/// compiled service providers registered, and the descriptor answered nothing: every request
/// reaching a `Request` macro died. The receiver is rebound through the runtime's own
/// `Closure::bind`; the scope argument is not forwarded (the runtime keeps the closure's scope
/// unless it addresses properties by a runtime name), the same limit compiled `bindTo` has.
fn eval_native_closure_method(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: &[EvaluatedCallArg],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("bindTo") {
        return Ok(None);
    }
    let new_this = evaluated_args
        .first()
        .map(|arg| arg.value)
        .filter(|value| values.type_tag(*value).is_ok_and(|tag| tag == EVAL_TAG_OBJECT));
    values.bind_native_closure(object, new_this).map(Some)
}

/// Dispatches an object method call while preserving named-argument metadata for eval methods.
pub(in crate::interpreter) fn eval_method_call_result_with_evaluated_args(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if crate::eval_trace::enabled() {
        let tag = values.type_tag(object).ok();
        if !matches!(tag, Some(EVAL_TAG_OBJECT)) {
            let call_site = context.call_site();
            eprintln!(
                "[elephc-eval-trace] phase=method_call_non_object method={method_name:?} tag={tag:?} file={:?} line={}",
                call_site.0,
                call_site.2,
            );
        }
    }
    if values.type_tag(object)? == EVAL_TAG_CALLABLE {
        if let Some(result) =
            eval_native_closure_method(object, method_name, &evaluated_args, context, values)?
        {
            return Ok(result);
        }
    }
    let Ok(identity) = values.object_identity(object) else {
        if crate::eval_trace::enabled() {
            let call_site = context.call_site();
            eprintln!(
                "[elephc-eval-trace] phase=method_call_non_object method={method_name:?} tag={:?} file={:?} line={}",
                values.type_tag(object).ok(),
                call_site.0,
                call_site.2,
            );
        }
        let evaluated_args = positional_evaluated_arg_values(evaluated_args)?;
        return values.method_call(object, method_name, evaluated_args);
    };
    eval_reject_fiber_object_switch_during_pcntl_dispatch(
        object,
        method_name,
        context,
        values,
    )?;
    if crate::eval_trace::enabled() {
        let dynamic_class = context
            .dynamic_object_declaring_class(identity)
            .map(|(_, class)| class.name().to_string());
        let runtime_class = runtime_object_class_name(object, values).ok();
        eprintln!(
            "[elephc-eval-trace] phase=method_dispatch method={method_name:?} identity={identity} dynamic_class={dynamic_class:?} runtime_class={runtime_class:?}",
        );
    }
    eval_rebind_foreign_reflection_target(object, identity, context, values)?;
    // An eval class extending a Reflection owner OVERRIDES the owner's methods, and the fast
    // paths below answer those methods for any reflector without asking which class the object
    // is: Doctrine's `RuntimeReflectionProperty::getValue()` (which reads the mangled property
    // out of `(array) $object`) was bypassed and the plain `ReflectionProperty::getValue()` ran
    // instead. An override the object's eval class provides is dispatched first.
    if let Some((_, class)) = context.dynamic_object_declaring_class(identity) {
        let called_class_name = class.name().to_string();
        let extends_reflection_owner = context
            .class_native_parent_name(&called_class_name)
            .is_some_and(|parent| eval_is_reflection_owner_class(&parent));
        if extends_reflection_owner {
            if let Some((class_name, method)) =
                eval_dynamic_method_for_call(&called_class_name, method_name, context)
            {
                if !method.is_abstract()
                    && !method.is_static()
                    && validate_eval_method_access(
                        &class_name,
                        method_name,
                        method.visibility(),
                        context,
                        values,
                    )
                    .is_ok()
                {
                    return eval_dynamic_method_with_values(
                        &class_name,
                        &called_class_name,
                        &method,
                        object,
                        evaluated_args,
                        context,
                        values,
                    );
                }
            }
        }
    }
    if crate::eval_trace::enabled() && method_name.eq_ignore_ascii_case("getInterfaces") {
        eprintln!("[elephc-eval-trace] phase=reflection_get_interfaces_dispatch stage=after_rebind");
    }
    if eval_object_is_array_iterator(identity, context) {
        let positional = positional_evaluated_arg_values(evaluated_args.clone())?;
        if let Some(result) = eval_array_iterator_method_result(
            identity,
            method_name,
            &positional,
            context,
            values,
        )? {
            return Ok(result);
        }
    }
    // A closure created while another eval context ran (a macro registered during provider boot,
    // then bound with `bindTo()` from the context a compiled frame opened for the request) is
    // registered in its OWNER only; wrap that target so `bindTo`/`call`/`__invoke` reach it.
    let closure_target = match context.closure_object_target(identity).cloned() {
        Some(target) => Some(target),
        None => match crate::interpreter::builtins::eval_foreign_closure_owner(
            object, context, values,
        )? {
            Some(owner) => {
                let owner_target = unsafe { owner.context_ptr().as_ref() }
                    .and_then(|owner_context| owner_context.closure_object_target(identity))
                    .cloned();
                owner_target.map(|target| EvalClosureObjectTarget::ForeignContext {
                    target: Box::new(target),
                    owner,
                })
            }
            None => None,
        },
    };
    if let Some(target) = closure_target {
        if let Some(result) =
            eval_closure_object_method_result(target, method_name, evaluated_args.clone(), context, values)?
        {
            return Ok(result);
        }
    }
    if let Some(attribute_metadata) = context.eval_reflection_attribute(identity).cloned() {
        if method_name.eq_ignore_ascii_case("newInstance") {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            if let Some(flags) = elephc_builtin_contract::core_attribute_flags(attribute_metadata.attribute().name()) {
                let name = attribute_metadata.attribute().name();
                if (flags as u64) & attribute_metadata.target() == 0 {
                    return eval_throw_error(&format!("Attribute \"{name}\" cannot target this declaration"), context, values);
                }
                if flags & elephc_builtin_contract::CORE_ATTRIBUTE_IS_REPEATABLE == 0 && attribute_metadata.is_repeated() {
                    return eval_throw_error(&format!("Attribute \"{name}\" must not be repeated"), context, values);
                }
            }
            return eval_reflection_attribute_new_instance_result(
                attribute_metadata.attribute(),
                context,
                values,
            );
        }
        if method_name.eq_ignore_ascii_case("getArguments") {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            let Some(args) = attribute_metadata.attribute().args() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            return eval_class_attribute_args_result(args, values);
        }
        if method_name.eq_ignore_ascii_case("getTarget") {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            return values.int(attribute_metadata.target() as i64);
        }
        if method_name.eq_ignore_ascii_case("isRepeated") {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            return values.bool_value(attribute_metadata.is_repeated());
        }
    }
    if let Some(result) = eval_reflection_get_attributes_result(
        object,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_parameter_legacy_type_predicate_result(
        object,
        method_name,
        evaluated_args.clone(),
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_parameter_to_string_result(
        object,
        method_name,
        evaluated_args.clone(),
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) =
        eval_reflection_type_to_string_result(object, method_name, evaluated_args.clone(), values)?
    {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_to_string_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_implements_interface_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_is_subclass_of_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_is_instance_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_source_location_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_lazy_relation_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_name_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    let basic_metadata_result = eval_reflection_class_basic_metadata_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    );
    if crate::eval_trace::enabled() && method_name.eq_ignore_ascii_case("getInterfaces") {
        let outcome = match &basic_metadata_result {
            Ok(Some(_)) => "handled",
            Ok(None) => "unhandled",
            Err(_) => "error",
        };
        eprintln!(
            "[elephc-eval-trace] phase=reflection_get_interfaces_dispatch stage=basic_metadata result={outcome}"
        );
    }
    if let Some(result) = basic_metadata_result? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_has_method_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_has_property_result(
        object,
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_has_constant_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    let enum_result = eval_reflection_enum_methods_result(
        object,
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    );
    if crate::eval_trace::enabled() && method_name.eq_ignore_ascii_case("getInterfaces") {
        let outcome = match &enum_result {
            Ok(Some(_)) => "handled",
            Ok(None) => "unhandled",
            Err(_) => "error",
        };
        eprintln!(
            "[elephc-eval-trace] phase=reflection_get_interfaces_dispatch stage=enum result={outcome}"
        );
    }
    if let Some(result) = enum_result? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_relation_objects_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_trait_aliases_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_constant_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_constants_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_default_properties_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_static_properties_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_static_property_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_set_static_property_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_function_invoke_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_method_invoke_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_function_method_metadata_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_slot_getter_result(
        object,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_name_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_function_method_to_string_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_method_prototype_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_generator_method_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_set_accessible_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_hooks_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_default_error_result(
        identity, method_name, evaluated_args.clone(), context, values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_is_initialized_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_lazy_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_to_string_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_constant_to_string_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_enum_case_get_enum_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_get_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_raw_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_set_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_reflection_constant_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_reflection_constants_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_members_result(
        object,
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_member_result(
        object,
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(instance) = eval_reflection_class_new_instance_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(instance);
    }
    if let Some(instance) = eval_reflection_class_new_instance_without_constructor_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(instance);
    }
    if let Some(instance) = eval_reflection_class_new_lazy_object_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(instance);
    }
    // Owner-aware on purpose. `dynamic_object_class` resolves the owner's class NAME against the
    // ASKER's table, so an object whose class only the owning context declared is reported as
    // having no eval class at all, and dispatch falls through to the native path with the
    // object's runtime class -- `stdClass` for every interpreted instance. That is how a Symfony
    // request after the first one died: the container meta file is unserialized through the
    // null-handle bridge context, which autoloads and declares `ContainerParametersResource`,
    // and the request context that then calls `getParameters()` on it had never declared that
    // name, so the call was dispatched as `stdClass::getParameters()` and fatally missed.
    let Some((_, class)) = context.dynamic_object_declaring_class(identity) else {
        let class_name = runtime_object_class_name(object, values)?;
        if method_name.eq_ignore_ascii_case("__clone") {
            if let Some((declaring_class, visibility, is_static, is_abstract)) =
                eval_aot_method_dispatch_metadata(&class_name, method_name, values)?
            {
                if is_static || is_abstract {
                    return Err(EvalStatus::RuntimeFatal);
                }
                if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                    return eval_throw_method_access_error(
                        &declaring_class,
                        method_name,
                        visibility,
                        context,
                        values,
                    );
                }
            }
        }
        return eval_native_method_with_evaluated_args(
            object,
            &class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    };
    let called_class_name = class.name().to_string();
    if eval_enum_static_builtin_applies(&called_class_name, method_name, context).is_some() {
        return eval_enum_builtin_static_method_result(
            &called_class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    }
    let mut inaccessible_method = None;
    if let Some((class_name, method)) =
        eval_dynamic_method_for_call(&called_class_name, method_name, context)
    {
        if method.is_abstract() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if validate_eval_method_access(&class_name, method_name, method.visibility(), context, values)
            .is_ok()
        {
            if method.is_static() {
                return eval_dynamic_static_method_with_values(
                    &class_name,
                    &called_class_name,
                    &method,
                    evaluated_args,
                    context,
                    values,
                );
            }
            return eval_dynamic_method_with_values(
                &class_name,
                &called_class_name,
                &method,
                object,
                evaluated_args,
                context,
                values,
            );
        }
        inaccessible_method = Some((class_name, method));
    }
    if inaccessible_method.is_none() {
        if let Some(parent) = context.class_native_parent_name(&called_class_name) {
            let metadata = eval_aot_method_dispatch_metadata_in_hierarchy(
                &parent,
                method_name,
                context,
                values,
            )?;
            // An eval class inheriting `getTraceAsString()`/`__toString()` from a BUILTIN
            // Throwable is rendered here: the compiled bodies call a prelude function the bridge
            // cannot reach, and the synthetic bridge accessor answered "". A compiled user class
            // declaring either keeps its own body.
            let key = method_name.to_ascii_lowercase();
            let builtin_declared = metadata.as_ref().is_none_or(|(declaring, ..)| {
                matches!(declaring.trim_start_matches('\\'), "Exception" | "Error")
            });
            if builtin_declared
                && matches!(key.as_str(), "gettraceasstring" | "__tostring")
                && evaluated_args.is_empty()
                && eval_foreach_object_is_a(object, "Throwable", context, values)?
            {
                let text = crate::interpreter::builtins::eval_throwable_string(
                    object,
                    &called_class_name,
                    key == "gettraceasstring",
                    context,
                    values,
                )?;
                return values.string(&text);
            }
            if let Some((declaring_class, _, _, _)) = metadata {
                return eval_native_method_with_evaluated_args_bridge_scope(
                    object,
                    &parent,
                    method_name,
                    evaluated_args,
                    Some(&declaring_class),
                    Some(&called_class_name),
                    context,
                    values,
                );
            }
            if eval_native_instance_magic_method_available(&parent, context, values)? {
                return eval_native_method_with_evaluated_args(
                    object,
                    &parent,
                    method_name,
                    evaluated_args,
                    context,
                    values,
                );
            }
        }
    }
    if let Some(result) = eval_magic_instance_method_call(
        object,
        &called_class_name,
        method_name,
        evaluated_args,
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some((declaring_class, method)) = inaccessible_method {
        return eval_throw_method_access_error(
            &declaring_class,
            method.name(),
            method.visibility(),
            context,
            values,
        );
    }
    eval_throw_undefined_method_call_error(&called_class_name, method_name, context, values)
}

/// Dispatches PHP-visible methods on eval-backed `Closure` objects.
pub(super) fn eval_closure_object_method_result(
    target: EvalClosureObjectTarget,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if method_name.eq_ignore_ascii_case("__invoke") {
        return eval_closure_object_invoke_result(target, evaluated_args, context, values)
            .map(Some);
    }
    if method_name.eq_ignore_ascii_case("bindTo") {
        let (bound_this, bound_scope, rebinds_function_scope) =
            eval_closure_bind_to_args(evaluated_args, context, values)?;
        return eval_closure_bind_target(
            target,
            bound_this,
            bound_scope,
            rebinds_function_scope,
            context,
            values,
        )
        .map(Some);
    }
    if !method_name.eq_ignore_ascii_case("call") {
        return Ok(None);
    }
    let (bound_this, call_args) = eval_closure_call_split_args(evaluated_args)?;
    match target {
        EvalClosureObjectTarget::ForeignContext { target, owner } => {
            let Some(owner_context) = (unsafe { owner.context_ptr().as_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_closure_object_method_result(
                *target,
                "call",
                std::iter::once(EvaluatedCallArg {
                    name: None,
                    value: bound_this.borrowed(),
                    ref_target: None,
                })
                .chain(call_args)
                .collect(),
                owner_context,
                values,
            )
        }
        EvalClosureObjectTarget::Named(name) => {
            if context.closure(&name).is_some() {
                let callable = EvaluatedCallable::BoundClosure {
                    name,
                    bound_this: Some(bound_this),
                    bound_scope: None,
                };
                return eval_evaluated_callable_with_by_value_call_args(
                    &callable, call_args, context, values,
                )
                .map(Some);
            }
            eval_closure_call_warning_null(
                "Cannot rebind scope of closure created from function",
                values,
            )
            .map(Some)
        }
        EvalClosureObjectTarget::BoundNamed {
            name, bound_scope, ..
        } => {
            if context.closure(&name).is_some() {
                let callable = EvaluatedCallable::BoundClosure {
                    name,
                    bound_this: Some(bound_this),
                    bound_scope,
                };
                return eval_evaluated_callable_with_by_value_call_args(
                    &callable, call_args, context, values,
                )
                .map(Some);
            }
            eval_closure_call_warning_null(
                "Cannot rebind scope of closure created from function",
                values,
            )
            .map(Some)
        }
        EvalClosureObjectTarget::InvokableObject { object } => {
            if !eval_closure_call_bound_class_matches(object, bound_this, context, values)? {
                return eval_closure_call_warning_null(
                    "Cannot rebind scope of closure created from method",
                    values,
                )
                .map(Some);
            }
            let callable = EvaluatedCallable::InvokableObject { object: bound_this };
            eval_evaluated_callable_with_by_value_call_args(&callable, call_args, context, values)
                .map(Some)
        }
        EvalClosureObjectTarget::ObjectMethod {
            object,
            method,
            called_class: _,
            native_class,
            bridge_scope,
        } => {
            if !eval_closure_call_bound_class_matches(object, bound_this, context, values)? {
                return eval_closure_call_warning_null(
                    "Cannot rebind scope of closure created from method",
                    values,
                )
                .map(Some);
            }
            let called_class = Some(eval_closure_bound_object_class_name(
                bound_this, context, values,
            )?);
            let callable = EvaluatedCallable::ObjectMethod {
                object: bound_this,
                method,
                called_class,
                native_class,
                bridge_scope,
            };
            eval_evaluated_callable_with_by_value_call_args(
                &callable, call_args, context, values,
            )
                .map(Some)
        }
        EvalClosureObjectTarget::StaticMethod { .. } => eval_closure_call_warning_null(
            "Cannot bind an instance to a static closure",
            values,
        )
        .map(Some),
    }
}

/// Invokes the callable target retained behind a PHP-visible eval `Closure` object.
pub(in crate::interpreter) fn eval_closure_object_invoke_result(
    target: EvalClosureObjectTarget,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callable = match target {
        EvalClosureObjectTarget::ForeignContext { target, owner } => {
            let Some(owner_context) = (unsafe { owner.context_ptr().as_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            return eval_closure_object_invoke_result(
                *target,
                evaluated_args,
                owner_context,
                values,
            );
        }
        EvalClosureObjectTarget::Named(name) => EvaluatedCallable::Named {
            display_name: name.clone(),
            name,
        },
        EvalClosureObjectTarget::BoundNamed {
            name,
            bound_this,
            bound_scope,
        } => EvaluatedCallable::BoundClosure {
            name,
            bound_this,
            bound_scope,
        },
        EvalClosureObjectTarget::InvokableObject { object } => {
            EvaluatedCallable::InvokableObject { object }
        }
        EvalClosureObjectTarget::ObjectMethod {
            object,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => EvaluatedCallable::ObjectMethod {
            object,
            method,
            called_class,
            native_class,
            bridge_scope,
        },
        EvalClosureObjectTarget::StaticMethod {
            class_name,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => EvaluatedCallable::StaticMethod {
            class_name,
            method,
            called_class,
            native_class,
            bridge_scope,
        },
    };
    eval_evaluated_callable_with_call_array_args(&callable, evaluated_args, context, values)
}

/// Splits `Closure::call()` arguments into the bound object and forwarded closure args.
pub(super) fn eval_closure_call_split_args(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<(RuntimeCellHandle, Vec<EvaluatedCallArg>), EvalStatus> {
    let mut bound_this = None;
    let mut consumed_positional_receiver = false;
    let mut call_args = Vec::with_capacity(evaluated_args.len().saturating_sub(1));

    for arg in evaluated_args {
        if arg
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("newThis"))
        {
            if bound_this.replace(arg.value).is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            continue;
        }
        if arg.name.is_none() && !consumed_positional_receiver && bound_this.is_none() {
            consumed_positional_receiver = true;
            bound_this = Some(arg.value);
            continue;
        }
        call_args.push(arg);
    }

    bound_this
        .map(|receiver| (receiver, call_args))
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns whether `Closure::call()` may bind a method closure to the new object.
pub(super) fn eval_closure_call_bound_class_matches(
    original_object: RuntimeCellHandle,
    bound_this: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let original_class = eval_closure_bound_object_class_name(original_object, context, values)?;
    let bound_class = eval_closure_bound_object_class_name(bound_this, context, values)?;
    Ok(original_class.eq_ignore_ascii_case(&bound_class))
}

/// Returns whether `Closure::bind()` may bind a method closure to the new object.
pub(super) fn eval_closure_bind_bound_class_matches_method(
    original_object: RuntimeCellHandle,
    method_name: &str,
    bound_this: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(declaring_class) =
        eval_closure_bind_method_declaring_class(original_object, method_name, context, values)?
    else {
        return Ok(false);
    };
    eval_closure_object_is_instance_of(bound_this, &declaring_class, context, values)
}

/// Resolves the class that declares the method captured by a method Closure target.
pub(super) fn eval_closure_bind_method_declaring_class(
    original_object: RuntimeCellHandle,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let original_class = eval_closure_bound_object_class_name(original_object, context, values)?;
    if let Some((declaring_class, method)) = context.class_method(&original_class, method_name) {
        if method.is_static() || method.is_abstract() {
            return Ok(None);
        }
        return Ok(Some(declaring_class));
    }
    let native_class = context
        .class_native_parent_name(&original_class)
        .unwrap_or_else(|| original_class.clone());
    let Some((_, _, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(&native_class, method_name, context, values)?
    else {
        return Ok(None);
    };
    if is_static || is_abstract {
        return Ok(None);
    }
    let declaring_class = eval_aot_method_declaring_class(&native_class, method_name, values)?;
    Ok(Some(declaring_class))
}

/// Returns whether an object is an instance of the requested eval or generated class name.
pub(super) fn eval_closure_object_is_instance_of(
    object: RuntimeCellHandle,
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let object_class = eval_closure_bound_object_class_name(object, context, values)?;
    Ok(eval_static_syntax_object_matches_class(
        &object_class,
        class_name,
        context,
    ))
}

/// Emits PHP's `Closure::call()` warning and returns `null`.
pub(super) fn eval_closure_call_warning_null(
    message: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.warning(message)?;
    values.null()
}

/// Re-binds a synthetic Reflection object to this eval context when another context built it.
///
/// A reflector's target is registered per eval context under the object's runtime identity, and a
/// reflector built inside a different eval invocation carries no registration in the context that
/// later calls a method on it. Every eval-backed handler then declined for want of that binding
/// and the call fell back to the synthesized body, which answers from generated AOT class
/// metadata: on a class only the interpreter declared, `getMethods()` returned an empty array and
/// `getMethod()` threw "does not exist" for a method PHP finds, while `getName()` and
/// `hasMethod()` still answered correctly because they read the reflector's own slots. Those slots
/// also carry the target, so the binding is recovered from the object here and restored before
/// dispatch.
fn eval_rebind_foreign_reflection_target(
    object: RuntimeCellHandle,
    identity: u64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let owner_class = runtime_object_class_name(object, values)?;
    // A live reflector is exactly one kind; entries of any other kind under its address belong
    // to a freed reflector of that kind (see `forget_eval_reflection_identity_except`).
    let live_kind = match owner_class.as_str() {
        "ReflectionClass" | "ReflectionObject" | "ReflectionEnum" => {
            Some(crate::context::EvalReflectionKind::Class)
        }
        "ReflectionMethod" => Some(crate::context::EvalReflectionKind::Method),
        "ReflectionProperty" => Some(crate::context::EvalReflectionKind::Property),
        "ReflectionFunction" => Some(crate::context::EvalReflectionKind::Function),
        "ReflectionAttribute" => Some(crate::context::EvalReflectionKind::Attribute),
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase" => {
            Some(crate::context::EvalReflectionKind::ClassConstant)
        }
        _ => None,
    };
    if let Some(live_kind) = live_kind {
        context.forget_eval_reflection_identity_except(identity, live_kind);
    }
    match owner_class.as_str() {
        "ReflectionClass" | "ReflectionObject" | "ReflectionEnum" => {
            // The registry is keyed by object ADDRESS and lives for the request, while the
            // reflector it describes can be freed long before. A later reflector allocated at the
            // same address then found the dead one's target and answered for the wrong class --
            // and which reflector landed where depended on heap layout, so the failure moved with
            // anything that allocated (the eval trace included). An entry is trusted only while
            // the object's own `__name` slot still names it.
            let slot_name_result =
                eval_reflection_slot_string(object, &owner_class, "__name", context, values);
            if crate::eval_trace::enabled() && owner_class == "ReflectionClass" {
                let outcome = if slot_name_result.is_ok() { "ok" } else { "error" };
                eprintln!(
                    "[elephc-eval-trace] phase=reflection_rebind kind=class identity={identity} stage=read_name_slot result={outcome}"
                );
            }
            let slot_name = slot_name_result?;
            if let Some(bound) = context.eval_reflection_class_name(identity) {
                let still_bound = slot_name.as_deref().is_none_or(|name| {
                    eval_reflection_names_match(bound, name, context)
                });
                if still_bound {
                    if crate::eval_trace::enabled() {
                        eprintln!(
                            "[elephc-eval-trace] phase=reflection_rebind kind=class identity={identity} stage=already_bound"
                        );
                    }
                    return Ok(());
                }
            }
            #[cfg(not(test))]
            context.sync_global_eval_classes();
            let Some(name) = slot_name else {
                if crate::eval_trace::enabled() {
                    eprintln!(
                        "[elephc-eval-trace] phase=reflection_rebind kind=class identity={identity} stage=missing_name_slot"
                    );
                }
                return Ok(());
            };
            let declared = eval_context_declared_class_like(&name, context);
            let target_name = eval_rebound_reflection_class_name(&name, context, values)?;
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=reflection_rebind kind=class identity={identity} stage=resolved name={name:?} declared={declared} target_name={target_name:?} has_class={} native_parent={:?}",
                    context.has_class(&name),
                    context.class_native_parent_name(&name),
                );
            }
            let Some(target_name) = target_name else {
                return Ok(());
            };
            context.register_eval_reflection_class(identity, &target_name);
        }
        "ReflectionMethod" | "ReflectionProperty" => {
            let is_method = owner_class == "ReflectionMethod";
            // Same address-reuse hazard as a class reflector above: trust the entry only while
            // the object's own slots still name the same member.
            let slot_target =
                eval_reflection_member_slot_target(object, &owner_class, context, values)?;
            let bound = if is_method {
                context.eval_reflection_method(identity)
            } else {
                context.eval_reflection_property(identity)
            };
            if let Some((bound_class, bound_name)) = bound {
                let still_bound = slot_target.as_ref().is_none_or(|(class, name)| {
                    name == bound_name
                        && (class.is_empty() || eval_reflection_names_match(bound_class, class, context))
                });
                if still_bound {
                    return Ok(());
                }
            }
            let Some((declaring_class, name)) = slot_target else {
                return Ok(());
            };
            let Some(declaring_class) =
                eval_rebound_reflection_class_name(&declaring_class, context, values)?
            else {
                return Ok(());
            };
            if is_method {
                context.register_eval_reflection_method(identity, &declaring_class, &name);
            } else {
                context.register_eval_reflection_property(identity, &declaring_class, &name);
            }
        }
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase" => {
            if context.eval_reflection_class_constant(identity).is_some() {
                return Ok(());
            }
            let owner_kind = match owner_class.as_str() {
                "ReflectionEnumUnitCase" => EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE,
                "ReflectionEnumBackedCase" => EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE,
                _ => EVAL_REFLECTION_OWNER_CLASS_CONSTANT,
            };
            let Some((declaring_class, name)) =
                eval_reflection_member_slot_target(object, &owner_class, context, values)?
            else {
                return Ok(());
            };
            let Some(declaring_class) =
                eval_rebound_reflection_class_name(&declaring_class, context, values)?
            else {
                return Ok(());
            };
            context.register_eval_reflection_class_constant(
                identity,
                &declaring_class,
                &name,
                owner_kind,
            );
        }
        // Not a registered reflector kind (not a reflector at all, or a `ReflectionParameter`,
        // `ReflectionNamedType`, ... that no table describes): whatever the registry still files
        // under this address belongs to a reflector freed from the same block, and must not
        // answer for this object. A `ReflectionParameter` allocated where a closure's
        // `ReflectionFunction` had lived answered `getName()` with the closure's name.
        other if live_kind.is_none() => {
            if context.forget_eval_reflection_identity(identity) && crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=reflection_rebind kind=stale identity={identity} runtime_class={other:?} stage=forgotten"
                );
            }
        }
        _ => {}
    }
    Ok(())
}

/// Returns whether a registered reflection target still names the class a reflector's slot holds.
///
/// The registry stores the RESOLVED name (an alias's target), so the slot's own spelling is
/// compared both as written and resolved, PHP class names being case-insensitive either way.
fn eval_reflection_names_match(bound: &str, slot: &str, context: &ElephcEvalContext) -> bool {
    let bound = bound.trim_start_matches('\\');
    let slot = slot.trim_start_matches('\\');
    bound.eq_ignore_ascii_case(slot)
        || context
            .resolve_class_like_name(slot)
            .is_some_and(|resolved| bound.eq_ignore_ascii_case(resolved.trim_start_matches('\\')))
}

/// Returns whether the interpreter itself declared this class-like symbol.
fn eval_context_declared_class_like(name: &str, context: &ElephcEvalContext) -> bool {
    context.has_class(name)
        || context.has_interface(name)
        || context.has_enum(name)
        || context.has_trait(name)
}

/// Resolves a foreign reflector only when eval or emitted AOT metadata can serve its target.
/// Native interfaces and traits also have reflection rows, despite not satisfying class_exists.
/// Source-location handlers keep using native getters for targets without eval declarations.
fn eval_rebound_reflection_class_name(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let resolved_name = context.resolve_class_like_name(name);
    let target_name = resolved_name.as_deref().unwrap_or(name);
    if eval_context_declared_class_like(target_name, context) {
        Ok(Some(target_name.to_string()))
    } else if target_name.trim_start_matches('\\').eq_ignore_ascii_case("Closure") {
        // The builtin `Closure` is in neither registry, yet its members are known to the
        // interpreter's reflection; unbound, `getMethods()` read the compiled reflector's
        // empty slots.
        Ok(Some("Closure".to_string()))
    } else {
        values.reflection_canonical_class_name(target_name)
    }
}

/// Reads one reflected member's declaring class name and own name from the reflector's slots.
///
/// A method or property reflector names its declaring class in the `__class` string slot; a
/// compiled one builds its `ReflectionClass` only when `getDeclaringClass()` asks, so its
/// `__declaring_class` slot is `false`. Other members, and a reflector whose `__class` is empty,
/// fall back to `__declaring_class`, whose `ReflectionClass` carries the name in its own `__name`
/// slot; a member whose declaring class was never materialized stores `false` there and cannot
/// be re-bound, so it is left to the handlers that decline on a missing binding.
fn eval_reflection_member_slot_target(
    object: RuntimeCellHandle,
    owner_class: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, String)>, EvalStatus> {
    let Some(name) = eval_reflection_slot_string(object, owner_class, "__name", context, values)?
    else {
        return Ok(None);
    };
    if matches!(owner_class, "ReflectionMethod" | "ReflectionProperty") {
        if let Some(declaring_class) =
            eval_reflection_slot_string(object, owner_class, "__class", context, values)?
        {
            return Ok(Some((declaring_class, name)));
        }
    }
    let declaring = eval_with_native_bridge_scope(owner_class, context, || {
        values.property_get(object, "__declaring_class")
    })?;
    if values.type_tag(declaring)? != EVAL_TAG_OBJECT {
        values.release(declaring)?;
        return Ok(None);
    }
    let declaring_class =
        eval_reflection_slot_string(declaring, "ReflectionClass", "__name", context, values)?;
    values.release(declaring)?;
    Ok(declaring_class.map(|declaring_class| (declaring_class, name)))
}

/// Reads one non-empty string slot from a synthetic Reflection object under its own class scope.
///
/// The slots are private to the synthesized Reflection classes, so the read runs in that class's
/// scope the way every other native-bridge member read does.
fn eval_reflection_slot_string(
    object: RuntimeCellHandle,
    owner_class: &str,
    slot: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let value_result = eval_with_native_bridge_scope(owner_class, context, || {
        values.property_get(object, slot)
    });
    let trace_slot = crate::eval_trace::enabled()
        && owner_class == "ReflectionClass"
        && slot == "__name";
    if trace_slot {
        eprintln!(
            "[elephc-eval-trace] phase=reflection_rebind kind=class stage=property_get result={}",
            if value_result.is_ok() { "ok" } else { "error" }
        );
    }
    let value = value_result?;
    let type_result = values.type_tag(value);
    if trace_slot {
        eprintln!(
            "[elephc-eval-trace] phase=reflection_rebind kind=class stage=name_slot_type result={}",
            if type_result.is_ok() { "ok" } else { "error" }
        );
    }
    if type_result? != EVAL_TAG_STRING {
        values.release(value)?;
        return Ok(None);
    }
    let bytes_result = values.string_bytes(value);
    if trace_slot {
        eprintln!(
            "[elephc-eval-trace] phase=reflection_rebind kind=class stage=name_slot_bytes result={}",
            if bytes_result.is_ok() { "ok" } else { "error" }
        );
    }
    let bytes = bytes_result;
    values.release(value)?;
    let bytes = bytes?;
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => {
            if trace_slot {
                let raw = error.as_bytes();
                let preview = raw
                    .iter()
                    .take(64)
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                eprintln!(
                    "[elephc-eval-trace] phase=reflection_rebind kind=class stage=invalid_name_utf8 len={} hex={preview}",
                    raw.len()
                );
            }
            return Err(EvalStatus::RuntimeFatal);
        }
    };
    if text.is_empty() {
        return Ok(None);
    }
    Ok(Some(text))
}
