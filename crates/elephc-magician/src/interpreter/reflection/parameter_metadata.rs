//! Purpose:
//! Builds reflected parameter lists, defaults, magic scopes, and type metadata.
//!
//! Called from:
//! - Function, method, property-hook, and ReflectionParameter construction paths.
//!
//! Key details:
//! - Promoted properties, nullable composites, variadics, and defaults remain aligned by index.

use super::*;

/// Returns PHP's required parameter count for a reflected method signature.
pub(super) fn eval_reflection_required_parameter_count(
    defaults: &[Option<EvalExpr>],
    variadic_flags: &[bool],
) -> usize {
    let fixed_count = variadic_flags
        .iter()
        .position(|is_variadic| *is_variadic)
        .unwrap_or(defaults.len());
    (0..fixed_count)
        .rfind(|position| !defaults.get(*position).is_some_and(Option::is_some))
        .map_or(0, |position| position + 1)
}

/// Builds PHP magic scope metadata for a reflected function parameter default.
pub(super) fn eval_reflection_function_parameter_magic_scope(
    function_name: &str,
) -> EvalReflectionParameterMagicScope {
    EvalReflectionParameterMagicScope {
        function_name: function_name.to_string(),
        method_name: function_name.to_string(),
        class_name: None,
        trait_name: None,
    }
}

/// Builds PHP magic scope metadata for a reflected method parameter default.
pub(super) fn eval_reflection_method_parameter_magic_scope(
    class_name: &str,
    function_name: &str,
    method_name: &str,
    trait_name: Option<&str>,
) -> EvalReflectionParameterMagicScope {
    EvalReflectionParameterMagicScope {
        function_name: function_name.to_string(),
        method_name: method_name.to_string(),
        class_name: Some(class_name.trim_start_matches('\\').to_string()),
        trait_name: trait_name.map(|trait_name| trait_name.trim_start_matches('\\').to_string()),
    }
}

/// Builds PHP magic scope metadata for an eval method's reflected parameter default.
pub(super) fn eval_reflection_eval_method_parameter_magic_scope(
    class_name: &str,
    method: &EvalClassMethod,
    fallback_trait_name: Option<&str>,
) -> EvalReflectionParameterMagicScope {
    eval_reflection_method_parameter_magic_scope(
        class_name,
        method.magic_function_name(),
        &method.magic_method_name(class_name),
        method
            .trait_origin()
            .or(fallback_trait_name),
    )
}

/// Builds parameter reflection metadata from eval parameter names and type flags.
pub(super) fn eval_reflection_parameters_from_names_and_type_flags(
    declaring_class_name: Option<&str>,
    declaring_function: Option<&EvalReflectionDeclaringFunctionMetadata>,
    names: &[String],
    has_type_flags: &[bool],
    parameter_types: &[Option<EvalParameterType>],
    parameter_attributes: &[Vec<EvalAttribute>],
    defaults: &[Option<EvalExpr>],
    by_ref_flags: &[bool],
    variadic_flags: &[bool],
    promoted_parameter_names: &[String],
) -> Vec<EvalReflectionParameterMetadata> {
    names
        .iter()
        .enumerate()
        .map(|(position, name)| {
            let has_type = has_type_flags.get(position).copied().unwrap_or(false);
            let default_value = defaults.get(position).and_then(Clone::clone);
            let default_value_constant_name = default_value
                .as_ref()
                .and_then(eval_reflection_default_constant_name);
            let type_metadata = parameter_types
                .get(position)
                .and_then(Option::as_ref)
                .and_then(eval_reflection_parameter_type_metadata)
                .filter(|_| has_type);
            let is_array_type =
                eval_reflection_parameter_has_named_type(type_metadata.as_ref(), "array");
            let is_callable_type =
                eval_reflection_parameter_has_named_type(type_metadata.as_ref(), "callable");
            EvalReflectionParameterMetadata {
                name: name.clone(),
                declaring_class_name: declaring_class_name.map(str::to_string),
                declaring_function: declaring_function.cloned(),
                attributes: parameter_attributes
                    .get(position)
                    .cloned()
                    .unwrap_or_default(),
                position,
                is_optional: defaults.get(position).is_some_and(Option::is_some)
                    || variadic_flags.get(position).copied().unwrap_or(false),
                is_variadic: variadic_flags.get(position).copied().unwrap_or(false),
                is_passed_by_reference: by_ref_flags.get(position).copied().unwrap_or(false),
                is_promoted: promoted_parameter_names
                    .iter()
                    .any(|promoted_name| promoted_name == name),
                has_type,
                allows_null: eval_reflection_parameter_allows_null(
                    has_type,
                    type_metadata.as_ref(),
                    default_value.as_ref(),
                ),
                is_array_type,
                is_callable_type,
                type_metadata,
                default_value,
                default_value_constant_name,
            }
        })
        .collect()
}

/// Returns whether retained parameter metadata is one named type with the requested name.
pub(super) fn eval_reflection_parameter_has_named_type(
    type_metadata: Option<&EvalReflectionParameterTypeMetadata>,
    expected_name: &str,
) -> bool {
    matches!(
        type_metadata,
        Some(EvalReflectionParameterTypeMetadata {
            kind: EvalReflectionParameterTypeKind::Named(named)
        }) if named.name.eq_ignore_ascii_case(expected_name)
    )
}

/// Returns PHP's ReflectionParameter default-constant name for retained eval defaults.
pub(super) fn eval_reflection_default_constant_name(default: &EvalExpr) -> Option<String> {
    match default {
        EvalExpr::ConstFetch(name) => Some(name.clone()),
        EvalExpr::NamespacedConstFetch { name, .. } => Some(name.clone()),
        EvalExpr::ClassConstantFetch {
            class_name,
            constant,
        } => Some(format!("{}::{}", class_name, constant)),
        _ => None,
    }
}

/// Builds ReflectionParameter metadata for eval-declared or native free functions.
pub(super) fn eval_reflection_function_parameters(
    function_name: &str,
    names: &[String],
    function_attributes: Vec<EvalAttribute>,
    parameter_attributes: &[Vec<EvalAttribute>],
    parameter_types: &[Option<EvalParameterType>],
    defaults: &[Option<EvalExpr>],
    by_ref_flags: &[bool],
    variadic_flags: &[bool],
) -> Vec<EvalReflectionParameterMetadata> {
    let has_type_flags = parameter_types
        .iter()
        .map(Option::is_some)
        .collect::<Vec<_>>();
    let flags = eval_reflection_callable_flags(&function_attributes);
    let declaring_function = EvalReflectionDeclaringFunctionMetadata {
        name: function_name.to_string(),
        declaring_class_name: None,
        magic_scope: Some(eval_reflection_function_parameter_magic_scope(function_name)),
        attributes: function_attributes,
        flags,
        required_parameter_count: eval_reflection_required_parameter_count(
            defaults,
            variadic_flags,
        ),
    };
    eval_reflection_parameters_from_names_and_type_flags(
        None,
        Some(&declaring_function),
        names,
        &has_type_flags,
        parameter_types,
        parameter_attributes,
        defaults,
        by_ref_flags,
        variadic_flags,
        &[],
    )
}

/// Returns promoted constructor parameter names for one eval class method.
pub(super) fn eval_reflection_promoted_parameter_names(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Vec<String> {
    if !method_name.eq_ignore_ascii_case("__construct") {
        return Vec::new();
    }
    context
        .class(class_name)
        .map(eval_reflection_promoted_property_names)
        .unwrap_or_default()
}

/// Returns promoted constructor parameter names for one eval trait method.
pub(super) fn eval_reflection_promoted_trait_parameter_names(
    trait_decl: &EvalTrait,
    method_name: &str,
) -> Vec<String> {
    if method_name.eq_ignore_ascii_case("__construct") {
        eval_reflection_promoted_property_names_from_slice(trait_decl.properties())
    } else {
        Vec::new()
    }
}

/// Returns property names marked as constructor-promoted in one eval class.
pub(super) fn eval_reflection_promoted_property_names(class: &EvalClass) -> Vec<String> {
    eval_reflection_promoted_property_names_from_slice(class.properties())
}

/// Returns property names marked as constructor-promoted in one property list.
pub(super) fn eval_reflection_promoted_property_names_from_slice(
    properties: &[EvalClassProperty],
) -> Vec<String> {
    properties
        .iter()
        .filter(|property| property.is_promoted())
        .map(|property| property.name().to_string())
        .collect()
}

/// Converts eval parameter type metadata into the supported ReflectionType subset.
pub(super) fn eval_reflection_parameter_type_metadata(
    parameter_type: &EvalParameterType,
) -> Option<EvalReflectionParameterTypeMetadata> {
    let variants = parameter_type.variants();
    if variants.is_empty() {
        return None;
    }
    let allows_null = parameter_type.allows_null();
    let mut types = variants
        .iter()
        .map(|variant| eval_reflection_named_type_variant_metadata(variant, false))
        .collect::<Option<Vec<_>>>()?;
    if types.len() == 1 {
        let mut named = types.pop()?;
        named.allows_null = allows_null;
        return Some(EvalReflectionParameterTypeMetadata {
            kind: EvalReflectionParameterTypeKind::Named(named),
        });
    }
    if parameter_type.is_intersection() {
        return Some(EvalReflectionParameterTypeMetadata {
            kind: EvalReflectionParameterTypeKind::Intersection(
                EvalReflectionIntersectionTypeMetadata { types },
            ),
        });
    }
    Some(EvalReflectionParameterTypeMetadata {
        kind: EvalReflectionParameterTypeKind::Union(EvalReflectionUnionTypeMetadata {
            types,
            allows_null,
        }),
    })
}

/// Returns PHP's `ReflectionParameter::allowsNull()` value for retained metadata.
pub(super) fn eval_reflection_parameter_allows_null(
    has_type: bool,
    type_metadata: Option<&EvalReflectionParameterTypeMetadata>,
    default_value: Option<&EvalExpr>,
) -> bool {
    !has_type
        || default_value.is_some_and(|default| matches!(default, EvalExpr::Const(EvalConst::Null)))
        || type_metadata.is_some_and(eval_reflection_type_allows_null)
}

/// Returns whether one retained ReflectionType metadata value accepts null.
pub(super) fn eval_reflection_type_allows_null(type_metadata: &EvalReflectionParameterTypeMetadata) -> bool {
    match &type_metadata.kind {
        EvalReflectionParameterTypeKind::Named(named_type) => named_type.allows_null,
        EvalReflectionParameterTypeKind::Union(union_type) => union_type.allows_null,
        EvalReflectionParameterTypeKind::Intersection(_) => false,
    }
}

/// Converts one eval parameter type variant into `ReflectionNamedType` metadata.
pub(super) fn eval_reflection_named_type_variant_metadata(
    variant: &EvalParameterTypeVariant,
    allows_null: bool,
) -> Option<EvalReflectionNamedTypeMetadata> {
    match variant {
        EvalParameterTypeVariant::Array => {
            Some(eval_reflection_builtin_named_type("array", allows_null))
        }
        EvalParameterTypeVariant::Bool => {
            Some(eval_reflection_builtin_named_type("bool", allows_null))
        }
        EvalParameterTypeVariant::Callable => {
            Some(eval_reflection_builtin_named_type("callable", allows_null))
        }
        EvalParameterTypeVariant::Class(name) => Some(EvalReflectionNamedTypeMetadata {
            name: name.clone(),
            allows_null,
            is_builtin: false,
        }),
        EvalParameterTypeVariant::Float => {
            Some(eval_reflection_builtin_named_type("float", allows_null))
        }
        EvalParameterTypeVariant::Int => {
            Some(eval_reflection_builtin_named_type("int", allows_null))
        }
        EvalParameterTypeVariant::Iterable => {
            Some(eval_reflection_builtin_named_type("iterable", allows_null))
        }
        EvalParameterTypeVariant::Mixed => Some(eval_reflection_builtin_named_type("mixed", true)),
        EvalParameterTypeVariant::Never => Some(eval_reflection_builtin_named_type("never", false)),
        EvalParameterTypeVariant::Object => {
            Some(eval_reflection_builtin_named_type("object", allows_null))
        }
        EvalParameterTypeVariant::String => {
            Some(eval_reflection_builtin_named_type("string", allows_null))
        }
        EvalParameterTypeVariant::Void => Some(eval_reflection_builtin_named_type("void", false)),
    }
}

/// Builds metadata for one builtin eval `ReflectionNamedType`.
pub(super) fn eval_reflection_builtin_named_type(
    name: &str,
    allows_null: bool,
) -> EvalReflectionNamedTypeMetadata {
    EvalReflectionNamedTypeMetadata {
        name: name.to_string(),
        allows_null,
        is_builtin: true,
    }
}
