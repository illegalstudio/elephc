//! Purpose:
//! Resolves AOT method metadata, invokes native constructors, and materializes defaults.
//!
//! Called from:
//! - Native class construction and signature binding.
//!
//! Key details:
//! - Parent metadata lookup, constructor visibility, and compound default ownership stay centralized.

use super::*;

/// Finds generated/AOT method metadata on a class or its native parent chain.
pub(in crate::interpreter) fn eval_aot_method_dispatch_metadata_in_hierarchy(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility, bool, bool)>, EvalStatus> {
    let mut current = class_name.trim_start_matches('\\').to_string();
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(current.to_ascii_lowercase()) {
            return Ok(None);
        }
        if let Some(metadata) = eval_aot_method_dispatch_metadata(&current, method_name, values)? {
            return Ok(Some(metadata));
        }
        let Some(parent) = context.native_class_parent(&current) else {
            return Ok(None);
        };
        current = parent.to_string();
    }
}

/// Runs one generated/AOT constructor after native signature binding.
pub(in crate::interpreter) fn eval_native_constructor_with_evaluated_args(
    class_name: &str,
    object: RuntimeCellHandle,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_native_constructor_with_evaluated_args_and_ref_mode(
        class_name,
        object,
        evaluated_args,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Runs one generated/AOT constructor with caller-selected by-ref binding behavior.
pub(super) fn eval_native_constructor_with_evaluated_args_and_ref_mode(
    class_name: &str,
    object: RuntimeCellHandle,
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if eval_is_reflection_owner_class(class_name) {
        return eval_reflection_owner_parent_constructor(
            class_name,
            object,
            evaluated_args,
            context,
            values,
        );
    }
    let constructor_metadata = eval_aot_method_dispatch_metadata_in_hierarchy(
        class_name,
        "__construct",
        context,
        values,
    )?;
    let constructor_class = constructor_metadata
        .as_ref()
        .map(|(declaring_class, _, _, _)| declaring_class.clone())
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    let constructor_target_is_known = constructor_metadata.is_some();
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=native_constructor stage=resolved requested={class_name:?} declaring={constructor_class:?} args={}",
            evaluated_args.len(),
        );
    }
    let access_error = eval_native_constructor_access_error(&constructor_class, context, values)
        .map_err(|status| trace_native_constructor_error("access_metadata", &constructor_class, status))?;
    if let Some(message) = access_error {
        return eval_throw_error(&message, context, values);
    }
    let bridge_scope = eval_native_constructor_bridge_scope(&constructor_class, context, values)
        .map_err(|status| trace_native_constructor_error("bridge_scope", &constructor_class, status))?;
    let mut evaluated_args = evaluated_args;
    eval_unwrap_aggregate_constructor_arg(&constructor_class, &mut evaluated_args, context, values)?;
    let mut signature = number_scalar_binding_signature(
        &constructor_class,
        context.native_constructor_signature(&constructor_class),
    );
    #[cfg(not(test))]
    if constructor_target_is_known && signature.is_none() {
        crate::context::sync_global_eval_aot_metadata(context);
        signature = number_scalar_binding_signature(
            &constructor_class,
            context.native_constructor_signature(&constructor_class),
        );
    }
    if crate::eval_trace::enabled() {
        let signature_details = signature.as_ref().map(|signature| {
            (
                signature.param_count(),
                signature.param_names().len(),
                signature.required_param_count(),
                signature.bridge_supported(),
            )
        });
        eprintln!(
            "[elephc-eval-trace] phase=native_constructor stage=signature class={constructor_class:?} details={signature_details:?}",
        );
    }
    let bound_args = bind_native_callable_bound_args_with_mode(
        signature,
        evaluated_args,
        by_ref_mode,
        context,
        values,
    )
    .map_err(|status| trace_native_constructor_error("bind", &constructor_class, status))?;
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=native_constructor stage=bound class={constructor_class:?} args={}",
            bound_args.len(),
        );
    }
    let result = context.with_eval_global_sync(|context| {
        if let Some(scope) = bridge_scope.as_deref() {
            eval_with_native_bridge_scope(scope, context, || {
                if constructor_target_is_known {
                    values.construct_object_for_class(
                        &constructor_class,
                        object,
                        native_bound_arg_values(&bound_args),
                    )
                } else {
                    values.construct_object(object, native_bound_arg_values(&bound_args))
                }
            })
        } else if constructor_target_is_known {
            values.construct_object_for_class(
                &constructor_class,
                object,
                native_bound_arg_values(&bound_args),
            )
        } else {
            values.construct_object(object, native_bound_arg_values(&bound_args))
        }
    });
    let writeback = write_back_native_callable_ref_args(&bound_args, context, values);
    let released = release_native_bound_args(&bound_args, context, values);
    result.and(writeback).and(released)
}

/// Hands `IteratorIterator` / `RecursiveIteratorIterator` the iterator an `IteratorAggregate` stands for.
///
/// Both constructors take a `Traversable` and unwrap an aggregate through `getIterator()` themselves,
/// but the compiled constructor recognizes an aggregate by its compiled class id, which a class the
/// interpreter declared does not have: `new IteratorIterator($orderedHashMap)` in Symfony Form's
/// `Form::setData()` ended in "Cannot assign object to property of type Iterator". The unwrap is
/// done here, with the interpreter's own method dispatch, until the argument is an `Iterator`.
fn eval_unwrap_aggregate_constructor_arg(
    constructor_class: &str,
    evaluated_args: &mut [EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let lowered = constructor_class.trim_start_matches('\\').to_ascii_lowercase();
    if lowered != "iteratoriterator" && lowered != "recursiveiteratoriterator" {
        return Ok(());
    }
    let Some(first) = evaluated_args.first_mut() else {
        return Ok(());
    };
    if first.name.as_deref().is_some_and(|name| name != "iterator") {
        return Ok(());
    }
    // A cycle of aggregates would never end; PHP itself stops at an object that is no aggregate.
    for _ in 0..16 {
        if values.type_tag(first.value)? != EVAL_TAG_OBJECT {
            return Ok(());
        }
        if eval_foreach_object_is_a(first.value, "Iterator", context, values)?
            || !eval_foreach_object_is_a(first.value, "IteratorAggregate", context, values)?
        {
            return Ok(());
        }
        let iterator = eval_method_call_result(first.value, "getIterator", Vec::new(), context, values)?;
        // The caller's own argument is borrowed; only an aggregate this loop already unwrapped is
        // an owner of ours to give back before the next unwrap replaces it.
        if !first.value.is_borrowed() {
            eval_release_value(context, values, first.value)?;
        }
        first.value = iterator;
        first.ref_target = None;
    }
    Ok(())
}

/// Runs a Reflection owner's constructor on behalf of an eval subclass instance.
///
/// A compiled `new ReflectionProperty(...)` is materialized by the compiler and the interpreter
/// builds its own `new \ReflectionProperty(...)` through `eval_reflection_owner_new_object`; in
/// neither case does the class's `__construct` body fill the reflector's backing fields, so an
/// eval class extending an owner -- Doctrine's `RuntimeReflectionProperty extends
/// \ReflectionProperty`, which backs every mapped entity field -- ended up with an empty
/// reflector: `getName()` answered `""` and `parent::getValue($entity)` answered `null`.
///
/// php runs the owner's constructor on the subclass instance itself. Here the interpreter builds
/// a complete owner from the same arguments and copies every declared backing field onto the
/// subclass instance, which has the owner's native layout (`native_class_parent`).
fn eval_reflection_owner_parent_constructor(
    class_name: &str,
    object: RuntimeCellHandle,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(template) = eval_reflection_owner_new_object(class_name, evaluated_args, context, values)?
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let owner = class_name.trim_start_matches('\\').to_string();
    let names = values.reflection_property_names(&owner)?;
    let names = eval_runtime_string_array_to_vec(names, values);
    // The backing fields are PRIVATE to the owner class, and the generated property bridge checks
    // visibility against the context's class scope -- so the copy runs in the owner's scope, the
    // scope php's own constructor would be running in.
    let copied = names.and_then(|names| eval_with_native_bridge_scope(&owner, context, || {
        for name in names {
            // A field the owner leaves uninitialized for this reflector kind (a typed slot the
            // materializer never wrote) stays uninitialized on the copy too.
            if !values.property_is_initialized(template, &name)? {
                continue;
            }
            let value = values.property_get(template, &name).map_err(|status| {
                trace_native_constructor_error(&format!("owner_field_get:{name}"), class_name, status)
            })?;
            // A null or EMPTY array is the default the freshly allocated instance already holds,
            // and a hook-map field reads back as null for a hookless property while being typed
            // as a string-keyed map the bridge will not store a null into.
            let tag = values.type_tag(value)?;
            if tag == EVAL_TAG_NULL
                || (matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) && values.array_len(value)? == 0)
            {
                values.release(value)?;
                continue;
            }
            values.property_set(object, &name, value).map_err(|status| {
                let tag = values.type_tag(value).ok();
                trace_native_constructor_error(
                    &format!("owner_field_set:{name}:tag={tag:?}"),
                    class_name,
                    status,
                )
            })?;
        }
        Ok(())
    }));
    values.release(template)?;
    copied
}

/// Emits the failed native-constructor phase under opt-in eval tracing.
fn trace_native_constructor_error(
    stage: &str,
    constructor_class: &str,
    status: EvalStatus,
) -> EvalStatus {
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=native_constructor_error stage={stage} class={constructor_class:?} status={status:?}",
        );
    }
    status
}

/// Returns the generated/AOT constructor scope that the runtime bridge can recognize.
pub(super) fn eval_native_constructor_bridge_scope(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some((declaring_class, visibility)) =
        eval_reflection_aot_non_public_constructor(class_name, values)?
    else {
        return Ok(None);
    };
    if eval_native_constructor_access_allowed(&declaring_class, visibility, context) {
        Ok(Some(declaring_class))
    } else {
        Ok(None)
    }
}

/// Returns PHP's constructor access error for generated/AOT constructors, if inaccessible.
pub(super) fn eval_native_constructor_access_error(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some((declaring_class, visibility)) =
        eval_reflection_aot_non_public_constructor(class_name, values)?
    else {
        return Ok(None);
    };
    if eval_native_constructor_access_allowed(&declaring_class, visibility, context) {
        return Ok(None);
    }
    Ok(Some(format!(
        "Call to {} {}::__construct() from {}",
        eval_visibility_label(visibility),
        declaring_class.trim_start_matches('\\'),
        eval_native_constructor_scope_label(context)
    )))
}

/// Returns whether the current eval scope may call one generated/AOT constructor.
pub(super) fn eval_native_constructor_access_allowed(
    declaring_class: &str,
    visibility: EvalVisibility,
    context: &ElephcEvalContext,
) -> bool {
    match visibility {
        EvalVisibility::Public => true,
        EvalVisibility::Private => context
            .current_class_scope()
            .is_some_and(|current| same_eval_class_name(current, declaring_class)),
        EvalVisibility::Protected => context
            .current_class_scope()
            .is_some_and(|current| eval_classes_are_related(current, declaring_class, context)),
    }
}

/// Returns PHP's scope phrase for constructor access diagnostics.
pub(super) fn eval_native_constructor_scope_label(context: &ElephcEvalContext) -> String {
    context.current_class_scope().map_or_else(
        || String::from("global scope"),
        |class_name| format!("scope {}", class_name.trim_start_matches('\\')),
    )
}

/// Returns PHP's lowercase visibility label.
pub(super) fn eval_visibility_label(visibility: EvalVisibility) -> &'static str {
    match visibility {
        EvalVisibility::Public => "public",
        EvalVisibility::Protected => "protected",
        EvalVisibility::Private => "private",
    }
}

/// Allocates a fresh runtime cell for one invocation-safe native AOT default.
pub(in crate::interpreter) fn materialize_native_callable_default(
    default: &NativeCallableDefault,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match default {
        NativeCallableDefault::DeferredError(message) =>
            super::super::throwables::eval_throw_error(message, context, values),
        NativeCallableDefault::Expression { expression, lexical_class, .. } => {
            context.push_class_scope(lexical_class.clone());
            let mut scope = ElephcEvalScope::new();
            let result = super::super::expressions::eval_owned_expr(
                expression, context, &mut scope, values,
            );
            context.pop_class_scope();
            result
        }
        NativeCallableDefault::ExecutablePath => eval_const_fetch("PHP_BINARY", context, values),
        NativeCallableDefault::Null => values.null(),
        NativeCallableDefault::Bool(value) => values.bool_value(*value),
        NativeCallableDefault::Int(value) => values.int(*value),
        NativeCallableDefault::Float(value) => values.float(*value),
        NativeCallableDefault::String(value) => values.string(value),
        NativeCallableDefault::Bytes(value) => values.string_bytes_value(value),
        NativeCallableDefault::EmptyArray => values.array_new(0),
        NativeCallableDefault::Array(elements) => {
            materialize_native_callable_array_default(elements, context, values)
        }
        NativeCallableDefault::Object { class_name, args } => {
            materialize_native_callable_object_default(class_name, args, context, values)
        }
    }
}

/// Allocates one array-valued native AOT parameter default with fresh element cells.
pub(super) fn materialize_native_callable_array_default<V: RuntimeValueOps>(
    elements: &[NativeCallableArrayDefaultElement],
    context: &mut ElephcEvalContext,
    values: &mut V,
) -> Result<RuntimeCellHandle, EvalStatus> {
    materialize_native_callable_array_default_elements(
        elements,
        values,
        |value: &NativeCallableDefault, values: &mut V| {
            materialize_native_callable_default(value, context, values)
        },
    )
}

/// Allocates one array metadata value with fresh element cells and PHP auto-index keys.
///
/// `element_value` materializes each element payload, letting callers that cannot supply an
/// eval context (AOT user constants) reuse the same key normalization, auto-index sequence,
/// and copy-on-write insertion path as constructor and parameter defaults.
pub(in crate::interpreter) fn materialize_native_callable_array_default_elements<V, F>(
    elements: &[NativeCallableArrayDefaultElement],
    values: &mut V,
    mut element_value: F,
) -> Result<RuntimeCellHandle, EvalStatus>
where
    V: RuntimeValueOps,
    F: FnMut(&NativeCallableDefault, &mut V) -> Result<RuntimeCellHandle, EvalStatus>,
{
    let has_string_key = elements.iter().any(|element| {
        matches!(
            element.key,
            Some(NativeCallableArrayDefaultKey::String(_) | NativeCallableArrayDefaultKey::Bytes(_))
        )
    });
    let mut array = if has_string_key {
        builtins::collection_builder::EvalArrayBuilder::assoc(values, elements.len())?
    } else {
        builtins::collection_builder::EvalArrayBuilder::indexed(values, elements.len())?
    };
    let mut next_auto_key = 0;
    for element in elements {
        array.entry(
            |values| element_value(&element.value, values),
            |values, _| match &element.key {
                Some(NativeCallableArrayDefaultKey::Int(value)) => {
                    if *value >= next_auto_key {
                        next_auto_key = value.saturating_add(1);
                    }
                    values.int(*value)
                }
                Some(NativeCallableArrayDefaultKey::String(value)) => values.string(value),
                Some(NativeCallableArrayDefaultKey::Bytes(value)) => values.string_bytes_value(value),
                None => {
                    let key = values.int(next_auto_key)?;
                    next_auto_key = next_auto_key.saturating_add(1);
                    Ok(key)
                }
            },
        )?;
    }
    Ok(array.finish())
}

/// Allocates and constructs one object-valued native AOT parameter default.
pub(super) fn materialize_native_callable_object_default(
    class_name: &str,
    args: &[NativeCallableObjectDefaultArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = values.new_object(class_name)?;
    let mut constructor_args = Vec::with_capacity(args.len());
    let constructed = (|| {
        for arg in args {
            constructor_args.push(EvaluatedCallArg {
                name: arg.name.clone(),
                value: materialize_native_callable_default(&arg.value, context, values)?,
                ref_target: None,
            });
        }
        eval_native_constructor_with_evaluated_args(
            class_name, object, constructor_args.clone(), context, values,
        )
    })();
    let mut released = Ok(());
    for arg in constructor_args {
        let cleanup = release_expr_result(arg.value, context, values);
        if released.is_ok() { released = cleanup; }
    }
    if let Err(err) = constructed.and(released) {
        let _ = values.release(object);
        return Err(err);
    }
    Ok(object)
}
