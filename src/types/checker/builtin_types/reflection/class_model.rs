//! Purpose:
//! Builds ReflectionClass, ReflectionObject, and ReflectionEnum shells.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Flattened inheritance exposes only methods supported by each synthetic owner.

use super::*;

/// Builds the `ReflectionClass` shell with retained eval metadata accessors.
pub(super) fn builtin_reflection_class() -> FlattenedClass {
    FlattenedClass {
        name: "ReflectionClass".to_string(),
        span: dummy(),
        extends: None,
        implements: Vec::new(),
        is_abstract: false,
        is_final: false,
        is_readonly_class: false,
        properties: vec![
            builtin_property(
                "__name",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__string",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__attrs",
                Visibility::Private,
                Some(object_array_type("ReflectionAttribute")),
                empty_array(),
            ),
            builtin_property(
                "__is_final",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_abstract",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_interface",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_trait",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_enum",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_readonly",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_anonymous",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_instantiable",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_cloneable",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_iterable",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_internal",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_user_defined",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__modifiers",
                Visibility::Private,
                Some(TypeExpr::Int),
                int_lit(0),
            ),
            builtin_property(
                "__short_name",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__namespace_name",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__in_namespace",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__interface_names",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__interfaces",
                Visibility::Private,
                Some(object_array_type("ReflectionClass")),
                empty_array(),
            ),
            builtin_property(
                "__trait_names",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__traits",
                Visibility::Private,
                Some(object_array_type("ReflectionClass")),
                empty_array(),
            ),
            builtin_property(
                "__trait_aliases",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__parent_names",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__method_names",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__property_names",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__constant_names",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__constants",
                Visibility::Private,
                Some(mixed_type()),
                empty_array(),
            ),
            builtin_property(
                "__default_properties",
                Visibility::Private,
                Some(mixed_type()),
                empty_array(),
            ),
            builtin_property(
                "__static_properties",
                Visibility::Private,
                Some(mixed_type()),
                empty_array(),
            ),
            builtin_property(
                "__reflection_constants",
                Visibility::Private,
                Some(object_array_type("ReflectionClassConstant")),
                empty_array(),
            ),
            builtin_property(
                "__methods",
                Visibility::Private,
                Some(object_array_type("ReflectionMethod")),
                empty_array(),
            ),
            builtin_property(
                "__constructor",
                Visibility::Private,
                Some(nullable_object_type("ReflectionMethod")),
                null_expr(),
            ),
            builtin_property(
                "__parent_class",
                Visibility::Private,
                Some(mixed_type()),
                false_bool(),
            ),
            builtin_property(
                "__properties",
                Visibility::Private,
                Some(object_array_type("ReflectionProperty")),
                empty_array(),
            ),
        ],
        methods: vec![
            builtin_reflection_owner_constructor_method(vec![(
                "class_name",
                Some(mixed_type()),
                None,
                false,
            )]),
            builtin_reflection_class_string_method("getName", "__name"),
            builtin_reflection_class_string_method("__toString", "__string"),
            builtin_reflection_constant_false_union_method("getDocComment"),
            builtin_reflection_constant_false_union_method("getExtensionName"),
            builtin_reflection_constant_null_mixed_method("getExtension"),
            builtin_reflection_class_string_method("getShortName", "__short_name"),
            builtin_reflection_class_string_method("getNamespaceName", "__namespace_name"),
            builtin_reflection_class_bool_method("inNamespace", "__in_namespace"),
            builtin_reflection_class_array_method(
                "getInterfaceNames",
                "__interface_names",
                string_array_type(),
            ),
            builtin_reflection_class_array_method(
                "getInterfaces",
                "__interfaces",
                object_array_type("ReflectionClass"),
            ),
            builtin_reflection_class_array_method(
                "getTraitNames",
                "__trait_names",
                string_array_type(),
            ),
            builtin_reflection_class_array_method(
                "getTraits",
                "__traits",
                object_array_type("ReflectionClass"),
            ),
            builtin_reflection_class_array_method(
                "getTraitAliases",
                "__trait_aliases",
                string_array_type(),
            ),
            builtin_reflection_class_bool_method("isFinal", "__is_final"),
            builtin_reflection_class_bool_method("isAbstract", "__is_abstract"),
            builtin_reflection_class_bool_method("isInterface", "__is_interface"),
            builtin_reflection_class_bool_method("isTrait", "__is_trait"),
            builtin_reflection_class_bool_method("isEnum", "__is_enum"),
            builtin_reflection_class_bool_method("isReadOnly", "__is_readonly"),
            builtin_reflection_class_bool_method("isAnonymous", "__is_anonymous"),
            builtin_reflection_class_bool_method("isInstantiable", "__is_instantiable"),
            builtin_reflection_class_bool_method("isCloneable", "__is_cloneable"),
            builtin_reflection_class_bool_method("isIterable", "__is_iterable"),
            builtin_reflection_class_bool_method("isIterateable", "__is_iterable"),
            builtin_reflection_class_bool_method("isInternal", "__is_internal"),
            builtin_reflection_class_bool_method("isUserDefined", "__is_user_defined"),
            builtin_reflection_class_int_method("getModifiers", "__modifiers"),
            builtin_reflection_class_has_name_method("hasMethod", "__method_names", true),
            builtin_reflection_class_has_name_method("hasProperty", "__property_names", false),
            builtin_reflection_class_has_name_method("hasConstant", "__constant_names", false),
            builtin_reflection_class_get_constant_method(),
            builtin_reflection_class_mixed_method("getConstants", "__constants"),
            builtin_reflection_class_mixed_method("getDefaultProperties", "__default_properties"),
            builtin_reflection_class_mixed_method("getStaticProperties", "__static_properties"),
            builtin_reflection_class_get_static_property_value_method(),
            builtin_reflection_class_set_static_property_value_method(),
            builtin_reflection_class_array_method(
                "getReflectionConstants",
                "__reflection_constants",
                object_array_type("ReflectionClassConstant"),
            ),
            builtin_reflection_class_get_reflection_constant_method(),
            builtin_reflection_class_implements_interface_method(),
            builtin_reflection_class_is_subclass_of_method(),
            builtin_reflection_class_is_instance_method(),
            builtin_reflection_class_get_member_method(
                "getMethod",
                "__methods",
                "ReflectionMethod",
                true,
            ),
            builtin_reflection_class_nullable_object_method(
                "getConstructor",
                "__constructor",
                "ReflectionMethod",
            ),
            builtin_reflection_class_mixed_method("getParentClass", "__parent_class"),
            builtin_reflection_class_filtered_array_method(
                "getMethods",
                "__methods",
                object_array_type("ReflectionMethod"),
            ),
            builtin_reflection_class_filtered_array_method(
                "getProperties",
                "__properties",
                object_array_type("ReflectionProperty"),
            ),
            builtin_reflection_class_get_member_method(
                "getProperty",
                "__properties",
                "ReflectionProperty",
                false,
            ),
            builtin_reflection_class_new_instance_method(),
            builtin_reflection_class_new_instance_args_method(),
            builtin_reflection_class_new_instance_without_constructor_method(),
            builtin_reflection_owner_get_attributes_method(),
        ],
        attributes: Vec::new(),
        constants: reflection_class_constants(),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    }
}

/// Builds the synthetic `ReflectionObject` class with ReflectionClass metadata slots.
pub(super) fn builtin_reflection_object_class() -> FlattenedClass {
    let mut class = builtin_reflection_class();
    class.name = "ReflectionObject".to_string();
    class.extends = Some("ReflectionClass".to_string());
    class.properties.push(builtin_property(
        "__object",
        Visibility::Private,
        Some(mixed_type()),
        null_expr(),
    ));
    if let Some(constructor) = class
        .methods
        .iter_mut()
        .find(|method| method.name == "__construct")
    {
        *constructor = builtin_reflection_owner_constructor_method(vec![(
            "object",
            Some(object_type()),
            None,
            false,
        )]);
    }
    class
}

/// Builds the synthetic `ReflectionEnum` class with flattened ReflectionClass members.
pub(super) fn builtin_reflection_enum_class() -> FlattenedClass {
    let mut class = builtin_reflection_class();
    class.name = "ReflectionEnum".to_string();
    class
        .methods
        .retain(|method| reflection_enum_inherited_method_is_supported(&method.name));
    class.properties.extend([
        builtin_property(
            "__case_names",
            Visibility::Private,
            Some(string_array_type()),
            empty_array(),
        ),
        builtin_property(
            "__cases",
            Visibility::Private,
            Some(array_type()),
            empty_array(),
        ),
        builtin_property(
            "__is_backed",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ),
        builtin_property(
            "__backing_type",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ),
    ]);
    class.methods.extend([
        builtin_reflection_class_bool_method("isBacked", "__is_backed"),
        builtin_reflection_class_mixed_method("getBackingType", "__backing_type"),
    ]);
    class
}

/// Returns whether a flattened ReflectionClass method is safe on ReflectionEnum.
pub(super) fn reflection_enum_inherited_method_is_supported(method_name: &str) -> bool {
    matches!(
        method_name.to_ascii_lowercase().as_str(),
        "__construct"
            | "__tostring"
            | "getname"
            | "getshortname"
            | "getnamespacename"
            | "innamespace"
            | "isfinal"
            | "isabstract"
            | "isinterface"
            | "istrait"
            | "isenum"
            | "isreadonly"
            | "isanonymous"
            | "isinstantiable"
            | "iscloneable"
            | "isiterable"
            | "isiterateable"
            | "isinternal"
            | "isuserdefined"
            | "getmodifiers"
            | "getattributes"
            | "getdoccomment"
            | "getextensionname"
            | "getextension"
            | "getfilename"
            | "getstartline"
            | "getendline"
    )
}
