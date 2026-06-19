//! Purpose:
//! Synthesises the built-in reflection class checker metadata so user code can
//! receive `ReflectionAttribute` instances and query class/member attributes
//! through a small PHP-compatible Reflection surface.
//!
//! Called from:
//! - `crate::types::checker::driver::init` (alongside `inject_builtin_throwables`).
//!
//! Key details:
//! - Property and method bodies are dummies, private-slot accessors, or small
//!   fallbacks; runtime population is handled by codegen-only reflection constructors.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::names::Name;
use crate::parser::ast::{
    BinOp, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind,
    TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;
use crate::types::PhpType;

use super::super::Checker;

/// Injects the built-in reflection types into `class_map` after verifying
/// none are already declared. Each type is a dummy shell; runtime population
/// happens in codegen. Returns an error if any reflection name is already in use.
pub(crate) fn inject_builtin_reflection(
    interface_map: &HashMap<String, super::InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
    trait_names: &HashSet<String>,
) -> Result<(), CompileError> {
    for builtin_name in [
        "ReflectionAttribute",
        "ReflectionClass",
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

    class_map.insert(
        "ReflectionAttribute".to_string(),
        FlattenedClass {
            name: "ReflectionAttribute".to_string(),
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
            ],
            methods: vec![
                builtin_reflection_attribute_constructor_method(),
                builtin_reflection_attribute_get_name_method(),
                builtin_reflection_attribute_get_arguments_method(),
                builtin_reflection_attribute_new_instance_method(),
            ],
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );
    class_map.insert("ReflectionClass".to_string(), builtin_reflection_class());
    class_map.insert(
        "ReflectionFunction".to_string(),
        builtin_reflection_owner_class(
            "ReflectionFunction",
            true,
            vec![("function", Some(TypeExpr::Str), None, false)],
        ),
    );
    class_map.insert(
        "ReflectionMethod".to_string(),
        builtin_reflection_owner_class(
            "ReflectionMethod",
            true,
            vec![
                ("class_name", Some(TypeExpr::Str), None, false),
                ("method_name", Some(TypeExpr::Str), None, false),
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
        builtin_reflection_parameter_class(),
    );
    class_map.insert(
        "ReflectionNamedType".to_string(),
        builtin_reflection_named_type_class(),
    );
    class_map.insert(
        "ReflectionUnionType".to_string(),
        builtin_reflection_union_type_class(),
    );
    class_map.insert(
        "ReflectionIntersectionType".to_string(),
        builtin_reflection_intersection_type_class(),
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

/// Builds a `ClassProperty` for a built-in reflection type with the given name,
/// visibility, optional type expression, and optional default value.
fn builtin_property(
    name: &str,
    visibility: Visibility,
    type_expr: Option<TypeExpr>,
    default: Option<Expr>,
) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility,
        set_visibility: None,
        type_expr,
        hooks: crate::parser::ast::PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        default,
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Returns a `StringLiteral` expression with an empty string value.
fn empty_string() -> Option<Expr> {
    Some(Expr::new(
        ExprKind::StringLiteral(String::new()),
        crate::span::Span::dummy(),
    ))
}

/// Returns an `ArrayLiteral` expression with no elements.
fn empty_array() -> Option<Expr> {
    Some(Expr::new(
        ExprKind::ArrayLiteral(Vec::new()),
        crate::span::Span::dummy(),
    ))
}

/// Returns a `BoolLiteral(false)` expression.
fn false_bool() -> Option<Expr> {
    Some(Expr::new(
        ExprKind::BoolLiteral(false),
        crate::span::Span::dummy(),
    ))
}

/// Returns a `BoolLiteral(true)` expression.
fn true_bool() -> Option<Expr> {
    Some(Expr::new(
        ExprKind::BoolLiteral(true),
        crate::span::Span::dummy(),
    ))
}

/// Returns an `IntLiteral` expression with the given value.
fn int_lit(value: i64) -> Option<Expr> {
    Some(Expr::new(
        ExprKind::IntLiteral(value),
        crate::span::Span::dummy(),
    ))
}

/// Returns a `null` expression for nullable synthetic property defaults.
fn null_expr() -> Option<Expr> {
    Some(Expr::new(ExprKind::Null, crate::span::Span::dummy()))
}

/// Returns a `TypeExpr` for the unqualified name `array`.
fn array_type() -> TypeExpr {
    TypeExpr::Named(crate::names::Name::unqualified("array"))
}

/// Returns a `TypeExpr` for an indexed array of strings.
fn string_array_type() -> TypeExpr {
    TypeExpr::Array(Box::new(TypeExpr::Str))
}

/// Returns a `TypeExpr` for an indexed array of objects with the given class name.
fn object_array_type(class_name: &str) -> TypeExpr {
    TypeExpr::Array(Box::new(TypeExpr::Named(Name::unqualified(class_name))))
}

/// Returns a nullable object type expression for one synthetic reflection class.
fn nullable_object_type(class_name: &str) -> TypeExpr {
    TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(class_name))))
}

/// Returns a `TypeExpr` for PHP's generic `object` type.
fn object_type() -> TypeExpr {
    TypeExpr::Named(Name::unqualified("object"))
}

/// Returns a `TypeExpr` for the unqualified name `mixed`.
fn mixed_type() -> TypeExpr {
    TypeExpr::Named(crate::names::Name::unqualified("mixed"))
}

/// Returns a `TypeExpr` for PHP's builtin boolean type.
fn bool_type() -> TypeExpr {
    TypeExpr::Bool
}

/// Returns a private parameterless `__construct` method for `ReflectionAttribute`.
fn builtin_reflection_attribute_constructor_method() -> ClassMethod {
    builtin_reflection_private_constructor_method()
}

/// Returns a private parameterless `__construct` for internally materialized reflection objects.
fn builtin_reflection_private_constructor_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "__construct".to_string(),
        visibility: Visibility::Private,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: None,
        body: Vec::new(),
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `getName()` method that returns the private `__name` property
/// as a `Str`.
fn builtin_reflection_attribute_get_name_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "getName".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: "__name".to_string(),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `getArguments()` method that returns the private `__args`
/// property as an `array`.
fn builtin_reflection_attribute_get_arguments_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "getArguments".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(crate::names::Name::unqualified("array"))),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: "__args".to_string(),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `newInstance()` method that returns `null` (placeholder until
/// codegen supplies the real implementation).
fn builtin_reflection_attribute_new_instance_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "newInstance".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(mixed_type()),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(ExprKind::Null, dummy_span))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public variadic `ReflectionClass::newInstance()` method.
///
/// Direct calls are lowered specially so their source arguments become
/// constructor arguments for the reflected class. The no-argument body keeps
/// indirect calls and metadata emission coherent when no argument forwarding is
/// required.
fn builtin_reflection_class_new_instance_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "newInstance".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: Some("args".to_string()),
        variadic_type: None,
        return_type: Some(mixed_type()),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::NewDynamic {
                    name_expr: Box::new(Expr::new(
                        ExprKind::PropertyAccess {
                            object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                            property: "__name".to_string(),
                        },
                        dummy_span,
                    )),
                    args: Vec::new(),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds the `ReflectionClass` shell with a private resolved-name slot,
/// private attribute array slot, public constructor, `getName()`, and
/// `getAttributes()`.
fn builtin_reflection_class() -> FlattenedClass {
    FlattenedClass {
        name: "ReflectionClass".to_string(),
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
                "__attrs",
                Visibility::Private,
                Some(array_type()),
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
                "__is_instantiable",
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
                "__trait_names",
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
                Some(TypeExpr::Str),
                None,
                false,
            )]),
            builtin_reflection_class_string_method("getName", "__name"),
            builtin_reflection_class_string_method("getShortName", "__short_name"),
            builtin_reflection_class_string_method("getNamespaceName", "__namespace_name"),
            builtin_reflection_class_bool_method("inNamespace", "__in_namespace"),
            builtin_reflection_class_array_method(
                "getInterfaceNames",
                "__interface_names",
                string_array_type(),
            ),
            builtin_reflection_class_array_method(
                "getTraitNames",
                "__trait_names",
                string_array_type(),
            ),
            builtin_reflection_class_bool_method("isFinal", "__is_final"),
            builtin_reflection_class_bool_method("isAbstract", "__is_abstract"),
            builtin_reflection_class_bool_method("isInterface", "__is_interface"),
            builtin_reflection_class_bool_method("isTrait", "__is_trait"),
            builtin_reflection_class_bool_method("isEnum", "__is_enum"),
            builtin_reflection_class_bool_method("isReadOnly", "__is_readonly"),
            builtin_reflection_class_bool_method("isInstantiable", "__is_instantiable"),
            builtin_reflection_class_int_method("getModifiers", "__modifiers"),
            builtin_reflection_class_has_name_method("hasMethod", "__method_names", true),
            builtin_reflection_class_has_name_method("hasProperty", "__property_names", false),
            builtin_reflection_class_has_name_method("hasConstant", "__constant_names", false),
            builtin_reflection_class_get_constant_method(),
            builtin_reflection_class_mixed_method("getConstants", "__constants"),
            builtin_reflection_class_array_method(
                "getReflectionConstants",
                "__reflection_constants",
                object_array_type("ReflectionClassConstant"),
            ),
            builtin_reflection_class_get_reflection_constant_method(),
            builtin_reflection_class_implements_interface_method(),
            builtin_reflection_class_is_subclass_of_method(),
            builtin_reflection_class_is_instance_method(),
            builtin_reflection_class_array_method(
                "getMethods",
                "__methods",
                object_array_type("ReflectionMethod"),
            ),
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
            builtin_reflection_class_array_method(
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
            builtin_reflection_owner_get_attributes_method(),
        ],
        attributes: Vec::new(),
        constants: reflection_class_constants(),
        used_traits: Vec::new(),
    }
}

/// Returns the public modifier constants exposed by PHP's `ReflectionClass`.
fn reflection_class_constants() -> Vec<ClassConst> {
    vec![
        builtin_class_const("IS_IMPLICIT_ABSTRACT", 16),
        builtin_class_const("IS_FINAL", 32),
        builtin_class_const("IS_EXPLICIT_ABSTRACT", 64),
        builtin_class_const("IS_READONLY", 65_536),
    ]
}

/// Builds a public integer class constant for a synthetic reflection type.
fn builtin_class_const(name: &str, value: i64) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        value: Expr::new(ExprKind::IntLiteral(value), crate::span::Span::dummy()),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Returns a public `ReflectionClass` string method backed by one private slot.
fn builtin_reflection_class_string_method(method_name: &str, property: &str) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: property.to_string(),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `ReflectionClass` integer method backed by one private slot.
fn builtin_reflection_class_int_method(method_name: &str, property: &str) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: property.to_string(),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `ReflectionClass` membership probe backed by a private string array.
fn builtin_reflection_class_has_name_method(
    method_name: &str,
    property: &str,
    case_insensitive: bool,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let name_arg = Expr::new(ExprKind::Variable("name".to_string()), dummy_span);
    let needle = if case_insensitive {
        Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("strtolower"),
                args: vec![name_arg],
            },
            dummy_span,
        )
    } else {
        name_arg
    };
    let haystack = Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::new(ExprKind::This, dummy_span)),
            property: property.to_string(),
        },
        dummy_span,
    );
    let contains = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("in_array"),
            args: vec![needle, haystack],
        },
        dummy_span,
    );
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("name".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        body: vec![Stmt::new(StmtKind::Return(Some(contains)), dummy_span)],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionClass::getConstant()` backed by the private constant-value map.
fn builtin_reflection_class_get_constant_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let name_arg = Expr::new(ExprKind::Variable("name".to_string()), dummy_span);
    let value = Expr::new(ExprKind::Variable("value".to_string()), dummy_span);
    let value_read = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(reflection_this_property("__constants", dummy_span)),
            index: Box::new(name_arg),
        },
        dummy_span,
    );
    let value_is_present = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(value.clone()),
            op: BinOp::StrictNotEq,
            right: Box::new(Expr::new(ExprKind::Null, dummy_span)),
        },
        dummy_span,
    );
    ClassMethod {
        name: "getConstant".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("name".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(mixed_type()),
        body: vec![
            Stmt::new(
                StmtKind::Assign {
                    name: "value".to_string(),
                    value: value_read,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::If {
                    condition: value_is_present,
                    then_body: vec![Stmt::new(StmtKind::Return(Some(value)), dummy_span)],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Return(Some(Expr::new(ExprKind::BoolLiteral(false), dummy_span))),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionClass::getReflectionConstant()` backed by reflected constant objects.
fn builtin_reflection_class_get_reflection_constant_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let name = variable_expr("name", dummy_span);
    let member = variable_expr("member", dummy_span);
    let exists = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(method_call_expr(
                member.clone(),
                "getName",
                Vec::new(),
                dummy_span,
            )),
            op: BinOp::StrictEq,
            right: Box::new(name.clone()),
        },
        dummy_span,
    );
    ClassMethod {
        name: "getReflectionConstant".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("name".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(mixed_type()),
        body: vec![
            Stmt::new(
                StmtKind::Foreach {
                    array: reflection_this_property("__reflection_constants", dummy_span),
                    key_var: None,
                    value_var: "member".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::If {
                            condition: exists,
                            then_body: vec![Stmt::new(StmtKind::Return(Some(member)), dummy_span)],
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ),
            Stmt::new(StmtKind::Return(false_bool()), dummy_span),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionClass::implementsInterface()` backed by interface-name metadata.
fn builtin_reflection_class_implements_interface_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let interface_var = Expr::new(ExprKind::Variable("interface".to_string()), dummy_span);
    let candidate_var = Expr::new(ExprKind::Variable("interfaceName".to_string()), dummy_span);
    let missing_interface_check = Stmt::new(
        StmtKind::If {
            condition: Expr::new(
                ExprKind::Not(Box::new(function_call(
                    "interface_exists",
                    vec![interface_var.clone()],
                    dummy_span,
                ))),
                dummy_span,
            ),
            then_body: vec![
                throw_if_class_like_exists(
                    "class_exists",
                    interface_var.clone(),
                    concat_expr(
                        interface_var.clone(),
                        string_lit(" is not an interface", dummy_span),
                        dummy_span,
                    ),
                    dummy_span,
                ),
                throw_if_class_like_exists(
                    "trait_exists",
                    interface_var.clone(),
                    concat_expr(
                        interface_var.clone(),
                        string_lit(" is not an interface", dummy_span),
                        dummy_span,
                    ),
                    dummy_span,
                ),
                throw_if_class_like_exists(
                    "enum_exists",
                    interface_var.clone(),
                    concat_expr(
                        interface_var.clone(),
                        string_lit(" is not an interface", dummy_span),
                        dummy_span,
                    ),
                    dummy_span,
                ),
                throw_new_reflection_exception(
                    concat_expr(
                        concat_expr(
                            string_lit("Interface \"", dummy_span),
                            interface_var.clone(),
                            dummy_span,
                        ),
                        string_lit("\" does not exist", dummy_span),
                        dummy_span,
                    ),
                    dummy_span,
                ),
            ],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        dummy_span,
    );
    let lowered_interface = strtolower_call(interface_var.clone(), dummy_span);
    let lowered_candidate = strtolower_call(candidate_var, dummy_span);
    let candidate_matches = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(lowered_candidate),
            op: BinOp::Eq,
            right: Box::new(lowered_interface.clone()),
        },
        dummy_span,
    );
    let reflected_name_matches = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(strtolower_call(
                reflection_this_property("__name", dummy_span),
                dummy_span,
            )),
            op: BinOp::Eq,
            right: Box::new(lowered_interface),
        },
        dummy_span,
    );
    let interface_self_matches = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(reflection_this_property("__is_interface", dummy_span)),
            op: BinOp::And,
            right: Box::new(reflected_name_matches),
        },
        dummy_span,
    );
    ClassMethod {
        name: "implementsInterface".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("interface".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(bool_type()),
        body: vec![
            missing_interface_check,
            Stmt::new(
                StmtKind::Foreach {
                    array: reflection_this_property("__interface_names", dummy_span),
                    key_var: None,
                    value_var: "interfaceName".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::If {
                            condition: candidate_matches,
                            then_body: vec![Stmt::new(StmtKind::Return(true_bool()), dummy_span)],
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::If {
                    condition: interface_self_matches,
                    then_body: vec![Stmt::new(StmtKind::Return(true_bool()), dummy_span)],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(StmtKind::Return(false_bool()), dummy_span),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionClass::isSubclassOf()` backed by parent and interface metadata.
fn builtin_reflection_class_is_subclass_of_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let class_var = variable_expr("class", dummy_span);
    let target_var = variable_expr("target", dummy_span);
    let parent_name_var = variable_expr("parentName", dummy_span);
    let interface_name_var = variable_expr("interfaceName", dummy_span);
    let target_missing = binary_expr(
        binary_expr(
            Expr::new(
                ExprKind::Not(Box::new(function_call(
                    "class_exists",
                    vec![class_var.clone()],
                    dummy_span,
                ))),
                dummy_span,
            ),
            BinOp::And,
            Expr::new(
                ExprKind::Not(Box::new(function_call(
                    "interface_exists",
                    vec![class_var.clone()],
                    dummy_span,
                ))),
                dummy_span,
            ),
            dummy_span,
        ),
        BinOp::And,
        binary_expr(
            Expr::new(
                ExprKind::Not(Box::new(function_call(
                    "trait_exists",
                    vec![class_var.clone()],
                    dummy_span,
                ))),
                dummy_span,
            ),
            BinOp::And,
            Expr::new(
                ExprKind::Not(Box::new(function_call(
                    "enum_exists",
                    vec![class_var.clone()],
                    dummy_span,
                ))),
                dummy_span,
            ),
            dummy_span,
        ),
        dummy_span,
    );
    let missing_target_check = Stmt::new(
        StmtKind::If {
            condition: target_missing,
            then_body: vec![throw_new_reflection_exception(
                concat_expr(
                    concat_expr(
                        string_lit("Class \"", dummy_span),
                        class_var.clone(),
                        dummy_span,
                    ),
                    string_lit("\" does not exist", dummy_span),
                    dummy_span,
                ),
                dummy_span,
            )],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        dummy_span,
    );
    let parent_matches = binary_expr(
        strtolower_call(parent_name_var, dummy_span),
        BinOp::Eq,
        target_var.clone(),
        dummy_span,
    );
    let interface_matches = binary_expr(
        strtolower_call(interface_name_var, dummy_span),
        BinOp::Eq,
        target_var.clone(),
        dummy_span,
    );
    ClassMethod {
        name: "isSubclassOf".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("class".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(bool_type()),
        body: vec![
            missing_target_check,
            Stmt::new(
                StmtKind::Assign {
                    name: "target".to_string(),
                    value: strtolower_call(class_var, dummy_span),
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Foreach {
                    array: reflection_this_property("__parent_names", dummy_span),
                    key_var: None,
                    value_var: "parentName".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::If {
                            condition: parent_matches,
                            then_body: vec![Stmt::new(StmtKind::Return(true_bool()), dummy_span)],
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Foreach {
                    array: reflection_this_property("__interface_names", dummy_span),
                    key_var: None,
                    value_var: "interfaceName".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::If {
                            condition: interface_matches,
                            then_body: vec![Stmt::new(StmtKind::Return(true_bool()), dummy_span)],
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ),
            Stmt::new(StmtKind::Return(false_bool()), dummy_span),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionClass::isInstance()` backed by PHP's class relation predicate.
fn builtin_reflection_class_is_instance_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "isInstance".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("object".to_string(), Some(object_type()), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(bool_type()),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::InstanceOf {
                    value: Box::new(variable_expr("object", dummy_span)),
                    target: InstanceOfTarget::Expr(Box::new(reflection_this_property(
                        "__name", dummy_span,
                    ))),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds `if (<predicate>($interface)) throw new ReflectionException($message);`.
fn throw_if_class_like_exists(
    predicate_name: &str,
    interface_var: Expr,
    message: Expr,
    span: crate::span::Span,
) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition: function_call(predicate_name, vec![interface_var], span),
            then_body: vec![throw_new_reflection_exception(message, span)],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    )
}

/// Builds a normal function call expression for synthetic Reflection method bodies.
fn function_call(name: &str, args: Vec<Expr>, span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified(name),
            args,
        },
        span,
    )
}

/// Builds a binary expression with the given operator and operands.
fn binary_expr(left: Expr, op: BinOp, right: Expr, span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        },
        span,
    )
}

/// Builds a PHP string literal expression for synthetic method bodies.
fn string_lit(value: &str, span: crate::span::Span) -> Expr {
    Expr::new(ExprKind::StringLiteral(value.to_string()), span)
}

/// Builds a PHP string concatenation expression.
fn concat_expr(left: Expr, right: Expr, span: crate::span::Span) -> Expr {
    binary_expr(left, BinOp::Concat, right, span)
}

/// Builds `throw new ReflectionException($message)`.
fn throw_new_reflection_exception(message: Expr, span: crate::span::Span) -> Stmt {
    Stmt::new(
        StmtKind::Throw(Expr::new(
            ExprKind::NewObject {
                class_name: Name::unqualified("ReflectionException"),
                args: vec![message],
            },
            span,
        )),
        span,
    )
}

/// Builds `$this->{$property}` for synthetic ReflectionClass method bodies.
fn reflection_this_property(property: &str, span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::new(ExprKind::This, span)),
            property: property.to_string(),
        },
        span,
    )
}

/// Builds a `strtolower()` call around an expression for case-insensitive class names.
fn strtolower_call(expr: Expr, span: crate::span::Span) -> Expr {
    function_call("strtolower", vec![expr], span)
}

/// Builds a variable expression for synthetic Reflection method bodies.
fn variable_expr(name: &str, span: crate::span::Span) -> Expr {
    Expr::new(ExprKind::Variable(name.to_string()), span)
}

/// Builds a method call expression for synthetic Reflection method bodies.
fn method_call_expr(object: Expr, method: &str, args: Vec<Expr>, span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::MethodCall {
            object: Box::new(object),
            method: method.to_string(),
            args,
        },
        span,
    )
}

/// Returns a public `ReflectionClass` array method backed by one private slot.
fn builtin_reflection_class_array_method(
    method_name: &str,
    property: &str,
    return_type: TypeExpr,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(return_type),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: property.to_string(),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `ReflectionClass::getMethod()` or `getProperty()` lookup method.
fn builtin_reflection_class_get_member_method(
    method_name: &str,
    property: &str,
    return_class: &str,
    case_insensitive: bool,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let name = variable_expr("name", dummy_span);
    let member = variable_expr("member", dummy_span);
    let member_name = method_call_expr(member.clone(), "getName", Vec::new(), dummy_span);
    let left = if case_insensitive {
        strtolower_call(member_name, dummy_span)
    } else {
        member_name
    };
    let right = if case_insensitive {
        strtolower_call(name.clone(), dummy_span)
    } else {
        name.clone()
    };
    let exists = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(left),
            op: if case_insensitive {
                BinOp::Eq
            } else {
                BinOp::StrictEq
            },
            right: Box::new(right),
        },
        dummy_span,
    );
    let message = if return_class == "ReflectionMethod" {
        concat_expr(
            concat_expr(
                concat_expr(
                    reflection_this_property("__name", dummy_span),
                    string_lit("::", dummy_span),
                    dummy_span,
                ),
                name.clone(),
                dummy_span,
            ),
            string_lit("() does not exist", dummy_span),
            dummy_span,
        )
    } else {
        concat_expr(
            concat_expr(
                concat_expr(
                    reflection_this_property("__name", dummy_span),
                    string_lit("::$", dummy_span),
                    dummy_span,
                ),
                name.clone(),
                dummy_span,
            ),
            string_lit(" does not exist", dummy_span),
            dummy_span,
        )
    };
    let message = concat_expr(
        string_lit(
            if return_class == "ReflectionMethod" {
                "Method "
            } else {
                "Property "
            },
            dummy_span,
        ),
        message,
        dummy_span,
    );
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("name".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified(return_class))),
        body: vec![
            Stmt::new(
                StmtKind::Foreach {
                    array: reflection_this_property(property, dummy_span),
                    key_var: None,
                    value_var: "member".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::If {
                            condition: exists,
                            then_body: vec![Stmt::new(StmtKind::Return(Some(member)), dummy_span)],
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ),
            throw_new_reflection_exception(message, dummy_span),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public nullable object `ReflectionClass` method backed by one private slot.
fn builtin_reflection_class_nullable_object_method(
    method_name: &str,
    property: &str,
    class_name: &str,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(nullable_object_type(class_name)),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: property.to_string(),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public mixed `ReflectionClass` method backed by one private slot.
fn builtin_reflection_class_mixed_method(method_name: &str, property: &str) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(mixed_type()),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: property.to_string(),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `ReflectionClass` boolean method backed by one private slot.
fn builtin_reflection_class_bool_method(method_name: &str, property: &str) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(bool_type()),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: property.to_string(),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public Reflection boolean method that always reports one literal value.
fn builtin_reflection_constant_bool_method(method_name: &str, value: bool) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(bool_type()),
        body: vec![Stmt::new(
            StmtKind::Return(if value { true_bool() } else { false_bool() }),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a `ReflectionMethod` predicate derived from its case-insensitive method name.
fn builtin_reflection_method_name_predicate_method(
    method_name: &str,
    expected_name: &str,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let lower_name = strtolower_call(reflection_this_property("__name", dummy_span), dummy_span);
    let comparison = binary_expr(
        lower_name,
        BinOp::StrictEq,
        string_lit(expected_name, dummy_span),
        dummy_span,
    );
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(bool_type()),
        body: vec![Stmt::new(StmtKind::Return(Some(comparison)), dummy_span)],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionProperty::hasType()` backed by a nullable private `__type` slot.
fn builtin_reflection_property_has_type_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "hasType".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(bool_type()),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(Expr::new(
                        ExprKind::PropertyAccess {
                            object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                            property: "__type".to_string(),
                        },
                        dummy_span,
                    )),
                    op: BinOp::StrictNotEq,
                    right: Box::new(Expr::new(ExprKind::Null, dummy_span)),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds a `FlattenedClass` for simple reflection owner classes
/// with a private `__attrs` array property and two methods: `__construct`
/// (public, accepting the supplied params) and `getAttributes` (public,
/// returning the `__attrs` array).
fn builtin_reflection_owner_class(
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
        methods.push(builtin_reflection_class_mixed_method(
            "getDeclaringClass",
            "__declaring_class",
        ));
    }
    if matches!(name, "ReflectionFunction" | "ReflectionMethod") {
        properties.push(builtin_property(
            "__parameters",
            Visibility::Private,
            Some(object_array_type("ReflectionParameter")),
            empty_array(),
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
    }
    properties.push(builtin_property(
        "__attrs",
        Visibility::Private,
        Some(array_type()),
        empty_array(),
    ));
    methods.push(builtin_reflection_owner_get_attributes_method());
    FlattenedClass {
        name: name.to_string(),
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
    }
}

/// Returns public class constants exposed by a synthetic reflection owner.
fn reflection_owner_constants(class_name: &str) -> Vec<ClassConst> {
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
    if class_name == "ReflectionClassConstant" {
        return vec![
            builtin_class_const("IS_PUBLIC", 1),
            builtin_class_const("IS_PROTECTED", 2),
            builtin_class_const("IS_PRIVATE", 4),
            builtin_class_const("IS_FINAL", 32),
        ];
    }
    Vec::new()
}

/// Builds `getNumberOfParameters()` over the retained parameter array.
fn builtin_reflection_parameter_count_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "getNumberOfParameters".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("count"),
                    args: vec![Expr::new(
                        ExprKind::PropertyAccess {
                            object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                            property: "__parameters".to_string(),
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Adds member visibility/staticity predicates for method and property reflection owners.
fn add_reflection_member_flag_methods(
    class_name: &str,
    properties: &mut Vec<ClassProperty>,
    methods: &mut Vec<ClassMethod>,
) {
    let visibility_flags = [
        ("__is_public", "isPublic"),
        ("__is_protected", "isProtected"),
        ("__is_private", "isPrivate"),
    ];
    if matches!(
        class_name,
        "ReflectionMethod" | "ReflectionProperty" | "ReflectionClassConstant"
    ) {
        for (property, method) in visibility_flags {
            properties.push(builtin_property(
                property,
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ));
            methods.push(builtin_reflection_class_bool_method(method, property));
        }
    }
    if matches!(class_name, "ReflectionMethod" | "ReflectionProperty") {
        properties.push(builtin_property(
            "__is_static",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isStatic",
            "__is_static",
        ));
    }
    if class_name == "ReflectionMethod" {
        properties.push(builtin_property(
            "__modifiers",
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ));
        methods.push(builtin_reflection_class_int_method(
            "getModifiers",
            "__modifiers",
        ));
        methods.push(builtin_reflection_method_name_predicate_method(
            "isConstructor",
            "__construct",
        ));
        methods.push(builtin_reflection_method_name_predicate_method(
            "isDestructor",
            "__destruct",
        ));
    }
    if class_name == "ReflectionProperty" {
        properties.push(builtin_property(
            "__type",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ));
        properties.push(builtin_property(
            "__has_default_value",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__default_value",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ));
        properties.push(builtin_property(
            "__modifiers",
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ));
        methods.push(builtin_reflection_class_int_method(
            "getModifiers",
            "__modifiers",
        ));
        methods.push(builtin_reflection_property_has_type_method());
        methods.push(builtin_reflection_class_mixed_method("getType", "__type"));
        methods.push(builtin_reflection_class_bool_method(
            "hasDefaultValue",
            "__has_default_value",
        ));
        methods.push(builtin_reflection_constant_bool_method("isDefault", true));
        methods.push(builtin_reflection_class_mixed_method(
            "getDefaultValue",
            "__default_value",
        ));
        for (property, method) in [("__is_final", "isFinal"), ("__is_abstract", "isAbstract")] {
            properties.push(builtin_property(
                property,
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ));
            methods.push(builtin_reflection_class_bool_method(method, property));
        }
        properties.push(builtin_property(
            "__is_readonly",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isReadOnly",
            "__is_readonly",
        ));
    }
    if class_name == "ReflectionClassConstant" {
        properties.push(builtin_property(
            "__value",
            Visibility::Private,
            Some(mixed_type()),
            Some(Expr::new(ExprKind::Null, crate::span::Span::dummy())),
        ));
        methods.push(builtin_reflection_class_mixed_method("getValue", "__value"));
        properties.push(builtin_property(
            "__is_enum_case",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isEnumCase",
            "__is_enum_case",
        ));
        properties.push(builtin_property(
            "__is_final",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isFinal",
            "__is_final",
        ));
        properties.push(builtin_property(
            "__modifiers",
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ));
        methods.push(builtin_reflection_class_int_method(
            "getModifiers",
            "__modifiers",
        ));
    }
    if matches!(
        class_name,
        "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        properties.push(builtin_property(
            "__value",
            Visibility::Private,
            Some(mixed_type()),
            Some(Expr::new(ExprKind::Null, crate::span::Span::dummy())),
        ));
        methods.push(builtin_reflection_class_mixed_method("getValue", "__value"));
    }
    if class_name == "ReflectionEnumBackedCase" {
        properties.push(builtin_property(
            "__backing_value",
            Visibility::Private,
            Some(mixed_type()),
            Some(Expr::new(ExprKind::Null, crate::span::Span::dummy())),
        ));
        methods.push(builtin_reflection_class_mixed_method(
            "getBackingValue",
            "__backing_value",
        ));
    }
    if class_name == "ReflectionMethod" {
        for (property, method) in [("__is_final", "isFinal"), ("__is_abstract", "isAbstract")] {
            properties.push(builtin_property(
                property,
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ));
            methods.push(builtin_reflection_class_bool_method(method, property));
        }
    }
}

/// Builds the synthetic `ReflectionParameter` shell used by method parameter reflection.
fn builtin_reflection_parameter_class() -> FlattenedClass {
    let properties = vec![
        builtin_property(
            "__name",
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
            "__type",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ),
        builtin_property(
            "__position",
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ),
        builtin_property(
            "__is_optional",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ),
        builtin_property(
            "__is_variadic",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ),
        builtin_property(
            "__is_passed_by_reference",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ),
        builtin_property(
            "__has_type",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ),
        builtin_property(
            "__has_default_value",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ),
        builtin_property(
            "__default_value",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ),
        builtin_property(
            "__declaring_class",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ),
        builtin_property(
            "__declaring_function",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ),
    ];
    let methods = vec![
        builtin_reflection_owner_constructor_method(vec![
            ("function", Some(mixed_type()), None, false),
            ("param", Some(mixed_type()), None, false),
        ]),
        builtin_reflection_class_string_method("getName", "__name"),
        builtin_reflection_class_int_method("getPosition", "__position"),
        builtin_reflection_class_bool_method("isOptional", "__is_optional"),
        builtin_reflection_class_bool_method("isVariadic", "__is_variadic"),
        builtin_reflection_class_bool_method("isPassedByReference", "__is_passed_by_reference"),
        builtin_reflection_class_bool_method("hasType", "__has_type"),
        builtin_reflection_class_mixed_method("getType", "__type"),
        builtin_reflection_owner_get_attributes_method(),
        builtin_reflection_class_bool_method("isDefaultValueAvailable", "__has_default_value"),
        builtin_reflection_parameter_get_default_value_method(),
        builtin_reflection_class_mixed_method("getDeclaringClass", "__declaring_class"),
        builtin_reflection_class_mixed_method("getDeclaringFunction", "__declaring_function"),
    ];
    FlattenedClass {
        name: "ReflectionParameter".to_string(),
        extends: None,
        implements: Vec::new(),
        is_abstract: false,
        is_final: true,
        is_readonly_class: false,
        properties,
        methods,
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
    }
}

/// Builds `ReflectionParameter::getDefaultValue()` over the retained default slot.
fn builtin_reflection_parameter_get_default_value_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "getDefaultValue".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(mixed_type()),
        body: vec![
            Stmt::new(
                StmtKind::If {
                    condition: Expr::new(
                        ExprKind::Not(Box::new(reflection_this_property(
                            "__has_default_value",
                            dummy_span,
                        ))),
                        dummy_span,
                    ),
                    then_body: vec![throw_new_reflection_exception(
                        string_lit(
                            "Internal error: Failed to retrieve the default value",
                            dummy_span,
                        ),
                        dummy_span,
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Return(Some(reflection_this_property(
                    "__default_value",
                    dummy_span,
                ))),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds the synthetic `ReflectionNamedType` shell returned by `ReflectionParameter::getType()`.
fn builtin_reflection_named_type_class() -> FlattenedClass {
    let properties = vec![
        builtin_property(
            "__name",
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
            "__allows_null",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ),
        builtin_property(
            "__is_builtin",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ),
    ];
    let methods = vec![
        builtin_reflection_private_constructor_method(),
        builtin_reflection_class_string_method("getName", "__name"),
        builtin_reflection_class_bool_method("allowsNull", "__allows_null"),
        builtin_reflection_class_bool_method("isBuiltin", "__is_builtin"),
    ];
    FlattenedClass {
        name: "ReflectionNamedType".to_string(),
        extends: None,
        implements: Vec::new(),
        is_abstract: false,
        is_final: true,
        is_readonly_class: false,
        properties,
        methods,
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
    }
}

/// Builds the synthetic `ReflectionUnionType` shell returned by `ReflectionParameter::getType()`.
fn builtin_reflection_union_type_class() -> FlattenedClass {
    let properties = vec![
        builtin_property(
            "__types",
            Visibility::Private,
            Some(object_array_type("ReflectionNamedType")),
            empty_array(),
        ),
        builtin_property(
            "__attrs",
            Visibility::Private,
            Some(object_array_type("ReflectionAttribute")),
            empty_array(),
        ),
        builtin_property(
            "__allows_null",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ),
    ];
    let methods = vec![
        builtin_reflection_private_constructor_method(),
        builtin_reflection_class_array_method(
            "getTypes",
            "__types",
            object_array_type("ReflectionNamedType"),
        ),
        builtin_reflection_class_bool_method("allowsNull", "__allows_null"),
    ];
    FlattenedClass {
        name: "ReflectionUnionType".to_string(),
        extends: None,
        implements: Vec::new(),
        is_abstract: false,
        is_final: true,
        is_readonly_class: false,
        properties,
        methods,
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
    }
}

/// Builds the synthetic `ReflectionIntersectionType` shell returned by `ReflectionParameter::getType()`.
fn builtin_reflection_intersection_type_class() -> FlattenedClass {
    let properties = vec![
        builtin_property(
            "__types",
            Visibility::Private,
            Some(object_array_type("ReflectionNamedType")),
            empty_array(),
        ),
        builtin_property(
            "__attrs",
            Visibility::Private,
            Some(object_array_type("ReflectionAttribute")),
            empty_array(),
        ),
        builtin_property(
            "__allows_null",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ),
    ];
    let methods = vec![
        builtin_reflection_private_constructor_method(),
        builtin_reflection_class_array_method(
            "getTypes",
            "__types",
            object_array_type("ReflectionNamedType"),
        ),
        builtin_reflection_class_bool_method("allowsNull", "__allows_null"),
    ];
    FlattenedClass {
        name: "ReflectionIntersectionType".to_string(),
        extends: None,
        implements: Vec::new(),
        is_abstract: false,
        is_final: true,
        is_readonly_class: false,
        properties,
        methods,
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
    }
}

/// Builds a public `__construct` method for a reflection owner class using the
/// provided parameter list: each tuple is (name, type_expr, default, by_ref).
fn builtin_reflection_owner_constructor_method(
    params: Vec<(&str, Option<TypeExpr>, Option<Expr>, bool)>,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "__construct".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: params
            .into_iter()
            .map(|(name, ty, default, by_ref)| (name.to_string(), ty, default, by_ref))
            .collect(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: None,
        body: Vec::new(),
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `getAttributes()` method that returns the private `__attrs`
/// property as an `array` of `ReflectionAttribute` objects.
fn builtin_reflection_owner_get_attributes_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "getAttributes".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(array_type()),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: "__attrs".to_string(),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Overrides the return types on the synthesized reflection class methods inside
/// `checker` to match PHP's actual signatures:
/// - `__construct` → `void`
/// - `getName` / `getArguments` → `string` / `array`
/// - `newInstance` → `mixed`
/// - `getAttributes` → `array<ReflectionAttribute>`
pub(crate) fn patch_builtin_reflection_signatures(checker: &mut Checker) {
    if let Some(class_info) = checker.classes.get_mut("ReflectionAttribute") {
        if let Some(sig) = class_info.methods.get_mut("__construct") {
            sig.return_type = PhpType::Void;
        }
        if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getName")) {
            sig.return_type = PhpType::Str;
        }
        if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getArguments")) {
            sig.return_type = PhpType::Array(Box::new(PhpType::Mixed));
        }
        if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("newInstance")) {
            sig.return_type = PhpType::Mixed;
        }
    }
    for class_name in [
        "ReflectionClass",
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
        if let Some(class_info) = checker.classes.get_mut(class_name) {
            if let Some(sig) = class_info.methods.get_mut("__construct") {
                sig.return_type = PhpType::Void;
            }
            if matches!(
                class_name,
                "ReflectionClass"
                    | "ReflectionFunction"
                    | "ReflectionMethod"
                    | "ReflectionProperty"
                    | "ReflectionParameter"
                    | "ReflectionNamedType"
                    | "ReflectionUnionType"
                    | "ReflectionIntersectionType"
                    | "ReflectionClassConstant"
                    | "ReflectionEnumUnitCase"
                    | "ReflectionEnumBackedCase"
            ) {
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getName")) {
                    sig.return_type = PhpType::Str;
                }
            }
            if class_name == "ReflectionClass" {
                for method_name in [
                    "isfinal",
                    "isabstract",
                    "isinterface",
                    "istrait",
                    "isenum",
                    "isreadonly",
                    "isinstantiable",
                    "hasmethod",
                    "hasproperty",
                    "implementsinterface",
                    "issubclassof",
                    "isinstance",
                ] {
                    if let Some(sig) = class_info.methods.get_mut(method_name) {
                        sig.return_type = PhpType::Bool;
                    }
                }
                for method_name in ["getinterfacenames", "gettraitnames"] {
                    if let Some(sig) = class_info.methods.get_mut(method_name) {
                        sig.return_type = PhpType::Array(Box::new(PhpType::Str));
                    }
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getMethods")) {
                    sig.return_type =
                        PhpType::Array(Box::new(PhpType::Object("ReflectionMethod".to_string())));
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getMethod")) {
                    sig.return_type = PhpType::Object("ReflectionMethod".to_string());
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("getReflectionConstants"))
                {
                    sig.return_type = PhpType::Array(Box::new(PhpType::Object(
                        "ReflectionClassConstant".to_string(),
                    )));
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("getReflectionConstant"))
                {
                    sig.return_type = PhpType::Mixed;
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getProperties")) {
                    sig.return_type =
                        PhpType::Array(Box::new(PhpType::Object("ReflectionProperty".to_string())));
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getProperty")) {
                    sig.return_type = PhpType::Object("ReflectionProperty".to_string());
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("getConstructor"))
                {
                    sig.return_type = PhpType::Union(vec![
                        PhpType::Object("ReflectionMethod".to_string()),
                        PhpType::Void,
                    ]);
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("getParentClass"))
                {
                    sig.return_type = PhpType::Mixed;
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getModifiers")) {
                    sig.return_type = PhpType::Int;
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("newInstance")) {
                    sig.return_type = PhpType::Mixed;
                    sig.variadic = Some("args".to_string());
                    if !sig.params.iter().any(|(name, _)| name == "args") {
                        sig.params
                            .push(("args".to_string(), PhpType::Array(Box::new(PhpType::Mixed))));
                        sig.param_type_exprs.push(None);
                        sig.defaults.push(None);
                        sig.ref_params.push(false);
                        sig.declared_params.push(false);
                    }
                }
            }
            if matches!(class_name, "ReflectionMethod" | "ReflectionProperty") {
                for method_name in ["isstatic", "ispublic", "isprotected", "isprivate"] {
                    if let Some(sig) = class_info.methods.get_mut(method_name) {
                        sig.return_type = PhpType::Bool;
                    }
                }
            }
            if class_name == "ReflectionProperty" {
                for method_name in ["isfinal", "isabstract", "isreadonly", "isdefault"] {
                    if let Some(sig) = class_info.methods.get_mut(method_name) {
                        sig.return_type = PhpType::Bool;
                    }
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("hasDefaultValue"))
                {
                    sig.return_type = PhpType::Bool;
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("getDefaultValue"))
                {
                    sig.return_type = PhpType::Mixed;
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getModifiers")) {
                    sig.return_type = PhpType::Int;
                }
            }
            if class_name == "ReflectionMethod" {
                for method_name in ["isfinal", "isabstract", "isconstructor", "isdestructor"] {
                    if let Some(sig) = class_info.methods.get_mut(method_name) {
                        sig.return_type = PhpType::Bool;
                    }
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getModifiers")) {
                    sig.return_type = PhpType::Int;
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getParameters")) {
                    sig.return_type = PhpType::Array(Box::new(PhpType::Object(
                        "ReflectionParameter".to_string(),
                    )));
                }
                for method_name in ["getNumberOfParameters", "getNumberOfRequiredParameters"] {
                    if let Some(sig) = class_info.methods.get_mut(&php_symbol_key(method_name)) {
                        sig.return_type = PhpType::Int;
                    }
                }
            }
            if class_name == "ReflectionFunction" {
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getParameters")) {
                    sig.return_type = PhpType::Array(Box::new(PhpType::Object(
                        "ReflectionParameter".to_string(),
                    )));
                }
                for method_name in ["getNumberOfParameters", "getNumberOfRequiredParameters"] {
                    if let Some(sig) = class_info.methods.get_mut(&php_symbol_key(method_name)) {
                        sig.return_type = PhpType::Int;
                    }
                }
            }
            if class_name == "ReflectionParameter" {
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getPosition")) {
                    sig.return_type = PhpType::Int;
                }
                for method_name in [
                    "isoptional",
                    "isvariadic",
                    "ispassedbyreference",
                    "hastype",
                    "isdefaultvalueavailable",
                ] {
                    if let Some(sig) = class_info.methods.get_mut(method_name) {
                        sig.return_type = PhpType::Bool;
                    }
                }
                for method_name in ["getType", "getDefaultValue"] {
                    if let Some(sig) = class_info.methods.get_mut(&php_symbol_key(method_name)) {
                        sig.return_type = PhpType::Mixed;
                    }
                }
            }
            if class_name == "ReflectionNamedType" {
                for method_name in ["allowsnull", "isbuiltin"] {
                    if let Some(sig) = class_info.methods.get_mut(method_name) {
                        sig.return_type = PhpType::Bool;
                    }
                }
            }
            if class_name == "ReflectionUnionType" {
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getTypes")) {
                    sig.return_type = PhpType::Array(Box::new(PhpType::Object(
                        "ReflectionNamedType".to_string(),
                    )));
                }
                if let Some(sig) = class_info.methods.get_mut("allowsnull") {
                    sig.return_type = PhpType::Bool;
                }
            }
            if class_name == "ReflectionIntersectionType" {
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getTypes")) {
                    sig.return_type = PhpType::Array(Box::new(PhpType::Object(
                        "ReflectionNamedType".to_string(),
                    )));
                }
                if let Some(sig) = class_info.methods.get_mut("allowsnull") {
                    sig.return_type = PhpType::Bool;
                }
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getAttributes")) {
                sig.return_type =
                    PhpType::Array(Box::new(PhpType::Object("ReflectionAttribute".to_string())));
            }
        }
    }
}
