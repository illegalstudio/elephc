//! Purpose:
//! Constructs Reflection owners for eval and AOT class-like targets.
//! It also centralizes instantiability and constructor visibility metadata.
//!
//! Called from:
//! - `crate::interpreter::reflection` for ReflectionClass/Object/Enum construction.
//!
//! Key details:
//! - Class-like flags merge eval declarations with focused AOT runtime metadata.

use super::*;

/// Builds an eval-backed `ReflectionClass` object when the reflected class-like exists in eval.
pub(super) fn eval_reflection_class_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("class_name")], evaluated_args)?;
    let class_name = eval_reflection_class_target_name(args[0], context, values)?;
    let reflected_name = context
        .resolve_class_like_name(&class_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    eval_reflection_class_owner_object_result(
        EVAL_REFLECTION_OWNER_CLASS,
        &reflected_name,
        context,
        values,
    )
}

/// Builds an eval-backed `ReflectionObject` from an object instance.
pub(super) fn eval_reflection_object_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("object")], evaluated_args)?;
    let tag = values.type_tag(args[0])?;
    if tag != EVAL_TAG_OBJECT {
        return super::class_lookup::eval_throw_type_error(
            &format!(
                "ReflectionObject::__construct(): Argument #1 ($object) must be of type object, {} given",
                eval_reflection_type_error_type_name(tag)
            ),
            context,
            values,
        );
    }
    let reflected_name = eval_reflection_object_class_name(args[0], context, values)?;
    let Some(object) = eval_reflection_class_owner_object_result(
        EVAL_REFLECTION_OWNER_OBJECT,
        &reflected_name,
        context,
        values,
    )?
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_reflection_with_declaring_class_scope("ReflectionObject", context, |_| {
        values.property_set(object, "__object", args[0])
    })?;
    Ok(Some(object))
}

/// Materializes class metadata for `ReflectionClass` or `ReflectionObject`.
pub(super) fn eval_reflection_class_owner_object_result(
    owner_kind: u64,
    reflected_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(metadata) = eval_reflection_class_like_attributes(reflected_name, context) else {
        if reflected_name
            .trim_start_matches('\\')
            .eq_ignore_ascii_case("Closure")
        {
            return eval_reflection_builtin_closure_class_object_result(
                owner_kind, context, values,
            )
            .map(Some);
        }
        let Some((flags, modifiers)) = eval_reflection_aot_class_flags(reflected_name, values)?
        else {
            return Ok(None);
        };
        let method_names = eval_reflection_aot_member_names(
            EVAL_REFLECTION_OWNER_METHOD,
            reflected_name,
            values,
        )?;
        let property_names = eval_reflection_aot_member_names(
            EVAL_REFLECTION_OWNER_PROPERTY,
            reflected_name,
            values,
        )?;
        let interface_names = eval_reflection_aot_class_interface_names(reflected_name, values)?;
        let trait_names = eval_reflection_aot_class_trait_names(reflected_name, values)?;
        let parent_class_name = eval_reflection_aot_parent_class_name(reflected_name, values)?;
        let attributes = context.native_class_attributes(reflected_name);
        return eval_reflection_owner_object(
            owner_kind,
            reflected_name,
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
        )
        .map(Some);
    };
    let interface_names =
        eval_reflection_eval_metadata_interface_names(&metadata, context, values)?;
    let flags = eval_reflection_eval_metadata_flags(&metadata, context, values)?;
    eval_reflection_owner_object(
        owner_kind,
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
    .map(Some)
}

/// Builds the minimal ReflectionClass metadata object for PHP's builtin Closure class.
pub(super) fn eval_reflection_builtin_closure_class_object_result(
    owner_kind: u64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let flags = EVAL_REFLECTION_CLASS_FLAG_FINAL | EVAL_REFLECTION_CLASS_FLAG_INTERNAL;
    let modifiers = eval_reflection_class_modifiers(true, false, false, false);
    eval_reflection_owner_object(
        owner_kind,
        "Closure",
        &[],
        &[],
        &[],
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
        context,
        values,
    )
}

/// Builds an eval-backed `ReflectionEnum` object for a declared enum.
pub(super) fn eval_reflection_enum_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("class_name")], evaluated_args)?;
    let class_name = eval_reflection_class_target_name(args[0], context, values)?;
    let reflected_name = context
        .resolve_enum_name(&class_name)
        .or_else(|| context.resolve_class_like_name(&class_name))
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    if context.enum_decl(&reflected_name).is_some() {
        return eval_reflection_enum_object_result(&reflected_name, context, values).map(Some);
    }
    if eval_reflection_class_like_exists(&reflected_name, context) {
        return eval_throw_reflection_exception(
            &format!("Class \"{}\" is not an enum", reflected_name),
            context,
            values,
        );
    }
    if eval_reflection_class_like_or_runtime_exists(&reflected_name, context, values)? {
        return Ok(None);
    }
    eval_throw_reflection_exception(
        &format!("Class \"{}\" does not exist", reflected_name),
        context,
        values,
    )
}

/// Materializes one eval-backed `ReflectionEnum` owner object.
pub(super) fn eval_reflection_enum_object_result(
    enum_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let reflected_name = context
        .resolve_enum_name(enum_name)
        .unwrap_or_else(|| enum_name.trim_start_matches('\\').to_string());
    let Some(metadata) = eval_reflection_class_like_attributes(&reflected_name, context) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if !context.has_enum(&metadata.resolved_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_ENUM,
        &metadata.resolved_name,
        &metadata.attributes,
        &metadata.interface_names,
        &metadata.trait_names,
        &metadata.method_names,
        &metadata.property_names,
        metadata.parent_class_name.as_deref(),
        &[],
        None,
        None,
        None,
        None,
        metadata.flags,
        metadata.modifiers,
        0,
        None,
        None,
        context,
        values,
    )
}

/// Resolves a ReflectionClass constructor target from a class-name string or object.
pub(super) fn eval_reflection_class_target_name(
    target: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if values.type_tag(target)? == EVAL_TAG_OBJECT {
        return eval_reflection_object_class_name(target, context, values);
    }
    eval_reflection_string_arg(target, values)
}

/// Returns generated/AOT class flags for synthetic ReflectionClass fallback objects.
pub(super) fn eval_reflection_aot_class_flags(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(u64, u64)>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let is_class = values.class_exists(runtime_class_name)?;
    let is_interface = eval_runtime_interface_exists(runtime_class_name, values)?;
    let is_trait = values.trait_exists(runtime_class_name)?;
    let is_enum = values.enum_exists(runtime_class_name)?;
    if !(is_class || is_interface || is_trait || is_enum) {
        return Ok(None);
    }
    let mut flags = 0;
    if eval_reflection_class_like_is_internal(runtime_class_name) {
        flags |= EVAL_REFLECTION_CLASS_FLAG_INTERNAL;
    } else {
        flags |= EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED;
    }
    let mut class_flags = values.reflection_class_flags(runtime_class_name)?.unwrap_or(0);
    if is_enum {
        class_flags &= !EVAL_REFLECTION_CLASS_FLAG_READONLY;
    }
    flags |= class_flags
        & (EVAL_REFLECTION_CLASS_FLAG_FINAL
            | EVAL_REFLECTION_CLASS_FLAG_ABSTRACT
            | EVAL_REFLECTION_CLASS_FLAG_READONLY);
    if is_interface {
        flags |= EVAL_REFLECTION_CLASS_FLAG_INTERFACE;
    }
    if is_trait {
        flags |= EVAL_REFLECTION_CLASS_FLAG_TRAIT;
    }
    if is_enum {
        flags |= EVAL_REFLECTION_CLASS_FLAG_FINAL | EVAL_REFLECTION_CLASS_FLAG_ENUM;
    }
    if eval_reflection_builtin_class_is_iterable(runtime_class_name) {
        flags |= EVAL_REFLECTION_CLASS_FLAG_ITERABLE;
    }
    if is_class && !is_enum && flags & EVAL_REFLECTION_CLASS_FLAG_ABSTRACT == 0 {
        if eval_reflection_aot_lifecycle_method_allows_public_reflection(
            runtime_class_name,
            "__construct",
            values,
        )? {
            flags |= EVAL_REFLECTION_CLASS_FLAG_INSTANTIABLE;
        }
        if eval_reflection_aot_lifecycle_method_allows_public_reflection(
            runtime_class_name,
            "__clone",
            values,
        )? {
            flags |= EVAL_REFLECTION_CLASS_FLAG_CLONEABLE;
        }
    }
    let modifiers = eval_reflection_class_modifiers(
        flags & EVAL_REFLECTION_CLASS_FLAG_FINAL != 0,
        flags & EVAL_REFLECTION_CLASS_FLAG_ABSTRACT != 0,
        flags & EVAL_REFLECTION_CLASS_FLAG_READONLY != 0,
        is_enum,
    );
    Ok(Some((flags, modifiers)))
}

/// Returns AOT class modifiers relevant to validating an eval `extends` clause.
pub(in crate::interpreter) fn eval_reflection_aot_class_inheritance_modifiers(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(bool, bool)>, EvalStatus> {
    let Some((flags, _)) = eval_reflection_aot_class_flags(class_name, values)? else {
        return Ok(None);
    };
    if flags
        & (EVAL_REFLECTION_CLASS_FLAG_INTERFACE
            | EVAL_REFLECTION_CLASS_FLAG_TRAIT
            | EVAL_REFLECTION_CLASS_FLAG_ENUM)
        != 0
    {
        return Ok(None);
    }
    Ok(Some((
        flags & EVAL_REFLECTION_CLASS_FLAG_FINAL != 0,
        flags & EVAL_REFLECTION_CLASS_FLAG_READONLY != 0,
    )))
}

/// Returns the catchable error for generated/AOT allocation without constructor, if any.
pub(in crate::interpreter) fn eval_reflection_aot_class_without_constructor_error(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some((flags, _)) = eval_reflection_aot_class_flags(class_name, values)? else {
        return Ok(None);
    };
    Ok(eval_reflection_non_instantiable_error_message(
        class_name, flags,
    ))
}

/// Returns the catchable error for generated/AOT public ReflectionClass construction, if any.
pub(in crate::interpreter) fn eval_reflection_aot_class_public_instantiation_error(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionInstantiationError>, EvalStatus> {
    let Some((flags, _)) = eval_reflection_aot_class_flags(class_name, values)? else {
        return Ok(None);
    };
    if let Some(message) = eval_reflection_non_instantiable_error_message(class_name, flags) {
        return Ok(Some(EvalReflectionInstantiationError::ThrowableError(message)));
    }
    if flags & EVAL_REFLECTION_CLASS_FLAG_INSTANTIABLE == 0 {
        return Ok(Some(
            EvalReflectionInstantiationError::ReflectionException(format!(
                "Access to non-public constructor of class {}",
                class_name.trim_start_matches('\\')
            )),
        ));
    }
    Ok(None)
}

/// Builds PHP's non-instantiable class-like message from ReflectionClass flags.
pub(super) fn eval_reflection_non_instantiable_error_message(class_name: &str, flags: u64) -> Option<String> {
    let class_name = class_name.trim_start_matches('\\');
    if flags & EVAL_REFLECTION_CLASS_FLAG_ABSTRACT != 0 {
        return Some(format!("Cannot instantiate abstract class {}", class_name));
    }
    if flags & EVAL_REFLECTION_CLASS_FLAG_INTERFACE != 0 {
        return Some(format!("Cannot instantiate interface {}", class_name));
    }
    if flags & EVAL_REFLECTION_CLASS_FLAG_TRAIT != 0 {
        return Some(format!("Cannot instantiate trait {}", class_name));
    }
    if flags & EVAL_REFLECTION_CLASS_FLAG_ENUM != 0 {
        return Some(format!("Cannot instantiate enum {}", class_name));
    }
    None
}

/// Returns whether an absent or public AOT lifecycle method allows public reflection.
pub(super) fn eval_reflection_aot_lifecycle_method_allows_public_reflection(
    class_name: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(flags) = values.reflection_method_flags(class_name, method_name)? else {
        return Ok(true);
    };
    Ok(flags & EVAL_REFLECTION_MEMBER_FLAG_PUBLIC != 0
        && flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT == 0)
}

/// Returns AOT constructor access metadata when the constructor is not public.
pub(in crate::interpreter) fn eval_reflection_aot_non_public_constructor(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility)>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let Some(flags) = values.reflection_method_flags(runtime_class_name, "__construct")? else {
        return Ok(None);
    };
    let visibility = if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        return Ok(None);
    };
    let declaring_class = values
        .reflection_method_declaring_class(runtime_class_name, "__construct")?
        .unwrap_or_else(|| runtime_class_name.to_string());
    Ok(Some((declaring_class, visibility)))
}
