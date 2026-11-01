//! Purpose:
//! Materializes common Reflection owner fields and nested member/type objects.
//!
//! Called from:
//! - Reflection owner-specific constructors after resolving metadata.
//!
//! Key details:
//! - Temporary arrays and object handles are transferred through runtime ownership APIs.
//! - Full and shallow class owners avoid recursive metadata expansion.

use super::*;

/// Stores owned metadata cells, releasing the builder's key/value references.
/// Array writes borrow the key and retain the value; the receiver is returned unchanged.
pub(super) fn eval_reflection_array_set_owned(
    array: RuntimeCellHandle,
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = values.array_set(array, key, value);
    let key_release = values.release(key);
    let value_release = values.release(value);
    let result = result.and_then(|array| {
        key_release?;
        value_release?;
        Ok(array)
    });
    if result.is_err() {
        let _ = values.release(array);
    }
    result
}

/// Materializes one Reflection owner object and transfers the temporary attribute array.
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
    let include_class_members = !eval_reflection_owner_uses_class_metadata(owner_kind);
    eval_reflection_owner_object_with_members(
        owner_kind,
        reflected_name,
        attributes,
        interface_names,
        trait_names,
        method_names,
        property_names,
        parent_class_name,
        parameter_metadata,
        type_metadata,
        settable_type_metadata,
        default_value,
        default_value_trait_origin,
        flags,
        modifiers,
        method_modifiers,
        constant_value,
        backing_value,
        include_class_members,
        context,
        values,
    )
}

/// Materializes one Reflection owner object with optional nested class member objects.
pub(super) fn eval_reflection_owner_object_with_members(
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
    include_class_members: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = eval_reflection_attribute_array_result(
        attributes,
        eval_reflection_attribute_target(owner_kind),
        context,
        values,
    )?;
    let interface_names_array = eval_reflection_string_array_result(interface_names, values)?;
    let trait_names_array = eval_reflection_string_array_result(trait_names, values)?;
    let method_names_array = eval_reflection_string_array_result(method_names, values)?;
    let property_names_array = eval_reflection_string_array_result(property_names, values)?;
    let class_metadata_owner = eval_reflection_owner_uses_class_metadata(owner_kind);
    let is_eval_class = class_metadata_owner
        && eval_reflection_class_like_exists(reflected_name, context);
    let method_objects = if class_metadata_owner && include_class_members {
        if is_eval_class || eval_reflection_is_builtin_closure(reflected_name, context) {
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
    let property_objects = if class_metadata_owner && include_class_members {
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
        match default_value {
            Some(default) => eval_reflection_class_like_default_value(
                parent_class_name,
                default_value_trait_origin,
                default,
                context,
                values,
            )?,
            None => values.null()?,
        }
    } else {
        values.array_new(0)?
    };
    let parent_class = eval_reflection_related_class_result(
        owner_kind,
        parent_class_name,
        include_class_members,
        context,
        values,
    )?;
    let constructor = eval_reflection_constructor_object_result(
        owner_kind,
        reflected_name,
        include_class_members,
        context,
        values,
    )?;
    let (constant_value_cell, release_constant_value) = if owner_kind
        == EVAL_REFLECTION_OWNER_PROPERTY
    {
        match settable_type_metadata {
            Some(type_metadata) => (
                eval_reflection_type_object_result(type_metadata, values)?,
                true,
            ),
            None => (values.null()?, true),
        }
    } else {
        match constant_value {
            Some(value) => (value, false),
            None => (values.null()?, true),
        }
    };
    let (backing_value_cell, release_backing_value) = match backing_value {
        Some(value) => (value, false),
        None => (values.null()?, true),
    };
    if crate::eval_trace::enabled()
        && matches!(owner_kind, EVAL_REFLECTION_OWNER_CLASS | EVAL_REFLECTION_OWNER_OBJECT)
    {
        eprintln!(
            "[elephc-eval-trace] phase=reflection_owner_materialize name={reflected_name:?} flags={flags:#x}"
        );
    }
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
    if matches!(
        owner_kind,
        EVAL_REFLECTION_OWNER_FUNCTION | EVAL_REFLECTION_OWNER_METHOD
    ) {
        let has_return_type = values.bool_value(type_metadata.is_some())?;
        let return_type = match type_metadata {
            Some(type_metadata) => eval_reflection_type_object_result(type_metadata, values)?,
            None => values.null()?,
        };
        let declaring_class = if owner_kind == EVAL_REFLECTION_OWNER_FUNCTION {
            "ReflectionFunction"
        } else {
            "ReflectionMethod"
        };
        let configured = eval_reflection_with_declaring_class_scope(
            declaring_class,
            context,
            |_| -> Result<(), EvalStatus> {
                values.property_set(object, "__has_return_type", has_return_type)?;
                values.property_set(object, "__type", return_type)
            },
        );
        let has_return_type_release = values.release(has_return_type);
        let return_type_release = values.release(return_type);
        configured?;
        has_return_type_release?;
        return_type_release?;
    }
    // PHP's public `$method->class` / `$property->class` is the DECLARING class, read from
    // `__class`. The compiled constructor fills it; an object built here never ran that
    // constructor, so it has to be written, or every eval-built member reports an empty class.
    if matches!(
        owner_kind,
        EVAL_REFLECTION_OWNER_METHOD | EVAL_REFLECTION_OWNER_PROPERTY
    ) {
        if let Some(declaring_class) = parent_class_name {
            let class_value = values.string(declaring_class.trim_start_matches('\\'))?;
            let owner_class = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
                "ReflectionMethod"
            } else {
                "ReflectionProperty"
            };
            let configured = eval_reflection_with_declaring_class_scope(
                owner_class,
                context,
                |_| -> Result<(), EvalStatus> { values.property_set(object, "__class", class_value) },
            );
            let class_value_release = values.release(class_value);
            configured?;
            class_value_release?;
        }
    }
    if owner_kind == EVAL_REFLECTION_OWNER_CLASS_CONSTANT {
        let has_type = values.bool_value(type_metadata.is_some())?;
        let type_value = match type_metadata {
            Some(type_metadata) => eval_reflection_type_object_result(type_metadata, values)?,
            None => values.null()?,
        };
        let configured = eval_reflection_with_declaring_class_scope(
            "ReflectionClassConstant",
            context,
            |_| -> Result<(), EvalStatus> {
                values.property_set(object, "__has_type", has_type)?;
                values.property_set(object, "__type", type_value)
            },
        );
        let has_type_release = values.release(has_type);
        let type_value_release = values.release(type_value);
        configured?;
        has_type_release?;
        type_value_release?;
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
    }
    if matches!(
        owner_kind,
        EVAL_REFLECTION_OWNER_METHOD | EVAL_REFLECTION_OWNER_FUNCTION
    ) {
        eval_reflection_store_native_source_slots(object, owner_kind, context, values)?;
    }
    if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY {
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
    values.release(attrs)?;
    values.release(interface_names_array)?;
    values.release(trait_names_array)?;
    values.release(method_names_array)?;
    values.release(property_names_array)?;
    values.release(method_objects)?;
    values.release(property_objects)?;
    values.release(parent_class)?;
    values.release(constructor)?;
    if release_constant_value {
        values.release(constant_value_cell)?;
    }
    if release_backing_value {
        values.release(backing_value_cell)?;
    }
    if matches!(owner_kind, EVAL_REFLECTION_OWNER_FUNCTION | EVAL_REFLECTION_OWNER_METHOD) {
        let identity = values.object_identity(object)?;
        if let Some(target) = eval_reflection_function_method_target(identity, context, values)? {
            let rendered = eval_reflection_function_method_to_string(&target);
            let owner = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
                "ReflectionMethod"
            } else { "ReflectionFunction" };
            if let Err(status) = eval_reflection_store_dump(object, owner, &rendered, context, values) {
                let _ = eval_release_value(context, values, object);
                return Err(status);
            }
        }
    }
    Ok(object)
}

/// Stores a dump in the slot read by native casts, releasing the temporary on both paths.
fn eval_reflection_store_dump(
    object: RuntimeCellHandle, owner: &str, rendered: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let text = values.string(rendered)?;
    let result = eval_reflection_with_declaring_class_scope(owner, context, |_| {
        values.property_set(object, "__string", text)
    });
    let released = values.release(text);
    result.and(released)
}

/// Writes a ReflectionFunction/ReflectionMethod's source file and lines into its native slots.
///
/// Compiled code calls `getFileName()`, `getStartLine()` and `getEndLine()` as plain slot
/// getters, and the compiled constructor is what fills those slots. An object built here never
/// ran that constructor, so compiled code read `""` and `0` for every reflector eval built --
/// which is every `new ReflectionMethod(...)` a frame with an eval context makes. The eval
/// dispatcher also defers to these slots for a method an AOT class declares, so a reflector
/// built in eval for such a method answered from them too. The values are the ones the eval
/// dispatcher itself answers with (see `eval_reflection_source_location_result`).
pub(super) fn eval_reflection_store_native_source_slots(
    object: RuntimeCellHandle,
    owner_kind: u64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let identity = values.object_identity(object)?;
    let Some(target) = eval_reflection_function_method_target(identity, context, values)? else {
        return Ok(());
    };
    let (source_file, source_location) = eval_reflection_function_method_source_location(&target, context);
    let Some(source_location) = source_location else {
        return Ok(());
    };
    let file = match source_file {
        Some(source_file) => source_file.to_string(),
        None => context.eval_file_magic(),
    };
    let owner_class = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        "ReflectionMethod"
    } else {
        "ReflectionFunction"
    };
    eval_reflection_write_source_slots(object, owner_class, &file, source_location, context, values)
}

/// Stores a reflected declaration's file and line range into the owner's source slots.
pub(super) fn eval_reflection_write_source_slots(
    object: RuntimeCellHandle,
    owner_class: &str,
    file: &str,
    source_location: EvalSourceLocation,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let file_value = values.string(file)?;
    let start_value = values.int(source_location.start_line())?;
    let end_value = values.int(source_location.end_line())?;
    let configured = eval_reflection_with_declaring_class_scope(
        owner_class,
        context,
        |_| -> Result<(), EvalStatus> {
            values.property_set(object, "__file_name", file_value)?;
            values.property_set(object, "__start_line", start_value)?;
            values.property_set(object, "__end_line", end_value)
        },
    );
    let file_release = values.release(file_value);
    let start_release = values.release(start_value);
    let end_release = values.release(end_value);
    configured?;
    file_release?;
    start_release?;
    end_release
}

/// Builds the `ReflectionClass|false` value stored in parent or declaring-class slots.
pub(super) fn eval_reflection_related_class_result(
    owner_kind: u64,
    related_class_name: Option<&str>,
    _include_class_members: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(related_class_name) = related_class_name else {
        return values.bool_value(false);
    };
    if matches!(
        owner_kind,
        EVAL_REFLECTION_OWNER_CLASS
            | EVAL_REFLECTION_OWNER_OBJECT
            | EVAL_REFLECTION_OWNER_ENUM
            | EVAL_REFLECTION_OWNER_METHOD
            | EVAL_REFLECTION_OWNER_PROPERTY
            | EVAL_REFLECTION_OWNER_CLASS_CONSTANT
            | EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE
            | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE
    ) {
        return eval_reflection_shallow_class_object_result(related_class_name, context, values);
    }
    values.bool_value(false)
}

/// Builds a full `ReflectionClass` object for parent-class metadata.
pub(super) fn eval_reflection_full_class_object_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_full_class_object_result_with_relation(class_name, None, context, values)
}

/// Builds a full `ReflectionClass` object, optionally for a declared relation member.
///
/// `getInterfaces()`/`getTraits()` read their names out of a compiled class's own metadata, so
/// each name is a class-like the program declares. The dynamic existence probes can still answer
/// false for one of them: a compiled interface whose source file no include ever reached has an
/// activation cell that no startup path raises, while `interface_exists()` consults exactly that
/// cell. Returning a non-object for such a name made Symfony's `ReflectionClassResource` recurse
/// with `false`/null and die on its `\ReflectionClass` parameter. A relation kind therefore lets
/// the compiled reflection metadata stand in for the missing declaration state.
fn eval_reflection_full_class_object_result_with_relation(
    class_name: &str,
    relation_kind: Option<&str>,
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
        let resolved = match eval_reflection_aot_class_flags(class_name, values)? {
            Some(pair) => Some(pair),
            None => compiled_relation_flags(class_name, relation_kind, values)?,
        };
        let Some((flags, modifiers)) = resolved else {
            return values.bool_value(false);
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
            EVAL_REFLECTION_OWNER_CLASS,
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

/// Builds a shallow `ReflectionClass` object for member declaring-class metadata.
pub(super) fn eval_reflection_shallow_class_object_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // `Closure` is neither eval nor AOT metadata; `Closure::__invoke()` is the member that
    // declares it (`new ReflectionMethod($closure, '__invoke')`).
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
        let interface_names = eval_reflection_aot_class_interface_names(class_name, values)?;
        let trait_names = eval_reflection_aot_class_trait_names(class_name, values)?;
        let attributes = context.native_class_attributes(class_name);
        return eval_reflection_owner_object_with_members(
            EVAL_REFLECTION_OWNER_CLASS,
            class_name.trim_start_matches('\\'),
            &attributes,
            &interface_names,
            &trait_names,
            &[],
            &[],
            None,
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
            false,
            context,
            values,
        );
    };
    let interface_names =
        eval_reflection_eval_metadata_interface_names(&metadata, context, values)?;
    let flags = eval_reflection_eval_metadata_flags(&metadata, context, values)?;
    eval_reflection_owner_object_with_members(
        EVAL_REFLECTION_OWNER_CLASS,
        &metadata.resolved_name,
        &metadata.attributes,
        &interface_names,
        &metadata.trait_names,
        &[],
        &[],
        None,
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
        false,
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
    let mut result = values.string_array_new(names.len())?;
    for name in names {
        result = values.string_array_push(result, name)?;
    }
    Ok(result)
}

/// Builds a string-keyed PHP associative array from owned string pairs.
pub(super) fn eval_reflection_string_assoc_result(
    pairs: Vec<(String, String)>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(pairs.len())?;
    for (key, value) in pairs {
        let key = values.string(&key)?;
        let value = values.string(&value)?;
        result = eval_reflection_array_set_owned(result, key, value, values)?;
    }
    Ok(result)
}

/// Builds a name-keyed PHP array of full ReflectionClass objects.
pub(super) fn eval_reflection_class_object_map_result(
    names: &[String],
    relation_kind: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(names.len())?;
    for name in names {
        let key = values.string(name)?;
        let object = eval_reflection_full_class_object_result_with_relation(
            name,
            relation_kind,
            context,
            values,
        )?;
        result = eval_reflection_array_set_owned(result, key, object, values)?;
    }
    Ok(result)
}

/// Synthesizes reflection flags for a declared interface or trait of a compiled class.
///
/// Only a relation lookup supplies a kind: the name came from the compiled class's own
/// `interfaces`/`traits` metadata, so it is a class-like the program declares even when the
/// dynamic existence probes answer false for it (an activation cell no startup path raised).
/// A name the compiled reflection metadata does not know either yields `None`, so a bogus entry
/// still cannot fabricate a reflector.
fn compiled_relation_flags(
    class_name: &str,
    relation_kind: Option<&str>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(u64, u64)>, EvalStatus> {
    let Some(relation_kind) = relation_kind else {
        return Ok(None);
    };
    let runtime_class_name = class_name.trim_start_matches('\\');
    let row_flags = values.reflection_class_flags(runtime_class_name)?;
    let has_source = values
        .reflection_class_source_file(runtime_class_name)?
        .is_some();
    if row_flags.is_none() && !has_source {
        return Ok(None);
    }
    let mut flags = row_flags.unwrap_or(0) | EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED;
    match relation_kind {
        "interfaces" => flags |= EVAL_REFLECTION_CLASS_FLAG_INTERFACE,
        "traits" => flags |= EVAL_REFLECTION_CLASS_FLAG_TRAIT,
        _ => return Ok(None),
    }
    let modifiers = eval_reflection_class_modifiers(
        flags & EVAL_REFLECTION_CLASS_FLAG_FINAL != 0,
        flags & EVAL_REFLECTION_CLASS_FLAG_ABSTRACT != 0,
        flags & EVAL_REFLECTION_CLASS_FLAG_READONLY != 0,
        flags & EVAL_REFLECTION_CLASS_FLAG_ENUM != 0,
    );
    Ok(Some((flags, modifiers)))
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
        EVAL_REFLECTION_OWNER_CLASS | EVAL_REFLECTION_OWNER_OBJECT
    )
}

/// Builds an indexed array of populated ReflectionParameter objects.
pub(super) fn eval_reflection_parameter_object_array_result(
    parameters: &[EvalReflectionParameterMetadata],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(parameters.len())?;
    for parameter in parameters {
        let parameter_object = eval_reflection_parameter_object_result(parameter, context, values)?;
        let key = values.int(parameter.position as i64)?;
        result = eval_reflection_array_set_owned(result, key, parameter_object, values)?;
    }
    Ok(result)
}

/// Materializes one ReflectionParameter object through the shared reflection helper.
pub(super) fn eval_reflection_parameter_object_result(
    parameter: &EvalReflectionParameterMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = eval_reflection_attribute_array_result(
        &parameter.attributes,
        EVAL_REFLECTION_ATTRIBUTE_TARGET_PARAMETER,
        context,
        values,
    )?;
    let declaring_function = match parameter.declaring_function.as_ref() {
        Some(metadata) => {
            eval_reflection_declaring_function_object_result(metadata, context, values)?
        }
        None => values.null()?,
    };
    let trait_names = values.array_new(0)?;
    let method_names = values.array_new(0)?;
    let property_names = values.array_new(0)?;
    let method_objects = values.array_new(0)?;
    let parent_class = match parameter.declaring_class_name.as_deref() {
        Some(declaring_class_name) => {
            eval_reflection_shallow_class_object_result(declaring_class_name, context, values)?
        }
        None => values.null()?,
    };
    let type_value = match parameter.type_metadata.as_ref() {
        Some(type_metadata) => eval_reflection_type_object_result(type_metadata, values)?,
        None => values.null()?,
    };
    let class_value = eval_reflection_parameter_class_value(parameter, context, values)?;
    let default_value = eval_reflection_parameter_default_value(parameter, context, values)?;
    let default_value_constant_name = match parameter.default_value_constant_name.as_deref() {
        Some(name) => values.string(name)?,
        None => values.null()?,
    };
    let constructor = values.null()?;
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
    values.release(attrs)?;
    values.release(declaring_function)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(method_objects)?;
    values.release(type_value)?;
    values.release(default_value)?;
    values.release(parent_class)?;
    values.release(default_value_constant_name)?;
    values.release(class_value)?;
    values.release(constructor)?;
    if let Err(status) = attach_parameter_default_factory(object, parameter, context, values) {
        let _ = eval_release_value(context, values, object);
        return Err(status);
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

/// Materializes the legacy ReflectionParameter::getClass() value for known named object types.
pub(super) fn eval_reflection_parameter_class_value(
    parameter: &EvalReflectionParameterMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match eval_reflection_parameter_class_name(parameter) {
        Some(class_name) => eval_reflection_shallow_class_object_result(class_name, context, values),
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
    // `self::` in a default value is the DECLARING class, and php evaluates the default under
    // that class scope. Only the magic-constant scope was pushed, so `self::CREATED` in
    // `MetadataBag::$meta` resolved against whatever class happened to be calling
    // `ReflectionClass::getDefaultProperties()` -- Symfony's DebugClassLoader -- and failed with
    // "Undefined constant DebugClassLoader::CREATED".
    context.push_class_like_member_magic_scope(declaring_class, trait_name);
    context.push_class_scope(declaring_class.to_string());
    context.push_called_class_scope(declaring_class.to_string());
    let result = eval_method_parameter_default(default, context, values);
    context.pop_called_class_scope();
    context.pop_class_scope();
    context.pop_magic_scope();
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
    let attrs = values.array_new(0)?;
    let interface_names = values.array_new(0)?;
    let trait_names = values.array_new(0)?;
    let method_names = values.array_new(0)?;
    let property_names = values.array_new(0)?;
    let method_objects = values.array_new(0)?;
    let property_objects = values.array_new(0)?;
    let parent_class = values.bool_value(false)?;
    let constant_value = values.null()?;
    let backing_value = values.null()?;
    let flags = eval_reflection_named_type_flags(type_metadata);
    let object = values.reflection_owner_new(
        EVAL_REFLECTION_OWNER_NAMED_TYPE,
        &type_metadata.name,
        attrs,
        interface_names,
        trait_names,
        method_names,
        property_names,
        method_objects,
        property_objects,
        parent_class,
        flags,
        0,
        0,
        constant_value,
        backing_value,
        constant_value,
    )?;
    values.release(attrs)?;
    values.release(interface_names)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(method_objects)?;
    values.release(property_objects)?;
    values.release(parent_class)?;
    values.release(constant_value)?;
    values.release(backing_value)?;
    Ok(object)
}

/// Materializes one ReflectionUnionType object through the shared reflection helper.
pub(super) fn eval_reflection_union_type_object_result(
    type_metadata: &EvalReflectionUnionTypeMetadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = values.array_new(0)?;
    let interface_names = values.array_new(0)?;
    let trait_names = values.array_new(0)?;
    let method_names = values.array_new(0)?;
    let property_names = values.array_new(0)?;
    let types = eval_reflection_named_type_object_array_result(&type_metadata.types, values)?;
    let property_objects = values.array_new(0)?;
    let parent_class = values.bool_value(false)?;
    let constant_value = values.null()?;
    let backing_value = values.null()?;
    let flags = eval_reflection_union_type_flags(type_metadata);
    let object = values.reflection_owner_new(
        EVAL_REFLECTION_OWNER_UNION_TYPE,
        "",
        attrs,
        interface_names,
        trait_names,
        method_names,
        property_names,
        types,
        property_objects,
        parent_class,
        flags,
        0,
        0,
        constant_value,
        backing_value,
        constant_value,
    )?;
    values.release(attrs)?;
    values.release(interface_names)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(types)?;
    values.release(property_objects)?;
    values.release(parent_class)?;
    values.release(constant_value)?;
    values.release(backing_value)?;
    Ok(object)
}

/// Materializes one ReflectionIntersectionType object through the shared reflection helper.
pub(super) fn eval_reflection_intersection_type_object_result(
    type_metadata: &EvalReflectionIntersectionTypeMetadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = values.array_new(0)?;
    let interface_names = values.array_new(0)?;
    let trait_names = values.array_new(0)?;
    let method_names = values.array_new(0)?;
    let property_names = values.array_new(0)?;
    let types = eval_reflection_named_type_object_array_result(&type_metadata.types, values)?;
    let property_objects = values.array_new(0)?;
    let parent_class = values.bool_value(false)?;
    let constant_value = values.null()?;
    let backing_value = values.null()?;
    let object = values.reflection_owner_new(
        EVAL_REFLECTION_OWNER_INTERSECTION_TYPE,
        "",
        attrs,
        interface_names,
        trait_names,
        method_names,
        property_names,
        types,
        property_objects,
        parent_class,
        0,
        0,
        0,
        constant_value,
        backing_value,
        constant_value,
    )?;
    values.release(attrs)?;
    values.release(interface_names)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(types)?;
    values.release(property_objects)?;
    values.release(parent_class)?;
    values.release(constant_value)?;
    values.release(backing_value)?;
    Ok(object)
}

/// Builds an indexed array of populated ReflectionNamedType objects.
pub(super) fn eval_reflection_named_type_object_array_result(
    types: &[EvalReflectionNamedTypeMetadata],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(types.len())?;
    for (position, type_metadata) in types.iter().enumerate() {
        let type_object = eval_reflection_named_type_object_result(type_metadata, values)?;
        let key = values.int(position as i64)?;
        result = eval_reflection_array_set_owned(result, key, type_object, values)?;
    }
    Ok(result)
}

/// Builds the `ReflectionMethod|null` value stored in ReflectionClass::__constructor.
pub(super) fn eval_reflection_constructor_object_result(
    owner_kind: u64,
    class_name: &str,
    include_class_members: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !eval_reflection_owner_uses_class_metadata(owner_kind) || !include_class_members {
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
    let type_metadata = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        member.return_type_metadata.as_ref()
    } else {
        member.type_metadata.as_ref()
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
        type_metadata,
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

/// Builds an indexed array of ReflectionMethod or ReflectionProperty objects for a ReflectionClass.
pub(super) fn eval_reflection_member_object_array_result(
    owner_kind: u64,
    class_name: &str,
    names: &[String],
    filter: Option<u64>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(names.len())?;
    let mut index = 0;
    for name in names {
        let Some(member) = eval_reflection_member_metadata(owner_kind, class_name, name, context)
        else {
            continue;
        };
        if !eval_reflection_member_matches_filter(&member, filter) {
            continue;
        }
        let member_object =
            eval_reflection_member_object_result(owner_kind, name, &member, context, values)?;
        let key = values.int(index)?;
        result = eval_reflection_array_set_owned(result, key, member_object, values)?;
        index += 1;
    }
    Ok(result)
}

/// Builds an indexed array of AOT ReflectionMethod or ReflectionProperty objects for a class.
pub(super) fn eval_reflection_aot_member_object_array_result(
    owner_kind: u64,
    class_name: &str,
    names: &[String],
    filter: Option<u64>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(names.len())?;
    let mut index = 0;
    for name in names {
        let member = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
            eval_reflection_aot_method_metadata_with_signature_if_exists(
                class_name, name, context, values,
            )?
        } else {
            eval_reflection_native_interface_property_requirement(class_name, name, context)
                .map(|(declaring_class, property)| {
                    eval_reflection_interface_property_metadata(declaring_class, &property)
                })
                .or(eval_reflection_aot_property_metadata_if_exists(
                    class_name, name, context, values,
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
        let member_object =
            eval_reflection_member_object_result(owner_kind, name, &member, context, values)?;
        let key = values.int(index)?;
        result = eval_reflection_array_set_owned(result, key, member_object, values)?;
        index += 1;
    }
    Ok(result)
}
