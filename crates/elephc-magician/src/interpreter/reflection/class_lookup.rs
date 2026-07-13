//! Purpose:
//! Resolves class-like relations, members, attributes, flags, and lookup errors.
//!
//! Called from:
//! - Reflection class APIs, constructors, formatters, and member metadata builders.
//!
//! Key details:
//! - Eval declarations, aliases, interfaces, traits, and AOT metadata use case-insensitive lookup.

use super::*;

/// Returns true when a ReflectionClass member passes an optional modifier filter.
pub(super) fn eval_reflection_member_matches_filter(
    member: &EvalReflectionMemberMetadata,
    filter: Option<u64>,
) -> bool {
    match filter {
        Some(filter) => member.modifiers & filter != 0,
        None => true,
    }
}

/// Parses the optional ReflectionClass member filter argument.
pub(super) fn eval_reflection_member_filter(
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<u64>, EvalStatus> {
    let mut filter = None;
    for arg in evaluated_args {
        if let Some(name) = arg.name.as_deref() {
            if name != "filter" {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        if filter.is_some() {
            return Err(EvalStatus::RuntimeFatal);
        }
        filter = Some(arg.value);
    }
    let Some(filter) = filter else {
        return Ok(None);
    };
    if values.is_null(filter)? {
        return Ok(None);
    }
    let cast_filter = values.cast_int(filter)?;
    let bytes = values.string_bytes(cast_filter)?;
    values.release(cast_filter)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    text.parse::<i64>()
        .map(|value| Some(value as u64))
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Returns generated AOT member names for one reflected class.
pub(super) fn eval_reflection_aot_member_names(
    owner_kind: u64,
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let names_array = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        values.reflection_method_names(runtime_class_name)?
    } else {
        values.reflection_property_names(runtime_class_name)?
    };
    let names = eval_reflection_string_array_to_vec(names_array, values)?;
    values.release(names_array)?;
    Ok(names)
}

/// Returns generated AOT interface names for one reflected class-like symbol.
pub(super) fn eval_reflection_aot_class_interface_names(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let names_array = values.reflection_class_interface_names(runtime_class_name)?;
    let names = eval_reflection_string_array_to_vec(names_array, values)?;
    values.release(names_array)?;
    Ok(names)
}

/// Returns eval metadata interface names expanded with generated/AOT ancestors.
pub(super) fn eval_reflection_eval_metadata_interface_names(
    metadata: &EvalReflectionClassMetadata,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    if context.has_class(&metadata.resolved_name) || context.has_enum(&metadata.resolved_name) {
        eval_reflection_eval_class_interface_names(&metadata.resolved_name, context, values)
    } else if context.has_interface(&metadata.resolved_name) {
        eval_reflection_eval_interface_parent_names(&metadata.resolved_name, context, values)
    } else {
        Ok(metadata.interface_names.clone())
    }
}

/// Returns eval metadata flags corrected for generated/AOT inherited interfaces.
pub(super) fn eval_reflection_eval_metadata_flags(
    metadata: &EvalReflectionClassMetadata,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<u64, EvalStatus> {
    let mut flags = metadata.flags;
    if flags & EVAL_REFLECTION_CLASS_FLAG_ITERABLE == 0
        && flags & EVAL_REFLECTION_CLASS_FLAG_ABSTRACT == 0
        && context.has_class(&metadata.resolved_name)
        && eval_reflection_interface_names_include_iterable(
            &eval_reflection_eval_class_interface_names(&metadata.resolved_name, context, values)?,
        )
    {
        flags |= EVAL_REFLECTION_CLASS_FLAG_ITERABLE;
    }
    Ok(flags)
}

/// Returns eval class interfaces plus interfaces inherited from generated/AOT parents.
pub(super) fn eval_reflection_eval_class_interface_names(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(parent) = context.class_native_parent_name(class_name) {
        for name in eval_reflection_aot_class_interface_names(&parent, values)? {
            eval_reflection_push_unique_class_name(name, &mut names, &mut seen);
        }
    }
    for name in context.class_interface_names(class_name) {
        eval_reflection_push_unique_class_name(name.clone(), &mut names, &mut seen);
        if !context.has_interface(&name) && eval_runtime_interface_exists(&name, values)? {
            for parent in eval_reflection_aot_class_interface_names(&name, values)? {
                eval_reflection_push_unique_class_name(parent, &mut names, &mut seen);
            }
        }
    }
    Ok(names)
}

/// Returns eval interface parents plus inherited generated/AOT interface parents.
pub(super) fn eval_reflection_eval_interface_parent_names(
    interface_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in context.interface_parent_names(interface_name) {
        eval_reflection_push_unique_class_name(name.clone(), &mut names, &mut seen);
        if !context.has_interface(&name) && eval_runtime_interface_exists(&name, values)? {
            for parent in eval_reflection_aot_class_interface_names(&name, values)? {
                eval_reflection_push_unique_class_name(parent, &mut names, &mut seen);
            }
        }
    }
    Ok(names)
}

/// Returns whether one interface list includes PHP iterable marker interfaces.
pub(super) fn eval_reflection_interface_names_include_iterable(interface_names: &[String]) -> bool {
    interface_names.iter().any(|name| {
        name.eq_ignore_ascii_case("Iterator") || name.eq_ignore_ascii_case("IteratorAggregate")
    })
}

/// Appends one class-like name while preserving PHP's case-insensitive uniqueness.
pub(super) fn eval_reflection_push_unique_class_name(
    name: String,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(name.to_ascii_lowercase()) {
        names.push(name);
    }
}

/// Returns generated AOT trait names for one reflected class-like symbol.
pub(super) fn eval_reflection_aot_class_trait_names(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let names_array = values.reflection_class_trait_names(runtime_class_name)?;
    let names = eval_reflection_string_array_to_vec(names_array, values)?;
    values.release(names_array)?;
    Ok(names)
}

/// Returns generated AOT trait aliases for one reflected class-like symbol.
pub(super) fn eval_reflection_aot_class_trait_aliases(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<(String, String)>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let alias_names_array = values.reflection_class_trait_alias_names(runtime_class_name)?;
    let alias_names = eval_reflection_string_array_to_vec(alias_names_array, values)?;
    values.release(alias_names_array)?;
    let alias_sources_array = values.reflection_class_trait_alias_sources(runtime_class_name)?;
    let alias_sources = eval_reflection_string_array_to_vec(alias_sources_array, values)?;
    values.release(alias_sources_array)?;
    if alias_names.len() != alias_sources.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(alias_names.into_iter().zip(alias_sources).collect())
}

/// Copies a runtime string array into Rust-owned strings for reflection metadata assembly.
pub(super) fn eval_reflection_string_array_to_vec(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.int(position as i64)?;
        let value = values.array_get(array, key)?;
        result.push(eval_reflection_string_arg(value, values)?);
    }
    Ok(result)
}

/// Returns member metadata for one ReflectionClass member-array entry.
pub(super) fn eval_reflection_member_metadata(
    owner_kind: u64,
    class_name: &str,
    name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionMemberMetadata> {
    match owner_kind {
        EVAL_REFLECTION_OWNER_METHOD => eval_reflection_method_metadata(class_name, name, context),
        EVAL_REFLECTION_OWNER_PROPERTY => {
            eval_reflection_property_metadata(class_name, name, context)
        }
        _ => None,
    }
}

/// Returns the eval-retained class-like attributes plus canonical reflected name.
pub(super) fn eval_reflection_class_like_attributes(
    name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionClassMetadata> {
    if let Some(class) = context.class(name) {
        let is_enum = context.has_enum(class.name());
        let mut flags = EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED;
        if class.is_final() {
            flags |= EVAL_REFLECTION_CLASS_FLAG_FINAL;
        }
        if class.is_abstract() {
            flags |= EVAL_REFLECTION_CLASS_FLAG_ABSTRACT;
        }
        if is_enum {
            flags |= EVAL_REFLECTION_CLASS_FLAG_ENUM;
        }
        if class.is_readonly_class() && !is_enum {
            flags |= EVAL_REFLECTION_CLASS_FLAG_READONLY;
        }
        if eval_reflection_class_is_instantiable(class, is_enum, context) {
            flags |= EVAL_REFLECTION_CLASS_FLAG_INSTANTIABLE;
        }
        if eval_reflection_class_is_cloneable(class, is_enum, context) {
            flags |= EVAL_REFLECTION_CLASS_FLAG_CLONEABLE;
        }
        if eval_reflection_class_is_iterable(class, is_enum, context) {
            flags |= EVAL_REFLECTION_CLASS_FLAG_ITERABLE;
        }
        if class.is_anonymous() {
            flags |= EVAL_REFLECTION_CLASS_FLAG_ANONYMOUS;
        }
        let modifiers = eval_reflection_class_modifiers(
            class.is_final(),
            class.is_abstract(),
            class.is_readonly_class(),
            is_enum,
        );
        return Some(EvalReflectionClassMetadata {
            resolved_name: class.name().trim_start_matches('\\').to_string(),
            source_location: class.source_location(),
            attributes: class.attributes().to_vec(),
            interface_names: context.class_interface_names(class.name()),
            trait_names: context.class_trait_names(class.name()),
            method_names: context.class_method_names(class.name()),
            property_names: context.class_property_names(class.name()),
            parent_class_name: eval_reflection_parent_class_name(class, context),
            flags,
            modifiers,
        });
    }
    if let Some(interface) = context.interface(name) {
        return Some(EvalReflectionClassMetadata {
            resolved_name: interface.name().trim_start_matches('\\').to_string(),
            source_location: interface.source_location(),
            attributes: interface.attributes().to_vec(),
            interface_names: context.interface_parent_names(interface.name()),
            trait_names: Vec::new(),
            method_names: context.interface_method_names(interface.name()),
            property_names: context.interface_property_names(interface.name()),
            parent_class_name: None,
            flags: EVAL_REFLECTION_CLASS_FLAG_INTERFACE | EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED,
            modifiers: 0,
        });
    }
    if let Some(trait_decl) = context.trait_decl(name) {
        return Some(EvalReflectionClassMetadata {
            resolved_name: trait_decl.name().trim_start_matches('\\').to_string(),
            source_location: trait_decl.source_location(),
            attributes: trait_decl.attributes().to_vec(),
            interface_names: Vec::new(),
            trait_names: context.trait_trait_names(trait_decl.name()),
            method_names: context.trait_method_names(trait_decl.name()),
            property_names: context.trait_property_names(trait_decl.name()),
            parent_class_name: None,
            flags: EVAL_REFLECTION_CLASS_FLAG_TRAIT | EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED,
            modifiers: 0,
        });
    }
    context
        .enum_decl(name)
        .map(|enum_decl| EvalReflectionClassMetadata {
            resolved_name: enum_decl.name().trim_start_matches('\\').to_string(),
            source_location: enum_decl.source_location(),
            attributes: enum_decl.attributes().to_vec(),
            interface_names: context.class_interface_names(enum_decl.name()),
            trait_names: context.class_trait_names(enum_decl.name()),
            method_names: context.class_method_names(enum_decl.name()),
            property_names: context.class_property_names(enum_decl.name()),
            parent_class_name: None,
            flags: EVAL_REFLECTION_CLASS_FLAG_FINAL
                | EVAL_REFLECTION_CLASS_FLAG_ENUM
                | EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED,
            modifiers: 32,
        })
}

/// Returns the PHP-visible parent class name for ReflectionClass metadata.
pub(super) fn eval_reflection_parent_class_name(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Option<String> {
    context.class_parent_names(class.name()).into_iter().next()
}

/// Returns PHP's `ReflectionClass::isInstantiable()` value for eval class metadata.
pub(super) fn eval_reflection_class_is_instantiable(
    class: &EvalClass,
    is_enum: bool,
    context: &ElephcEvalContext,
) -> bool {
    if class.is_abstract() || is_enum {
        return false;
    }
    context
        .class_method(class.name(), "__construct")
        .map(|(_, method)| method.visibility() == EvalVisibility::Public)
        .unwrap_or(true)
}

/// Returns PHP's `ReflectionClass::isCloneable()` value for eval class metadata.
pub(super) fn eval_reflection_class_is_cloneable(
    class: &EvalClass,
    is_enum: bool,
    context: &ElephcEvalContext,
) -> bool {
    if class.is_abstract() || is_enum {
        return false;
    }
    context
        .class_method(class.name(), "__clone")
        .map(|(_, method)| method.visibility() == EvalVisibility::Public)
        .unwrap_or(true)
}

/// Returns PHP's `ReflectionClass::isIterable()` value for eval class metadata.
pub(super) fn eval_reflection_class_is_iterable(
    class: &EvalClass,
    is_enum: bool,
    context: &ElephcEvalContext,
) -> bool {
    if class.is_abstract() || is_enum {
        return false;
    }
    context
        .class_interface_names(class.name())
        .iter()
        .any(|name| {
            name.eq_ignore_ascii_case("Iterator") || name.eq_ignore_ascii_case("IteratorAggregate")
        })
}

/// Returns PHP's `ReflectionClass::isIterable()` value for compiler-injected class names.
pub(super) fn eval_reflection_builtin_class_is_iterable(class_name: &str) -> bool {
    matches!(
        class_name
            .trim_start_matches('\\')
            .to_ascii_lowercase()
            .as_str(),
        "__elephcappenditeratorarrayiterator"
            | "appenditerator"
            | "arrayiterator"
            | "arrayobject"
            | "cachingiterator"
            | "callbackfilteriterator"
            | "directoryiterator"
            | "emptyiterator"
            | "filesystemiterator"
            | "generator"
            | "globiterator"
            | "infiniteiterator"
            | "internaliterator"
            | "iteratoriterator"
            | "limititerator"
            | "multipleiterator"
            | "norewinditerator"
            | "parentiterator"
            | "recursivearrayiterator"
            | "recursivecachingiterator"
            | "recursivecallbackfilteriterator"
            | "recursivedirectoryiterator"
            | "recursiveiteratoriterator"
            | "recursiveregexiterator"
            | "regexiterator"
            | "spldoublylinkedlist"
            | "splfixedarray"
            | "splfileobject"
            | "splmaxheap"
            | "splminheap"
            | "splobjectstorage"
            | "splpriorityqueue"
            | "splqueue"
            | "splstack"
            | "spltempfileobject"
    )
}

/// Returns whether one reflected class-like name belongs to compiler-injected metadata.
pub(super) fn eval_reflection_class_like_is_internal(class_name: &str) -> bool {
    let trimmed = class_name.trim_start_matches('\\');
    if EVAL_SPL_CLASS_NAMES
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(trimmed))
    {
        return true;
    }
    matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "__elephcappenditeratorarrayiterator"
            | "fiber"
            | "fibererror"
            | "generator"
            | "internaliterator"
            | "jsonexception"
            | "phar"
            | "phardata"
            | "pharfileinfo"
            | "php_user_filter"
            | "reflectionattribute"
            | "reflectionclass"
            | "reflectionclassconstant"
            | "reflectionenum"
            | "reflectionenumbackedcase"
            | "reflectionenumunitcase"
            | "reflectionexception"
            | "reflectionfunction"
            | "reflectionintersectiontype"
            | "reflectionmethod"
            | "reflectionnamedtype"
            | "reflectionparameter"
            | "reflectionproperty"
            | "reflectionuniontype"
            | "sortdirection"
            | "splheap"
            | "splmaxheap"
            | "splminheap"
            | "splobjectstorage"
            | "splpriorityqueue"
            | "stdclass"
    )
}

/// Computes PHP's `ReflectionClass::getModifiers()` bitmask for eval metadata.
pub(super) fn eval_reflection_class_modifiers(
    is_final: bool,
    is_abstract: bool,
    is_readonly_class: bool,
    is_enum: bool,
) -> u64 {
    let mut modifiers = 0;
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    if is_readonly_class && !is_enum {
        modifiers |= 65_536;
    }
    modifiers
}

/// Computes PHP's `ReflectionClassConstant::getModifiers()` bitmask for eval metadata.
pub(super) fn eval_reflection_class_constant_modifiers(visibility: EvalVisibility, is_final: bool) -> u64 {
    let mut modifiers = match visibility {
        EvalVisibility::Public => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Private => 4,
    };
    if is_final {
        modifiers |= 32;
    }
    modifiers
}

/// Computes PHP's `ReflectionMethod::getModifiers()` bitmask for eval metadata.
pub(super) fn eval_reflection_method_modifiers(
    visibility: EvalVisibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
) -> u64 {
    let mut modifiers = match visibility {
        EvalVisibility::Public => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Private => 4,
    };
    if is_static {
        modifiers |= 16;
    }
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    modifiers
}

/// Computes PHP's `ReflectionProperty::getModifiers()` bitmask for eval metadata.
pub(super) fn eval_reflection_property_modifiers(
    visibility: EvalVisibility,
    set_visibility: Option<EvalVisibility>,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
    is_virtual: bool,
) -> u64 {
    let mut modifiers = match visibility {
        EvalVisibility::Public => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Private => 4,
    };
    if is_static {
        modifiers |= 16;
    }
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    if is_readonly {
        modifiers |= 128;
    }
    if is_virtual {
        modifiers |= 512;
    }
    match set_visibility {
        Some(EvalVisibility::Private) => modifiers |= 32 | 4096,
        Some(EvalVisibility::Protected) => modifiers |= 2048,
        _ if is_readonly && visibility == EvalVisibility::Public => modifiers |= 2048,
        _ => {}
    }
    modifiers
}

/// Returns declaring class, attributes, visibility, finality, and enum-case kind.
pub(super) fn eval_reflection_class_constant_metadata(
    class_name: &str,
    constant_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, Vec<EvalAttribute>, EvalVisibility, bool, bool)>, EvalStatus> {
    if let Some(enum_decl) = context.enum_decl(class_name) {
        if let Some(case) = enum_decl.case(constant_name) {
            return Ok(Some((
                enum_decl.name().to_string(),
                case.attributes().to_vec(),
                EvalVisibility::Public,
                false,
                true,
            )));
        }
    }
    if let Some(metadata) = context
        .class_constant(class_name, constant_name)
        .map(|(declaring_class, constant)| {
            (
                declaring_class,
                constant.attributes().to_vec(),
                constant.visibility(),
                constant.is_final(),
                false,
            )
        }) {
        return Ok(Some(metadata));
    }
    let runtime_class_name = class_name.trim_start_matches('\\');
    let Some(flags) = values.reflection_constant_flags(runtime_class_name, constant_name)? else {
        return Ok(None);
    };
    let declaring_class = values
        .reflection_constant_declaring_class(runtime_class_name, constant_name)?
        .unwrap_or_else(|| runtime_class_name.to_string());
    let attributes = eval_reflection_aot_constant_attributes(
        runtime_class_name,
        &declaring_class,
        constant_name,
        context,
    );
    let visibility = if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    };
    Ok(Some((
        declaring_class,
        attributes,
        visibility,
        flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0,
        flags & EVAL_REFLECTION_MEMBER_FLAG_ENUM_CASE != 0,
    )))
}

/// Returns registered generated/AOT class-constant attributes for one reflected constant.
pub(super) fn eval_reflection_aot_constant_attributes(
    runtime_class_name: &str,
    declaring_class_name: &str,
    constant_name: &str,
    context: &ElephcEvalContext,
) -> Vec<EvalAttribute> {
    let attributes = context.native_constant_attributes(declaring_class_name, constant_name);
    if !attributes.is_empty() || declaring_class_name == runtime_class_name {
        return attributes;
    }
    context.native_constant_attributes(runtime_class_name, constant_name)
}

/// Returns true when a name resolves to an eval-declared class-like symbol.
pub(super) fn eval_reflection_class_like_exists(name: &str, context: &ElephcEvalContext) -> bool {
    context.has_class(name)
        || context.has_interface(name)
        || context.has_trait(name)
        || context.has_enum(name)
}

/// Returns true when a name resolves to eval or runtime class-like metadata.
pub(super) fn eval_reflection_class_like_or_runtime_exists(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(eval_reflection_class_like_exists(name, context)
        || values.class_exists(name)?
        || eval_runtime_interface_exists(name, values)?
        || values.trait_exists(name)?
        || values.enum_exists(name)?)
}

/// Returns true when one name exists as an eval or runtime interface.
pub(super) fn eval_reflection_interface_exists(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(context.has_interface(name) || eval_runtime_interface_exists(name, values)?)
}

/// Returns true when one name exists as a non-interface class-like symbol.
pub(super) fn eval_reflection_non_interface_exists(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.has_class(name)
        || context.has_trait(name)
        || context.has_enum(name)
        || values.class_exists(name)?
        || values.trait_exists(name)?
    {
        return Ok(true);
    }
    values.enum_exists(name)
}

/// Returns true when reflected eval metadata implements or extends an interface name.
pub(super) fn eval_reflection_class_implements_interface_name(
    reflected_name: &str,
    interface_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.has_interface(reflected_name) {
        if eval_reflection_same_class_like_name(reflected_name, interface_name) {
            return Ok(true);
        }
        return Ok(eval_reflection_eval_interface_parent_names(
            reflected_name,
            context,
            values,
        )?
        .iter()
        .any(|parent| eval_reflection_same_class_like_name(parent, interface_name)));
    }
    if context.has_class(reflected_name) || context.has_enum(reflected_name) {
        return Ok(eval_reflection_eval_class_interface_names(reflected_name, context, values)?
            .iter()
            .any(|interface| eval_reflection_same_class_like_name(interface, interface_name)));
    }
    Ok(false)
}

/// Returns true when reflected eval metadata is a subclass or subinterface of a target.
pub(super) fn eval_reflection_class_is_subclass_of_name(
    reflected_name: &str,
    target_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.has_interface(reflected_name) {
        return Ok(eval_reflection_eval_interface_parent_names(
            reflected_name,
            context,
            values,
        )?
            .iter()
            .any(|parent| eval_reflection_same_class_like_name(parent, target_name)));
    }
    if context.has_class(reflected_name) || context.has_enum(reflected_name) {
        if context.class_is_a(reflected_name, target_name, true) {
            return Ok(true);
        }
        return Ok(eval_reflection_eval_class_interface_names(reflected_name, context, values)?
            .iter()
            .any(|interface| eval_reflection_same_class_like_name(interface, target_name)));
    }
    Ok(false)
}

/// Returns true when two PHP class-like names match case-insensitively.
pub(super) fn eval_reflection_same_class_like_name(left: &str, right: &str) -> bool {
    left.trim_start_matches('\\')
        .eq_ignore_ascii_case(right.trim_start_matches('\\'))
}

/// Creates a catchable `ReflectionException` and propagates it through eval throw state.
pub(super) fn eval_throw_reflection_exception(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let exception = values.new_object("ReflectionException")?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Creates a catchable `ValueError` and propagates it through eval throw state.
pub(super) fn eval_throw_value_error(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let exception = values.new_object("ValueError")?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Creates a catchable `TypeError` and propagates it through eval throw state.
pub(super) fn eval_throw_type_error(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let exception = values.new_object("TypeError")?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Returns PHP's type name spelling used in argument type error messages.
pub(super) fn eval_reflection_type_error_type_name(tag: u64) -> &'static str {
    match tag {
        EVAL_TAG_INT => "int",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_FLOAT => "float",
        EVAL_TAG_BOOL => "bool",
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_NULL => "null",
        EVAL_TAG_RESOURCE => "resource",
        EVAL_TAG_OBJECT => "object",
        _ => "unknown",
    }
}
