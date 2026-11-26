//! Purpose:
//! Materializes common Reflection owner fields and nested member/type objects.
//!
//! Called from:
//! - Reflection owner-specific constructors after resolving metadata.
//!
//! Key details:
//! - Temporary arrays and object handles are transferred through runtime ownership APIs.
//! - Member and parameter owners store their declaring class's NAME, never a reflector: the
//!   compiled getters build a fresh, complete `ReflectionClass` (a `ReflectionEnum` for an
//!   enum, as php's class factory does) from it on every call.
//! - Reflector lists (`__methods`, `__properties`, `__parameters`, `__types`) are direct object
//!   arrays, the storage the compiled `array<Class>` readers of those slots expect.
//! - Compiled getters read `__constants`, `__default_properties`, `__is_backed` and
//!   `__backing_type` from the object, so class owners fill those slots after construction;
//!   eval's own calls on the same reflector are intercepted by identity and never read them.

use super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, StagedRuntimeOwners, with_runtime_owners};

/// Materializes one Reflection owner object and transfers its temporary metadata cells.
///
/// Class owners carry their member reflectors; member and parameter owners carry only their
/// declaring class's name (see `eval_reflection_lazy_class_name_result`).
pub(super) fn eval_reflection_owner_object(
    owner_kind: u64,
    reflected_name: &str,
    attributes: &[EvalAttribute],
    interface_names: &[String],
    trait_names: &[String],
    method_names: &[String],
    property_names: &[String],
    parent_class_name: Option<&str>,
    parameter_metadata: &[EvalReflectionParameterMetadata],
    type_metadata: Option<&EvalReflectionParameterTypeMetadata>,
    settable_type_metadata: Option<&EvalReflectionParameterTypeMetadata>,
    default_value: Option<&EvalExpr>,
    default_value_trait_origin: Option<&str>,
    flags: u64,
    modifiers: u64,
    method_modifiers: u64,
    constant_value: Option<RuntimeCellHandle>,
    backing_value: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // Constructing a reflector initializes nothing in php-src; the defaults and member values
    // materialized here for the compiled getters are evaluated silently.
    context.begin_deprecation_silence();
    let object = eval_reflection_owner_object_silently(
        owner_kind, reflected_name, attributes, interface_names, trait_names, method_names,
        property_names, parent_class_name, parameter_metadata, type_metadata,
        settable_type_metadata, default_value, default_value_trait_origin, flags, modifiers,
        method_modifiers, constant_value, backing_value, context, values,
    );
    context.end_deprecation_silence();
    let object = object?;
    let lazy = eval_reflection_lazy_source(owner_kind, reflected_name, parent_class_name, default_value.is_some(), object, context, values)
        .and_then(|lazy| match lazy {
            Some((scope, source)) => eval_reflection_attach_lazy(object, &scope, "__lazy", &source, None, context, values),
            None => eval_reflection_clear_lazy(owner_kind, object, context, values),
        });
    if let Err(status) = lazy {
        let _ = eval_release_value(context, values, object);
        return Err(status);
    }
    Ok(object)
}

/// Stores, on a `ReflectionParameter` eval builds, the `__default_factory` closure the compiled
/// `getDefaultValue()` calls: php-src evaluates the default on every call, so compiled code gets
/// a fresh evaluation through eval, with its notices and errors, rather than the value
/// previewed at construction. A closure's parameter, which cannot be named again, is reached
/// through the closure object being reflected. A default eval leaves to a native factory keeps
/// that factory.
fn eval_reflection_attach_parameter_lazy_default(
    object: RuntimeCellHandle,
    parameter: &EvalReflectionParameterMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(default) = parameter.default_value.as_ref() else { return Ok(()) };
    if parameter_default_is_deferred(default) {
        return Ok(());
    }
    let Some(function) = parameter.declaring_function.as_ref() else { return Ok(()) };
    if function.name.contains('{') {
        let Some(closure) = context.reflected_closure() else { return Ok(()) };
        let source = format!(
            "return static function () use ($__target) {{ return (new \\ReflectionFunction($__target))->getParameters()[{}]->getDefaultValue(); }};",
            parameter.position
        );
        return eval_reflection_attach_lazy(object, "ReflectionParameter", "__default_factory", &source, Some(closure), context, values);
    }
    let target = match function.declaring_class_name.as_deref() {
        Some(class) => format!(
            "[{}, {}]",
            eval_php_single_quoted(class.trim_start_matches('\\')),
            eval_php_single_quoted(&function.name)
        ),
        None => eval_php_single_quoted(function.name.trim_start_matches('\\')),
    };
    let source = format!(
        "return static function () {{ return (new \\ReflectionParameter({target}, {}))->getDefaultValue(); }};",
        parameter.position
    );
    eval_reflection_attach_lazy(object, "ReflectionParameter", "__default_factory", &source, None, context, values)
}

/// Stores `null` in the `__lazy` slot of a reflector whose getters answer from its own slots.
/// The runtime allocates an eval-built reflector's payload zeroed, which a compiled
/// `__lazy !== null` test would read as a value.
fn eval_reflection_clear_lazy(
    owner_kind: u64,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let scope = if eval_reflection_owner_uses_class_metadata(owner_kind) {
        eval_reflection_class_owner_scope(owner_kind).to_string()
    } else if matches!(
        owner_kind,
        EVAL_REFLECTION_OWNER_PROPERTY
            | EVAL_REFLECTION_OWNER_CLASS_CONSTANT
            | EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE
            | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE
    ) {
        runtime_object_class_name(object, values)?
    } else {
        return Ok(());
    };
    let null = values.null()?;
    let stored = crate::interpreter::statements::eval_set_reflector_slots(object, &scope, &[("__lazy", null)], context, values);
    let released = values.release(null);
    stored.and(released)
}

/// Quotes `text` as a PHP single-quoted string literal.
fn eval_php_single_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Returns the class scope and the PHP source of the closure a reflector `eval()` builds stores
/// in `__lazy`, or `None` when its compiled getters can answer from the slots filled at
/// construction. php-src computes a reflected class constant, enum case, property default,
/// constant table, or default-property map on the getter call, so a value not final when the
/// reflector is built is left to the closure, which makes that evaluation through eval (raising
/// its notices and errors) when compiled code calls the getter. An eval call never reaches the
/// compiled getter (`eval_reflection_evaluating_getter_result`).
fn eval_reflection_lazy_source(
    owner_kind: u64,
    reflected_name: &str,
    parent_class_name: Option<&str>,
    has_default: bool,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, String)>, EvalStatus> {
    if eval_reflection_owner_uses_class_metadata(owner_kind) {
        if !eval_reflection_class_like_exists(reflected_name, context) {
            return Ok(None);
        }
        let class = eval_php_single_quoted(reflected_name.trim_start_matches('\\'));
        let source = format!(
            "return static function (string $m, ...$a) {{ $r = new \\ReflectionClass({class}); \
             if ($m === 'getConstant') {{ return $r->getConstant($a[0]); }} \
             if ($m === 'getConstants') {{ return $r->getConstants(); }} \
             return $r->getDefaultProperties(); }};"
        );
        return Ok(Some((eval_reflection_class_owner_scope(owner_kind).to_string(), source)));
    }
    let Some(holder) = parent_class_name else { return Ok(None) };
    if !eval_reflection_class_like_exists(holder, context) {
        return Ok(None);
    }
    let quoted_holder = eval_php_single_quoted(holder.trim_start_matches('\\'));
    let quoted_name = eval_php_single_quoted(reflected_name);
    if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY {
        // Only a default eval evaluates itself (`eval_reflected_property_default`) gets the
        // closure: for any other (an implicit `null`, a native class) eval answers nothing, and
        // the compiled getter would call the closure again.
        let evaluates = context.class(holder).is_some_and(|class| {
            class.properties().iter().any(|property| property.name() == reflected_name && property.default().is_some())
        });
        if !has_default || !evaluates {
            return Ok(None);
        }
        let source = format!(
            "return static function (string $m) {{ return (new \\ReflectionProperty({quoted_holder}, {quoted_name}))->getDefaultValue(); }};"
        );
        return Ok(Some((String::from("ReflectionProperty"), source)));
    }
    if !matches!(
        owner_kind,
        EVAL_REFLECTION_OWNER_CLASS_CONSTANT | EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE
    ) {
        return Ok(None);
    }
    let evaluated = if crate::interpreter::statements::eval_is_enum_case(holder, reflected_name, context) {
        context.enum_case(holder, reflected_name).is_some()
    } else {
        context
            .class_constant(holder, reflected_name)
            .is_none_or(|(owner, constant)| context.class_constant_cell(&owner, constant.name()).is_some())
    };
    if evaluated {
        return Ok(None);
    }
    let reflector_class = runtime_object_class_name(object, values)?;
    let source = format!(
        "return static function (string $m) {{ $r = new \\{reflector_class}({quoted_holder}, {quoted_name}); \
         return $m === 'getBackingValue' ? $r->getBackingValue() : $r->getValue(); }};"
    );
    Ok(Some((reflector_class, source)))
}

/// Builds the closure `source` (a `return` of a closure expression, which may capture
/// `$__target`, bound to `target`) and stores it in reflector slot `slot`, from inside class
/// scope `scope`.
fn eval_reflection_attach_lazy(
    object: RuntimeCellHandle,
    scope: &str,
    slot: &str,
    source: &str,
    target: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let program = crate::parser::parse_fragment(source.as_bytes()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let Some(EvalStmt::Return(Some(expr))) = program.statements().first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let mut closure_scope = ElephcEvalScope::new();
    if let Some(target) = target {
        closure_scope.set("__target", target, crate::scope::ScopeCellOwnership::Borrowed);
    }
    let closure = RuntimeOwnerSlot::new(eval_expr(expr, context, &mut closure_scope, values)?);
    with_runtime_owners(&closure, || {
        let stored = crate::interpreter::statements::eval_set_reflector_slots(
            object,
            scope,
            &[(slot, closure.borrowed())],
            context,
            values,
        );
        let released = release_expr_result(closure.take(), context, values);
        stored.and(released)
    })
}

/// Builds the owner object for [`eval_reflection_owner_object`] inside its silent evaluation.
fn eval_reflection_owner_object_silently(
    owner_kind: u64,
    reflected_name: &str,
    attributes: &[EvalAttribute],
    interface_names: &[String],
    trait_names: &[String],
    method_names: &[String],
    property_names: &[String],
    parent_class_name: Option<&str>,
    parameter_metadata: &[EvalReflectionParameterMetadata],
    type_metadata: Option<&EvalReflectionParameterTypeMetadata>,
    settable_type_metadata: Option<&EvalReflectionParameterTypeMetadata>,
    default_value: Option<&EvalExpr>,
    default_value_trait_origin: Option<&str>,
    flags: u64,
    modifiers: u64,
    method_modifiers: u64,
    constant_value: Option<RuntimeCellHandle>,
    backing_value: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = with_reflection_staging(values, |temporaries, output, values| {
        // The caller transfers any owned preview cells, even if another field fails first.
        if let Some(value) = constant_value { temporaries.stage(value); }
        if let Some(value) = backing_value { temporaries.stage(value); }
        let attrs = temporaries.stage(eval_reflection_attribute_array_result(
            attributes,
            eval_reflection_attribute_target(owner_kind),
            context,
            values,
        )?);
        let interface_names_array = temporaries.stage(eval_reflection_string_array_result(interface_names, values)?);
        let trait_names_array = temporaries.stage(eval_reflection_string_array_result(trait_names, values)?);
        let method_names_array = temporaries.stage(eval_reflection_string_array_result(method_names, values)?);
        let property_names_array = temporaries.stage(eval_reflection_string_array_result(property_names, values)?);
        let class_metadata_owner = eval_reflection_owner_uses_class_metadata(owner_kind);
        let is_eval_class = class_metadata_owner
            && eval_reflection_class_like_exists(reflected_name, context);
        let method_objects = if class_metadata_owner {
            if is_eval_class {
                eval_reflection_member_object_array_result(
                    EVAL_REFLECTION_OWNER_METHOD,
                    reflected_name,
                    method_names,
                    None,
                    context,
                    values,
                )?
            } else {
                eval_reflection_aot_member_object_array_result(
                    EVAL_REFLECTION_OWNER_METHOD,
                    reflected_name,
                    method_names,
                    None,
                    context,
                    values,
                )?
            }
        } else if matches!(
            owner_kind,
            EVAL_REFLECTION_OWNER_METHOD | EVAL_REFLECTION_OWNER_FUNCTION
        ) {
            eval_reflection_parameter_object_array_result(parameter_metadata, context, values)?
        } else if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY {
            match type_metadata {
                Some(type_metadata) => eval_reflection_type_object_result(type_metadata, values)?,
                None => values.null()?,
            }
        } else {
            values.array_new(0)?
        };
        let method_objects = temporaries.stage(method_objects);
        let property_objects = if class_metadata_owner {
            if is_eval_class {
                eval_reflection_member_object_array_result(
                    EVAL_REFLECTION_OWNER_PROPERTY,
                    reflected_name,
                    property_names,
                    None,
                    context,
                    values,
                )?
            } else {
                eval_reflection_aot_member_object_array_result(
                    EVAL_REFLECTION_OWNER_PROPERTY,
                    reflected_name,
                    property_names,
                    None,
                    context,
                    values,
                )?
            }
        } else if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY {
            // A default that needs a value php-src has not evaluated yet (or throws) is stored as
            // null; `getDefaultValue()` evaluates it (`eval_reflection_evaluating_getter_result`, or the
            // `__lazy` closure from compiled code).
            let preview = match default_value {
                Some(default) => crate::interpreter::statements::eval_preview_outcome(
                    eval_reflection_class_like_default_value(
                        parent_class_name,
                        default_value_trait_origin,
                        default,
                        context,
                        values,
                    ),
                    context,
                    values,
                )?,
                None => None,
            };
            match preview {
                Some(value) => value,
                None => values.null()?,
            }
        } else {
            values.array_new(0)?
        };
        let property_objects = temporaries.stage(property_objects);
        let parent_class = temporaries.stage(
            eval_reflection_related_class_result(owner_kind, parent_class_name, context, values)?,
        );
        let constructor = temporaries.stage(
            eval_reflection_constructor_object_result(owner_kind, reflected_name, context, values)?,
        );
        let constant_value_cell = if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY {
            temporaries.stage(match settable_type_metadata {
                Some(type_metadata) => eval_reflection_type_object_result(type_metadata, values)?,
                None => values.null()?,
            })
        } else {
            match constant_value {
                Some(value) => value,
                None => temporaries.stage(values.null()?),
            }
        };
        let backing_value_cell = match backing_value {
            Some(value) => value,
            None => temporaries.stage(values.null()?),
        };
        let object = values.reflection_owner_new(
            owner_kind,
            reflected_name,
            attrs,
            interface_names_array,
            trait_names_array,
            method_names_array,
            property_names_array,
            method_objects,
            property_objects,
            parent_class,
            flags,
            modifiers,
            method_modifiers,
            constant_value_cell,
            backing_value_cell,
            constructor,
        )?;
        output.replace(Some(object));
        if owner_kind == EVAL_REFLECTION_OWNER_CLASS_CONSTANT {
            let has_type = temporaries.stage(values.bool_value(type_metadata.is_some())?);
            let type_value = match type_metadata {
                Some(type_metadata) => eval_reflection_type_object_result(type_metadata, values)?,
                None => values.null()?,
            };
            let type_value = temporaries.stage(type_value);
            eval_reflection_with_declaring_class_scope(
                "ReflectionClassConstant",
                context,
                |_| -> Result<(), EvalStatus> {
                    values.property_set(object, "__has_type", has_type)?;
                    values.property_set(object, "__type", type_value)
                },
            )?;
        }
        if class_metadata_owner {
            let stored =
                eval_reflection_store_class_constants(object, owner_kind, reflected_name, context, values)
                    .and_then(|()| {
                        eval_reflection_store_class_defaults(object, owner_kind, reflected_name, context, values)
                    });
            stored?;
        }
        if owner_kind == EVAL_REFLECTION_OWNER_ENUM {
            eval_reflection_store_enum_backing(object, reflected_name, context, values)?;
        }
        if matches!(
            owner_kind,
            EVAL_REFLECTION_OWNER_CLASS | EVAL_REFLECTION_OWNER_OBJECT | EVAL_REFLECTION_OWNER_ENUM
        ) {
            let identity = values.object_identity(object)?;
            context.register_eval_reflection_class(identity, reflected_name);
        } else if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
            if let Some(declaring_class) = parent_class_name {
                let identity = values.object_identity(object)?;
                context.register_eval_reflection_method(identity, declaring_class, reflected_name);
            }
        } else if owner_kind == EVAL_REFLECTION_OWNER_FUNCTION {
            let identity = values.object_identity(object)?;
            context.register_eval_reflection_function(identity, reflected_name);
        } else if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY {
            if let Some(declaring_class) = parent_class_name {
                if flags & EVAL_REFLECTION_MEMBER_FLAG_DYNAMIC != 0 {
                    let identity = values.object_identity(object)?;
                    context.register_eval_dynamic_reflection_property(
                        identity,
                        declaring_class,
                        reflected_name,
                    );
                } else {
                    let identity = values.object_identity(object)?;
                    context.register_eval_reflection_property(
                        identity,
                        declaring_class,
                        reflected_name,
                    );
                }
            }
        } else if matches!(
            owner_kind,
            EVAL_REFLECTION_OWNER_CLASS_CONSTANT
                | EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE
                | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE
        ) {
            if let Some(declaring_class) = parent_class_name {
                let identity = values.object_identity(object)?;
                context.register_eval_reflection_class_constant(
                    identity,
                    declaring_class,
                    reflected_name,
                    owner_kind,
                );
            }
        }
        Ok(())
    })?;
    let rendered = with_runtime_owners(&object, || {
        if matches!(owner_kind, EVAL_REFLECTION_OWNER_FUNCTION | EVAL_REFLECTION_OWNER_METHOD) {
            let identity = values.object_identity(object)?;
            if let Some(target) = eval_reflection_function_method_target(identity, context, values)? {
                let rendered = eval_reflection_function_method_to_string(&target);
                let owner = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
                    "ReflectionMethod"
                } else {
                    "ReflectionFunction"
                };
                eval_reflection_store_dump(object, owner, &rendered, context, values)?;
            }
        }
        Ok(())
    });
    if let Err(status) = rendered {
        let _ = eval_release_value(context, values, object);
        return Err(status);
    }
    Ok(object)
}

/// Returns the class scope that reaches a class-metadata owner's `__constants`,
/// `__default_properties` and `__default_error` slots.
///
/// `ReflectionObject` and `ReflectionEnum` redeclare every `ReflectionClass` slot, so each of
/// their objects carries two same-named private slots: the parent's, hidden, and their own,
/// which their compiled getters read. The eval property writer picks the slot by scope, so the
/// write must come from the object's own class; the parent's scope lands in the hidden slot.
fn eval_reflection_class_owner_scope(owner_kind: u64) -> &'static str {
    match owner_kind {
        EVAL_REFLECTION_OWNER_OBJECT => "ReflectionObject",
        EVAL_REFLECTION_OWNER_ENUM => "ReflectionEnum",
        _ => "ReflectionClass",
    }
}

/// Stores the constant map the compiled `getConstants()` getter reads from a class reflector's
/// `__constants` slot; eval never filled it, so compiled code got `null` from every eval-built
/// class reflector.
fn eval_reflection_store_class_constants(
    object: RuntimeCellHandle,
    owner_kind: u64,
    reflected_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let constants = eval_reflection_constants_map(reflected_name, None, context, values)?;
    let scope = eval_reflection_class_owner_scope(owner_kind);
    with_eval_value_lease(constants, context, values, |constants, context, values| {
        eval_reflection_with_declaring_class_scope(scope, context, |_| {
            values.property_set(object, "__constants", constants)
        })
    })
}

/// Stores what the compiled `getDefaultProperties()` getter reads from a class reflector: the
/// default-property map in `__default_properties`, or the retained default Error message in
/// `__default_error`, which the getter throws as php does.
///
/// The slots are a cache for the compiled getter, so a default eval cannot evaluate (one that
/// throws, or one it does not support yet) leaves them untouched instead of failing the
/// reflector's construction: the getter answers `null`, as it did before the fill, and eval's
/// own `getDefaultProperties()` handler still reports the failure to eval code.
fn eval_reflection_store_class_defaults(
    object: RuntimeCellHandle,
    owner_kind: u64,
    reflected_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let scope = eval_reflection_class_owner_scope(owner_kind);
    let defaults = match eval_reflection_default_properties(reflected_name, context, values) {
        Ok(defaults) => defaults,
        Err(status) => {
            if matches!(status, EvalStatus::UncaughtThrowable) {
                if let Some(throwable) = context.take_pending_throw() {
                    eval_release_value(context, values, throwable)?;
                }
            }
            return Ok(());
        }
    };
    match defaults {
        EvalReflectionDefaultProperties::Values(defaults) => {
            with_eval_value_lease(defaults, context, values, |defaults, context, values| {
                eval_reflection_with_declaring_class_scope(scope, context, |_| {
                    values.property_set(object, "__default_properties", defaults)
                })
            })
        }
        EvalReflectionDefaultProperties::DeferredError(message) => {
            let text = values.string(&message)?;
            with_eval_value_lease(text, context, values, |text, context, values| {
                eval_reflection_with_declaring_class_scope(scope, context, |_| {
                    values.property_set(object, "__default_error", text)
                })
            })
        }
    }
}

/// Stores the `isBacked()` / `getBackingType()` answers compiled code reads from an
/// eval-declared enum's reflector; a compiled enum reflected through the bridge keeps the slot
/// defaults, as its backing type is not in the eval catalog.
fn eval_reflection_store_enum_backing(
    object: RuntimeCellHandle,
    reflected_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(backing_type) = context
        .enum_decl(reflected_name)
        .and_then(EvalEnum::backing_type)
    else {
        return Ok(());
    };
    let is_backed = RuntimeOwnerSlot::new(values.bool_value(true)?);
    let metadata = eval_reflection_enum_backing_type_metadata(backing_type);
    let type_object = match eval_reflection_type_object_result(&metadata, values) {
        Ok(type_object) => RuntimeOwnerSlot::new(type_object),
        Err(status) => {
            let _ = values.release(is_backed.take());
            return Err(status);
        }
    };
    with_runtime_owners(&(&is_backed, &type_object), || {
        let stored = eval_reflection_with_declaring_class_scope("ReflectionEnum", context, |_| {
            values.property_set(object, "__is_backed", is_backed.borrowed())?;
            values.property_set(object, "__backing_type", type_object.borrowed())
        });
        let is_backed_released = values.release(is_backed.take());
        let type_object_released = values.release(type_object.take());
        stored.and(is_backed_released).and(type_object_released)
    })
}

/// Stores a dump in the slot read by native casts, releasing the temporary on both paths.
fn eval_reflection_store_dump(
    object: RuntimeCellHandle, owner: &str, rendered: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let text = RuntimeOwnerSlot::new(values.string(rendered)?);
    with_runtime_owners(&text, || {
        let result = eval_reflection_with_declaring_class_scope(owner, context, |_| {
            values.property_set(object, "__string", text.borrowed())
        });
        let released = values.release(text.take());
        result.and(released)
    })
}

/// Builds the value stored in a class's parent slot or a member's declaring-class slot.
///
/// A class owner stores its parent's complete `ReflectionClass` (or `false`); a member owner
/// stores its declaring class's name (see `eval_reflection_lazy_class_name_result`).
pub(super) fn eval_reflection_related_class_result(
    owner_kind: u64,
    related_class_name: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(related_class_name) = related_class_name else {
        return values.bool_value(false);
    };
    if eval_reflection_owner_uses_class_metadata(owner_kind) {
        return eval_reflection_full_class_object_result(related_class_name, context, values);
    }
    if matches!(
        owner_kind,
        EVAL_REFLECTION_OWNER_METHOD
            | EVAL_REFLECTION_OWNER_PROPERTY
            | EVAL_REFLECTION_OWNER_CLASS_CONSTANT
            | EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE
            | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE
    ) {
        return eval_reflection_lazy_class_name_result(related_class_name, context, values);
    }
    values.bool_value(false)
}

/// Builds the lazy class value a member or parameter reflector stores in its declaring-class
/// (or `ReflectionParameter::getClass()`) slot: the class's declared name, or `false` when no
/// eval or compiled class has that name.
///
/// The compiled getters turn the name into a fresh, complete `ReflectionClass` on every call
/// through `__elephc_eval_reflection_class_build()` (see
/// `crate::interpreter::execute_context_reflection_class_outcome`), as php builds a new
/// reflector per call. Storing one reflector instead would hand the same object out on every
/// call and keep it alive for as long as the member.
pub(super) fn eval_reflection_lazy_class_name_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(metadata) = eval_reflection_class_like_attributes(class_name, context) {
        return values.string(&metadata.resolved_name);
    }
    if eval_reflection_aot_class_flags(class_name, values)?.is_some() {
        return values.string(class_name.trim_start_matches('\\'));
    }
    values.bool_value(false)
}

/// Builds a complete `ReflectionClass` object, member reflectors included, for one class.
///
/// An enum is reflected as a `ReflectionEnum`, as php's class factory does for every getter
/// that hands back a class (`getDeclaringClass()`, `ReflectionParameter::getClass()`, ...).
pub(in crate::interpreter) fn eval_reflection_full_class_object_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("Closure")
    {
        return eval_reflection_builtin_closure_class_object_result(
            EVAL_REFLECTION_OWNER_CLASS,
            context,
            values,
        );
    }
    let Some(metadata) = eval_reflection_class_like_attributes(class_name, context) else {
        let Some((flags, modifiers)) = eval_reflection_aot_class_flags(class_name, values)? else {
            return values.bool_value(false);
        };
        let owner_kind = if flags & EVAL_REFLECTION_CLASS_FLAG_ENUM != 0 {
            EVAL_REFLECTION_OWNER_ENUM
        } else {
            EVAL_REFLECTION_OWNER_CLASS
        };
        let runtime_class_name = class_name.trim_start_matches('\\');
        let method_names = eval_reflection_aot_member_names(
            EVAL_REFLECTION_OWNER_METHOD,
            runtime_class_name,
            values,
        )?;
        let property_names = eval_reflection_aot_member_names(
            EVAL_REFLECTION_OWNER_PROPERTY,
            runtime_class_name,
            values,
        )?;
        let interface_names =
            eval_reflection_aot_class_interface_names(runtime_class_name, values)?;
        let trait_names = eval_reflection_aot_class_trait_names(runtime_class_name, values)?;
        let parent_class_name = eval_reflection_aot_parent_class_name(runtime_class_name, values)?;
        let attributes = context.native_class_attributes(runtime_class_name);
        return eval_reflection_owner_object(
            owner_kind,
            runtime_class_name,
            &attributes,
            &interface_names,
            &trait_names,
            &method_names,
            &property_names,
            parent_class_name.as_deref(),
            &[],
            None,
            None,
            None,
            None,
            flags,
            modifiers,
            0,
            None,
            None,
            context,
            values,
        );
    };
    if context.has_enum(&metadata.resolved_name) {
        return eval_reflection_enum_object_result(&metadata.resolved_name, context, values);
    }
    let interface_names =
        eval_reflection_eval_metadata_interface_names(&metadata, context, values)?;
    let flags = eval_reflection_eval_metadata_flags(&metadata, context, values)?;
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_CLASS,
        &metadata.resolved_name,
        &metadata.attributes,
        &interface_names,
        &metadata.trait_names,
        &metadata.method_names,
        &metadata.property_names,
        metadata.parent_class_name.as_deref(),
        &[],
        None,
        None,
        None,
        None,
        flags,
        metadata.modifiers,
        0,
        None,
        None,
        context,
        values,
    )
}

/// Returns the generated/AOT parent class name for a reflected class, if any.
pub(super) fn eval_reflection_aot_parent_class_name(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let class_cell = values.string(runtime_class_name)?;
    let parent_cell = match values.parent_class_name(class_cell) {
        Ok(parent_cell) => parent_cell,
        Err(err) => {
            values.release(class_cell)?;
            return Err(err);
        }
    };
    values.release(class_cell)?;
    let parent_bytes = match values.string_bytes(parent_cell) {
        Ok(parent_bytes) => parent_bytes,
        Err(err) => {
            values.release(parent_cell)?;
            return Err(err);
        }
    };
    values.release(parent_cell)?;
    let parent_name = String::from_utf8(parent_bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    if parent_name.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parent_name))
    }
}

/// Builds an indexed PHP string array for ReflectionClass metadata names.
pub(super) fn eval_reflection_string_array_result(
    names: &[String],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_reflection_staging(values, |_, output, values| {
        output.replace(Some(values.string_array_new(names.len())?));
        for name in names {
            let updated = values.string_array_push(output.borrowed(), name)?;
            output.update_receiver(updated);
        }
        Ok(())
    })
}

/// Builds a string-keyed PHP associative array from owned string pairs.
pub(super) fn eval_reflection_string_assoc_result(
    pairs: Vec<(String, String)>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::assoc(values, pairs.len())?;
    for (key, value) in pairs {
        result.string(&key, |values| values.string(&value))?;
    }
    Ok(result.finish())
}

/// Builds a name-keyed PHP array of full ReflectionClass objects.
pub(super) fn eval_reflection_class_object_map_result(
    names: &[String],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::assoc(values, names.len())?;
    for name in names {
        result.string(name, |values| eval_reflection_full_class_object_result(name, context, values))?;
    }
    Ok(result.finish())
}

/// Maps a synthetic reflection owner kind to PHP's `Attribute::TARGET_*` bitmask.
pub(super) fn eval_reflection_attribute_target(owner_kind: u64) -> u64 {
    match owner_kind {
        EVAL_REFLECTION_OWNER_CLASS | EVAL_REFLECTION_OWNER_OBJECT | EVAL_REFLECTION_OWNER_ENUM => {
            EVAL_REFLECTION_ATTRIBUTE_TARGET_CLASS
        }
        EVAL_REFLECTION_OWNER_FUNCTION => EVAL_REFLECTION_ATTRIBUTE_TARGET_FUNCTION,
        EVAL_REFLECTION_OWNER_METHOD => EVAL_REFLECTION_ATTRIBUTE_TARGET_METHOD,
        EVAL_REFLECTION_OWNER_PROPERTY => EVAL_REFLECTION_ATTRIBUTE_TARGET_PROPERTY,
        EVAL_REFLECTION_OWNER_CLASS_CONSTANT
        | EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE
        | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE => EVAL_REFLECTION_ATTRIBUTE_TARGET_CLASS_CONSTANT,
        _ => 0,
    }
}

/// Returns whether a synthetic owner stores `ReflectionClass`-style metadata.
pub(super) fn eval_reflection_owner_uses_class_metadata(owner_kind: u64) -> bool {
    matches!(
        owner_kind,
        EVAL_REFLECTION_OWNER_CLASS | EVAL_REFLECTION_OWNER_OBJECT | EVAL_REFLECTION_OWNER_ENUM
    )
}

/// Builds a direct object array of populated ReflectionParameter objects, in position order.
pub(super) fn eval_reflection_parameter_object_array_result(
    parameters: &[EvalReflectionParameterMetadata],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::objects(values, parameters.len())?;
    for parameter in parameters {
        result.object(|values| eval_reflection_parameter_object_result(parameter, context, values))?;
    }
    Ok(result.finish())
}

/// Materializes one ReflectionParameter object through the shared reflection helper.
pub(super) fn eval_reflection_parameter_object_result(
    parameter: &EvalReflectionParameterMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = with_reflection_staging(values, |temporaries, output, values| {
        let attrs = temporaries.stage(eval_reflection_attribute_array_result(
            &parameter.attributes,
            EVAL_REFLECTION_ATTRIBUTE_TARGET_PARAMETER,
            context,
            values,
        )?);
        let declaring_function = temporaries.stage(match parameter.declaring_function.as_ref() {
            Some(metadata) => {
                eval_reflection_declaring_function_object_result(metadata, context, values)?
            }
            None => values.null()?,
        });
        let trait_names = temporaries.stage(values.array_new(0)?);
        let method_names = temporaries.stage(values.array_new(0)?);
        let property_names = temporaries.stage(values.array_new(0)?);
        let parent_class = temporaries.stage(match parameter.declaring_class_name.as_deref() {
            Some(declaring_class_name) => {
                eval_reflection_lazy_class_name_result(declaring_class_name, context, values)?
            }
            None => values.null()?,
        });
        let type_value = temporaries.stage(match parameter.type_metadata.as_ref() {
            Some(type_metadata) => eval_reflection_type_object_result(type_metadata, values)?,
            None => values.null()?,
        });
        let class_value = temporaries.stage(eval_reflection_parameter_class_value(parameter, context, values)?);
        // Materializing the reflector is not a PHP evaluation; `getDefaultValue()` raises the
        // default's notices on each call (`eval_reflection_evaluating_getter_result`). A default
        // that constructs an object is not previewed at all: that would run its constructor, and
        // both getters evaluate it themselves (eval directly, compiled code through
        // `__default_factory`).
        let constructs = parameter
            .default_value
            .as_ref()
            .is_some_and(crate::interpreter::statements::eval_expr_constructs_object);
        context.begin_deprecation_silence();
        let default_value = if constructs {
            values.null()
        } else {
            eval_reflection_parameter_default_value(parameter, context, values)
        };
        context.end_deprecation_silence();
        // A default that needs a value php-src has not evaluated yet (or throws) is stored as null;
        // eval's `getDefaultValue()` evaluates it on the call.
        let default_value = temporaries.stage(match crate::interpreter::statements::eval_preview_outcome(default_value, context, values)? {
            Some(value) => value,
            None => values.null()?,
        });
        let default_value_constant_name = temporaries.stage(match parameter.default_value_constant_name.as_deref() {
            Some(name) => values.string(name)?,
            None => values.null()?,
        });
        let constructor = temporaries.stage(values.null()?);
        let flags = eval_reflection_parameter_flags(parameter);
        let object = values.reflection_owner_new(
            EVAL_REFLECTION_OWNER_PARAMETER,
            &parameter.name,
            attrs,
            declaring_function,
            trait_names,
            method_names,
            property_names,
            type_value,
            default_value,
            parent_class,
            flags,
            parameter.position as u64,
            0,
            default_value_constant_name,
            class_value,
            constructor,
        )?;
        output.replace(Some(object));
        Ok(())
    })?;
    if let Err(status) = attach_parameter_default_factory(object, parameter, context, values)
        .and_then(|()| eval_reflection_attach_parameter_lazy_default(object, parameter, context, values))
    {
        let _ = eval_release_value(context, values, object);
        return Err(status);
    }
    if let Some(default) = parameter.default_value.as_ref() {
        if let Ok(identity) = values.object_identity(object) {
            let magic_scope = parameter
                .declaring_function
                .as_ref()
                .and_then(|function| function.magic_scope.as_ref())
                .map(|scope| {
                    (scope.function_name.clone(), scope.method_name.clone(), scope.class_name.clone(), scope.trait_name.clone())
                });
            context.register_reflected_parameter_default(
                identity,
                crate::context::EvalReflectedParameterDefault {
                    default: default.clone(),
                    declaring_class: parameter.declaring_class_name.clone(),
                    magic_scope,
                },
            );
        }
    }
    let rendered = eval_reflection_function_method_parameter_to_string(parameter);
    if let Err(status) = eval_reflection_store_dump(
        object, "ReflectionParameter", &rendered, context, values,
    ) {
        let _ = eval_release_value(context, values, object);
        return Err(status);
    }
    Ok(object)
}

/// Builds the lazy legacy `ReflectionParameter::getClass()` value for known named object types.
pub(super) fn eval_reflection_parameter_class_value(
    parameter: &EvalReflectionParameterMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match eval_reflection_parameter_class_name(parameter) {
        Some(class_name) => eval_reflection_lazy_class_name_result(class_name, context, values),
        None => values.null(),
    }
}

/// Returns the retained object type name used by ReflectionParameter::getClass().
pub(super) fn eval_reflection_parameter_class_name(
    parameter: &EvalReflectionParameterMetadata,
) -> Option<&str> {
    match &parameter.type_metadata.as_ref()?.kind {
        EvalReflectionParameterTypeKind::Named(named_type) if !named_type.is_builtin => {
            Some(named_type.name.as_str())
        }
        _ => None,
    }
}

/// Materializes one ReflectionParameter default using declaring class and magic scopes.
pub(super) fn eval_reflection_parameter_default_value(
    parameter: &EvalReflectionParameterMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(default) = parameter.default_value.as_ref() else {
        return values.null();
    };
    // The factory, not metadata construction, owns evaluation of a deferred tree.
    if parameter_default_is_deferred(default) {
        return values.null();
    }
    if let Some(class_name) = parameter.declaring_class_name.as_deref() {
        context.push_class_scope(class_name.to_string());
        context.push_called_class_scope(class_name.to_string());
    }
    let magic_scope = parameter
        .declaring_function
        .as_ref()
        .and_then(|function| function.magic_scope.as_ref());
    if let Some(magic_scope) = magic_scope {
        context.push_callable_magic_scope(
            &magic_scope.function_name,
            &magic_scope.method_name,
            magic_scope.class_name.as_deref(),
            magic_scope.trait_name.as_deref(),
        );
    }
    let result = eval_method_parameter_default(default, context, values);
    if magic_scope.is_some() {
        context.pop_magic_scope();
    }
    if parameter.declaring_class_name.is_some() {
        context.pop_called_class_scope();
        context.pop_class_scope();
    }
    result
}

/// Evaluates one reflected property default with its declaring class-like magic scope.
pub(super) fn eval_reflection_member_default_value(
    member: &EvalReflectionMemberMetadata,
    default: &EvalExpr,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_class_like_default_value(
        member.declaring_class_name.as_deref(),
        member.default_value_trait_origin.as_deref(),
        default,
        context,
        values,
    )
}

/// Evaluates one class-like default expression with PHP `__CLASS__` and `__TRAIT__`.
pub(super) fn eval_reflection_class_like_default_value(
    declaring_class: Option<&str>,
    trait_origin: Option<&str>,
    default: &EvalExpr,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(declaring_class) = declaring_class else {
        return eval_method_parameter_default(default, context, values);
    };
    let trait_name =
        trait_origin.or_else(|| context.has_trait(declaring_class).then_some(declaring_class));
    // `self::`/`static::` in a property default resolve against the declaring class, as they
    // do for a parameter default (`eval_reflection_parameter_default_value`); the magic scope
    // alone left `public int $x = self::M` unevaluable from reflection.
    context.push_class_scope(declaring_class.to_string());
    context.push_called_class_scope(declaring_class.to_string());
    context.push_class_like_member_magic_scope(declaring_class, trait_name);
    let result = eval_method_parameter_default(default, context, values);
    context.pop_magic_scope();
    context.pop_called_class_scope();
    context.pop_class_scope();
    result
}

/// Builds a shallow ReflectionMethod object for a parameter's declaring function metadata.
pub(super) fn eval_reflection_declaring_function_object_result(
    metadata: &EvalReflectionDeclaringFunctionMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let owner_kind = if metadata.declaring_class_name.is_some() {
        EVAL_REFLECTION_OWNER_METHOD
    } else {
        EVAL_REFLECTION_OWNER_FUNCTION
    };
    let method_modifiers = if metadata.declaring_class_name.is_some() {
        eval_reflection_method_modifiers_from_flags(metadata.flags)
    } else {
        0
    };
    eval_reflection_owner_object(
        owner_kind,
        &metadata.name,
        &metadata.attributes,
        &[],
        &[],
        &[],
        &[],
        metadata.declaring_class_name.as_deref(),
        &[],
        None,
        None,
        None,
        None,
        metadata.flags,
        metadata.required_parameter_count as u64,
        method_modifiers,
        None,
        None,
        context,
        values,
    )
}

/// Materializes one parameter ReflectionType object through the shared reflection helper.
pub(super) fn eval_reflection_type_object_result(
    type_metadata: &EvalReflectionParameterTypeMetadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match &type_metadata.kind {
        EvalReflectionParameterTypeKind::Named(named_type) => {
            eval_reflection_named_type_object_result(named_type, values)
        }
        EvalReflectionParameterTypeKind::Union(union_type) => {
            eval_reflection_union_type_object_result(union_type, values)
        }
        EvalReflectionParameterTypeKind::Intersection(intersection_type) => {
            eval_reflection_intersection_type_object_result(intersection_type, values)
        }
    }
}

/// Materializes one ReflectionNamedType object through the shared reflection helper.
pub(super) fn eval_reflection_named_type_object_result(
    type_metadata: &EvalReflectionNamedTypeMetadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_type_owner(
        EVAL_REFLECTION_OWNER_NAMED_TYPE, &type_metadata.name,
        eval_reflection_named_type_flags(type_metadata), None, values,
    )
}

/// Materializes one ReflectionUnionType object through the shared reflection helper.
pub(super) fn eval_reflection_union_type_object_result(
    type_metadata: &EvalReflectionUnionTypeMetadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_type_owner(
        EVAL_REFLECTION_OWNER_UNION_TYPE, "", eval_reflection_union_type_flags(type_metadata),
        Some(&type_metadata.types), values,
    )
}

/// Materializes one ReflectionIntersectionType object through the shared reflection helper.
pub(super) fn eval_reflection_intersection_type_object_result(
    type_metadata: &EvalReflectionIntersectionTypeMetadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_type_owner(
        EVAL_REFLECTION_OWNER_INTERSECTION_TYPE, "", 0, Some(&type_metadata.types), values,
    )
}

/// Stages the fields shared by named and composite Reflection type objects.
fn eval_reflection_type_owner(
    kind: u64,
    name: &str,
    flags: u64,
    types: Option<&[EvalReflectionNamedTypeMetadata]>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_reflection_staging(values, |temporaries, output, values| {
        let attrs = temporaries.stage(values.array_new(0)?);
        let interface_names = temporaries.stage(values.array_new(0)?);
        let trait_names = temporaries.stage(values.array_new(0)?);
        let method_names = temporaries.stage(values.array_new(0)?);
        let property_names = temporaries.stage(values.array_new(0)?);
        let types = temporaries.stage(match types {
            Some(types) => eval_reflection_named_type_object_array_result(types, values)?,
            None => values.array_new(0)?,
        });
        let property_objects = temporaries.stage(values.array_new(0)?);
        let parent_class = temporaries.stage(values.bool_value(false)?);
        let constant_value = temporaries.stage(values.null()?);
        let backing_value = temporaries.stage(values.null()?);
        let object = values.reflection_owner_new(
            kind, name, attrs, interface_names, trait_names, method_names, property_names,
            types, property_objects, parent_class, flags, 0, 0, constant_value,
            backing_value, constant_value,
        )?;
        output.replace(Some(object));
        Ok(())
    })
}

/// Exposes staged metadata and an unpublished reflector to native cleanup and collection.
fn with_reflection_staging<V: RuntimeValueOps>(
    values: &mut V,
    construct: impl FnOnce(&StagedRuntimeOwners, &RuntimeOwnerSlot, &mut V) -> Result<(), EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let temporaries = StagedRuntimeOwners::default();
    let output = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&temporaries, &output), || {
        let result = construct(&temporaries, &output, values);
        temporaries.finish(result, &output, |value| values.release(value))
    })
}

/// Builds a direct object array of populated ReflectionNamedType objects.
pub(super) fn eval_reflection_named_type_object_array_result(
    types: &[EvalReflectionNamedTypeMetadata],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::objects(values, types.len())?;
    for type_metadata in types {
        result.object(|values| eval_reflection_named_type_object_result(type_metadata, values))?;
    }
    Ok(result.finish())
}

/// Builds the `ReflectionMethod|null` value stored in ReflectionClass::__constructor.
pub(super) fn eval_reflection_constructor_object_result(
    owner_kind: u64,
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !eval_reflection_owner_uses_class_metadata(owner_kind) {
        return values.null();
    }
    let Some(member) = eval_reflection_method_metadata(class_name, "__construct", context) else {
        if !eval_reflection_class_like_exists(class_name, context) {
            if let Some(member) = eval_reflection_aot_method_metadata_with_signature_if_exists(
                class_name,
                "__construct",
                context,
                values,
            )? {
                return eval_reflection_member_object_result(
                    EVAL_REFLECTION_OWNER_METHOD,
                    "__construct",
                    &member,
                    context,
                    values,
                );
            }
        }
        return values.null();
    };
    eval_reflection_member_object_result(
        EVAL_REFLECTION_OWNER_METHOD,
        "__construct",
        &member,
        context,
        values,
    )
}

/// Materializes one eval-backed ReflectionMethod or ReflectionProperty object.
pub(super) fn eval_reflection_member_object_result(
    owner_kind: u64,
    reflected_name: &str,
    member: &EvalReflectionMemberMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut flags = eval_reflection_member_flags(
        member.visibility,
        member.is_static,
        member.is_final,
        member.is_abstract,
        member.is_readonly,
    );
    if member.default_value.is_some() || (owner_kind == EVAL_REFLECTION_OWNER_PROPERTY
        && member.declaring_class_name.as_deref().is_some_and(|class| {
            context.native_property_default_error(class, reflected_name).is_some()
        }))
    {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_HAS_DEFAULT_VALUE;
    }
    if member.is_promoted {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_PROMOTED;
    }
    if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY && (member.modifiers & 512) != 0 {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_VIRTUAL;
    }
    if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY && (member.modifiers & 4096) != 0 {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET | EVAL_REFLECTION_MEMBER_FLAG_FINAL;
    } else if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY
        && (member.modifiers & 2048) != 0
        && !member.is_readonly
    {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_PROTECTED_SET;
    }
    if member.is_dynamic {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_DYNAMIC;
    }
    if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        flags |= eval_reflection_callable_flags(&member.attributes);
    }
    let owner_modifiers = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        member.required_parameter_count as u64
    } else {
        member.modifiers
    };
    let method_modifiers = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        member.modifiers
    } else {
        0
    };
    eval_reflection_owner_object(
        owner_kind,
        reflected_name,
        &member.attributes,
        &[],
        &[],
        &[],
        &[],
        member.declaring_class_name.as_deref(),
        &member.parameters,
        member.type_metadata.as_ref(),
        member.settable_type_metadata.as_ref(),
        member.default_value.as_ref(),
        member.default_value_trait_origin.as_deref(),
        flags,
        owner_modifiers,
        method_modifiers,
        None,
        None,
        context,
        values,
    )
}

/// Builds a direct object array of ReflectionMethod or ReflectionProperty objects for a
/// ReflectionClass.
pub(super) fn eval_reflection_member_object_array_result(
    owner_kind: u64,
    class_name: &str,
    names: &[String],
    filter: Option<u64>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::objects(values, names.len())?;
    for name in names {
        let Some(member) = eval_reflection_member_metadata(owner_kind, class_name, name, context)
        else {
            continue;
        };
        if !eval_reflection_member_matches_filter(&member, filter) {
            continue;
        }
        result.object(|values| eval_reflection_member_object_result(owner_kind, name, &member, context, values))?;
    }
    Ok(result.finish())
}

/// Builds a direct object array of AOT ReflectionMethod or ReflectionProperty objects for a
/// class.
pub(super) fn eval_reflection_aot_member_object_array_result(
    owner_kind: u64,
    class_name: &str,
    names: &[String],
    filter: Option<u64>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::objects(values, names.len())?;
    for name in names {
        let member = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
            eval_reflection_aot_method_metadata_with_signature_if_exists(
                class_name, name, context, result.values(),
            )?
        } else {
            eval_reflection_native_interface_property_requirement(class_name, name, context)
                .map(|(declaring_class, property)| {
                    eval_reflection_interface_property_metadata(declaring_class, &property)
                })
                .or(eval_reflection_aot_property_metadata_if_exists(
                    class_name, name, context, result.values(),
                )?)
        };
        let Some(member) = member else {
            continue;
        };
        if !eval_reflection_member_matches_filter(&member, filter) {
            continue;
        }
        // `names` comes from the generated AOT table, which records each method's DECLARED
        // spelling. Lowercasing it here made `ReflectionMethod::getName()` inside `eval()`
        // answer `match` for a method written `Match`, disagreeing with the same program's AOT
        // reflection and with PHP (issue #571). Property names were never keyed
        // case-insensitively, so they were already passed through.
        result.object(|values| eval_reflection_member_object_result(owner_kind, name, &member, context, values))?;
    }
    Ok(result.finish())
}
