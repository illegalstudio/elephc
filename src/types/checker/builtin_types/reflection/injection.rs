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
        "ReflectionFunctionAbstract",
        "ReflectionFunction",
        "ReflectionMethod",
        "ReflectionProperty",
        "ReflectionParameter",
        "ReflectionReference",
        "ReflectionType",
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
                builtin_property("name", Visibility::Public, Some(TypeExpr::Str), empty_string()),
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
                // Filled when the object is materialized, like every other reflection class's
                // `__string`. BOTH materializers fill it: the compile-time emitter in
                // `lower_inst::builtins::attributes`, and `__elephc_eval_reflection_attribute_new`
                // for the eval bridge, which is the one a Symfony build actually uses.
                builtin_property(
                    "__string",
                    Visibility::Private,
                    Some(TypeExpr::Str),
                    empty_string(),
                ),
            ],
            methods: vec![
                builtin_reflection_attribute_constructor_method(),
                builtin_reflection_attribute_get_name_method(),
                builtin_reflection_attribute_get_arguments_method(),
                builtin_reflection_attribute_new_instance_method(),
                builtin_reflection_class_int_method("getTarget", "__target"),
                builtin_reflection_class_bool_method("isRepeated", "__is_repeated"),
                builtin_reflection_class_string_method("__toString", "__string"),
            ],
            attributes: Vec::new(),
            // PHP declares exactly one constant on this class, and it is the second argument to
            // every `getAttributes()` call that wants subclasses as well as exact matches. Without
            // it the flag is unnameable from PHP, so the filter added for issue #983 could only be
            // reached by writing the bare literal 2.
            constants: vec![builtin_class_const("IS_INSTANCEOF", 2)],
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
    let mut reflection_function = builtin_reflection_function();
    let mut reflection_method = builtin_reflection_owner_class(
            "ReflectionMethod",
            true,
            vec![
                ("class_name", Some(class_string_or_object_type()), None, false),
                (
                    "method_name",
                    Some(TypeExpr::Nullable(Box::new(TypeExpr::Str))),
                    null_expr(),
                    false,
                ),
            ],
    );
    let reflection_function_abstract = builtin_reflection_function_abstract_class(
        &reflection_function,
        &reflection_method,
    );
    if crate::reflection_table::enabled() {
        member_table_ctor::apply_method_table(&mut reflection_method);
    }
    if crate::reflection_table::enabled() {
        member_table_ctor::apply_function_table(&mut reflection_function);
    }
    reflection_function.extends = Some("ReflectionFunctionAbstract".to_string());
    reflection_function.is_final = false;
    reflection_method.extends = Some("ReflectionFunctionAbstract".to_string());
    reflection_method.is_final = false;
    class_map.insert(
        "ReflectionFunctionAbstract".to_string(),
        reflection_function_abstract,
    );
    class_map.insert("ReflectionFunction".to_string(), reflection_function);
    class_map.insert("ReflectionMethod".to_string(), reflection_method);
    let mut reflection_property = builtin_reflection_owner_class(
        "ReflectionProperty",
        true,
        vec![
            ("class_name", Some(class_string_or_object_type()), None, false),
            ("property_name", Some(TypeExpr::Str), None, false),
        ],
    );
    if crate::reflection_table::enabled() {
        member_table_ctor::apply_property_table(&mut reflection_property);
    }
    class_map.insert("ReflectionProperty".to_string(), reflection_property);
    let mut reflection_parameter = builtin_reflection_parameter();
    if crate::reflection_table::enabled() {
        member_table_ctor::apply_parameter_table(&mut reflection_parameter);
    }
    class_map.insert("ReflectionParameter".to_string(), reflection_parameter);
    class_map.insert(
        "ReflectionReference".to_string(),
        builtin_reflection_reference(),
    );
    class_map.insert("ReflectionType".to_string(), builtin_reflection_type());
    let mut named_type = builtin_reflection_named_type();
    let mut union_type = builtin_reflection_union_type();
    let mut intersection_type = builtin_reflection_intersection_type();
    if crate::reflection_table::enabled() {
        member_table_ctor::apply_type_tables(&mut named_type, &mut union_type, &mut intersection_type);
    }
    class_map.insert("ReflectionNamedType".to_string(), named_type);
    class_map.insert("ReflectionUnionType".to_string(), union_type);
    class_map.insert("ReflectionIntersectionType".to_string(), intersection_type);
    for class_name in [
        "ReflectionClassConstant",
        "ReflectionEnumUnitCase",
        "ReflectionEnumBackedCase",
    ] {
        let mut class = builtin_reflection_owner_class(
            class_name,
            true,
            vec![
                ("class_name", Some(class_string_or_object_type()), None, false),
                ("constant_name", Some(TypeExpr::Str), None, false),
            ],
        );
        if crate::reflection_table::enabled() {
            let kind = match class_name {
                "ReflectionClassConstant" => member_table_ctor::ConstantKind::Constant,
                "ReflectionEnumUnitCase" => member_table_ctor::ConstantKind::UnitCase,
                _ => member_table_ctor::ConstantKind::BackedCase,
            };
            member_table_ctor::apply_constant_table(&mut class, kind);
        }
        class_map.insert(class_name.to_string(), class);
    }

    Ok(())
}
