//! Purpose:
//! Implements Reflection-driven class and attribute instantiation plus access checks.
//!
//! Called from:
//! - Method dispatch for ReflectionClass and ReflectionAttribute owners.
//!
//! Key details:
//! - Constructor visibility, without-constructor allocation, and attribute argument materialization align with PHP.

use super::*;

/// Returns the runtime-visible class name for a non-eval object receiver.
pub(in crate::interpreter) fn runtime_object_class_name(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let class_name = values.object_class_name(object)?;
    let bytes = values.string_bytes(class_name);
    values.release(class_name)?;
    String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Instantiates the class named by a materialized eval `ReflectionClass` object.
pub(super) fn eval_reflection_class_new_instance_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let direct_new_instance = method_name.eq_ignore_ascii_case("newInstance");
    if !direct_new_instance && !method_name.eq_ignore_ascii_case("newInstanceArgs") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=reflection_new_instance stage=target identity={identity} class={reflected_name:?} eval_class={}",
            context.class(&reflected_name).is_some(),
        );
    }
    if direct_new_instance {
        let arguments = eval_reflection_constructor_by_value_args(evaluated_args);
        return eval_reflection_class_instantiate(&reflected_name, arguments, context, values);
    }
    let array = eval_reflection_class_new_instance_args(evaluated_args)?;
    with_eval_array_call_arguments(array, context, values, |arguments, context, values| {
        eval_reflection_class_instantiate(&reflected_name, arguments, context, values)?
            .ok_or(EvalStatus::RuntimeFatal)
    }).map(Some)
}

/// Instantiates an already identified reflected class while its caller owns constructor arguments.
fn eval_reflection_class_instantiate(
    reflected_name: &str,
    constructor_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(message) =
        eval_reflection_eval_instantiation_error_message(reflected_name, context)
    {
        return eval_throw_error(&message, context, values);
    }
    if let Some(class) = context.class(reflected_name).cloned() {
        if crate::eval_trace::enabled() {
            let argument_shapes = constructor_args
                .iter()
                .map(|arg| {
                    (
                        arg.name.clone(),
                        values.type_tag(arg.value).ok(),
                        values.array_len(arg.value).ok(),
                    )
                })
                .collect::<Vec<_>>();
            eprintln!(
                "[elephc-eval-trace] phase=reflection_new_instance stage=eval_class_metadata class={:?} abstract={} constructor={} args={} argument_shapes={argument_shapes:?} enum={} trait={} native_parent={:?}",
                class.name(),
                class.is_abstract(),
                context.class_method(class.name(), "__construct").is_some(),
                constructor_args.len(),
                context.has_enum(class.name()),
                context.has_trait(class.name()),
                context.class_native_parent_name(class.name()),
            );
        }
        if let Some((_, constructor)) = context.class_method(class.name(), "__construct") {
            if constructor.visibility() != EvalVisibility::Public {
                return eval_throw_reflection_exception(
                    &format!(
                        "Access to non-public constructor of class {}",
                        class.name()
                    ),
                    context,
                    values,
                );
            }
        }
        let result = eval_reflection_public_constructor_scope(context, values, |context, values| {
            let constructor_name =
                format!("{}::__construct", class.name().trim_start_matches('\\'));
            let by_ref_mode = EvalByRefBindingMode::WarnByValue {
                callable_name: &constructor_name,
            };
            let mut scope = ElephcEvalScope::new();
            eval_dynamic_class_new_object_with_ref_mode(
                &class,
                constructor_args,
                by_ref_mode,
                context,
                &mut scope,
                values,
            )
        });
        return match result {
            Ok(instance) => Ok(Some(instance)),
            Err(status) => {
                if crate::eval_trace::enabled() {
                    eprintln!(
                        "[elephc-eval-trace] phase=reflection_new_instance stage=eval_class_instantiation class={:?} status={status:?}",
                        class.name(),
                    );
                }
                Err(status)
            }
        };
    }
    let class_name = context
        .resolve_class_name(reflected_name)
        .unwrap_or_else(|| reflected_name.to_string());
    let instantiation_error = match eval_reflection_aot_class_public_instantiation_error(
        &class_name,
        values,
    ) {
        Ok(error) => error,
        Err(status) => {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=reflection_new_instance stage=aot_class_flags class={class_name:?} status={status:?}"
                );
            }
            return Err(status);
        }
    };
    if let Some(error) = instantiation_error {
        return eval_throw_reflection_instantiation_error(error, context, values);
    }
    eval_reflection_public_constructor_scope(context, values, |context, values| {
        let constructor_name = format!("{}::__construct", class_name.trim_start_matches('\\'));
        let by_ref_mode = EvalByRefBindingMode::WarnByValue {
            callable_name: &constructor_name,
        };
        let instance = match values.new_object(&class_name) {
            Ok(instance) => instance,
            Err(status) => {
                if crate::eval_trace::enabled() {
                    eprintln!(
                        "[elephc-eval-trace] phase=reflection_new_instance stage=aot_allocation class={class_name:?} status={status:?}"
                    );
                }
                return Err(status);
            }
        };
        let constructor_result = eval_native_constructor_with_evaluated_args_and_ref_mode(
            &class_name,
            instance,
            constructor_args,
            by_ref_mode,
            context,
            values,
        );
        if let Err(status) = constructor_result {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=reflection_new_instance stage=aot_constructor class={class_name:?} status={status:?}"
                );
            }
            return Err(status);
        }
        Ok(Some(instance))
    })
}

/// Removes caller writeback targets for ReflectionClass::newInstance() by-value forwarding.
pub(super) fn eval_reflection_constructor_by_value_args(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Vec<EvaluatedCallArg> {
    evaluated_args
        .into_iter()
        .map(|arg| EvaluatedCallArg {
            name: arg.name,
            value: arg.value,
            ref_target: None,
        })
        .collect()
}

/// Binds the borrowed array passed to `ReflectionClass::newInstanceArgs()`.
pub(super) fn eval_reflection_class_new_instance_args(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("args")], evaluated_args)?;
    Ok(args[0])
}

/// Runs ReflectionClass construction with only public constructor visibility.
pub(super) fn eval_reflection_public_constructor_scope<T, V: RuntimeValueOps>(
    context: &mut ElephcEvalContext,
    values: &mut V,
    action: impl FnOnce(&mut ElephcEvalContext, &mut V) -> Result<T, EvalStatus>,
) -> Result<T, EvalStatus> {
    context.push_class_scope(String::new());
    let result = action(context, values);
    context.pop_class_scope();
    result
}

/// Allocates the class named by a materialized eval `ReflectionClass` without running `__construct()`.
pub(super) fn eval_reflection_class_new_instance_without_constructor_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("newInstanceWithoutConstructor") {
        return Ok(None);
    }
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    if let Some(message) =
        eval_reflection_eval_instantiation_error_message(&reflected_name, context)
    {
        return eval_throw_error(&message, context, values);
    }
    if let Some(class) = context.class(&reflected_name).cloned() {
        let mut scope = ElephcEvalScope::new();
        return eval_dynamic_class_allocate_object(&class, context, &mut scope, values).map(Some);
    }
    if context.has_interface(&reflected_name)
        || context.has_trait(&reflected_name)
        || context.has_enum(&reflected_name)
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let class_name = context
        .resolve_class_name(&reflected_name)
        .unwrap_or(reflected_name);
    if let Some(message) =
        eval_reflection_aot_class_without_constructor_error(&class_name, values)?
    {
        return eval_throw_error(&message, context, values);
    }
    values.new_object(&class_name).map(Some)
}

/// Answers the lazy-object API of `ReflectionClass` for classes the interpreter declared.
///
/// The compiled bodies call `$this->newInstanceWithoutConstructor()` compiled-to-compiled, so for
/// an interpreter-declared class they looked the name up in the compiled class table and died
/// "Class not found"; classes the compiled side knows keep going to those bodies.
///
/// `newLazyGhost()` is genuinely lazy: the instance is allocated without its constructor and the
/// initializer is parked in the context, to run on the first property access
/// ([`eval_initialize_lazy_ghost`]), exactly when php would run it. Running it eagerly is
/// observable -- a service container hands out ghosts whose initializer resolves configuration
/// the current command never needs, and a failure there (an empty secret, say) aborted commands
/// php completes. `newLazyProxy()` still calls its factory at once, with an uninitialized
/// instance standing in for the proxy, and returns the real object.
pub(super) fn eval_reflection_class_new_lazy_object_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(result) =
        eval_reflection_lazy_object_state_result(method_name, &evaluated_args, context, values)?
    {
        return Ok(Some(result));
    }
    let ghost = method_name.eq_ignore_ascii_case("newLazyGhost");
    if !ghost && !method_name.eq_ignore_ascii_case("newLazyProxy") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    if context.class(&reflected_name).is_none() {
        return Ok(None);
    }
    let Some(callback) = evaluated_args.first().map(|arg| arg.value) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let instance = eval_reflection_class_new_instance_without_constructor_result(
        identity,
        "newInstanceWithoutConstructor",
        Vec::new(),
        context,
        values,
    )?
    .ok_or(EvalStatus::RuntimeFatal)?;
    if ghost {
        let object_identity = match values.object_identity(instance) {
            Ok(object_identity) => object_identity,
            Err(status) => {
                values.release(instance)?;
                return Err(status);
            }
        };
        let initializer = values.retain(callback)?;
        if let Some(replaced) = context.register_lazy_ghost(object_identity, initializer) {
            values.release(replaced)?;
        }
        return Ok(Some(instance));
    }
    // `call_user_func` transfers its by-value argument cells to the callback activation, which
    // releases them when that activation ends. Keep the allocator's original instance owner
    // separate: the proxy returns the factory's result and discards this uninitialized instance.
    let factory_instance = values.retain(instance)?;
    let result = eval_call_user_func_with_values(vec![callback, factory_instance], context, values);
    values.release(instance)?;
    result.map(Some)
}

/// Answers `isUninitializedLazyObject()`, `initializeLazyObject()` and
/// `markLazyObjectAsInitialized()` for objects that carry a parked ghost initializer.
///
/// Objects without one fall through to the compiled methods, whose eager lazy objects are never
/// uninitialized.
fn eval_reflection_lazy_object_state_result(
    method_name: &str,
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let is_query = method_name.eq_ignore_ascii_case("isUninitializedLazyObject");
    let initialize = method_name.eq_ignore_ascii_case("initializeLazyObject");
    let mark = method_name.eq_ignore_ascii_case("markLazyObjectAsInitialized");
    if !is_query && !initialize && !mark {
        return Ok(None);
    }
    let Some(object) = evaluated_args.first().map(|arg| arg.value) else {
        return Ok(None);
    };
    let Ok(object_identity) = values.object_identity(object) else {
        return Ok(None);
    };
    if !context.is_uninitialized_lazy_ghost(object_identity) {
        return Ok(None);
    }
    if is_query {
        return values.bool_value(true).map(Some);
    }
    if initialize {
        eval_initialize_lazy_ghost(object_identity, object, context, values)?;
    } else if let Some(initializer) = context.take_lazy_ghost_initializer(object_identity) {
        values.release(initializer)?;
    }
    values.retain(object).map(Some)
}

/// Runs the parked initializer of a lazy ghost, once, before its first property access.
///
/// The entry is removed BEFORE the call: the initializer's own writes (a `__construct()` on the
/// ghost) are property accesses too and must not re-enter it. When the initializer throws, php
/// leaves the object uninitialized, so the entry is put back.
pub(in crate::interpreter) fn eval_initialize_lazy_ghost(
    object_identity: u64,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(initializer) = context.take_lazy_ghost_initializer(object_identity) else {
        return Ok(());
    };
    let argument = values.retain(object)?;
    match eval_call_user_func_with_values(vec![initializer, argument], context, values) {
        Ok(result) => {
            values.release(result)?;
            values.release(initializer)
        }
        Err(status) => {
            if let Some(replaced) = context.register_lazy_ghost(object_identity, initializer) {
                values.release(replaced)?;
            }
            Err(status)
        }
    }
}

/// Allocates an eval-declared class without running `__construct()`, for a COMPILED
/// `ReflectionClass::newInstanceWithoutConstructor()` whose class only the interpreter knows.
///
/// Returns `Ok(None)` when this context does not declare the class, so the caller can try the
/// next context. A class another context declared is imported first, as for `method_exists()`.
/// Symfony's `polyfill-deepclone` rebuilds cached Doctrine `ParserResult`s this way, and the
/// compiled call died "Class not found" on every class loaded at run time.
pub(in crate::interpreter) fn eval_new_object_without_constructor_result(
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    #[cfg(not(test))]
    context.sync_global_eval_classes();
    let name = name.trim_start_matches('\\');
    let known = context.class(name).is_some()
        || context.has_interface(name)
        || context.has_trait(name)
        || context.has_enum(name);
    if !known {
        return Ok(None);
    }
    if let Some(message) = eval_reflection_eval_instantiation_error_message(name, context) {
        return eval_throw_error(&message, context, values);
    }
    let Some(class) = context.class(name).cloned() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let mut scope = ElephcEvalScope::new();
    eval_dynamic_class_allocate_object(&class, context, &mut scope, values).map(Some)
}

/// Builds PHP's reflection instantiation error for eval non-instantiable class-likes.
pub(super) fn eval_reflection_eval_instantiation_error_message(
    reflected_name: &str,
    context: &ElephcEvalContext,
) -> Option<String> {
    if let Some(class) = context.class(reflected_name) {
        if class.is_abstract() {
            return Some(format!("Cannot instantiate abstract class {}", class.name()));
        }
        if let Some(enum_decl) = context.enum_decl(class.name()) {
            return Some(format!("Cannot instantiate enum {}", enum_decl.name()));
        }
        return None;
    }
    if let Some(interface) = context.interface(reflected_name) {
        return Some(format!("Cannot instantiate interface {}", interface.name()));
    }
    if let Some(trait_decl) = context.trait_decl(reflected_name) {
        return Some(format!("Cannot instantiate trait {}", trait_decl.name()));
    }
    context
        .enum_decl(reflected_name)
        .map(|enum_decl| format!("Cannot instantiate enum {}", enum_decl.name()))
}

/// Instantiates an attribute class for `ReflectionAttribute::newInstance()`.
pub(super) fn eval_reflection_attribute_new_instance_result(
    attribute: &EvalAttribute,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = eval_reflection_attribute_evaluated_args(attribute, values)?;
    if let Some(class) = context.class(attribute.name()).cloned() {
        let mut scope = ElephcEvalScope::new();
        return eval_dynamic_class_new_object(&class, args, context, &mut scope, values);
    }
    let class_name = context
        .resolve_class_name(attribute.name())
        .unwrap_or_else(|| attribute.name().trim_start_matches('\\').to_string());
    if !values.class_exists(&class_name)? {
        // php autoloads the attribute class here: an attribute is often the ONLY mention of its
        // class, so nothing loaded it before. Answering null instead made Symfony UX's
        // `LiveComponentMetadataFactory` pass null for every `#[LiveProp]`.
        let _ = crate::interpreter::eval_spl_autoload_class(&class_name, context, values)?;
        #[cfg(not(test))]
        context.sync_global_eval_classes();
        if let Some(class) = context.class(&class_name).cloned() {
            let mut scope = ElephcEvalScope::new();
            return eval_dynamic_class_new_object(&class, args, context, &mut scope, values);
        }
        if !values.class_exists(&class_name)? {
            return eval_throw_error(
                &format!("Attribute class \"{class_name}\" not found"),
                context,
                values,
            );
        }
    }
    let object = values.new_object(&class_name)?;
    if let Err(err) = eval_native_constructor_with_evaluated_args(
        &class_name,
        object,
        args,
        context,
        values,
    ) {
        let _ = values.release(object);
        return Err(err);
    }
    Ok(object)
}

/// Materializes eval attribute literal arguments as evaluated constructor args.
pub(super) fn eval_reflection_attribute_evaluated_args(
    attribute: &EvalAttribute,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let Some(args) = attribute.args() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    args.iter()
        .map(|arg| {
            Ok(EvaluatedCallArg {
                name: arg.name().map(str::to_string),
                value: eval_reflection_attribute_arg_value(arg.value(), values)?,
                ref_target: None,
            })
        })
        .collect()
}

/// Materializes one eval attribute literal as a constructor argument cell.
pub(super) fn eval_reflection_attribute_arg_value(
    arg: &EvalAttributeArg,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match arg {
        EvalAttributeArg::String(value) => values.string_literal(value),
        EvalAttributeArg::Bytes(value) => values.string_literal_bytes(value),
        EvalAttributeArg::Int(value) => values.int(*value),
        EvalAttributeArg::Float(bits) => values.float(f64::from_bits(*bits)),
        EvalAttributeArg::Bool(value) => values.bool_value(*value),
        EvalAttributeArg::Null => values.null(),
        EvalAttributeArg::Array(elements) => {
            eval_reflection_attribute_array_arg_value(elements, values)
        }
        EvalAttributeArg::Named { value, .. } | EvalAttributeArg::IntKeyed { value, .. } => {
            eval_reflection_attribute_arg_value(value, values)
        }
        // An expression needs the interpreter to evaluate it; reflection folds it before it
        // registers the attribute this metadata comes from.
        EvalAttributeArg::Expr(_) => Err(EvalStatus::RuntimeFatal),
    }
}

/// Materializes one retained attribute array literal for constructor calls.
pub(super) fn eval_reflection_attribute_array_arg_value(
    elements: &[EvalAttributeArg],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = if elements
        .iter()
        .any(|element| element.name().is_some() || element.int_key().is_some())
    {
        values.assoc_new(elements.len())?
    } else {
        values.array_new(elements.len())?
    };
    for (index, element) in elements.iter().enumerate() {
        let key = match element.name() {
            Some(name) => values.string(name)?,
            None => values.int(element.int_key().unwrap_or(index as i64))?,
        };
        let value = eval_reflection_attribute_arg_value(element.value(), values)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Resolves the method metadata visible from the current class scope.
pub(in crate::interpreter) fn eval_dynamic_method_for_call(
    object_class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassMethodRef)> {
    if let Some(current_class) = context.current_class_scope() {
        if context.class_is_a(object_class_name, current_class, false) {
            if let Some((declaring_class, method)) =
                context.class_own_method(current_class, method_name)
            {
                if method.visibility() == EvalVisibility::Private {
                    return Some((declaring_class, method));
                }
            }
        }
    }
    context.class_method(object_class_name, method_name)
}

/// Returns whether the current eval class scope can access one declared member.
pub(in crate::interpreter) fn validate_eval_member_access(
    declaring_class: &str,
    visibility: EvalVisibility,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if visibility == EvalVisibility::Public {
        return Ok(());
    }
    // `ReflectionMethod::invoke()`, `invokeArgs()` and `getClosure()` reach private and protected
    // methods by design (php has ignored `setAccessible()` since 8.1). Their compiled bodies call
    // through the ordinary callable machinery, so the reflection class is the calling scope here;
    // `MicroKernelTrait::loadRoutes()` invokes the private `configureRoutes()` exactly this way.
    let Some(current_class) = context.current_class_scope() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    // `ReflectionProperty::getValue()` / `setValue()` likewise reach private and protected
    // properties: their compiled bodies read and write `$object->{$name}` from the reflection
    // class's scope. Doctrine hydrates every entity through them; refusing the write left a
    // refreshed `App\Entity\User` without its private password, so the session token looked
    // stale and every admin page bounced back to the login form.
    if same_eval_class_name(current_class, "ReflectionMethod")
        || same_eval_class_name(current_class, "ReflectionProperty")
    {
        return Ok(());
    }
    match visibility {
        EvalVisibility::Public => Ok(()),
        EvalVisibility::Private => same_eval_class_name(current_class, declaring_class)
            .then_some(())
            .ok_or(EvalStatus::RuntimeFatal),
        EvalVisibility::Protected => {
            eval_classes_are_related(current_class, declaring_class, context)
                .then_some(())
                .ok_or(EvalStatus::RuntimeFatal)
        }
    }
}

/// Validates access to a METHOD, resolving a protected one against its ROOT declaring class.
///
/// php checks a protected method against the topmost ancestor whose declaration the override
/// replaces (`zend_get_function_root_class`), not against the class that happens to carry the
/// concrete body. Two SIBLINGS that both implement an abstract protected method of a shared
/// parent may therefore call it on each other -- which is exactly what Twig does:
/// `Twig\Template::yield()` calls `$this->doDisplay()` on a template object whose class is a
/// sibling of the caller's, both extending `Twig\Template`. Checking against the concrete class
/// refused that call and took every Twig render down with it.
pub(in crate::interpreter) fn validate_eval_method_access(
    declaring_class: &str,
    method_name: &str,
    visibility: EvalVisibility,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if visibility != EvalVisibility::Protected {
        return validate_eval_member_access(declaring_class, visibility, context);
    }
    if validate_eval_member_access(declaring_class, visibility, context).is_ok() {
        return Ok(());
    }
    let root = eval_method_root_declaring_class(declaring_class, method_name, context, values)?;
    validate_eval_member_access(&root, visibility, context)
}

/// Returns the topmost ancestor of `declaring_class` that declares `method_name` itself.
pub(in crate::interpreter) fn eval_method_root_declaring_class(
    declaring_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let mut root = declaring_class.trim_start_matches('\\').to_string();
    for ancestor in context.class_parent_names(declaring_class) {
        let declares_here = context.class_own_method(&ancestor, method_name).is_some()
            || eval_aot_method_dispatch_metadata(&ancestor, method_name, values)?
                .is_some_and(|(declaring, ..)| same_eval_class_name(&declaring, &ancestor));
        if declares_here {
            root = ancestor;
        }
    }
    Ok(root)
}

/// Returns true when two PHP class names refer to the same eval class.
pub(super) fn same_eval_class_name(left: &str, right: &str) -> bool {
    left.trim_start_matches('\\')
        .eq_ignore_ascii_case(right.trim_start_matches('\\'))
}

/// Returns true when two eval or generated classes are in the same inheritance family.
pub(super) fn eval_classes_are_related(left: &str, right: &str, context: &ElephcEvalContext) -> bool {
    same_eval_class_name(left, right)
        || context.class_is_a(left, right, false)
        || context.class_is_a(right, left, false)
        || native_class_is_a(left, right, context)
        || native_class_is_a(right, left, context)
}

/// Returns true when generated AOT parent metadata proves one class extends another.
pub(in crate::interpreter) fn native_class_is_a(
    class_name: &str,
    target: &str,
    context: &ElephcEvalContext,
) -> bool {
    // A class cannot be its own ancestor, so the parent walk ends; the step bound only keeps a
    // corrupt parent table from looping. It replaces a per-call seen-set that allocated and
    // SipHashed a lowercased copy of every class on the way -- ~1% of a Symfony request.
    const MAX_ANCESTORS: usize = 1024;
    let mut current = class_name.trim_start_matches('\\');
    let target = target.trim_start_matches('\\');
    for _ in 0..MAX_ANCESTORS {
        if same_eval_class_name(current, target) {
            return true;
        }
        let Some(parent) = context.native_class_parent(current) else {
            return false;
        };
        current = parent;
    }
    false
}
