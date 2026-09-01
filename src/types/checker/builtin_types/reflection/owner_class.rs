//! Purpose:
//! Builds the shared synthetic classes for Reflection member owners.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Per-owner properties, methods, and constants remain selected by class name.

use super::*;

/// Builds a `FlattenedClass` for simple reflection owner classes
/// with a private `__attrs` array property and two methods: `__construct`
/// (public, accepting the supplied params) and `getAttributes` (public,
/// returning the `__attrs` array).
pub(super) fn builtin_reflection_owner_class(
    name: &str,
    has_name: bool,
    constructor_params: Vec<(&str, Option<TypeExpr>, Option<Expr>, bool)>,
) -> FlattenedClass {
    let mut properties = Vec::new();
    let mut methods = vec![builtin_reflection_owner_constructor_method(
        constructor_params,
    )];
    if has_name {
        properties.push(builtin_property(
            "__name",
            Visibility::Private,
            Some(TypeExpr::Str),
            empty_string(),
        ));
        methods.push(builtin_reflection_class_string_method("getName", "__name"));
    }
    if reflection_owner_has_doc_comment_method(name) {
        methods.push(builtin_reflection_constant_false_union_method(
            "getDocComment",
        ));
    }
    if reflection_owner_has_extension_methods(name) {
        methods.push(builtin_reflection_constant_false_union_method(
            "getExtensionName",
        ));
        methods.push(builtin_reflection_constant_null_mixed_method(
            "getExtension",
        ));
    }
    add_reflection_function_method_origin_methods(name, &mut properties, &mut methods);
    add_reflection_member_flag_methods(name, &mut properties, &mut methods);
    if matches!(
        name,
        "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    ) {
        properties.push(builtin_property(
            "__declaring_class",
            Visibility::Private,
            Some(mixed_type()),
            false_bool(),
        ));
        methods.push(builtin_reflection_class_object_method(
            "getDeclaringClass",
            "__declaring_class",
            "ReflectionClass",
        ));
    }
    if matches!(
        name,
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        properties.push(builtin_property(
            "__string",
            Visibility::Private,
            Some(TypeExpr::Str),
            empty_string(),
        ));
        methods.push(builtin_reflection_class_string_method(
            "__toString",
            "__string",
        ));
        if name == "ReflectionClassConstant" {
            properties.push(builtin_property(
                "__is_deprecated",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ));
            methods.push(builtin_reflection_class_bool_method(
                "isDeprecated",
                "__is_deprecated",
            ));
            properties.push(builtin_property(
                "__has_type",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ));
            properties.push(builtin_property(
                "__type",
                Visibility::Private,
                Some(mixed_type()),
                null_expr(),
            ));
            methods.push(builtin_reflection_slot_getter(
                "hasType",
                "__has_type",
                TypeExpr::Bool,
            ));
            methods.push(builtin_reflection_slot_getter(
                "getType",
                "__type",
                mixed_type(),
            ));
        } else {
            methods.push(builtin_reflection_constant_false_bool_method(
                "isDeprecated",
            ));
            methods.push(builtin_reflection_constant_false_bool_method("hasType"));
            methods.push(builtin_reflection_constant_null_mixed_method("getType"));
        }
    }
    if matches!(name, "ReflectionFunction" | "ReflectionMethod") {
        properties.push(builtin_property(
            "__string",
            Visibility::Private,
            Some(TypeExpr::Str),
            empty_string(),
        ));
        properties.push(builtin_property(
            "__parameters",
            Visibility::Private,
            Some(object_array_type("ReflectionParameter")),
            empty_array(),
        ));
        properties.push(builtin_property(
            "__is_deprecated",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__is_generator",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__type",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ));
        properties.push(builtin_property(
            "__has_return_type",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__tentative_type",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ));
        properties.push(builtin_property(
            "__has_tentative_return_type",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__required_parameter_count",
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ));
        methods.push(builtin_reflection_class_array_method(
            "getParameters",
            "__parameters",
            object_array_type("ReflectionParameter"),
        ));
        methods.push(builtin_reflection_parameter_count_method());
        methods.push(builtin_reflection_class_int_method(
            "getNumberOfRequiredParameters",
            "__required_parameter_count",
        ));
        methods.push(builtin_reflection_class_bool_method(
            "hasReturnType",
            "__has_return_type",
        ));
        methods.push(builtin_reflection_class_mixed_method("getReturnType", "__type"));
        methods.push(builtin_reflection_constant_false_bool_method("isClosure"));
        methods.push(builtin_reflection_class_bool_method(
            "isDeprecated",
            "__is_deprecated",
        ));
        methods.push(builtin_reflection_constant_false_bool_method(
            "returnsReference",
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isGenerator",
            "__is_generator",
        ));
        methods.push(builtin_reflection_class_bool_method(
            "hasTentativeReturnType",
            "__has_tentative_return_type",
        ));
        methods.push(builtin_reflection_class_mixed_method(
            "getTentativeReturnType",
            "__tentative_type",
        ));
        methods.push(builtin_reflection_function_method_is_variadic_method());
        methods.push(builtin_reflection_class_string_method(
            "__toString",
            "__string",
        ));
    }
    if name == "ReflectionMethod" {
        properties.push(builtin_property(
            "__has_prototype",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__prototype",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ));
        methods.push(builtin_reflection_class_bool_method(
            "hasPrototype",
            "__has_prototype",
        ));
        methods.push(builtin_reflection_method_get_prototype_method());
        methods.push(builtin_reflection_method_invoke_method());
        methods.push(builtin_reflection_method_invoke_args_method());
        methods.push(builtin_reflection_method_create_from_method_name_method());
        methods.push(builtin_reflection_set_accessible_method());
    }
    if name == "ReflectionFunction" {
        methods.push(builtin_reflection_function_invoke_method());
        methods.push(builtin_reflection_function_invoke_args_method());
        methods.push(builtin_reflection_constant_empty_array_method(
            "getClosureUsedVariables",
        ));
        methods.push(builtin_reflection_constant_false_bool_method(
            "isDisabled",
        ));
    }
    if name == "ReflectionProperty" {
        methods.push(builtin_reflection_set_accessible_method());
    }
    properties.push(builtin_property(
        "__attrs",
        Visibility::Private,
        Some(object_array_type("ReflectionAttribute")),
        empty_array(),
    ));
    methods.push(builtin_reflection_owner_get_attributes_method());
    FlattenedClass {
        name: name.to_string(),
        span: dummy(),
        extends: None,
        implements: Vec::new(),
        is_abstract: false,
        is_final: true,
        is_readonly_class: false,
        properties,
        methods,
        attributes: Vec::new(),
        constants: reflection_owner_constants(name),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    }
}

/// Returns true when PHP exposes `getDocComment()` on this synthetic reflection owner.
pub(super) fn reflection_owner_has_doc_comment_method(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionFunction"
            | "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    )
}

/// Returns true when PHP exposes extension-origin APIs on this reflection owner.
pub(super) fn reflection_owner_has_extension_methods(class_name: &str) -> bool {
    matches!(class_name, "ReflectionFunction" | "ReflectionMethod")
}

/// Returns public class constants exposed by a synthetic reflection owner.
pub(super) fn reflection_owner_constants(class_name: &str) -> Vec<ClassConst> {
    if class_name == "ReflectionMethod" {
        return vec![
            builtin_class_const("IS_PUBLIC", 1),
            builtin_class_const("IS_PROTECTED", 2),
            builtin_class_const("IS_PRIVATE", 4),
            builtin_class_const("IS_STATIC", 16),
            builtin_class_const("IS_FINAL", 32),
            builtin_class_const("IS_ABSTRACT", 64),
        ];
    }
    if class_name == "ReflectionProperty" {
        return vec![
            builtin_class_const("IS_STATIC", 16),
            builtin_class_const("IS_READONLY", 128),
            builtin_class_const("IS_PUBLIC", 1),
            builtin_class_const("IS_PROTECTED", 2),
            builtin_class_const("IS_PRIVATE", 4),
            builtin_class_const("IS_ABSTRACT", 64),
            builtin_class_const("IS_PROTECTED_SET", 2048),
            builtin_class_const("IS_PRIVATE_SET", 4096),
            builtin_class_const("IS_VIRTUAL", 512),
            builtin_class_const("IS_FINAL", 32),
        ];
    }
    if matches!(
        class_name,
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        return vec![
            builtin_class_const("IS_PUBLIC", 1),
            builtin_class_const("IS_PROTECTED", 2),
            builtin_class_const("IS_PRIVATE", 4),
            builtin_class_const("IS_FINAL", 32),
        ];
    }
    Vec::new()
}
