//! Purpose:
//! Reads, writes, binds, tests, and unsets eval instance properties.
//!
//! Called from:
//! - Expression and statement dispatch for object property operations.
//!
//! Key details:
//! - Declared, dynamic, magic, hooked, and reference-backed properties share visibility checks.

use super::*;

/// Stores one value in the dynamic-property overlay, which OWNS what it keeps.
///
/// This is the ONLY way the overlay is written, because the ownership rule is not optional and
/// was not being applied consistently: three of the six former write points retained the value
/// and three did not, while object destruction releases everything the overlay holds — the FFI
/// destructor hands `forget_dynamic_object`'s values straight to `release`. A value stored
/// without a reference was therefore given back twice, and the site that got it right looked
/// exactly like the site that did not. Keeping the rule in one function is what stops the two
/// drifting apart again.
///
/// Re-storing the cell the overlay already holds is a no-op rather than a retain, so a repeated
/// write does not accumulate references.
pub(in crate::interpreter) fn eval_store_dynamic_property_value(
    identity: u64,
    storage_property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if context.dynamic_property_value(identity, storage_property_name) == Some(value) {
        return Ok(());
    }
    let stored = values.retain(value)?;
    if let Some(replaced) =
        context.set_dynamic_property_value(identity, storage_property_name, stored)
    {
        eval_release_value(context, values, replaced)?;
    }
    Ok(())
}

/// Reads one object property as the target of a REFERENCE bind (`&$o->p`).
///
/// PHP has a third answer here, distinct from both the ordinary read and the quiet fetch.
/// Measured against `php -n` 8.5.6 on an uninitialized typed property:
///
/// - non-nullable (`array $p`) raises `Error: Cannot access uninitialized non-nullable property
///   C::$p by reference` — a DIFFERENT sentence from the read's "must not be accessed before
///   initialization", naming the reference and the non-nullability;
/// - nullable (`?array $p`) does NOT raise: the reference initializes the property to null;
/// - untyped (`$p`) does not raise either, and neither does an initialized property.
///
/// Answering the read's message here was wrong twice over: the wrong sentence for the
/// non-nullable case, and a raise at all for the nullable one.
pub(in crate::interpreter) fn eval_property_reference_target_get_result(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_property_reference_bind_precheck(object, property_name, context, values)?;
    eval_property_get_result(object, property_name, context, values)
}

/// Applies PHP's by-reference rule to an uninitialized typed property before the bind reads it.
///
/// Separate from the read because a reference bind resolves its TARGET and its value through
/// different machinery; both entry points need the same rule, and only one of them wants the
/// value back. See `eval_property_reference_target_get_result` for the measured behaviour.
pub(in crate::interpreter) fn eval_property_reference_bind_precheck(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Ok(identity) = values.object_identity(object) {
        if let Some((declaring_class, property)) = context
            .dynamic_object_class(identity)
            .map(|class| class.name().to_string())
            .and_then(|class_name| {
                eval_dynamic_property_for_access(&class_name, property_name, context)
            })
        {
            let storage_property_name =
                eval_instance_property_storage_name(&declaring_class, &property);
            if let Some(declared) = property.property_type() {
                if !context.dynamic_property_is_initialized(identity, &storage_property_name) {
                    if !declared.allows_null() {
                        return eval_throw_error(
                            &format!(
                                "Cannot access uninitialized non-nullable property {}::${} by reference",
                                declaring_class.trim_start_matches('\\'),
                                property.name(),
                            ),
                            context,
                            values,
                        );
                    }
                    // A nullable typed property taken by reference is initialized to null.
                    let null_value = values.null()?;
                    eval_property_set_result(
                        object,
                        property_name,
                        null_value,
                        context,
                        values,
                    )?;
                }
            }
        }
    }
    Ok(())
}

/// Reads one object property as the TARGET of an array write, auto-initializing it like PHP.
///
/// `$o->p['k'] = 1` on an uninitialized typed property does not raise in PHP: the property is
/// auto-initialized to an empty array, exactly as an undefined variable would be. Measured
/// against `php -n` 8.5.6, that holds for `array`, `?array` and an untyped property, it nests
/// (`$o->p['k']['j'] = 1`), and it covers the append form `$o->p[] = 1`.
///
/// It does NOT hold for every type. A property whose declared type cannot contain an array
/// raises `TypeError: Cannot auto-initialize an array inside property C::$p of type int` — a
/// different class and a different sentence from the uninitialized-read `Error`, so the two
/// cannot share one message.
///
/// The read runs in quiet-fetch mode and the callers' existing "not array-like, so make a new
/// array" fallback performs the auto-initialization. This function only decides whether PHP
/// would ALLOW it, which needs the DECLARED type and so cannot live at the call sites.
pub(in crate::interpreter) fn eval_property_array_target_get_result(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Ok(identity) = values.object_identity(object) {
        if let Some((declaring_class, property)) = context
            .dynamic_object_class(identity)
            .map(|class| class.name().to_string())
            .and_then(|class_name| {
                eval_dynamic_property_for_access(&class_name, property_name, context)
            })
        {
            let storage_property_name =
                eval_instance_property_storage_name(&declaring_class, &property);
            let uninitialized = property.property_type().is_some()
                && !context.dynamic_property_is_initialized(identity, &storage_property_name);
            if uninitialized {
                if let Some(declared) = property.property_type() {
                    if !eval_property_type_admits_array(declared) {
                        return eval_throw_type_error(
                            &format!(
                                "Cannot auto-initialize an array inside property {}::${} of type {}",
                                declaring_class.trim_start_matches('\\'),
                                property.name(),
                                eval_property_type_name(declared),
                            ),
                            context,
                            values,
                        );
                    }
                }
            }
        }
    }
    context.push_quiet_property_fetch();
    let result = eval_property_get_result(object, property_name, context, values);
    context.pop_quiet_property_fetch();
    result
}

/// Reports whether a declared property type can hold an auto-initialized array.
///
/// `mixed` and `iterable` admit one, as does an explicit `array`; a union admits one when any
/// atom does. Everything else is the `TypeError` case.
fn eval_property_type_admits_array(declared: &EvalParameterType) -> bool {
    declared.variants().iter().any(|variant| {
        matches!(
            variant,
            EvalParameterTypeVariant::Array
                | EvalParameterTypeVariant::Mixed
                | EvalParameterTypeVariant::Iterable
        )
    })
}

/// Renders a declared property type the way PHP names it in the auto-initialize `TypeError`.
fn eval_property_type_name(declared: &EvalParameterType) -> String {
    let mut rendered = declared
        .variants()
        .iter()
        .map(|variant| match variant {
            EvalParameterTypeVariant::Generic(ty) => ty.to_string(),
            EvalParameterTypeVariant::Array => "array".to_string(),
            EvalParameterTypeVariant::Bool => "bool".to_string(),
            EvalParameterTypeVariant::Callable => "callable".to_string(),
            EvalParameterTypeVariant::Class(name) => name.trim_start_matches('\\').to_string(),
            EvalParameterTypeVariant::False => "false".to_string(),
            EvalParameterTypeVariant::Float => "float".to_string(),
            EvalParameterTypeVariant::Int => "int".to_string(),
            EvalParameterTypeVariant::Iterable => "iterable".to_string(),
            EvalParameterTypeVariant::Mixed => "mixed".to_string(),
            EvalParameterTypeVariant::Never => "never".to_string(),
            EvalParameterTypeVariant::Object => "object".to_string(),
            EvalParameterTypeVariant::String => "string".to_string(),
            EvalParameterTypeVariant::True => "true".to_string(),
            EvalParameterTypeVariant::Void => "void".to_string(),
        })
        .collect::<Vec<_>>()
        .join(if declared.is_intersection() { "&" } else { "|" });
    if declared.allows_null() {
        if rendered.is_empty() {
            rendered = "null".to_string();
        } else if declared.variants().len() == 1 && !declared.is_intersection() {
            rendered = format!("?{}", rendered);
        } else {
            rendered = format!("{}|null", rendered);
        }
    }
    rendered
}

/// Answers ABSENT for a not-yet-initialized property while PHP's quiet fetch is on.
///
/// `isset`/`empty`/`??`/`??=` -- and the CONTAINER of an indexed write or an `unset` -- fetch
/// quietly: measured with `php -n` 8.5.6 against `private array $p;` left unassigned,
/// `isset($o->p)`, `empty($o->p)`, `$o->p ?? 'D'`, `$o->p['k'] ?? 'D'`, `$o->p['k'] = 1`,
/// `$o->p[] = 1`, `$o->p['k'] ??= 7` and `unset($o->p['k'])` all answer without raising, while
/// the ordinary read, `count()`, `foreach`, and a read-modify-write (`++`, `.=`, `+=`) all raise
/// `Error: Typed property C::$p must not be accessed before initialization`.
///
/// The eval-DECLARED branch of `eval_property_get_result` applies that rule from its own property
/// record. An AOT-backed object has no such record -- `dynamic_object_class` is None for a purely
/// compiled class, and `eval_dynamic_class_native_property_metadata` only covers an eval class
/// with a native parent -- so all three of those paths fell through to a raw `values.property_get`
/// that raises. The initialization marker is therefore asked through the bridge, the same probe
/// `eval_property_isset` already uses.
///
/// Gated on the mode being ON, so an ordinary read still raises. Under the mode, answering null
/// is right for an undefined property too, which is what PHP does there as well.
fn eval_quiet_uninitialized_property_null(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !context.quiet_property_fetch() {
        return Ok(None);
    }
    if values.property_is_initialized(object, property_name)? {
        return Ok(None);
    }
    values.null().map(Some)
}

/// Reads one object property while enforcing eval-declared member visibility.
pub(in crate::interpreter) fn eval_property_get_result(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_property_get_result_with_ownership(object, property_name, context, values, None)
}

/// Reads a property while optionally retaining reference-backed values for a native caller.
pub(in crate::interpreter) fn eval_property_get_result_with_ownership(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    owned: Option<&mut Vec<RuntimeCellHandle>>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return values.property_get(object, property_name);
    };
    // Any property access is what php initializes a lazy ghost on.
    eval_initialize_lazy_ghost(identity, object, context, values)?;
    context.import_foreign_object_class(identity);
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = eval_runtime_object_class_name(object, values)?;
        if let Some(storage_property_name) =
            eval_reflection_public_property_storage_name(&class_name, property_name)
        {
            return eval_reflection_with_declaring_class_scope(&class_name, context, |_| {
                values.property_get(object, storage_property_name)
            });
        }
        let aot_metadata =
            eval_reflection_aot_property_access_metadata(&class_name, property_name, values)?;
        let aot_declared = aot_metadata.is_some();
        if let Some((declaring_class, visibility, _, is_static)) = aot_metadata {
            if !is_static {
                if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        visibility,
                        context,
                        values,
                    );
                }
                if visibility != EvalVisibility::Public {
                    return eval_reflection_with_declaring_class_scope(
                        &declaring_class,
                        context,
                        |context| {
                            if let Some(null) = eval_quiet_uninitialized_property_null(
                                object,
                                property_name,
                                context,
                                values,
                            )? {
                                return Ok(null);
                            }
                            values.property_get(object, property_name)
                        },
                    );
                }
            }
            if !is_static {
                return eval_native_property_get_authorized(
                    object, &declaring_class, property_name, context, values,
                );
            }
        }
        // A DYNAMIC property on a compiled object lives in the interpreter's overlay, not in the
        // object's frozen layout, so it has to be consulted before the bridge is asked for a slot
        // the class never declared.
        if let Some(value) = context.dynamic_property_value(identity, property_name) {
            return values.retain(value);
        }
        // A purely AOT class has no eval record at all, so this is where the Symfony stop landed:
        // `CheckCircularReferencesPass::$checkedLazyNodes`, declared `private array` and never
        // assigned, read through the bridge by compiled `empty()`/indexed-write/`unset`. Only a
        // DECLARED slot can be uninitialized: a name the class never declared is not a quiet
        // null but a miss `__isset()`/`__get()` answer, which `isset($app->config['view'])` on
        // Laravel's container needs.
        if aot_declared {
            if let Some(null) =
                eval_quiet_uninitialized_property_null(object, property_name, context, values)?
            {
                return Ok(null);
            }
        }
        // A name the compiled class neither declares nor holds as a dynamic property is what
        // `__get()` answers, before the bridge reads it as an undeclared null.
        if !aot_declared
            && eval_aot_method_dispatch_metadata_in_hierarchy(&class_name, "__get", context, values)?
                .is_some()
            && !eval_object_public_property_exists(object, property_name, values)?
        {
            if let Some(value) =
                eval_aot_magic_property_get(object, &class_name, property_name, context, values)?
            {
                return Ok(value);
            }
        }
        // The bridge only knows the slots the class DECLARED. A name it refuses is an undeclared
        // property, which PHP reads as null; failing here turned
        // `$container->getService ??= …` in a generated Symfony container fragment into
        // `unsupported NullCoalesceAssign expression`, and with the routing loader dead every
        // route 404ed while the error page stayed byte-perfect.
        return match values.property_get(object, property_name) {
            Ok(value) => Ok(value),
            Err(_) => match eval_aot_magic_property_get(
                object,
                &class_name,
                property_name,
                context,
                values,
            )? {
                Some(value) => Ok(value),
                None => values.null(),
            },
        };
    };
    let object_class_name = class.name().to_string();
    let mut storage_property_name = property_name.to_string();
    let mut declared_property_found = false;
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        declared_property_found = true;
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_err() {
            if let Some(result) =
                eval_magic_property_get(object, &object_class_name, property_name, context, values)?
            {
                return Ok(result);
            }
            // `isset`/`empty`/`??` answer "absent" for a property the caller cannot see, they
            // never raise. A compiled `isset($object->$name)` on an interpreted object reaches
            // here in quiet mode: Twig's `CoreExtension::getAttribute()` asks it first for every
            // `post.title`, and the Error it raised left every entity attribute empty.
            if context.quiet_property_fetch() {
                return values.null();
            }
            return eval_throw_property_access_error(
                &declaring_class,
                property.name(),
                property.visibility(),
                context,
                values,
            );
        }
        storage_property_name = eval_instance_property_storage_name(&declaring_class, &property);
        if property.has_get_hook()
            && !current_eval_property_hook_is(
                &declaring_class,
                property.name(),
                &property_hook_get_method(property.name()),
                context,
            )
        {
            let (hook_class, hook_method) = context
                .class_method(
                    &declaring_class,
                    &property_hook_get_method(property.name()),
                )
                .ok_or(EvalStatus::RuntimeFatal)?;
            return eval_dynamic_method_with_values(
                &hook_class,
                &object_class_name,
                &hook_method,
                object,
                Vec::new(),
                context,
                values,
            );
        }
        if property.property_type().is_some()
            && !context.dynamic_property_is_initialized(identity, &storage_property_name)
        {
            // `isset`/`empty`/`??`/`??=` fetch QUIETLY: PHP answers "absent" for an
            // uninitialized typed property instead of raising, and the mode reaches down a
            // whole property/dim chain. Only the ordinary read raises.
            if context.quiet_property_fetch() {
                return values.null();
            }
            return eval_throw_uninitialized_property_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
    }
    // An undeclared property written as `$object->name = ...` lands in the context overlay, not
    // in the object's slots, so the slot probe below cannot see it. Asking the overlay first
    // keeps this early return from answering null for a property that plainly has a value.
    if !declared_property_found {
        if let Some(stored) = context.dynamic_property_value(identity, property_name) {
            return values.retain(stored);
        }
    }
    if !declared_property_found
        && eval_object_public_property_exists(object, property_name, values)?
    {
        return values.property_get(object, property_name);
    }
    if !declared_property_found {
        if let Some((declaring_class, visibility, _, is_static)) =
            eval_dynamic_class_native_property_metadata(
                &object_class_name,
                property_name,
                context,
                values,
            )?
        {
            if !is_static {
                if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                    if let Some(result) = eval_magic_property_get(
                        object,
                        &object_class_name,
                        property_name,
                        context,
                        values,
                    )? {
                        return Ok(result);
                    }
                    if context.quiet_property_fetch() {
                        return values.null();
                    }
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        visibility,
                        context,
                        values,
                    );
                }
                // Same rule for an eval class inheriting an AOT typed property: the marker is read
                // in the declaring class's bridge scope, as `eval_property_isset` does.
                if context.quiet_property_fetch()
                    && !eval_with_native_bridge_scope(&declaring_class, context, || {
                        values.property_is_initialized(object, property_name)
                    })?
                {
                    return values.null();
                }
                return eval_native_property_get_authorized(
                    object, &declaring_class, property_name, context, values,
                );
            }
        }
    }
    if !declared_property_found
        && eval_object_public_property_exists(object, property_name, values)?
    {
        return values.property_get(object, property_name);
    }
    if !declared_property_found {
        if let Some(result) =
            eval_magic_property_get(object, &object_class_name, property_name, context, values)?
        {
            return Ok(result);
        }
        // `__get()` may live on a COMPILED ancestor: an eval class extending `Fluent` inherits
        // the magic reader from the native class, and the class record only lists its own
        // methods. Laravel's `ColumnDefinition extends Fluent` answered every `$column->type`
        // with null, so no migration could name a column type.
        if let Some(parent) = context.class_native_parent_name(&object_class_name) {
            if let Some(result) =
                eval_aot_magic_property_get(object, &parent, property_name, context, values)?
            {
                return Ok(result);
            }
        }
    }
    if let Some(target) = context
        .dynamic_property_alias(identity, &storage_property_name)
        .cloned()
    {
        return match owned {
                Some(owners) => eval_owned_reference_target_value(&target, context, values, owners),
                None => eval_reference_target_value(&target, context, values),
            };
    }
    let overlay_value = context.dynamic_property_value(identity, &storage_property_name);
    if crate::eval_trace::enabled() {
        let call_site = context.call_site();
        eprintln!(
            "[elephc-eval-trace] phase=dynamic_property_lookup class={object_class_name:?} identity={identity} property={storage_property_name:?} declared={declared_property_found} overlay_found={} file={:?} line={}",
            overlay_value.is_some(),
            call_site.0,
            call_site.2,
        );
    }
    if let Some(value) = overlay_value {
        if crate::eval_trace::enabled() {
            let call_site = context.call_site();
            let tag = values.type_tag(value);
            // A short preview of string values: comparing two reads is otherwise blind.
            let preview = match tag {
                Ok(EVAL_TAG_STRING) => values
                    .string_bytes(value)
                    .map(|bytes| {
                        String::from_utf8_lossy(&bytes[..bytes.len().min(48)]).into_owned()
                    })
                    .ok(),
                _ => None,
            };
            eprintln!(
                "[elephc-eval-trace] phase=dynamic_property_get class={object_class_name:?} property={storage_property_name:?} value_tag={:?} preview={preview:?} file={:?} line={}",
                tag,
                call_site.0,
                call_site.2,
            );
        }
        return values.retain(value);
    }
    if let Some(null) =
        eval_quiet_uninitialized_property_null(object, &storage_property_name, context, values)?
    {
        return Ok(null);
    }
    eval_with_native_property_storage_scope(
        &object_class_name, &storage_property_name, context, values,
        |values| values.property_get(object, &storage_property_name),
    )
}

/// Maps native Reflection owner public read-only fields onto their private metadata slots.
///
/// Eval materializes Reflection owners through a generated object helper, which stores metadata
/// in private slots. PHP exposes a small public read-only view of those slots; mapping the reads
/// here keeps eval-owned owners consistent with AOT reflection lowering without permitting
/// ordinary dynamic-property creation.
fn eval_reflection_public_property_storage_name(
    class_name: &str,
    property_name: &str,
) -> Option<&'static str> {
    let class_name = class_name.trim_start_matches('\\');
    match property_name {
        "name"
            if [
                "ReflectionAttribute",
                "ReflectionClass",
                "ReflectionObject",
                "ReflectionEnum",
                "ReflectionFunction",
                "ReflectionMethod",
                "ReflectionProperty",
                "ReflectionParameter",
                "ReflectionClassConstant",
                "ReflectionEnumUnitCase",
                "ReflectionEnumBackedCase",
            ]
            .iter()
            .any(|owner| class_name.eq_ignore_ascii_case(owner)) =>
        {
            Some("__name")
        }
        "class"
            if [
                "ReflectionMethod",
                "ReflectionProperty",
                "ReflectionClassConstant",
                "ReflectionEnumUnitCase",
                "ReflectionEnumBackedCase",
            ]
            .iter()
            .any(|owner| class_name.eq_ignore_ascii_case(owner)) =>
        {
            Some("__class")
        }
        _ => None,
    }
}

/// Writes one object property while enforcing eval-declared member visibility.
pub(crate) fn eval_property_set_result(
    object: RuntimeCellHandle,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return values.property_set(object, property_name, value);
    };
    // Any property access is what php initializes a lazy ghost on.
    eval_initialize_lazy_ghost(identity, object, context, values)?;
    context.import_foreign_object_class(identity);
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = eval_runtime_object_class_name(object, values)?;
        if eval_reflection_public_property_storage_name(&class_name, property_name).is_some() {
            return eval_throw_reflection_exception(
                &format!(
                    "Cannot set read-only property {}::${}",
                    class_name.trim_start_matches('\\'),
                    property_name
                ),
                context,
                values,
            )
            .map(|_| ());
        }
        if let Some((declaring_class, visibility, write_visibility, is_static)) =
            eval_native_instance_property_metadata_for_access(
                &class_name,
                &class_name,
                property_name,
                context,
                values,
            )?
        {
            if !is_static && validate_eval_member_access(&declaring_class, visibility, context).is_ok() {
                validate_native_readonly_property_access(
                    object, identity, &declaring_class, property_name, write_visibility, context, values,
                )?;
            }
            if !is_static
                && validate_eval_member_access(&declaring_class, write_visibility, context).is_err()
            {
                if eval_object_public_property_exists(object, property_name, values)? {
                    return eval_write_public_dynamic_property(
                        object,
                        identity,
                        &class_name,
                        property_name,
                        value,
                        context,
                        values,
                    );
                }
                if eval_native_magic_property_set(
                    object,
                    &class_name,
                    property_name,
                    value,
                    context,
                    values,
                )? {
                    return Ok(());
                }
                return eval_throw_property_access_error(
                    &declaring_class,
                    property_name,
                    write_visibility,
                    context,
                    values,
                );
            }
            if !is_static {
                validate_native_readonly_property_write(
                    object,
                    identity,
                    &declaring_class,
                    property_name,
                    context,
                    values,
                )?;
                return eval_native_property_set_authorized(
                    object, &declaring_class, property_name, value, context, values,
                );
            }
        }
        if eval_object_public_property_exists(object, property_name, values)? {
            return eval_write_public_dynamic_property(
                object,
                identity,
                &class_name,
                property_name,
                value,
                context,
                values,
            );
        }
        if eval_native_magic_property_set(
            object,
            &class_name,
            property_name,
            value,
            context,
            values,
        )? {
            return Ok(());
        }
        if values
            .reflection_class_flags(&class_name)?
            .is_some_and(|flags| flags & EVAL_REFLECTION_CLASS_FLAG_READONLY != 0)
        {
            return eval_throw_dynamic_property_creation_error(
                &class_name,
                property_name,
                context,
                values,
            );
        }
        if !eval_object_public_property_exists(object, property_name, values)?
            && !eval_class_allows_dynamic_properties(&class_name, context)
        {
            eval_dispatch_php_error(
                &format!(
                    "Creation of dynamic property {}::${property_name} is deprecated",
                    class_name.trim_start_matches('\\')
                ),
                E_DEPRECATED,
                context,
                values,
            )?;
        }
        // A name the compiled object has no slot for is a DYNAMIC property: PHP creates it (with a
        // deprecation notice since 8.2) rather than failing, and the interpreter's overlay is where
        // it lives. Only reached when the bridge refuses, so a declared slot still takes the
        // ordinary path.
        return match values.property_set(object, property_name, value) {
            Ok(()) => Ok(()),
            Err(_) => eval_store_dynamic_property_value(identity, property_name, value, context, values),
        };
    };
    let object_class_name = class.name().to_string();
    if context.has_enum(&object_class_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let class_is_readonly = class.is_readonly_class();
    let mut storage_property_name = property_name.to_string();
    let mut declared_property_found = false;
    let mut declared_property_is_private = false;
    let mut declared_property_is_public = false;
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        declared_property_found = true;
        declared_property_is_private = property.visibility() == EvalVisibility::Private;
        declared_property_is_public = property.visibility() == EvalVisibility::Public;
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_err() {
            if eval_object_public_property_exists(object, property_name, values)? {
                return eval_write_public_dynamic_property(
                    object,
                    identity,
                    &object_class_name,
                    property_name,
                    value,
                    context,
                    values,
                );
            }
            if eval_magic_property_set(
                object,
                &object_class_name,
                property_name,
                value,
                context,
                values,
            )? {
                return Ok(());
            }
            return eval_throw_property_access_error(
                &declaring_class,
                property.name(),
                property.visibility(),
                context,
                values,
            );
        }
        storage_property_name = eval_instance_property_storage_name(&declaring_class, &property);
        // An initialized readonly value rejects an overwrite before implicit protected(set)
        // access is checked. An authorized clone can still consume its one rewrite later.
        if property.is_readonly()
            && context.dynamic_property_is_initialized(identity, &storage_property_name)
            && !context.clone_property_can_be_reinitialized(identity, &storage_property_name)
        {
            return eval_throw_readonly_property_modification_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
        if validate_eval_property_write_access(&declaring_class, &property, context).is_err() {
            return eval_throw_property_write_access_error(
                &declaring_class,
                &property,
                context,
                values,
            );
        }
        if validate_eval_readonly_property_write(
            &declaring_class,
            &property,
            Some((identity, &storage_property_name)),
            context,
        )
        .is_err()
            && !eval_readonly_property_first_initialization(
                identity,
                &declaring_class,
                &property,
                context,
            )
        {
            return eval_throw_readonly_property_modification_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
        if property.requires_set_hook() && property.is_readonly() && !property.has_set_hook() {
            // The inherited abstract setter is callable but has no body or backing-slot write.
            return eval_inherited_abstract_setter_write(
                &declaring_class, property.name(), value, context, values,
            );
        }
        if property.has_set_hook() {
            if !current_eval_property_hook_is(
                &declaring_class,
                property.name(),
                &property_hook_set_method(property.name()),
                context,
            ) {
                let (hook_class, hook_method) = context
                    .class_method(
                        &declaring_class,
                        &property_hook_set_method(property.name()),
                    )
                    .ok_or(EvalStatus::RuntimeFatal)?;
                let hook_result = eval_dynamic_method_with_values(
                    &hook_class,
                    &object_class_name,
                    &hook_method,
                    object,
                    vec![EvaluatedCallArg {
                        name: None,
                        value: value.borrowed(),
                        ref_target: None,
                    }],
                    context,
                    values,
                )?;
                values.release(hook_result)?;
                return Ok(());
            }
        } else if property.has_get_hook() && property.is_virtual() {
            return eval_throw_property_hook_readonly_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
    }
    if !declared_property_found {
        let native_property = eval_dynamic_class_native_property_metadata(
                &object_class_name,
                property_name,
                context,
                values,
            )
            .map_err(|status| {
                trace_instance_property_error(
                    "native_metadata",
                    &object_class_name,
                    property_name,
                    status,
                    context,
                )
            })?;
        if let Some((declaring_class, visibility, write_visibility, is_static)) = native_property {
            if !is_static {
                if validate_eval_member_access(&declaring_class, visibility, context).is_ok() {
                    validate_native_readonly_property_access(
                        object, identity, &declaring_class, property_name, write_visibility, context, values,
                    )?;
                }
                if validate_eval_member_access(&declaring_class, write_visibility, context)
                    .is_err()
                {
                    if eval_object_public_property_exists(object, property_name, values)? {
                        return eval_write_public_dynamic_property(
                            object,
                            identity,
                            &object_class_name,
                            property_name,
                            value,
                            context,
                            values,
                        );
                    }
                    if eval_magic_property_set(
                        object,
                        &object_class_name,
                        property_name,
                        value,
                        context,
                        values,
                    )? {
                        return Ok(());
                    }
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        write_visibility,
                        context,
                        values,
                    );
                }
                validate_native_readonly_property_write(
                    object,
                    identity,
                    &declaring_class,
                    property_name,
                    context,
                    values,
                )?;
                return eval_native_property_set_authorized(
                    object, &declaring_class, property_name, value, context, values,
                )
                .map_err(|status| {
                    trace_instance_property_error(
                        "native_store",
                        &object_class_name,
                        property_name,
                        status,
                        context,
                    )
                });
            }
        }
    }
    let existing_public_dynamic = !declared_property_found
        && eval_object_public_property_exists(object, property_name, values)?;
    if !declared_property_found
        && !existing_public_dynamic
        && eval_magic_property_set(
            object,
            &object_class_name,
            property_name,
            value,
            context,
            values,
        )?
    {
        return Ok(());
    }
    if !declared_property_found && class_is_readonly {
        return eval_throw_dynamic_property_creation_error(
            &object_class_name,
            property_name,
            context,
            values,
        );
    }
    if !declared_property_found
        && !existing_public_dynamic
        && !eval_class_allows_dynamic_properties(&object_class_name, context)
    {
        eval_dispatch_php_error(
            &format!(
                "Creation of dynamic property {}::${property_name} is deprecated",
                object_class_name.trim_start_matches('\\')
            ),
            E_DEPRECATED,
            context,
            values,
        )?;
    }
    // A declared non-private property that a COMPILED ancestor also declares lives in that
    // ancestor's native slot as far as its compiled methods are concerned: an eval subclass that
    // redeclares `protected array $parameters` and fills it in its constructor must be seen by the
    // parent's `$this->parameters[$name]`. The slot already exists and its own metadata keeps it
    // protected, so this write cannot leak it to an enumerator the way the generic mirror below
    // would.
    if declared_property_found && !declared_property_is_private {
        if let Some((declaring_class, visibility, _, is_static)) =
            eval_dynamic_class_native_property_metadata(
                &object_class_name,
                property_name,
                context,
                values,
            )?
        {
            if !is_static && visibility != EvalVisibility::Private {
                eval_with_native_bridge_scope(&declaring_class, context, || {
                    eval_native_property_store_with_array_shape_fallback(
                        object,
                        property_name,
                        value,
                        values,
                    )
                })?;
            }
        }
    }
    if let Some(target) = context
        .dynamic_property_alias(identity, &storage_property_name)
        .cloned()
    {
        let storage = match &target {
            EvalReferenceTarget::Cell { cell } if values.is_reference(*cell)? => *cell,
            _ => value,
        };
        eval_reference_target_write(
            identity,
            &storage_property_name,
            target,
            value,
            context,
            values,
        )?;
        context.mark_dynamic_property_initialized(identity, &storage_property_name);
        eval_store_dynamic_property_value(
            identity,
            &storage_property_name,
            value,
            context,
            values,
        )?;
        let _ = eval_with_native_property_storage_scope(
            &object_class_name, &storage_property_name, context, values,
            |values| values.property_set(object, &storage_property_name, storage),
        );
        return Ok(());
    }
    // Mirror the value into the object's own slot as well. The overlay is what reads consult,
    // but the enumerators walk the slots, so a property written only to the overlay is
    // invisible to `print_r`, `var_dump`, `json_encode` and the `(array)` cast. Keeping the two
    // in step at the single write point is what lets those keep reading one store. TWO
    // INDEPENDENT CONDITIONS GOVERN THAT MIRROR — whether to attempt it, and what a missing slot
    // means.
    //
    // WHETHER: a DECLARED non-public property is deliberately left out, matching the same rule at
    // construction. `json_encode()` exports public properties by walking the slots, and this
    // interpreter leaves a protected storage name unmangled, so a protected property in the slots
    // would be exported where `php -n` 8.5.6 omits it. An UNDECLARED property is always public in
    // PHP, so it is always mirrored.
    //
    // A MISSING SLOT: the write is BEST-EFFORT and has to be. An eval class that extends an AOT
    // class is backed by an instance of THAT class, whose slots were fixed at compile time, so a
    // property the eval class declares and its native parent does not has no slot to receive it.
    // Symfony's dumped container is exactly that shape — `App_KernelDevDebugContainer extends
    // Container` writing `$this->targetDir` and its promoted `$buildParameters` — and treating the
    // missing slot as a failure turned every warm-container request into `eval() runtime failed`.
    // The overlay below still takes the value and `$object->property` still reads it.
    // `eval_store_dynamic_property_value` is the ONE place the overlay's reference is taken and
    // given back: it releases the cell it displaces, and re-storing the cell already held is a
    // no-op precisely because nothing was displaced. This site kept a release of its own from
    // before that consolidation, which gave the same cell back TWICE -- and in the no-op case
    // gave back the overlay's own LIVE reference. An append into a nested property array is
    // exactly that case, because the array is mutated in place and the write re-stores the same
    // handle: the value then survived only on the mirror slot, and the next write to that slot
    // freed it, so the whole property read back empty. `replaced` stays for the trace line only.
    let replaced = context.dynamic_property_value(identity, &storage_property_name);
    eval_store_dynamic_property_value(identity, &storage_property_name, value, context, values)?;
    if crate::eval_trace::enabled() {
        let call_site = context.call_site();
        eprintln!(
            "[elephc-eval-trace] phase=dynamic_property_set class={object_class_name:?} property={storage_property_name:?} value_tag={:?} replaced={} file={:?} line={}",
            values.type_tag(value),
            replaced.is_some(),
            call_site.0,
            call_site.2,
        );
    }
    // Mirror the value into the object's own slot as well, best effort: the overlay above is what
    // reads consult, but the enumerators (`print_r`, `var_dump`, `json_encode`, the `(array)` cast)
    // walk the slots. A DECLARED non-public property is deliberately left out, so `json_encode()`
    // does not export it; and a missing slot (an eval class extending an AOT class has only the
    // parent's) is not a failure -- the overlay still takes the value.
    if !declared_property_found || declared_property_is_public {
        let _ = eval_with_native_property_storage_scope(
            &object_class_name, &storage_property_name, context, values,
            |values| values.property_set(object, &storage_property_name, value),
        );
    }
    context.mark_dynamic_property_initialized(identity, &storage_property_name);
    Ok(())
}

/// Retries one native property write with the alternate PHP array storage shape.
fn eval_native_property_store_with_array_shape_fallback(
    object: RuntimeCellHandle,
    property_name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let first_status = match values.property_set(object, property_name, value) {
        Ok(()) => return Ok(()),
        Err(status) => status,
    };
    let source_tag = values.type_tag(value)?;
    if !matches!(source_tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(first_status);
    }
    let len = values.array_len(value)?;
    let mut converted = if source_tag == EVAL_TAG_ARRAY {
        values.assoc_new(len)?
    } else {
        values.array_new(len)?
    };
    for position in 0..len {
        let key = values.array_iter_key(value, position)?;
        let element = values.array_get(value, key)?;
        converted = values.array_set(converted, key, element)?;
    }
    let result = values.property_set(object, property_name, converted);
    values.release(converted)?;
    result
}

/// Emits the instance-property substage that failed under opt-in runtime tracing.
fn trace_instance_property_error(
    stage: &str,
    class_name: &str,
    property_name: &str,
    status: EvalStatus,
    context: &ElephcEvalContext,
) -> EvalStatus {
    if crate::eval_trace::enabled() {
        let call_site = context.call_site();
        eprintln!(
            "[elephc-eval-trace] phase=instance_property_error stage={stage} class={class_name:?} property={property_name:?} status={status:?} file={:?} line={}",
            call_site.0,
            call_site.2,
        );
    }
    status
}

/// Returns whether an eval or seeded native class inherits permission for dynamic properties.
fn eval_class_allows_dynamic_properties(class_name: &str, context: &ElephcEvalContext) -> bool {
    let mut current = class_name.trim_start_matches('\\').to_string();
    let mut visited = std::collections::HashSet::new();
    while visited.insert(current.to_ascii_lowercase()) {
        if current.eq_ignore_ascii_case("stdClass") {
            return true;
        }
        if let Some(class) = context.class(&current) {
            if eval_class_has_allow_dynamic_properties_attribute(class) {
                return true;
            }
            let Some(parent) = class.parent() else {
                return false;
            };
            current = context
                .resolve_class_name(parent)
                .unwrap_or_else(|| parent.trim_start_matches('\\').to_string());
            continue;
        }
        if eval_attributes_have_global_builtin_attribute(
            &context.native_class_attributes(&current),
            "AllowDynamicProperties",
        ) {
            return true;
        }
        let Some(parent) = context.native_class_parent(&current) else {
            return false;
        };
        current = parent.to_string();
    }
    false
}

/// Writes a public dynamic entry without resolving an inaccessible declared slot of the same name.
fn eval_write_public_dynamic_property(
    object: RuntimeCellHandle,
    identity: u64,
    class_name: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_with_native_property_storage_scope(class_name, property_name, context, values, |values| {
        values.property_set(object, property_name, value)
    })?;
    context.mark_dynamic_property_initialized(identity, property_name);
    Ok(())
}

/// Binds one eval object property to a by-reference source parameter.
/// Checks an AOT array property before forwarding a write to its native setter.
pub(super) fn validate_eval_native_array_property_assignment(
    declaring_class: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(property_type) = context.native_property_type(declaring_class, property_name) else {
        return Ok(());
    };
    let requires_array = !property_type.allows_null()
        && !property_type.is_intersection()
        && !property_type.variants().is_empty()
        && property_type.variants().iter().all(|variant| matches!(variant, EvalParameterTypeVariant::Array));
    if !requires_array || matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Ok(());
    }
    eval_throw_type_error(
        &format!("Cannot assign value to property {}::{} of type array", declaring_class, property_name),
        context,
        values,
    )
}

/// Binds a writable instance property to a source reference and publishes its initialized storage.
pub(super) fn eval_property_reference_bind_result(
    object: RuntimeCellHandle,
    property_name: &str,
    source_name: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let identity = values.object_identity(object)?;
    let class = context
        .dynamic_object_class(identity)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let object_class_name = class.name().to_string();
    if context.has_enum(&object_class_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (declaring_class, property) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?;
    validate_eval_property_write_access(&declaring_class, &property, context)?;
    if property.is_readonly() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let storage_property_name = eval_instance_property_storage_name(&declaring_class, &property);
    let target = eval_property_reference_target(
        identity,
        &storage_property_name,
        source_name,
        context,
        scope,
        values,
    )?;
    // The alias outlives the activation that bound it (`new Holder($this->items)` inside a method
    // that returns, the holder kept for good), and an object target names its owner by cell
    // handle. That handle is the activation's own `$this` cell, which the frame releases on exit:
    // keep a reference of the alias's own, or the next write through it reads a freed cell.
    let target = match target {
        EvalReferenceTarget::ObjectProperty {
            object,
            property,
            access_scope,
        } => EvalReferenceTarget::ObjectProperty {
            object: values.retain(object)?,
            property,
            access_scope,
        },
        other => other,
    };
    let value = eval_reference_target_value(&target, context, values)?;
    context.bind_dynamic_property_alias(identity, &storage_property_name, target);
    eval_store_dynamic_property_value(identity, &storage_property_name, value, context, values)?;
    let _ = eval_with_native_property_storage_scope(
        &object_class_name, &storage_property_name, context, values,
        |values| values.property_set(object, &storage_property_name, value),
    );
    context.mark_dynamic_property_initialized(identity, &storage_property_name);
    Ok(())
}

/// Resolves a local by-reference source into a persistent property alias target.
pub(super) fn eval_property_reference_target(
    object_identity: u64,
    storage_property_name: &str,
    source_name: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalReferenceTarget, EvalStatus> {
    if let Some(target) = scope.reference_target(source_name).cloned() {
        return Ok(target);
    }
    if context.current_function().is_some() {
        if !generic_local_contracts(scope, source_name, context).is_empty() {
            if let Some(cell) = eval_persistent_variable_reference(source_name, context, scope, values)? {
                return Ok(EvalReferenceTarget::Cell { cell });
            }
        }
        let cell =
            visible_scope_cell(context, scope, source_name).map_or_else(|| values.null(), Ok)?;
        return Ok(EvalReferenceTarget::Cell { cell });
    }
    let alias_name = eval_property_reference_alias_name(object_identity, storage_property_name);
    for replaced in set_reference_alias(context, scope, &alias_name, source_name, values)? {
        values.release(replaced)?;
    }
    Ok(EvalReferenceTarget::Variable {
        scope: crate::scope::EvalScopeRef::new(scope),
        name: alias_name,
    })
}

/// Builds the hidden scope variable name that backs one property reference alias.
pub(super) fn eval_property_reference_alias_name(object_identity: u64, storage_property_name: &str) -> String {
    format!("\0elephc_property_ref:{object_identity}:{storage_property_name}")
}

/// Reads the current value from a persistent reference target.
pub(in crate::interpreter) fn eval_reference_target_value(
    target: &EvalReferenceTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match target {
        EvalReferenceTarget::Variable { scope, name } => {
            let Some(scope) = (unsafe { scope.live_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            visible_scope_cell(context, scope, name).map_or_else(|| values.null(), Ok)
        }
        EvalReferenceTarget::ArrayElement {
            scope,
            array_name,
            index,
        } => {
            let Some(scope) = (unsafe { scope.live_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let array =
                visible_scope_cell(context, scope, array_name).map_or_else(|| values.null(), Ok)?;
            let index = eval_array_reference_key_value(index, values)?;
            let result = values.array_get(array, index);
            values.release(index)?;
            result
        }
        EvalReferenceTarget::NestedArrayElement {
            array_target,
            index,
        } => {
            let array = eval_reference_target_value(array_target, context, values)?;
            let index = eval_array_reference_key_value(index, values)?;
            let result = values.array_get(array, index);
            values.release(index)?;
            // The container read is an owner of its own unless a variable lent it.
            if !array.is_borrowed() {
                values.release(array)?;
            }
            result
        }
        EvalReferenceTarget::ObjectProperty {
            object,
            property,
            access_scope,
        } => {
            let previous_scope = context.replace_execution_scope(access_scope.clone());
            let result =
                eval_property_reference_target_get_result(*object, property, context, values);
            context.replace_execution_scope(previous_scope);
            result
        }
        EvalReferenceTarget::StaticProperty {
            class_name,
            property,
            access_scope,
        } => {
            let previous_scope = context.replace_execution_scope(access_scope.clone());
            let result = eval_static_property_get_result(class_name, property, context, values);
            context.replace_execution_scope(previous_scope);
            result
        }
        EvalReferenceTarget::StaticLocal { function, name } => context
            .static_local(function, name)
            .map_or_else(|| values.null(), Ok),
        EvalReferenceTarget::Cell { cell } => Ok(cell.borrowed()),
        EvalReferenceTarget::InvokerSlot { slot, source_tag } => {
            eval_invoker_slot_ref_target_value(*slot, *source_tag, values)
        }
    }
}

/// Writes a new value to a persistent reference target.
pub(super) fn eval_reference_target_write(
    object_identity: u64,
    storage_property_name: &str,
    target: EvalReferenceTarget,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let EvalReferenceTarget::Variable { scope, name } = &target {
        let scope = unsafe { scope.live_mut() }.ok_or(EvalStatus::RuntimeFatal)?;
        return write_back_owned_variable_ref_target(scope, name, value, context, values);
    }
    if let EvalReferenceTarget::Cell { cell } = target {
        if values.is_reference(cell)? { return write_back_method_ref_target(&EvalReferenceTarget::Cell { cell }, value, context, values); }
        context.bind_dynamic_property_alias(
            object_identity,
            storage_property_name,
            EvalReferenceTarget::Cell { cell: value },
        );
        return Ok(());
    }
    write_back_method_ref_target(&target, value, context, values)
}

/// Evaluates PHP `isset($object->property)` without forcing `__get()` first.
pub(in crate::interpreter) fn eval_property_isset_result(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        let value = values.property_get(object, property_name)?;
        return Ok(!values.is_null(value)?);
    };
    eval_initialize_lazy_ghost(identity, object, context, values)?;
    context.import_foreign_object_class(identity);
    let Some(class) = context.dynamic_object_class(identity) else {
        // Native typed slots represent unset with an initialization marker.
        // Their ordinary getter deliberately rejects that state, while isset
        // must return false without attempting the read.
        if !values.property_is_initialized(object, property_name)? {
            // The marker probe only knows DECLARED slots. A property created by
            // `$object->name = ...` on a compiled object (`stdClass`) lives in the per-instance
            // hash, so a miss here must still ask that hash before answering false.
            if !eval_object_public_property_exists(object, property_name, values)? {
                return Ok(false);
            }
        }
        let value = values.property_get(object, property_name)?;
        return Ok(!values.is_null(value)?);
    };
    let object_class_name = class.name().to_string();
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_ok() {
            let storage_property_name =
                eval_instance_property_storage_name(&declaring_class, &property);
            if property.property_type().is_some()
                && !context.dynamic_property_is_initialized(identity, &storage_property_name)
            {
                return Ok(false);
            }
            let value = eval_property_get_result(object, property_name, context, values)?;
            return Ok(!values.is_null(value)?);
        }
        return eval_magic_property_isset(
            object,
            &object_class_name,
            property_name,
            context,
            values,
        )
        .map(|result| result.unwrap_or(false));
    }
    // Same store decision as the read: an undeclared property lives in the overlay, so
    // `isset($object->name)` answered false for a property the very next read returns.
    // The cell is inspected in place, without taking a reference the caller would have to drop.
    if let Some(stored) = context.dynamic_property_value(identity, property_name) {
        return Ok(!values.is_null(stored)?);
    }
    if eval_object_public_property_exists(object, property_name, values)? {
        let value = values.property_get(object, property_name)?;
        return Ok(!values.is_null(value)?);
    }
    if let Some((declaring_class, visibility, _, is_static)) =
        eval_dynamic_class_native_property_metadata(
            &object_class_name,
            property_name,
            context,
            values,
        )?
    {
        if !is_static {
            if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                return eval_magic_property_isset(
                    object,
                    &object_class_name,
                    property_name,
                    context,
                    values,
                )
                .map(|result| result.unwrap_or(false));
            }
            if !eval_with_native_bridge_scope(&declaring_class, context, || {
                values.property_is_initialized(object, property_name)
            })? {
                return Ok(false);
            }
            let value = eval_with_native_bridge_scope(&declaring_class, context, || {
                values.property_get(object, property_name)
            })?;
            return Ok(!values.is_null(value)?);
        }
    }
    eval_magic_property_isset(object, &object_class_name, property_name, context, values)
        .map(|result| result.unwrap_or(false))
}

/// Evaluates PHP `unset($object->property)` across eval declarations and native storage.
pub(in crate::interpreter) fn eval_property_unset_result(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(());
    };
    eval_initialize_lazy_ghost(identity, object, context, values)?;
    context.import_foreign_object_class(identity);
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = eval_runtime_object_class_name(object, values)?;
        if !eval_native_property_unset_result(object, &class_name, property_name, context, values)? {
            eval_magic_property_unset(object, &class_name, property_name, context, values)?;
        }
        return Ok(());
    };
    let object_class_name = class.name().to_string();
    if context.has_enum(&object_class_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if eval_native_private_property_unset_scope(&object_class_name, property_name, context, values)?.is_some() {
        return eval_native_property_unset_result(object, &object_class_name, property_name, context, values)
            .map(|_| ());
    }
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_ok() {
            if property.has_get_hook() || property.has_set_hook() {
                return eval_throw_hooked_property_unset_error(
                    &object_class_name, property.name(), context, values,
                );
            }
            let storage_property_name =
                eval_instance_property_storage_name(&declaring_class, &property);
            if property.is_readonly()
                && context.dynamic_property_is_initialized(identity, &storage_property_name)
            {
                return eval_throw_readonly_property_unset_error(
                    &declaring_class, property.name(), context, values,
                );
            }
            if property.is_readonly() {
                eval_validate_readonly_unset_scope(
                    &declaring_class, property.name(), property.write_visibility(), context, values,
                )?;
            }
            if validate_eval_property_write_access(&declaring_class, &property, context).is_err() {
                return eval_throw_property_unset_access_error(
                    &declaring_class,
                    &property,
                    context,
                    values,
                );
            }
            context.remove_dynamic_property_alias(identity, &storage_property_name);
            context.mark_dynamic_property_uninitialized(identity, &storage_property_name);
            if let Some(removed) =
                context.remove_dynamic_property_value(identity, &storage_property_name)
            {
                values.release(removed)?;
                // The overlay is only the interpreter's view. A property a COMPILED ancestor also
                // declares is mirrored into that ancestor's native slot, and the ancestor's
                // compiled methods read the slot: returning here left `$this->label` initialized
                // for them after an eval-side `unset()`. The slot is cleared too, best effort,
                // because an eval-only declaration has no native slot to clear.
                let native_unset = eval_with_native_property_storage_scope(
                    &object_class_name, &storage_property_name, context, values,
                    |values| values.unset_native_typed_property(object, &storage_property_name),
                );
                match native_unset {
                    Ok(true) => {}
                    _ if property.property_type().is_some() => {}
                    _ => {
                        let null = values.null()?;
                        let _ = eval_with_native_property_storage_scope(
                            &object_class_name, &storage_property_name, context, values,
                            |values| values.property_set(object, &storage_property_name, null),
                        );
                        release_expr_result(null, context, values)?;
                    }
                }
                return Ok(());
            }
            if eval_with_native_property_storage_scope(
                &object_class_name, &storage_property_name, context, values,
                |values| values.unset_native_typed_property(object, &storage_property_name),
            )? {
                return Ok(());
            }
            let null = values.null()?;
            let written = eval_with_native_property_storage_scope(
                &object_class_name, &storage_property_name, context, values,
                |values| values.property_set(object, &storage_property_name, null),
            );
            let released = release_expr_result(null, context, values);
            return written.and(released);
        }
        if eval_magic_property_unset(object, &object_class_name, property_name, context, values)? {
            return Ok(());
        }
        return eval_throw_property_access_error(
            &declaring_class, property.name(), property.visibility(), context, values,
        );
    }
    if eval_native_property_unset_result(object, &object_class_name, property_name, context, values)? {
        return Ok(());
    }
    // An UNDECLARED property reaches here, and on an eval-declared object it lives in the
    // context overlay rather than in the object's slots. Clearing only the slot left it
    // findable by everything that consults the overlay — `property_exists()` answered true
    // after an `unset()`, where `php -n` 8.5.6 answers false. Both stores are cleared.
    let removed_overlay = context.remove_dynamic_property_value(identity, property_name);
    if let Some(removed) = removed_overlay {
        context.remove_dynamic_property_alias(identity, property_name);
        context.mark_dynamic_property_uninitialized(identity, property_name);
        values.release(removed)?;
    }
    if eval_object_public_property_exists(object, property_name, values)? {
        let null = values.null()?;
        let written = values.property_set(object, property_name, null);
        let released = release_expr_result(null, context, values);
        return written.and(released);
    }
    if removed_overlay.is_some() {
        return Ok(());
    }
    let _ = eval_magic_property_unset(object, &object_class_name, property_name, context, values)?;
    Ok(())
}

/// Dispatches an undefined or inaccessible eval property read through `__get()`.
pub(super) fn eval_magic_property_get(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(object_class_name, "__get") else {
        return Ok(None);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let property = values.string(property_name)?;
    // The argument list borrows the name; this function owns it, as its AOT twin does.
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        object_class_name,
        &method,
        object,
        positional_args(vec![property]),
        context,
        values,
    );
    values.release(property)?;
    result.map(Some)
}

/// Dispatches a read of a name a COMPILED object does not declare through its class's `__get()`.
///
/// php calls `__get()` for any property it cannot find, and that holds for a compiled object read
/// from interpreted code as much as for an interpreted one. The bridge only knows the slots a
/// compiled class declares, so every other name read as null here: Laravel's
/// `$this->app->config['view']` (the container answers `config` through `__get()`) found nothing
/// once the reading closure carried an eval context. Returns `None` when no `__get()` exists.
fn eval_aot_magic_property_get(
    object: RuntimeCellHandle,
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if eval_aot_method_dispatch_metadata_in_hierarchy(class_name, "__get", context, values)?
        .is_none()
    {
        return Ok(None);
    }
    // Inside `isset()`/`empty()`/`??` php asks `__isset()` first, when the class has one, and a
    // false answer is a miss without ever calling `__get()`.
    if context.quiet_property_fetch()
        && eval_aot_method_dispatch_metadata_in_hierarchy(class_name, "__isset", context, values)?
            .is_some()
    {
        let name = values.string(property_name)?;
        let answer = eval_method_call_result(object, "__isset", vec![name], context, values);
        values.release(name)?;
        let answer = answer?;
        let set = values.truthy(answer)?;
        values.release(answer)?;
        if !set {
            return values.null().map(Some);
        }
    }
    let name = values.string(property_name)?;
    let result = eval_method_call_result(object, "__get", vec![name], context, values);
    values.release(name)?;
    result.map(Some)
}

/// Dispatches an undefined or inaccessible eval property write through `__set()`.
pub(super) fn eval_magic_property_set(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let identity = values.object_identity(object)?;
    let Some((declaring_class, method)) = context.class_method(object_class_name, "__set") else {
        // The magic writer may live on a COMPILED ancestor the class record does not list.
        if let Some(parent) = context.class_native_parent_name(object_class_name) {
            if eval_aot_method_dispatch_metadata_in_hierarchy(&parent, "__set", context, values)?
                .is_some()
            {
                let name = values.string(property_name)?;
                let result =
                    eval_method_call_result(object, "__set", vec![name, value], context, values);
                values.release(name)?;
                values.release(result?)?;
                return Ok(true);
            }
        }
        return Ok(false);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut native_node = std::pin::pin!([0_u64; 4]);
    if !values.native_magic_set_guard_push(
        identity,
        property_name,
        native_node.as_mut().get_mut(),
    )? {
        return Ok(false);
    }
    let result = (|| {
        let property = values.string(property_name)?;
        let result = eval_dynamic_method_with_values(
            &declaring_class,
            object_class_name,
            &method,
            object,
            positional_args(vec![property, value]),
            context,
            values,
        );
        values.release(property)?;
        values.release(result?)
    })();
    let popped = values.native_magic_set_guard_pop(native_node.as_mut().get_mut());
    result.and(popped).map(|()| true)
}

/// Dispatches an undefined or inaccessible eval property probe through `__isset()`.
pub(super) fn eval_magic_property_isset(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<bool>, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(object_class_name, "__isset") else {
        // The magic probe may live on a COMPILED ancestor the class record does not list.
        if let Some(parent) = context.class_native_parent_name(object_class_name) {
            if eval_aot_method_dispatch_metadata_in_hierarchy(&parent, "__isset", context, values)?
                .is_some()
            {
                let name = values.string(property_name)?;
                let answer = eval_method_call_result(object, "__isset", vec![name], context, values);
                values.release(name)?;
                let answer = answer?;
                let truthy = values.truthy(answer)?;
                values.release(answer)?;
                return Ok(Some(truthy));
            }
        }
        return Ok(None);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let property = values.string(property_name)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        object_class_name,
        &method,
        object,
        positional_args(vec![property]),
        context,
        values,
    );
    values.release(property)?;
    let result = result?;
    let truthy = values.truthy(result)?;
    values.release(result)?;
    Ok(Some(truthy))
}

/// Dispatches an undefined or inaccessible eval property unset through `__unset()`.
pub(super) fn eval_magic_property_unset(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let identity = values.object_identity(object)?;
    let Some(_guard) = EvalPropertyUnsetGuard::enter(identity, property_name) else {
        return Ok(false);
    };
    eval_magic_property_unset_inner(object, object_class_name, property_name, context, values)
}

/// Calls an eval or native magic unsetter while its recursion guard is held by the caller.
fn eval_magic_property_unset_inner(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(object_class_name, "__unset") else {
        return eval_native_magic_property_unset(object, object_class_name, property_name, context, values);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let property = values.string(property_name)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        object_class_name,
        &method,
        object,
        positional_args(vec![property.borrowed()]),
        context,
        values,
    );
    let result = result.and_then(|result| release_expr_result(result, context, values));
    let released = release_expr_result(property, context, values);
    result.and(released).map(|()| true)
}

/// Returns whether the object already has a public dynamic property with this exact name.
pub(super) fn eval_object_public_property_exists(
    object: RuntimeCellHandle,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    values.object_dynamic_property_exists(object, property_name)
}

/// Validates that an object property may be used as a by-reference method argument.
pub(in crate::interpreter) fn validate_property_ref_target(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(());
    };
    context.import_foreign_object_class(identity);
    let Some(class) = context.dynamic_object_class(identity) else {
        return Ok(());
    };
    let object_class_name = class.name().to_string();
    if context.has_enum(&object_class_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        validate_eval_member_access(&declaring_class, property.visibility(), context)?;
        if property.is_readonly() {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns true while executing the named hook accessor for one property.
pub(in crate::interpreter) fn current_eval_property_hook_is(
    declaring_class: &str,
    property_name: &str,
    hook_method: &str,
    context: &ElephcEvalContext,
) -> bool {
    let Some(current_class) = context.current_class_scope() else {
        return false;
    };
    let Some((_, method)) = context
        .current_function()
        .and_then(|function| function.rsplit_once("::"))
    else {
        return false;
    };
    let same_property_hook = method.eq_ignore_ascii_case(hook_method)
        || method.eq_ignore_ascii_case(&property_hook_get_method(property_name))
        || method.eq_ignore_ascii_case(&property_hook_set_method(property_name));
    if !same_property_hook {
        return false;
    }
    // A redeclared child property may inherit either concrete accessor independently.
    // Its backing access belongs to that accessor's lexical owner, not to the child.
    context.class_method(declaring_class, method)
        .is_some_and(|(owner, _)| same_eval_class_name(current_class, &owner))
}
