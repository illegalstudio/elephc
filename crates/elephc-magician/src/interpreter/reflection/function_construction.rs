//! Purpose:
//! Constructs `ReflectionFunction` owners for eval, closure, and native targets.
//!
//! Called from:
//! - `crate::interpreter::reflection::eval_reflection_owner_new_object()`.
//!
//! Key details:
//! - Closure targets and native parameter/default metadata are attached once here.

use super::*;
use crate::context::{decode_eval_callback_adapter_capture, DecodedCallableCapture};
use std::ffi::c_void;

/// Normalized callable target used while constructing a reflected function.
struct EvalReflectionFunctionCallableArg {
    target: EvalClosureObjectTarget,
    lookup_name: String,
    display_name: String,
    metadata_closure: Option<EvalClosure>,
}

/// Builds an eval-backed `ReflectionFunction` object for eval or registered native functions.
pub(super) fn eval_reflection_function_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    eval_reflection_callable_new(evaluated_args, false, context, values)
}

/// Builds the `ReflectionMethod` php returns for `new ReflectionMethod($closure, '__invoke')`.
///
/// `Closure::__invoke()` has the closure's own signature, so the parameters are exactly the ones
/// `new ReflectionFunction($closure)` reports -- for an eval closure, a compiled closure or a
/// first-class callable alike -- under the method name `__invoke` of class `Closure`. Callers
/// reflect an invocable this way to describe it: Twig's `debug:twig` prints every function whose
/// callable is a first-class callable (`$this->generateAbsoluteUrl(...)`) from it, and reported
/// "Unsupported callback type" instead.
pub(super) fn eval_reflection_closure_invoke_method_new(
    closure: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let argument = EvaluatedCallArg {
        name: None,
        value: closure.borrowed(),
        ref_target: None,
    };
    eval_reflection_callable_new(vec![argument], true, context, values)
}

/// Shared body of [`eval_reflection_function_new`] and
/// [`eval_reflection_closure_invoke_method_new`]: resolves the callable's signature once, then
/// materializes it as a function or as `Closure::__invoke()`.
fn eval_reflection_callable_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    invoke_method: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("function")], evaluated_args)?;
    let callable_target =
        eval_reflection_function_callable_target_arg(args[0], context, values)?;
    let closure_target = callable_target.as_ref().map(|target| target.target.clone());
    let requested_name = match callable_target.as_ref() {
        Some(target) => target.display_name.clone(),
        None => eval_reflection_function_name_arg(args[0], context, values)?,
    };
    let lookup_name = callable_target
        .as_ref()
        .map(|target| target.lookup_name.clone())
        .unwrap_or_else(|| requested_name.trim_start_matches('\\').to_ascii_lowercase());
    // A first-class callable built from a METHOD resolves to that method, never to a global
    // function that shares its bare name: `$this->dump(...)` in Symfony's `YamlExtension` sits
    // next to var-dumper's global `dump(...$vars)`, and the by-name fallback below made
    // `(new \ReflectionMethod($closure, '__invoke'))->getParameters()` answer with the global
    // function's single variadic instead of the method's three parameters.
    let method_target = eval_reflection_closure_target_method(
        callable_target.as_ref().map(|target| &target.target),
        context,
        values,
    );
    let metadata_closure = callable_target
        .as_ref()
        .and_then(|target| target.metadata_closure.clone())
        .or_else(|| {
            method_target
                .is_none()
                .then(|| context.closure(&lookup_name).cloned())
                .flatten()
        });
    if let Some(closure) = metadata_closure {
        let function = closure.function();
        let required_parameter_count = eval_reflection_required_parameter_count(
            function.parameter_defaults(),
            function.parameter_is_variadic(),
        );
        let parameters = eval_reflection_function_parameters(
            function.name(),
            function.params(),
            function.attributes().to_vec(),
            function.parameter_attributes(),
            function.parameter_types(),
            function.parameter_defaults(),
            function.parameter_is_by_ref(),
            function.parameter_is_variadic(),
        );
        let return_type_metadata = function
            .return_type()
            .and_then(eval_reflection_parameter_type_metadata);
        return eval_reflection_callable_object_result(
            invoke_method,
            &requested_name,
            function.attributes(),
            &parameters,
            return_type_metadata.as_ref(),
            required_parameter_count,
            context,
            values,
        )
        .and_then(|object| {
            let object = eval_reflection_attach_callable_target(
                invoke_method,
                object,
                closure_target,
                args[0],
                context,
                values,
            )?;
            // The closure may belong to another eval context (the one that evaluated its
            // literal), which this context's by-name lookups cannot see; its own record is
            // the authority for the declaring file and line range.
            if !invoke_method {
                if let Some(location) = closure.function().source_location() {
                    let file = closure
                        .declaring_call_site()
                        .and_then(|site| site.3.clone())
                        .unwrap_or_else(|| context.eval_file_magic());
                    eval_reflection_write_source_slots(
                        object,
                        "ReflectionFunction",
                        &file,
                        location,
                        context,
                        values,
                    )?;
                }
            }
            Ok(object)
        })
        .map(Some);
    }
    if method_target.is_none() {
    if let Some(function) = context.function(&lookup_name).cloned() {
        let required_parameter_count = eval_reflection_required_parameter_count(
            function.parameter_defaults(),
            function.parameter_is_variadic(),
        );
        let parameters = eval_reflection_function_parameters(
            function.name(),
            function.params(),
            function.attributes().to_vec(),
            function.parameter_attributes(),
            function.parameter_types(),
            function.parameter_defaults(),
            function.parameter_is_by_ref(),
            function.parameter_is_variadic(),
        );
        let return_type_metadata = function
            .return_type()
            .and_then(eval_reflection_parameter_type_metadata);
        return eval_reflection_callable_object_result(
            invoke_method,
            function.name(),
            function.attributes(),
            &parameters,
            return_type_metadata.as_ref(),
            required_parameter_count,
            context,
            values,
        )
        .and_then(|object| {
            eval_reflection_attach_callable_target(
                    invoke_method,
                object,
                closure_target,
                args[0],
                context,
                values,
            )
        })
        .map(Some);
    }
    if let Some(function) = context.native_function(&lookup_name) {
        let reflected_name = requested_name.trim_start_matches('\\');
        let required_parameter_count = function.required_param_count();
        let mut parameters = eval_reflection_native_function_parameters(reflected_name, &function);
        // A compiled method closure (`$command->__invoke(...)`) arrives as a descriptor named
        // `Class::method`; its native record carries no attributes, so the method's and its
        // parameters' come from the AOT registry -- Symfony's InvokableCommand reads its
        // `#[Option]`/`#[Argument]` parameters through exactly this reflector.
        let mut method_attributes = function.attributes().to_vec();
        if let Some((class_name, method_name)) = reflected_name.split_once("::") {
            if method_attributes.is_empty() {
                method_attributes = eval_reflection_aot_method_attributes(
                    class_name,
                    class_name,
                    method_name,
                    context,
                );
            }
            for (index, parameter) in parameters.iter_mut().enumerate() {
                if parameter.attributes.is_empty() {
                    parameter.attributes = eval_reflection_aot_method_attributes(
                        class_name,
                        class_name,
                        &format!("{}#{}", method_name, index),
                        context,
                    );
                }
            }
        }
        let return_type_metadata = function
            .return_type()
            .and_then(eval_reflection_parameter_type_metadata);
        return eval_reflection_callable_object_result(
            invoke_method,
            reflected_name,
            &method_attributes,
            &parameters,
            return_type_metadata.as_ref(),
            required_parameter_count,
            context,
            values,
        )
        .and_then(|object| {
            eval_reflection_attach_callable_target(
                    invoke_method,
                object,
                closure_target,
                args[0],
                context,
                values,
            )
        })
        .map(Some);
    }
    }
    // A first-class callable built from a METHOD -- `$obj(...)`, `$obj->m(...)`, `Cls::m(...)`
    // -- has no entry in any FUNCTION table, so all three lookups above miss it and the
    // no-parameter fallback below used to claim it. PHP reflects such a Closure with the
    // method's own parameters, and this object's `ReflectionParameter` list is materialized
    // HERE, at construction: leaving it empty is what made Symfony's `ArgumentMetadataFactory`
    // -- it reads `(new \ReflectionFunction($controller(...)))->getParameters()` -- resolve
    // zero arguments for an invokable-object controller, so the controller call then bound
    // nothing to a required parameter.
    if let Some((declaring_class, method_name)) = method_target {
        let method_metadata =
            match eval_reflection_method_metadata(&declaring_class, &method_name, context) {
                Some(method_metadata) => Some(method_metadata),
                None => eval_reflection_aot_method_metadata_with_signature_if_exists(
                    &declaring_class,
                    &method_name,
                    context,
                    values,
                )?,
            };
        if let Some(method) = method_metadata {
            return eval_reflection_callable_object_result(
            invoke_method,
                &requested_name,
                &method.attributes,
                &method.parameters,
                method.return_type_metadata.as_ref(),
                method.required_parameter_count,
                context,
                values,
            )
            .and_then(|object| {
                eval_reflection_attach_callable_target(
                    invoke_method,
                    object,
                    closure_target,
                    args[0],
                    context,
                    values,
                )
            })
            .map(Some);
        }
    }
    if eval_builtin_uses_owned_arguments(&lookup_name) {
        let object = values.new_object("ReflectionFunction")?;
        let result = eval_native_constructor_with_evaluated_args(
            "ReflectionFunction", object, positional_args(args), context, values,
        ).and_then(|()| values.object_identity(object));
        return match result {
            Ok(identity) => {
                // Keep native reflection metadata while routing invocation through the shared eval binding.
                context.register_eval_reflection_function(identity, &lookup_name);
                Ok(Some(object))
            }
            Err(status) => {
                let _ = eval_release_value(context, values, object);
                Err(status)
            }
        };
    }
    if closure_target.is_some() {
        return eval_reflection_callable_object_result(
            invoke_method,
            &requested_name,
            &[],
            &[],
            None,
            0,
            context,
            values,
        )
        .and_then(|object| {
            eval_reflection_attach_callable_target(
                    invoke_method,
                object,
                closure_target,
                args[0],
                context,
                values,
            )
        })
        .map(Some);
    }
    if closure_target.is_none() {
        if let Some(contract) = elephc_builtin_contract::lookup(&lookup_name) {
            return eval_reflection_builtin_function_object_result(
                invoke_method,
                contract,
                context,
                values,
            )
            .map(Some);
        }
        // A name no function table knows is php's catchable ReflectionException, not an
        // unsupported construct: VarDumper's ExceptionCaster reflects every trace frame's
        // function name -- `{closure}` among them -- and relies on catching it.
        if !invoke_method && values.type_tag(args[0])? == EVAL_TAG_STRING {
            return eval_throw_reflection_exception(
                &format!("Function {}() does not exist", requested_name.trim_start_matches('\\')),
                context,
                values,
            );
        }
    }
    Ok(None)
}

/// Builds `new ReflectionFunction('<builtin>')` from the shared builtin contract.
///
/// A php builtin has no eval or AOT function record, so every lookup above missed it and the
/// construction failed outright -- Twig's `debug:twig` reflects `max`/`min`, which it registers as
/// plain string callables, to print their parameters. The contract carries exactly what
/// Reflection reports for an internal function: names, types, defaults, by-reference and
/// variadic parameters.
fn eval_reflection_builtin_function_object_result(
    invoke_method: bool,
    contract: &'static elephc_builtin_contract::BuiltinContract,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut names = Vec::new();
    let mut types = Vec::new();
    let mut defaults = Vec::new();
    let mut by_ref = Vec::new();
    let mut variadic = Vec::new();
    for param in contract.params {
        names.push(param.name.to_string());
        types.push(eval_reflection_builtin_param_type(&param.ty));
        defaults.push(param.default.as_ref().and_then(eval_reflection_builtin_default));
        by_ref.push(param.by_ref);
        variadic.push(false);
    }
    if let Some(spec) = contract.variadic {
        names.push(spec.name.to_string());
        types.push(None);
        defaults.push(None);
        by_ref.push(matches!(spec.passing, elephc_builtin_contract::PassingMode::ByReference));
        variadic.push(true);
    }
    let attribute_lists = vec![Vec::new(); names.len()];
    let parameters = eval_reflection_function_parameters(
        contract.name,
        &names,
        Vec::new(),
        &attribute_lists,
        &types,
        &defaults,
        &by_ref,
        &variadic,
    );
    let required = eval_reflection_required_parameter_count(&defaults, &variadic);
    eval_reflection_callable_object_result(
        invoke_method,
        contract.name,
        &[],
        &parameters,
        None,
        required,
        context,
        values,
    )
}

/// Maps one contract parameter type to reflection type metadata, when php declares one.
fn eval_reflection_builtin_param_type(
    ty: &elephc_builtin_contract::TypeSpec,
) -> Option<EvalParameterType> {
    let spec = eval_reflection_builtin_type_spec(ty)?;
    crate::ffi::native_methods::native_callable_param_type_from_abi(spec.as_ptr(), spec.len() as u64)
}

/// Spells a contract type the way generated native signature registration does.
fn eval_reflection_builtin_type_spec(ty: &elephc_builtin_contract::TypeSpec) -> Option<String> {
    use elephc_builtin_contract::TypeSpec;
    Some(match ty {
        TypeSpec::Int => "int".to_string(),
        TypeSpec::NullableInt => "?int".to_string(),
        TypeSpec::Float => "float".to_string(),
        TypeSpec::Str => "string".to_string(),
        TypeSpec::Bool => "bool".to_string(),
        TypeSpec::Mixed => "mixed".to_string(),
        TypeSpec::Array => "array".to_string(),
        TypeSpec::Callable => "callable".to_string(),
        TypeSpec::Object => "object".to_string(),
        TypeSpec::False => "false".to_string(),
        TypeSpec::Null => "null".to_string(),
        TypeSpec::Nullable(inner) => format!("?{}", eval_reflection_builtin_type_spec(inner)?),
        TypeSpec::Union(members) => members
            .iter()
            .map(eval_reflection_builtin_type_spec)
            .collect::<Option<Vec<_>>>()?
            .join("|"),
        TypeSpec::Void | TypeSpec::Ptr => return None,
    })
}

/// Maps one contract default to the literal expression Reflection evaluates for it.
fn eval_reflection_builtin_default(default: &elephc_builtin_contract::DefaultSpec) -> Option<EvalExpr> {
    use elephc_builtin_contract::DefaultSpec;
    Some(match default {
        DefaultSpec::Null => EvalExpr::Const(EvalConst::Null),
        DefaultSpec::Int(value) => EvalExpr::Const(EvalConst::Int(*value)),
        DefaultSpec::Bool(value) => EvalExpr::Const(EvalConst::Bool(*value)),
        DefaultSpec::Float(value) => EvalExpr::Const(EvalConst::Float(*value)),
        DefaultSpec::Str(value) => EvalExpr::Const(EvalConst::String((*value).to_string())),
        DefaultSpec::IntMax => EvalExpr::Const(EvalConst::Int(i64::MAX)),
        // The profile-selected mask is whatever the program's `E_ALL` constant names.
        DefaultSpec::ErrorAll => EvalExpr::ConstFetch("E_ALL".to_string()),
        DefaultSpec::EmptyArray => EvalExpr::Array(Vec::new()),
        DefaultSpec::Constant(name) => EvalExpr::ConstFetch((*name).to_string()),
        DefaultSpec::Expr(_) | DefaultSpec::ClassConstant { .. } => return None,
    })
}

/// Attaches the reflected callable to a `ReflectionFunction`; the `Closure::__invoke()`
/// `ReflectionMethod` carries its parameters already and has no `__callable` slot to fill.
fn eval_reflection_attach_callable_target(
    invoke_method: bool,
    object: RuntimeCellHandle,
    closure_target: Option<EvalClosureObjectTarget>,
    source: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if invoke_method {
        return Ok(object);
    }
    eval_reflection_attach_function_callable_target(object, closure_target, source, context, values)
}

/// Returns the retained callable target for a Closure object or descriptor value.
fn eval_reflection_function_callable_target_arg(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionFunctionCallableArg>, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_OBJECT => {
            let identity = values.object_identity(value)?;
            let Some(target) = context.closure_object_target(identity).cloned() else {
                return eval_reflection_foreign_closure_target_arg(identity, context);
            };
            let lookup_name = eval_reflection_function_closure_target_name(&target);
            Ok(Some(EvalReflectionFunctionCallableArg {
                display_name: lookup_name.clone(),
                lookup_name,
                target,
                metadata_closure: None,
            }))
        }
        EVAL_TAG_CALLABLE => {
            let descriptor = values.raw_value_word(value)? as usize as *mut c_void;
            let trace = crate::eval_trace::enabled();
            if trace {
                eprintln!(
                    "[elephc-eval-trace] phase=reflection-callable descriptor={descriptor:p}"
                );
            }
            let Some(decoded) = (unsafe { decode_callable_descriptor(descriptor) }) else {
                let captures = unsafe { decode_eval_callback_adapter_capture(descriptor.cast()) };
                let Some((captured_context, captured)) = captures else {
                    if trace {
                        eprintln!(
                            "[elephc-eval-trace] phase=reflection-adapter-missing descriptor={descriptor:p}"
                        );
                    }
                    return Err(EvalStatus::RuntimeFatal);
                };
                if trace {
                    eprintln!(
                        "[elephc-eval-trace] phase=reflection-adapter descriptor={descriptor:p} context=0x{captured_context:x} capture_tag={} capture=0x{:x}",
                        captured.type_tag, captured.value_word
                    );
                }
                let captured = if captured.type_tag == EVAL_TAG_MIXED {
                    RuntimeCellHandle::from_raw(
                        captured.value_word as usize as *mut crate::value::RuntimeCell,
                    )
                } else {
                    values.raw_word_value(captured.type_tag, captured.value_word)?
                };
                if values.type_tag(captured)? == EVAL_TAG_OBJECT {
                    let identity = values.object_identity(captured)?;
                    let captured_context = unsafe {
                        (captured_context as usize as *mut ElephcEvalContext).as_ref()
                    }
                    .ok_or(EvalStatus::RuntimeFatal)?;
                    if trace {
                        eprintln!(
                            "[elephc-eval-trace] phase=reflection-adapter-object identity={identity} target_found={}",
                            captured_context.closure_object_target(identity).is_some()
                        );
                    }
                    if let Some(target) = captured_context.closure_object_target(identity).cloned() {
                        let lookup_name = eval_reflection_function_closure_target_name(&target);
                        return Ok(Some(EvalReflectionFunctionCallableArg {
                            display_name: lookup_name.clone(),
                            metadata_closure: captured_context.closure(&lookup_name).cloned(),
                            lookup_name,
                            target,
                        }));
                    }
                }
                // The adapter wraps an ARRAY callable: compiled `$callback(...)` on
                // `[$provider, 'boot']`, where the provider's class was declared at run time.
                // php's `$callback(...)` is a Closure over that method, so `new
                // ReflectionFunction($callback(...))` reflects the method; a bare array is not a
                // Closure and keeps falling through to the name reading below. Laravel's
                // `Container::getClassForCallable()` asks exactly this of every service
                // provider's `boot` -- and reached it as soon as `is_callable()` answered
                // truthfully for an interpreted class the frame's context already knew.
                if values.is_array_like(captured)? {
                    if let Ok(callable) = eval_callable(captured.borrowed(), context, values) {
                        let mut target = eval_closure_object_target_from_callable(callable.into_callable_transferring_receiver());
                        // The normalizer hands the receiver over OWNED; the adapter's own
                        // capture keeps the object alive for as long as the reflection can
                        // name it, and the targets of Closure objects are stored borrowed too.
                        let receiver = target.receiver_mut().copied();
                        if let Some(receiver) = receiver {
                            release_expr_result(receiver, context, values)?;
                        }
                        let lookup_name = eval_reflection_function_closure_target_name(&target);
                        return Ok(Some(EvalReflectionFunctionCallableArg {
                            display_name: lookup_name.clone(),
                            lookup_name,
                            target,
                            metadata_closure: None,
                        }));
                    }
                }
                return eval_reflection_function_callable_target_arg(captured, context, values);
            };
            if trace {
                eprintln!(
                    "[elephc-eval-trace] phase=reflection-descriptor descriptor={descriptor:p} display={:?}",
                    decoded.display_name
                );
            }
            let bound_this = eval_reflection_descriptor_bound_this(decoded.bound_this, values)?;
            let lookup_name = format!("{{closure:native:{:x}}}", descriptor as usize);
            if context.native_function(&lookup_name).is_none() {
                if let Err(_existing) =
                    context.define_native_function(lookup_name.clone(), decoded.function)
                {
                    if trace {
                        eprintln!(
                            "[elephc-eval-trace] phase=reflection-descriptor-register failed=true"
                        );
                    }
                    return Err(EvalStatus::RuntimeFatal);
                }
            }
            let target = match bound_this {
                Some(bound_this) => EvalClosureObjectTarget::BoundNamed {
                    name: lookup_name.clone(),
                    bound_scope: Some(eval_closure_bound_object_class_name(
                        bound_this,
                        context,
                        values,
                    )?),
                    bound_this: Some(bound_this),
                },
                None => EvalClosureObjectTarget::Named(lookup_name.clone()),
            };
            Ok(Some(EvalReflectionFunctionCallableArg {
                target,
                lookup_name,
                display_name: decoded.display_name,
                metadata_closure: None,
            }))
        }
        _ => Ok(None),
    }
}

/// Resolves a closure object that ANOTHER eval context created, through its owner.
///
/// A closure an interpreted file made -- a service provider's `static function () {...}` stored
/// in a compiled container -- is registered only with the context that evaluated it. Compiled
/// `new ReflectionFunction($callback)` runs in a context of its own, found no target, and fell
/// through to the function-NAME reading of its argument, which an object is not: "eval()
/// fragment uses an unsupported construct". The owner still knows the closure, so its target
/// and metadata are borrowed from there, under a lease that keeps the owner alive.
fn eval_reflection_foreign_closure_target_arg(
    identity: u64,
    context: &ElephcEvalContext,
) -> Result<Option<EvalReflectionFunctionCallableArg>, EvalStatus> {
    let Some(owner) = crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity) else {
        return Ok(None);
    };
    if std::ptr::eq(owner as *const ElephcEvalContext, context as *const ElephcEvalContext) {
        return Ok(None);
    }
    let Some(owner_context) = (unsafe { owner.as_ref() }) else {
        return Ok(None);
    };
    let Some(owner_target) = owner_context.closure_object_target(identity).cloned() else {
        return Ok(None);
    };
    let lookup_name = eval_reflection_function_closure_target_name(&owner_target);
    let metadata_closure = owner_context.closure(&lookup_name).cloned();
    let Some(lease) = crate::context::pcntl_runtime::begin_foreign_context_use(owner) else {
        return Ok(None);
    };
    Ok(Some(EvalReflectionFunctionCallableArg {
        display_name: lookup_name.clone(),
        lookup_name,
        target: EvalClosureObjectTarget::ForeignContext {
            target: Box::new(owner_target),
            owner: lease,
        },
        metadata_closure,
    }))
}

/// Materializes a descriptor's `$this` capture only when it contains an object receiver.
fn eval_reflection_descriptor_bound_this(
    capture: Option<DecodedCallableCapture>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(capture) = capture else {
        return Ok(None);
    };
    let value = values.raw_word_value(capture.type_tag, capture.value_word)?;
    if values.type_tag(value)? == EVAL_TAG_OBJECT {
        return Ok(Some(value));
    }
    values.release(value)?;
    Ok(None)
}

/// Returns the function-like name exposed for a Closure-backed ReflectionFunction.
pub(super) fn eval_reflection_function_closure_target_name(target: &EvalClosureObjectTarget) -> String {
    match target {
        EvalClosureObjectTarget::ForeignContext { target, .. } => {
            eval_reflection_function_closure_target_name(target)
        }
        EvalClosureObjectTarget::Named(name)
        | EvalClosureObjectTarget::BoundNamed { name, .. } => name.clone(),
        EvalClosureObjectTarget::InvokableObject { .. } => String::from("__invoke"),
        EvalClosureObjectTarget::ObjectMethod { method, .. }
        | EvalClosureObjectTarget::StaticMethod { method, .. } => method.clone(),
    }
}

/// Attaches original callable metadata and storage to a synthetic reflected function.
pub(super) fn eval_reflection_attach_function_callable_target(
    object: RuntimeCellHandle,
    closure_target: Option<EvalClosureObjectTarget>,
    source: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(closure_target) = closure_target else {
        return Ok(object);
    };
    let identity = values.object_identity(object)?;
    context.register_eval_reflection_function_closure_target(identity, closure_target);
    if values.type_tag(source)? == EVAL_TAG_CALLABLE {
        eval_reflection_with_declaring_class_scope("ReflectionFunction", context, |_| {
            values.property_set(object, "__callable", source)
        })?;
    }
    // The owner was materialized before its closure target existed, so a compiled closure's
    // `START:END:FILE` record (reached only through that target) was not yet readable.
    eval_reflection_store_native_source_slots(object, EVAL_REFLECTION_OWNER_FUNCTION, context, values)?;
    Ok(object)
}

/// Returns parameter names for a registered native function, filling missing bridge names.
pub(super) fn eval_reflection_native_function_parameter_names(function: &NativeFunction) -> Vec<String> {
    (0..function.param_count())
        .map(|index| {
            function
                .param_names()
                .get(index)
                .filter(|name| !name.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("arg{}", index))
        })
        .collect()
}

/// Builds ReflectionParameter metadata for one registered native AOT function.
pub(super) fn eval_reflection_native_function_parameters(
    function_name: &str,
    function: &NativeFunction,
) -> Vec<EvalReflectionParameterMetadata> {
    // Only the PHP-visible slots: a compiler-internal one (the hidden `func_get_args()`
    // collector, a hidden argument count) is not a parameter php would report.
    let visible = function.frame_shape().visible_param_indexes();
    let all_names = eval_reflection_native_function_parameter_names(function);
    let all_types = eval_reflection_native_function_parameter_types(function);
    let all_defaults = eval_reflection_native_function_parameter_defaults(function);
    let parameter_names = visible
        .iter()
        .map(|index| all_names.get(*index).cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    let parameter_count = parameter_names.len();
    let parameter_attributes = vec![Vec::new(); parameter_count];
    let parameter_types = visible
        .iter()
        .map(|index| all_types.get(*index).cloned().flatten())
        .collect::<Vec<_>>();
    let parameter_defaults = visible
        .iter()
        .map(|index| all_defaults.get(*index).cloned().flatten())
        .collect::<Vec<_>>();
    let parameter_is_by_ref = visible
        .iter()
        .map(|index| function.param_by_ref(*index))
        .collect::<Vec<_>>();
    let parameter_is_variadic = visible
        .iter()
        .map(|index| function.param_variadic(*index))
        .collect::<Vec<_>>();
    let mut parameters = eval_reflection_function_parameters(
        function_name,
        &parameter_names,
        function.attributes().to_vec(),
        &parameter_attributes,
        &parameter_types,
        &parameter_defaults,
        &parameter_is_by_ref,
        &parameter_is_variadic,
    );
    for (index, parameter) in parameters.iter_mut().enumerate() {
        if let Some(name) = function.param_default_constant_name(index) {
            parameter.default_value_constant_name = Some(name.to_string());
        }
        if let Some(NativeCallableDefault::Expression { reflection_factory, .. }) = function.param_default(index) {
            parameter.native_default_factory = *reflection_factory;
        }
    }
    parameters
}

/// Converts registered native function parameter types into reflection metadata input.
pub(super) fn eval_reflection_native_function_parameter_types(
    function: &NativeFunction,
) -> Vec<Option<EvalParameterType>> {
    (0..function.param_count())
        .map(|index| function.param_type(index).cloned())
        .collect()
}

/// Converts registered native function defaults into eval constant expressions.
pub(super) fn eval_reflection_native_function_parameter_defaults(
    function: &NativeFunction,
) -> Vec<Option<EvalExpr>> {
    (0..function.param_count())
        .map(|index| {
            function
                .param_default(index)
                .map(eval_reflection_native_callable_default_expr)
        })
        .collect()
}

/// Builds one `ReflectionFunction` object from retained eval function metadata.
pub(super) fn eval_reflection_function_object_result(
    function_name: &str,
    attributes: &[EvalAttribute],
    parameters: &[EvalReflectionParameterMetadata],
    return_type_metadata: Option<&EvalReflectionParameterTypeMetadata>,
    required_parameter_count: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_FUNCTION,
        function_name,
        attributes,
        &[],
        &[],
        &[],
        &[],
        None,
        parameters,
        return_type_metadata,
        None,
        None,
        None,
        eval_reflection_callable_flags(attributes),
        required_parameter_count as u64,
        0,
        None,
        None,
        context,
        values,
    )
}

/// Materializes one resolved callable signature as a `ReflectionFunction`, or, for
/// `invoke_method`, as the `ReflectionMethod` of `Closure::__invoke()`.
fn eval_reflection_callable_object_result(
    invoke_method: bool,
    function_name: &str,
    attributes: &[EvalAttribute],
    parameters: &[EvalReflectionParameterMetadata],
    return_type_metadata: Option<&EvalReflectionParameterTypeMetadata>,
    required_parameter_count: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !invoke_method {
        // A first-class callable built from a method (`$obj(...)`, `$obj->m(...)`, `Cls::m(...)`)
        // arrives as a descriptor named `Class::method` (or `object::method`); php reports just
        // the method name. A `{closure:...}` name legitimately contains `::` and is kept whole.
        let function_name = match function_name.rsplit_once("::") {
            Some((_, method)) if !function_name.starts_with('{') && !method.is_empty() => method,
            _ => function_name,
        };
        return eval_reflection_function_object_result(
            function_name,
            attributes,
            parameters,
            return_type_metadata,
            required_parameter_count,
            context,
            values,
        );
    }
    let member = EvalReflectionMemberMetadata {
        declaring_class_name: Some("Closure".to_string()),
        source_file: None,
        source_location: None,
        attributes: attributes.to_vec(),
        visibility: EvalVisibility::Public,
        is_static: false,
        is_final: false,
        is_abstract: false,
        is_readonly: false,
        is_promoted: false,
        is_dynamic: false,
        modifiers: eval_reflection_method_modifiers(EvalVisibility::Public, false, false, false),
        type_metadata: None,
        settable_type_metadata: None,
        return_type_metadata: return_type_metadata.cloned(),
        default_value: None,
        default_value_trait_origin: None,
        required_parameter_count,
        // `Closure::__invoke()` reflects the internal method's argument info: a parameter stays
        // optional, but php reports no default value for it (`isDefaultValueAvailable()` is false
        // where `new ReflectionFunction($closure)` has one). Twig's `debug:twig` prints
        // `path(name, parameters, relative)` from exactly that.
        parameters: parameters
            .iter()
            .cloned()
            .map(|mut parameter| {
                parameter.default_value = None;
                parameter.default_value_constant_name = None;
                parameter
            })
            .collect(),
    };
    eval_reflection_member_object_result(
        EVAL_REFLECTION_OWNER_METHOD,
        "__invoke",
        &member,
        context,
        values,
    )
}
