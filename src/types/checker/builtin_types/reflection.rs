//! Purpose:
//! Synthesises the built-in reflection class checker metadata so user code can
//! receive `ReflectionAttribute` instances and query class/member attributes
//! through a small PHP-compatible Reflection surface.
//!
//! Called from:
//! - `crate::types::checker::driver::init` (alongside `inject_builtin_throwables`).
//!
//! Key details:
//! - Property and method bodies are dummies or simple private-slot accessors;
//!   runtime population is handled by codegen-only reflection constructors.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{
    ClassMethod, ClassProperty, Expr, ExprKind, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;
use crate::types::PhpType;

use super::super::Checker;

/// Injects the four built-in reflection types into `class_map` after verifying
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
        "ReflectionMethod",
        "ReflectionProperty",
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
                &format!("Cannot redeclare built-in reflection type: {}", builtin_name),
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
                builtin_property("__name", Visibility::Private, Some(TypeExpr::Str), empty_string()),
                builtin_property("__args", Visibility::Private, Some(array_type()), empty_array()),
                builtin_property("__factory", Visibility::Private, Some(TypeExpr::Int), int_lit(0)),
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
    class_map.insert(
        "ReflectionClass".to_string(),
        builtin_reflection_owner_class(
            "ReflectionClass",
            vec![("class_name", Some(TypeExpr::Str), None, false)],
        ),
    );
    class_map.insert(
        "ReflectionMethod".to_string(),
        builtin_reflection_owner_class(
            "ReflectionMethod",
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
            vec![
                ("class_name", Some(TypeExpr::Str), None, false),
                ("property_name", Some(TypeExpr::Str), None, false),
            ],
        ),
    );

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

/// Returns an `IntLiteral` expression with the given value.
fn int_lit(value: i64) -> Option<Expr> {
    Some(Expr::new(
        ExprKind::IntLiteral(value),
        crate::span::Span::dummy(),
    ))
}

/// Returns a `TypeExpr` for the unqualified name `array`.
fn array_type() -> TypeExpr {
    TypeExpr::Named(crate::names::Name::unqualified("array"))
}

/// Returns a `TypeExpr` for the unqualified name `mixed`.
fn mixed_type() -> TypeExpr {
    TypeExpr::Named(crate::names::Name::unqualified("mixed"))
}

/// Returns a private parameterless `__construct` method for `ReflectionAttribute`.
fn builtin_reflection_attribute_constructor_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "__construct".to_string(),
        visibility: Visibility::Private,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        variadic: None,
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
        variadic: None,
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
        variadic: None,
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
        variadic: None,
        return_type: Some(mixed_type()),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(ExprKind::Null, dummy_span))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds a `FlattenedClass` for `ReflectionClass`, `ReflectionMethod`, or
/// `ReflectionProperty` with a private `__attrs` array property and two methods:
/// `__construct` (public, accepting the supplied params) and `getAttributes`
/// (public, returning the `__attrs` array).
fn builtin_reflection_owner_class(
    name: &str,
    constructor_params: Vec<(&str, Option<TypeExpr>, Option<Expr>, bool)>,
) -> FlattenedClass {
    FlattenedClass {
        name: name.to_string(),
        extends: None,
        implements: Vec::new(),
        is_abstract: false,
        is_final: true,
        is_readonly_class: false,
        properties: vec![builtin_property(
            "__attrs",
            Visibility::Private,
            Some(array_type()),
            empty_array(),
        )],
        methods: vec![
            builtin_reflection_owner_constructor_method(constructor_params),
            builtin_reflection_owner_get_attributes_method(),
        ],
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
        variadic: None,
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
        variadic: None,
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
    for class_name in ["ReflectionClass", "ReflectionMethod", "ReflectionProperty"] {
        if let Some(class_info) = checker.classes.get_mut(class_name) {
            if let Some(sig) = class_info.methods.get_mut("__construct") {
                sig.return_type = PhpType::Void;
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getAttributes")) {
                sig.return_type = PhpType::Array(Box::new(PhpType::Object(
                    "ReflectionAttribute".to_string(),
                )));
            }
        }
    }
}
