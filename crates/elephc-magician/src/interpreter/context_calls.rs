//! Purpose:
//! Exposes persistent-context function, object, and metadata operations to generated code.
//!
//! Called from:
//! - The Magician FFI function/object call and symbol-query adapters.
//!
//! Key details:
//! - Arguments arrive already evaluated and borrow the caller's runtime cells.
//! - Generic templates specialize before ordinary dispatch, and thrown owners become outcomes.

use super::*;

/// Executes a zero-argument function declared in the shared eval context.
pub fn execute_context_function_zero_args(
    context: &mut ElephcEvalContext,
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    execute_context_function(context, name, Vec::new(), values)
}

/// Executes a function declared in the shared eval context with prepared argument cells.
pub fn execute_context_function(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match execute_context_function_outcome(context, name, args, values)? {
        EvalOutcome::Value(result) => Ok(result),
        EvalOutcome::Throwable(error) => {
            context.set_pending_throw(error);
            Err(EvalStatus::UncaughtThrowable)
        }
    }
}

/// Executes a function declared in the shared eval context and preserves thrown cells.
pub fn execute_context_function_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let result = (|| {
        let arguments = args
            .iter()
            .map(|&value| EvaluatedCallArg { name: None, value, ref_target: None })
            .collect::<Vec<_>>();
        let function = match context.function(name).cloned() {
            Some(function) => function,
            None => match generic_function_for_call(name, &arguments, context, values)? {
                Some(function) => function,
                None => {
                    // A name this context does not declare can still be one the bridge answers
                    // itself: the two backtrace functions, the OPcache family and the procedural
                    // date aliases are runtime handlers rather than PHP-visible builtins.
                    // `eval_context_function_exists()` already reports exactly that set, so
                    // answering the CALL from the same table is what stops the two from
                    // disagreeing. They did: an unqualified `debug_backtrace()` written inside a
                    // namespace arrives here as PHP's global fallback, and refusing it turned the
                    // call into `Call to undefined function <namespace>\debug_backtrace()`.
                    return eval_builtin_with_values(name, &args, context, values)?
                        .ok_or(EvalStatus::UnsupportedConstruct);
                }
            },
        };
        eval_dynamic_function_with_evaluated_args(&function, arguments, context, values)
    })();
    match result {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Reports whether the bridge can answer a call to this function name by itself.
///
/// This is the predicate `function_exists()` answers with AND the set
/// `execute_context_function_outcome()` dispatches through, deliberately the same one: a name this
/// reports callable is a name that call resolves, so the existence answer and the call answer
/// cannot drift apart the way they had.
pub fn context_function_is_callable(context: &ElephcEvalContext, name: &str) -> bool {
    eval_function_probe_exists(context, name)
}

/// Executes a named eval-context callable with arguments from a PHP array container.
pub fn execute_context_function_call_array(
    context: &mut ElephcEvalContext,
    name: &str,
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match execute_context_function_call_array_outcome(context, name, arg_array, values)? {
        EvalOutcome::Value(result) => Ok(result),
        EvalOutcome::Throwable(error) => {
            context.set_pending_throw(error);
            Err(EvalStatus::UncaughtThrowable)
        }
    }
}

/// Executes a named eval-context callable from an argument array and preserves thrown cells.
pub fn execute_context_function_call_array_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    if !values.is_array_like(arg_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = with_eval_array_call_arguments(arg_array, context, values, |arguments, context, values| {
        eval_callable_with_call_array_args(name, arguments, context, values)
    });
    match result {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Executes a callback value with a prepared argument array in the shared eval context.
pub fn execute_context_callable_call_array_outcome(
    context: &mut ElephcEvalContext,
    callback: RuntimeCellHandle,
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    match eval_call_user_func_array_with_values(callback, arg_array, context, values) {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Rebinds `$this` on an eval `Closure` object on behalf of generated `Closure::bind` code.
pub fn execute_context_closure_bind_this(
    context: &mut ElephcEvalContext,
    closure: RuntimeCellHandle,
    new_this: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_closure_bind_this_for_ffi(closure, new_this, context, values)
}

/// Probes whether a callback value is callable in the shared eval context.
pub fn execute_context_is_callable(
    context: &ElephcEvalContext,
    callback: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    eval_is_callable_value(callback, None, context, values)
}

/// Constructs a class declared in the shared eval context with prepared positional arguments.
///
/// THE TERMINAL DECISION for `new` through the bridge. Its `try_` half answers `None` when no
/// route could construct the name — not AOT metadata, not this context, not an autoloader — and
/// that `None` used to become `EvalStatus::UnsupportedConstruct`, printed as
/// `Fatal error: eval() fragment uses an unsupported construct`. That wording is wrong for both
/// of the two things it covered, and they are not the same thing:
///
/// A name PHP itself provides as a builtin class is a GAP IN THIS BUILD. PHP would have
/// constructed the object, so the message has to say so in elephc's own voice rather than blame
/// the program. The catalog is `reflection::class_lookup::eval_reflection_class_like_is_internal`,
/// the predicate `ReflectionClass::isInternal()` already answers from, so the two cannot drift.
///
/// Any other name is genuinely undefined, and PHP's answer is a CATCHABLE `Error` reading
/// `Class "X" not found`. Measured with `php -n` 8.5.6: a runtime include followed by
/// `try { new NoSuchClassAnywhere(); } catch (\Throwable $e) { … }` prints
/// `loaded;Error: Class "NoSuchClassAnywhere" not found;done` and exits 0. Reporting a fatal
/// there took the `catch` away, and it matters more now that the checker DEFERS unknown class
/// names for programs that include PHP at run time — deferring a compile-time diagnostic is only
/// honest if the runtime raises the right thing, in a form user code can receive.
///
/// The throwable is handed back as an `EvalOutcome::Throwable`, the same channel the arms inside
/// `execute_context_try_new_object_outcome` already use, so the AOT side unwinds into the
/// caller's `catch` instead of aborting.
pub fn execute_context_new_object_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    if let Some(outcome) = execute_context_try_new_object_outcome(context, name, args, values)? {
        return Ok(outcome);
    }
    if eval_reflection_class_like_is_internal(name) {
        note_eval_runtime_failure(
            format!(
                "builtin class \"{}\" is not available in this build",
                name.trim_start_matches('\\')
            ),
            context,
        );
        return Err(EvalStatus::RuntimeFatal);
    }
    match eval_throw_class_not_found_error::<()>(name, context, values) {
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
        Ok(()) => Err(EvalStatus::RuntimeFatal),
    }
}

/// Constructs a runtime Reflection owner from prepared positional arguments.
pub fn execute_reflection_new_object_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let evaluated_args = args
        .into_iter()
        .map(|value| EvaluatedCallArg {
            name: None,
            value: value.borrowed(),
            ref_target: None,
        })
        .collect();
    match eval_reflection_owner_new_object(name, evaluated_args, context, values) {
        Ok(Some(result)) => Ok(EvalOutcome::Value(result)),
        Ok(None) => Err(EvalStatus::UnsupportedConstruct),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Attempts to construct a runtime-owned built-in or eval-declared class.
/// Returns `None` when neither runtime surface recognizes the class name.
pub fn execute_context_try_new_object_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalOutcome>, EvalStatus> {
    let args = args.into_iter().map(|value| (None, value)).collect();
    execute_context_try_new_object_named_outcome(context, name, args, values)
}

/// Unpacks a compiled `new $class(...$args)` argument container into call arguments.
///
/// The container is the array the spread produced: an integer key is a positional argument and a
/// string key a named one, exactly as PHP binds `...$array`. The cells are borrowed from it.
pub fn eval_unpack_constructor_arg_container(
    container: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<(Option<String>, RuntimeCellHandle)>, EvalStatus> {
    let len = values.array_len(container)?;
    let mut args = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.array_iter_key(container, position)?;
        let value = values.array_get(container, key)?;
        let name = if values.type_tag(key)? == EVAL_TAG_STRING {
            Some(String::from_utf8(values.string_bytes(key)?).map_err(|_| EvalStatus::RuntimeFatal)?)
        } else {
            None
        };
        args.push((name, value));
    }
    Ok(args)
}

/// Allocates an eval-declared class without its constructor; `None` when this context lacks it.
pub fn execute_context_try_new_object_without_constructor_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalOutcome>, EvalStatus> {
    match eval_new_object_without_constructor_result(name, context, values) {
        Ok(result) => Ok(result.map(EvalOutcome::Value)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .map(Some)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Attempts construction with arguments that may carry PHP named-argument names.
pub fn execute_context_try_new_object_named_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<(Option<String>, RuntimeCellHandle)>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalOutcome>, EvalStatus> {
    let evaluated_args = args
        .into_iter()
        .map(|(name, value)| EvaluatedCallArg {
            name,
            value: value.borrowed(),
            ref_target: None,
        })
        .collect::<Vec<_>>();
    // A generic class name specializes before construction; every other name comes back as is.
    let concrete = match ensure_generic_class(
        name,
        Some(&evaluated_args),
        "__construct",
        context,
        &mut ElephcEvalScope::new(),
        values,
    ) {
        Ok(concrete) => concrete,
        Err(EvalStatus::UncaughtThrowable) => {
            return context
                .take_pending_throw()
                .map(EvalOutcome::Throwable)
                .map(Some)
                .ok_or(EvalStatus::UncaughtThrowable);
        }
        Err(status) => return Err(status),
    };
    let name = concrete.as_str();
    match eval_reflection_owner_new_object(name, evaluated_args.clone(), context, values) {
        Ok(Some(result)) => return Ok(Some(EvalOutcome::Value(result))),
        Ok(None) => {}
        Err(EvalStatus::UncaughtThrowable) => {
            return context
                .take_pending_throw()
                .map(EvalOutcome::Throwable)
                .map(Some)
                .ok_or(EvalStatus::UncaughtThrowable);
        }
        Err(status) => {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=try_new_object_error stage=reflection class={name:?} status={status:?}",
                );
            }
            return Err(status);
        }
    }
    let class_name = name.trim_start_matches('\\');
    if values.class_exists(class_name)? {
        let mut scope = ElephcEvalScope::new();
        return match eval_new_object_result(
            class_name,
            evaluated_args,
            context,
            &mut scope,
            values,
        ) {
            Ok(result) => Ok(Some(EvalOutcome::Value(result))),
            Err(EvalStatus::UncaughtThrowable) => context
                .take_pending_throw()
                .map(EvalOutcome::Throwable)
                .map(Some)
                .ok_or(EvalStatus::UncaughtThrowable),
            Err(status) => {
                if crate::eval_trace::enabled() {
                    eprintln!(
                        "[elephc-eval-trace] phase=try_new_object_error stage=aot_class class={class_name:?} status={status:?}",
                    );
                }
                Err(status)
            }
        };
    }
    if !context.has_class(class_name) {
        if let Err(status) = eval_spl_autoload_classlike_definition(class_name, context, values) {
            // Loading the class file can itself THROW: a file guarded by
            // `if (!interface_exists(X::class)) { throw new \LogicException(...); }` does, when X
            // is not installed. php hands that exception to the `new` expression's caller, which
            // may catch it; returning the bare status made it an uncatchable fatal.
            if status == EvalStatus::UncaughtThrowable {
                if let Some(thrown) = context.take_pending_throw() {
                    return Ok(Some(EvalOutcome::Throwable(thrown)));
                }
            }
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=try_new_object_error stage=autoload class={class_name:?} status={status:?}",
                );
            }
            return Err(status);
        }
    }
    let Some(class) = context.class(class_name).cloned() else {
        return Ok(None);
    };
    let mut scope = ElephcEvalScope::new();
    match eval_dynamic_class_new_object(&class, evaluated_args, context, &mut scope, values) {
        Ok(result) => Ok(Some(EvalOutcome::Value(result))),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .map(Some)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=try_new_object_error stage=dynamic_class class={class_name:?} status={status:?}",
                );
            }
            Err(status)
        }
    }
}

/// Calls a method on a value that may be an eval-created object.
pub fn execute_context_method_call_outcome(
    context: &mut ElephcEvalContext,
    object: RuntimeCellHandle,
    method: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let evaluated_args = eval_bridge_positional_args(args, values)?;
    match eval_method_call_result_with_evaluated_args(object, method, evaluated_args, context, values) {
        // `return $this` hands back the receiver cell itself without a retain: the CALL side
        // compensates, as the interpreter's own operand handling does for interpreted callers.
        // A compiled caller boxed the receiver just for this call and releases that box
        // afterwards, so without this retain the fluent `$compiler->write(...)->raw(...)` chain
        // Twig's nodes compile with read a freed cell on its second link.
        Ok(result) if result == object => Ok(EvalOutcome::Value(values.retain(result)?)),
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Makes a clone COMPILED code made of an eval-declared object an object of the same class.
///
/// Compiled `clone` copies the object's native layout, which for an eval-declared class is its
/// compiled ancestor's: the copy came back as that ancestor, with none of the properties the
/// eval class keeps in its context. Symfony's `Request::duplicate()` does `clone $this`, so the
/// Laravel subclass's `duplicate(): static` failed its own return type. Returns whether the source
/// belonged to this context's class table.
pub fn adopt_compiled_object_clone(
    source_identity: u64,
    clone: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(class_name) = context
        .dynamic_object_class(source_identity)
        .map(|class| class.name().to_string())
    else {
        return Ok(false);
    };
    let clone_identity = values.object_identity(clone)?;
    context.register_dynamic_object(clone_identity, &class_name);
    for (property, value) in context.dynamic_property_values_for_clone(source_identity) {
        statements::eval_store_dynamic_property_value(
            clone_identity,
            &property,
            value,
            context,
            values,
        )?;
    }
    context.clone_dynamic_property_aliases(source_identity, clone_identity);
    Ok(true)
}

/// Drains an interpreted `Traversable` into a COMPILED `ArrayIterator` a compiled `foreach` can
/// drive through its own Iterator dispatch.
///
/// An object an eval context built wraps the generic native header: the compiled `foreach`
/// found neither `getIterator()` nor `rewind()` on it and iterated nothing, so Twig's compiled
/// helpers saw every interpreted Doctrine collection as empty. The entries are read the way
/// `iterator_to_array($object, true)` reads them -- the same walker, generators and aggregates
/// included -- which drains the source when the loop STARTS rather than step by step.
pub fn execute_context_native_iterator(
    context: &mut ElephcEvalContext,
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let entries = crate::interpreter::iterator_to_array::eval_iterator_to_array_result(
        object, true, context, values,
    )?;
    let iterator = values.new_object("ArrayIterator")?;
    let args = vec![EvaluatedCallArg {
        name: None,
        value: entries.borrowed(),
        ref_target: None,
    }];
    let constructed = eval_native_constructor_with_evaluated_args(
        "ArrayIterator",
        iterator,
        args,
        context,
        values,
    );
    let released = values.release(entries);
    if let Err(status) = constructed {
        let _ = values.release(iterator);
        return Err(status);
    }
    released?;
    Ok(iterator)
}

/// Resolves an interpreted `IteratorAggregate` to the traversable it hands over, or `None` when
/// `object` is not one.
///
/// `new IteratorIterator($aggregate)` wraps `getIterator()`'s result, not the aggregate. A compiled
/// constructor cannot see the interfaces of an interpreted class (it has no compiled class id), so
/// it asks here. The hand-off is followed through nested aggregates, bounded like the spread
/// operand's. The result is an owned cell.
pub fn execute_context_unwrap_aggregate(
    context: &mut ElephcEvalContext,
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let mut current = object;
    let mut owned = false;
    for _ in 0..10 {
        let identity = values.object_identity(current)?;
        if context.has_eval_generator(identity)
            || !eval_foreach_object_is_a(current, "IteratorAggregate", context, values)?
        {
            return Ok(owned.then_some(current));
        }
        let inner = eval_method_call_result(current, "getIterator", Vec::new(), context, values)?;
        if owned {
            values.release(current)?;
        }
        current = inner;
        owned = true;
        if values.type_tag(current)? != EVAL_TAG_OBJECT {
            values.release(current)?;
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    values.release(current)?;
    Err(EvalStatus::RuntimeFatal)
}

/// Reads an instance property through the active eval context.
pub fn execute_context_property_get_outcome(
    context: &mut ElephcEvalContext,
    object: RuntimeCellHandle,
    property: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    match eval_property_get_result(object, property, context, values) {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Converts one boxed value through eval's PHP string-context semantics.
///
/// Dynamic objects dispatch their eval-declared `__toString()` implementation, while an
/// exception raised by that method is returned as an uncaught throwable outcome for the C ABI.
pub fn execute_context_string_outcome(
    context: &mut ElephcEvalContext,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    match eval_string_context_value(value, context, values) {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Calls a static method on a class-like symbol known to the shared eval context.
pub fn execute_context_static_method_call_outcome(
    context: &mut ElephcEvalContext,
    class_name: &str,
    method: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let evaluated_args = eval_bridge_positional_args(args, values)?;
    let result = (|| {
        let name = ensure_generic_class(class_name, Some(&evaluated_args), method, context, &mut ElephcEvalScope::new(), values)?;
        eval_static_method_call_result(&name, method, evaluated_args, context, values)
    })();
    match result {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Decodes native bridge reference markers into positional eval call arguments.
fn eval_bridge_positional_args(
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    args.into_iter()
        .map(|marker| {
            let (value, ref_target) = eval_invoker_ref_arg_value_and_target(marker, None, values)?;
            // A by-reference marker is decoded into a FRESH cell over the caller's slot (a
            // retained box for an array or object). The caller owns the marker, not that cell,
            // so the call's cleanup must drop it: left unowned, `Z::f($localArray)` kept the
            // whole array alive once per call. A plain argument is the caller's own box.
            Ok(EvaluatedCallArg {
                name: None,
                value: if value != marker { value } else { value.borrowed() },
                ref_target,
            })
        })
        .collect()
}

/// Resolves object class-name builtins against eval dynamic-object metadata first.
pub fn execute_context_object_class_name(
    context: &mut ElephcEvalContext,
    lookup: &str,
    object_or_class: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match lookup {
        "get_class" => eval_get_class_result(object_or_class, context, values),
        "get_parent_class" => eval_get_parent_class_result(object_or_class, context, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Resolves class/interface/trait relation metadata through eval dynamic metadata.
pub fn execute_context_class_relation(
    context: &mut ElephcEvalContext,
    name: &str,
    target: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_class_relation_result(name, &[target], context, values)
}

/// Answers `defined($name)` (`want_value == false`) or `constant($name)` for a runtime name the
/// compiled registry does not hold: `Class::CONST` and enum cases, and constants `eval()` defined.
pub fn execute_context_dynamic_constant(
    context: &mut ElephcEvalContext,
    want_value: bool,
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let result = if want_value {
        eval_constant_result(&[name], context, values)
    } else {
        eval_defined_result(&[name], context, values)
    };
    match result {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Fetches a class-like constant through eval dynamic metadata and runtime fallback hooks.
pub fn execute_context_class_constant_fetch(
    context: &mut ElephcEvalContext,
    class_name: &str,
    constant_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let result = (|| {
        let name = ensure_generic_class(class_name, None, "", context, &mut ElephcEvalScope::new(), values)?;
        eval_class_constant_fetch_result(&name, constant_name, context, values)
    })();
    match result {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Reads a static property through eval dynamic metadata and runtime fallback hooks.
pub fn execute_context_static_property_get(
    context: &mut ElephcEvalContext,
    class_name: &str,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let result = (|| {
        let name = ensure_generic_class(class_name, None, "", context, &mut ElephcEvalScope::new(), values)?;
        eval_static_property_get_result(&name, property_name, context, values)
    })();
    match result {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Writes a static property through eval dynamic metadata and runtime fallback hooks.
pub fn execute_context_static_property_set(
    context: &mut ElephcEvalContext,
    class_name: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalOutcome>, EvalStatus> {
    let result = (|| {
        let name = ensure_generic_class(class_name, None, "", context, &mut ElephcEvalScope::new(), values)?;
        eval_static_property_set_result(&name, property_name, value, context, values)
    })();
    match result {
        Ok(()) => Ok(None),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .map(Some)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Tests an object relation against eval dynamic-object metadata before AOT metadata.
pub fn execute_context_object_is_a(
    context: &mut ElephcEvalContext,
    object: RuntimeCellHandle,
    target_class: &str,
    exclude_self: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Ok(false);
    }
    let target_class = target_class.trim_start_matches('\\');
    let resolved_target_class = context
        .resolve_class_like_name(target_class)
        .unwrap_or_else(|| target_class.to_string());
    dynamic_object_is_a(
        object,
        &resolved_target_class,
        exclude_self,
        context,
        values,
    )?
    .map_or_else(
        || values.object_is_a(object, &resolved_target_class, exclude_self),
        Ok,
    )
}

/// Tests an object relation when the target is a runtime string or object cell.
pub fn execute_context_object_is_a_dynamic(
    context: &mut ElephcEvalContext,
    object: RuntimeCellHandle,
    target: RuntimeCellHandle,
    exclude_self: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let target_class = match values.type_tag(target)? {
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(target)?;
            let target = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
            target.trim_start_matches('\\').to_string()
        }
        EVAL_TAG_OBJECT => {
            let identity = values.object_identity(target)?;
            if let Some((_, class)) = context.dynamic_object_declaring_class(identity) {
                class.name().to_string()
            } else {
                let class_name = values.object_class_name(target)?;
                let bytes = values.string_bytes(class_name);
                values.release(class_name)?;
                let class_name = String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
                class_name.trim_start_matches('\\').to_string()
            }
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    execute_context_object_is_a(context, object, &target_class, exclude_self, values)
}

/// Tests whether a method or property exists through eval dynamic metadata.
pub fn execute_context_member_exists(
    context: &mut ElephcEvalContext,
    name: &str,
    target: RuntimeCellHandle,
    member: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let result = eval_member_exists_result(name, &[target, member], context, values)?;
    let exists = values.truthy(result)?;
    values.release(result)?;
    Ok(exists)
}
