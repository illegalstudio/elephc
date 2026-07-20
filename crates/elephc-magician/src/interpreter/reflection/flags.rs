//! Purpose:
//! Packs Reflection runtime flags and parses common constructor arguments.
//!
//! Called from:
//! - Reflection owner factories and metadata materializers.
//!
//! Key details:
//! - Bit layouts mirror the runtime owner factory contract and remain centralized here.

use super::*;

/// Packs ReflectionMethod/ReflectionProperty predicate flags for the runtime owner factory.
pub(super) fn eval_reflection_member_flags(
    visibility: EvalVisibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
) -> u64 {
    let mut flags = 0;
    if is_static {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_STATIC;
    }
    match visibility {
        EvalVisibility::Public => flags |= EVAL_REFLECTION_MEMBER_FLAG_PUBLIC,
        EvalVisibility::Protected => flags |= EVAL_REFLECTION_MEMBER_FLAG_PROTECTED,
        EvalVisibility::Private => flags |= EVAL_REFLECTION_MEMBER_FLAG_PRIVATE,
    }
    if is_final {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_FINAL;
    }
    if is_abstract {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT;
    }
    if is_readonly {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_READONLY;
    }
    flags
}

/// Packs callable-only ReflectionFunctionAbstract predicate flags.
pub(super) fn eval_reflection_callable_flags(attributes: &[EvalAttribute]) -> u64 {
    if eval_reflection_attributes_include_deprecated(attributes) {
        EVAL_REFLECTION_CALLABLE_FLAG_DEPRECATED
    } else {
        0
    }
}

/// Returns whether an attribute list contains PHP's global `#[Deprecated]` marker.
pub(super) fn eval_reflection_attributes_include_deprecated(attributes: &[EvalAttribute]) -> bool {
    attributes
        .iter()
        .any(|attribute| attribute.name().eq_ignore_ascii_case("Deprecated"))
}

/// Packs ReflectionParameter predicate flags for the runtime parameter factory.
pub(super) fn eval_reflection_parameter_flags(parameter: &EvalReflectionParameterMetadata) -> u64 {
    let mut flags = 0;
    if parameter.is_optional {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_OPTIONAL;
    }
    if parameter.is_variadic {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_VARIADIC;
    }
    if parameter.is_passed_by_reference {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_BY_REF;
    }
    if parameter.is_promoted {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_PROMOTED;
    }
    if parameter.has_type {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_HAS_TYPE;
    }
    if parameter.default_value.is_some() {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_HAS_DEFAULT_VALUE;
    }
    if parameter.allows_null {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_ALLOWS_NULL;
    }
    if parameter.default_value_constant_name.is_some() {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_DEFAULT_VALUE_CONSTANT;
    }
    if parameter.is_array_type {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_ARRAY_TYPE;
    }
    if parameter.is_callable_type {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_CALLABLE_TYPE;
    }
    flags
}

/// Packs ReflectionNamedType predicate flags for the runtime type factory.
pub(super) fn eval_reflection_named_type_flags(type_metadata: &EvalReflectionNamedTypeMetadata) -> u64 {
    let mut flags = 0;
    if type_metadata.allows_null {
        flags |= EVAL_REFLECTION_NAMED_TYPE_FLAG_ALLOWS_NULL;
    }
    if type_metadata.is_builtin {
        flags |= EVAL_REFLECTION_NAMED_TYPE_FLAG_BUILTIN;
    }
    flags
}

/// Packs ReflectionUnionType predicate flags for the runtime type factory.
pub(super) fn eval_reflection_union_type_flags(type_metadata: &EvalReflectionUnionTypeMetadata) -> u64 {
    if type_metadata.allows_null {
        EVAL_REFLECTION_NAMED_TYPE_FLAG_ALLOWS_NULL
    } else {
        0
    }
}

/// Converts a ReflectionFunction argument into a function or eval-closure name.
pub(super) fn eval_reflection_function_name_arg(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if values.type_tag(value)? == EVAL_TAG_OBJECT {
        let identity = values.object_identity(value)?;
        if let Some(name) = context.closure_object_name(identity) {
            return Ok(name.to_string());
        }
    }
    eval_reflection_string_arg(value, values)
}

/// Converts one reflection constructor argument to a Rust UTF-8 string.
pub(super) fn eval_reflection_string_arg(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Maps a PHP reflection owner class name to the helper owner kind.
pub(super) fn reflection_owner_kind(class_name: &str) -> Option<u64> {
    match class_name
        .trim_start_matches('\\')
        .to_ascii_lowercase()
        .as_str()
    {
        "reflectionclass" => Some(EVAL_REFLECTION_OWNER_CLASS),
        "reflectionobject" => Some(EVAL_REFLECTION_OWNER_OBJECT),
        "reflectionenum" => Some(EVAL_REFLECTION_OWNER_ENUM),
        "reflectionfunction" => Some(EVAL_REFLECTION_OWNER_FUNCTION),
        "reflectionmethod" => Some(EVAL_REFLECTION_OWNER_METHOD),
        "reflectionproperty" => Some(EVAL_REFLECTION_OWNER_PROPERTY),
        "reflectionparameter" => Some(EVAL_REFLECTION_OWNER_PARAMETER),
        "reflectionclassconstant" => Some(EVAL_REFLECTION_OWNER_CLASS_CONSTANT),
        "reflectionenumunitcase" => Some(EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE),
        "reflectionenumbackedcase" => Some(EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE),
        _ => None,
    }
}
