//! Purpose:
//! Eval registry entry and implementation for `is_a`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Eval-created classes are checked first, then generated/AOT object and
//!   interface metadata fills inherited relationships.

eval_builtin! {
    contract: "is_a",
    area: Symbols,
    direct: Symbols,
    values: Symbols,
}

use crate::interpreter::builtins::spl::array_iterator::{
    eval_array_iterator_is_a, eval_object_is_array_iterator,
};
use super::super::super::*;

/// Evaluates direct `is_a(...)` calls over eval boxed object cells and class strings.
pub(in crate::interpreter) fn eval_is_a_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_is_a_relation("is_a", args, context, scope, values)
}

/// Evaluates materialized `is_a(...)` arguments.
pub(in crate::interpreter) fn eval_is_a_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_is_a_relation_result("is_a", evaluated_args, context, values)
}

/// Evaluates `is_a(...)` and `is_subclass_of(...)` over eval boxed object cells.
pub(in crate::interpreter) fn eval_builtin_is_a_relation(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_is_a_relation_result(name, &evaluated_args, context, values)
}

/// Evaluates materialized `is_a(...)` or `is_subclass_of(...)` builtin arguments.
pub(in crate::interpreter) fn eval_is_a_relation_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (object_or_class, target_class, allow_string) = match evaluated_args {
        [object_or_class, target_class] => {
            (*object_or_class, *target_class, name == "is_subclass_of")
        }
        [object_or_class, target_class, allow_string] => (
            *object_or_class,
            *target_class,
            values.truthy(*allow_string)?,
        ),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let target_class = values.string_bytes(target_class)?;
    let target_class = String::from_utf8(target_class).map_err(|_| EvalStatus::RuntimeFatal)?;
    let target_class = target_class.trim_start_matches('\\');
    let resolved_target_class = context
        .resolve_class_like_name(target_class)
        .unwrap_or_else(|| target_class.to_string());
    let is_object = values.type_tag(object_or_class)? == 6;
    let exclude_self = name == "is_subclass_of";
    let result = if is_object {
        dynamic_object_is_a(
            object_or_class,
            &resolved_target_class,
            exclude_self,
            context,
            values,
        )?
        .map_or_else(
            || values.object_is_a(object_or_class, &resolved_target_class, exclude_self),
            Ok,
        )?
    } else if allow_string && values.type_tag(object_or_class)? == EVAL_TAG_STRING {
        let source_class = values.string_bytes(object_or_class)?;
        let source_class = String::from_utf8(source_class).map_err(|_| EvalStatus::RuntimeFatal)?;
        let resolved_source_class = context
            .resolve_class_like_name(&source_class)
            .unwrap_or_else(|| source_class.trim_start_matches('\\').to_string());
        // `is_a($name, $target, true)` LOADS the class it names, as php's `zend_lookup_class`
        // does. Answering from what happened to be declared already made
        // `getAttributes(Route::class, IS_INSTANCEOF)` drop every attribute whose class only the
        // autoloader could supply -- symfony/routing's own `#[Route]`, in the interpreter.
        if context.class(&resolved_source_class).is_none()
            && context.interface(&resolved_source_class).is_none()
            && context.trait_decl(&resolved_source_class).is_none()
            && !values.class_exists(&resolved_source_class)?
            && !values.interface_exists(&resolved_source_class)?
        {
            let _ = crate::interpreter::eval_spl_autoload_class(
                &resolved_source_class,
                context,
                values,
            )?;
            #[cfg(not(test))]
            context.sync_global_eval_classes();
        }
        if context.class(&resolved_source_class).is_some() {
            eval_class_string_is_a(
                &resolved_source_class,
                &resolved_target_class,
                exclude_self,
                context,
                values,
            )?
        } else if context.interface(&resolved_source_class).is_some() {
            eval_interface_string_is_a(
                &resolved_source_class,
                &resolved_target_class,
                exclude_self,
                context,
                values,
            )?
        } else if context.trait_decl(&resolved_source_class).is_some() {
            !exclude_self
                && eval_class_like_name_matches(&resolved_source_class, &resolved_target_class)
        } else if values.interface_exists(&resolved_source_class)? {
            // A COMPILED interface: the runtime relation helper walks class parents and
            // implemented interfaces only, so `is_a(ContainerInterface::class,
            // PsrContainerInterface::class, true)` answered false and `lint:container` rejected
            // every service taking the PSR container. Its inherited interfaces are the same
            // runtime metadata `class_implements()` answers from.
            (!exclude_self
                && eval_class_like_name_matches(&resolved_source_class, &resolved_target_class))
                || eval_runtime_class_interface_names(&resolved_source_class, values)?
                    .iter()
                    .any(|parent| eval_class_like_name_matches(parent, &resolved_target_class))
        } else {
            values.object_is_a(object_or_class, &resolved_target_class, exclude_self)?
        }
    } else if allow_string {
        values.object_is_a(object_or_class, &resolved_target_class, exclude_self)?
    } else {
        false
    };
    values.bool_value(result)
}

/// Returns whether an interface string source satisfies a class-like target.
fn eval_interface_string_is_a(
    source_class: &str,
    target_class: &str,
    exclude_self: bool,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if !exclude_self && eval_class_like_name_matches(source_class, target_class) {
        return Ok(true);
    }
    Ok(eval_interface_runtime_parent_names(source_class, context, values)?
        .iter()
        .any(|parent| eval_class_like_name_matches(parent, target_class)))
}

/// Returns whether an eval class-string source satisfies a class-like target.
fn eval_class_string_is_a(
    source_class: &str,
    target_class: &str,
    exclude_self: bool,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.class_is_a(source_class, target_class, exclude_self) {
        return Ok(true);
    }
    Ok(eval_class_runtime_interface_names(source_class, context, values)?
        .iter()
        .any(|interface| eval_class_like_name_matches(interface, target_class)))
}

/// Returns eval class interfaces plus generated/AOT inherited interface names.
fn eval_class_runtime_interface_names(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(parent) = context.class_native_parent_name(class_name) {
        for name in eval_runtime_class_interface_names(&parent, values)? {
            eval_push_unique_class_name(name, &mut names, &mut seen);
        }
    }
    for name in context.class_interface_names(class_name) {
        eval_push_unique_class_name(name.clone(), &mut names, &mut seen);
        if !context.has_interface(&name) && eval_runtime_interface_exists(&name, values)? {
            for parent in eval_runtime_class_interface_names(&name, values)? {
                eval_push_unique_class_name(parent, &mut names, &mut seen);
            }
        }
    }
    Ok(names)
}

/// Returns eval interface parents plus generated/AOT inherited interface names.
fn eval_interface_runtime_parent_names(
    interface_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in context.interface_parent_names(interface_name) {
        eval_push_unique_class_name(name.clone(), &mut names, &mut seen);
        if !context.has_interface(&name) && eval_runtime_interface_exists(&name, values)? {
            for parent in eval_runtime_class_interface_names(&name, values)? {
                eval_push_unique_class_name(parent, &mut names, &mut seen);
            }
        }
    }
    Ok(names)
}

/// Returns generated/AOT interface names visible for one class-like symbol.
fn eval_runtime_class_interface_names(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let names_array = values.reflection_class_interface_names(class_name)?;
    let names = eval_string_array_to_vec(names_array, values);
    let released = values.release(names_array);
    let names = names?;
    released?;
    Ok(names)
}

/// Copies a runtime string array into Rust-owned names.
fn eval_string_array_to_vec(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.int(position as i64)?;
        let value = values.array_get(array, key);
        let released_key = values.release(key);
        let value = value?;
        let bytes = values.string_bytes(value);
        let released_value = values.release(value);
        released_key?;
        let bytes = bytes?;
        released_value?;
        result.push(String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?);
    }
    Ok(result)
}

/// Appends one class-like name while preserving PHP's case-insensitive uniqueness.
fn eval_push_unique_class_name(
    name: String,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(name.to_ascii_lowercase()) {
        names.push(name);
    }
}

/// Returns whether two class-like names match PHP's case-insensitive class-name rules.
fn eval_class_like_name_matches(left: &str, right: &str) -> bool {
    left.trim_start_matches('\\')
        .eq_ignore_ascii_case(right.trim_start_matches('\\'))
}

/// Returns whether one class-like NAME satisfies another by PHP's `instanceof` relation --
/// self, subclass, or implemented interface -- with neither side an object instance.
///
/// Mirrors `is_a($name, $target, true)` exactly (verified against php -n: self, subclass, and
/// interface implementation all return `true`; an unrelated or reversed pair returns `false`),
/// which is also PHP's own semantics for `ReflectionAttribute::IS_INSTANCEOF`. Reused by
/// `getAttributes()` filtering (`crate::interpreter::reflection::member_api`) where only the two
/// class NAMES are known, not object instances.
pub(in crate::interpreter) fn eval_class_like_name_is_instance_of(
    source_class: &str,
    target_class: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let source = values.string(source_class)?;
    let target = values.string(target_class)?;
    let allow_string = values.bool_value(true)?;
    let result = eval_is_a_relation_result("is_a", &[source, target, allow_string], context, values)?;
    let matches = values.truthy(result)?;
    values.release(source)?;
    values.release(target)?;
    values.release(allow_string)?;
    values.release(result)?;
    Ok(matches)
}

/// Returns whether an eval-created object matches a dynamic class/interface target.
pub(in crate::interpreter) fn dynamic_object_is_a(
    object: RuntimeCellHandle,
    target_class: &str,
    exclude_self: bool,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<bool>, EvalStatus> {
    let identity = values.object_identity(object)?;
    if context.dynamic_object_is_class(identity, "Closure") {
        return Ok(Some(
            !exclude_self && eval_class_like_name_matches("Closure", target_class),
        ));
    }
    if eval_object_is_array_iterator(identity, context) {
        // `ArrayIterator` has no eval declaration to walk, so its interfaces are answered here.
        // Without this `foreach` never reaches the Iterator protocol and `count()` never reaches
        // `Countable::count()`, whatever the members do.
        return Ok(Some(
            !exclude_self && eval_array_iterator_is_a(target_class),
        ));
    }
    let Some((declaring, class)) = context.dynamic_object_declaring_class(identity) else {
        return Ok(None);
    };
    if eval_class_string_is_a(class.name(), target_class, exclude_self, declaring, values)? {
        return Ok(Some(true));
    }
    // An eval-declared object is carried by a `stdClass` runtime object, so the AOT metadata
    // fallback below would answer "yes" to `$obj instanceof stdClass` for EVERY class the
    // interpreter declared. `Router::toResponse()` tests `$response instanceof stdClass` and then
    // JSON-encoded a Blade `View` as `{}`. Only an explicit `extends stdClass` (answered above)
    // makes such an object a `stdClass`.
    if eval_class_like_name_matches("stdClass", target_class)
        && context.class_native_parent_name(class.name()).is_none()
    {
        return Ok(Some(false));
    }
    // A "no" from the eval tables is not proof. They hold what THIS request declared, and a
    // compiled ancestor or a compiled interface can be absent from them entirely -- the AOT
    // metadata is the other half of the world, and it is what a compiled class id resolves
    // through. Asking it before answering no is what stops a true `instanceof` from reading as
    // false: `Twig\Environment::addExtension(ExtensionInterface $extension)` threw a TypeError on
    // a `TranslationExtension` whose only declared ancestry left the eval table at
    // `AbstractExtension`, and the Symfony container swallowed it into a null `twig` service.
    //
    // The two can only disagree in one direction: a subclass inherits every interface its
    // ancestors declare, so the bridge answering true over an eval "no" is the correct answer,
    // never a looser one.
    values
        .object_is_a(object, target_class, exclude_self)
        .map(Some)
}

/// Answers `is_a($source, $target, true)` / `is_subclass_of()` for two class-name strings on
/// behalf of compiled code.
///
/// Compiled code settles a class-name relation from its own class table first and asks here only
/// when that table said no, which is exactly the case of a class the interpreter declared (an
/// autoloaded file the compiler never saw). It takes the path an interpreted `is_a()` takes,
/// autoload included.
pub(crate) fn eval_class_name_is_a_bridge(
    source_class: &str,
    target_class: &str,
    exclude_self: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let source = values.string(source_class)?;
    let target = match values.string(target_class) {
        Ok(target) => target,
        Err(status) => {
            let _ = values.release(source);
            return Err(status);
        }
    };
    let allow_string = match values.bool_value(true) {
        Ok(flag) => flag,
        Err(status) => {
            let _ = values.release(source);
            let _ = values.release(target);
            return Err(status);
        }
    };
    let name = if exclude_self { "is_subclass_of" } else { "is_a" };
    let result = eval_is_a_relation_result(name, &[source, target, allow_string], context, values)
        .and_then(|cell| {
            let truthy = values.truthy(cell);
            let released = values.release(cell);
            let truthy = truthy?;
            released?;
            Ok(truthy)
        });
    let _ = values.release(source);
    let _ = values.release(target);
    let _ = values.release(allow_string);
    result
}
