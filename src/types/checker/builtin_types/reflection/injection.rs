//! Purpose:
//! Injects the synthetic Reflection class family into checker metadata.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Redeclaration checks run before any built-in class is inserted.

use super::*;

/// Injects the built-in reflection types into `class_map` after verifying none are already
/// declared. Each type is a dummy shell; runtime population happens in codegen. Returns an error
/// if any reflection name is already in use.
///
/// `register` is `program_may_reference_reflection`'s answer, and it governs the INSERTS ONLY.
/// The redeclaration loop below runs either way, because it is a statement about the USER's
/// declarations rather than about ours: `class ReflectionClass {}` is an error in a program that
/// never uses reflection just as much as in one that does. Gating the check alongside the
/// registration is precisely the bug the SPL gate shipped and had to fix — declaring a name is
/// not REFERENCING it, so the predicate says "no reflection here", nothing is registered, and
/// nothing is left to collide with. It failed silently, which is the worst way for a check to
/// fail.
pub(crate) fn inject_builtin_reflection(
    interface_map: &HashMap<String, super::InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
    trait_names: &HashSet<String>,
    register: bool,
) -> Result<(), CompileError> {
    for builtin_name in [
        "ReflectionAttribute",
        "ReflectionClass",
        "ReflectionObject",
        "ReflectionEnum",
        "ReflectionFunction",
        "ReflectionMethod",
        "ReflectionProperty",
        "ReflectionParameter",
        "ReflectionNamedType",
        "ReflectionUnionType",
        "ReflectionIntersectionType",
        "ReflectionClassConstant",
        "ReflectionEnumUnitCase",
        "ReflectionEnumBackedCase",
    ] {
        let builtin_key = php_symbol_key(builtin_name);
        if interface_map
            .keys()
            .chain(class_map.keys())
            .chain(trait_names.iter())
            .any(|name| php_symbol_key(name) == builtin_key)
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Cannot redeclare built-in reflection type: {}",
                    builtin_name
                ),
            ));
        }
    }

    if !register {
        return Ok(());
    }

    class_map.insert(
        "ReflectionAttribute".to_string(),
        FlattenedClass {
            name: "ReflectionAttribute".to_string(),
            span: dummy(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: true,
            is_readonly_class: false,
            properties: vec![
                builtin_property(
                    "__name",
                    Visibility::Private,
                    Some(TypeExpr::Str),
                    empty_string(),
                ),
                builtin_property(
                    "__args",
                    Visibility::Private,
                    Some(array_type()),
                    empty_array(),
                ),
                builtin_property(
                    "__factory",
                    Visibility::Private,
                    Some(TypeExpr::Int),
                    int_lit(0),
                ),
                builtin_property(
                    "__target",
                    Visibility::Private,
                    Some(TypeExpr::Int),
                    int_lit(0),
                ),
                builtin_property(
                    "__is_repeated",
                    Visibility::Private,
                    Some(TypeExpr::Bool),
                    false_bool(),
                ),
            ],
            methods: vec![
                builtin_reflection_attribute_constructor_method(),
                builtin_reflection_attribute_get_name_method(),
                builtin_reflection_attribute_get_arguments_method(),
                builtin_reflection_attribute_new_instance_method(),
                builtin_reflection_class_int_method("getTarget", "__target"),
                builtin_reflection_class_bool_method("isRepeated", "__is_repeated"),
            ],
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
    class_map.insert("ReflectionClass".to_string(), builtin_reflection_class());
    class_map.insert(
        "ReflectionObject".to_string(),
        builtin_reflection_object_class(),
    );
    class_map.insert("ReflectionEnum".to_string(), builtin_reflection_enum_class());
    class_map.insert("ReflectionFunction".to_string(), builtin_reflection_function());
    class_map.insert(
        "ReflectionMethod".to_string(),
        builtin_reflection_owner_class(
            "ReflectionMethod",
            true,
            vec![
                ("class_name", Some(TypeExpr::Str), None, false),
                (
                    "method_name",
                    Some(TypeExpr::Nullable(Box::new(TypeExpr::Str))),
                    null_expr(),
                    false,
                ),
            ],
        ),
    );
    class_map.insert(
        "ReflectionProperty".to_string(),
        builtin_reflection_owner_class(
            "ReflectionProperty",
            true,
            vec![
                ("class_name", Some(TypeExpr::Str), None, false),
                ("property_name", Some(TypeExpr::Str), None, false),
            ],
        ),
    );
    class_map.insert(
        "ReflectionParameter".to_string(),
        builtin_reflection_parameter(),
    );
    class_map.insert("ReflectionNamedType".to_string(), builtin_reflection_named_type());
    class_map.insert(
        "ReflectionUnionType".to_string(),
        builtin_reflection_union_type(),
    );
    class_map.insert(
        "ReflectionIntersectionType".to_string(),
        builtin_reflection_intersection_type(),
    );
    for class_name in [
        "ReflectionClassConstant",
        "ReflectionEnumUnitCase",
        "ReflectionEnumBackedCase",
    ] {
        class_map.insert(
            class_name.to_string(),
            builtin_reflection_owner_class(
                class_name,
                true,
                vec![
                    ("class_name", Some(TypeExpr::Str), None, false),
                    ("constant_name", Some(TypeExpr::Str), None, false),
                ],
            ),
        );
    }

    Ok(())
}
