//! Purpose:
//! Synthesises the built-in `ReflectionAttribute` class checker metadata so user code can
//! receive `ReflectionAttribute` instances from `class_get_attributes()` without colliding
//! with a user-defined declaration.
//!
//! Called from:
//! - `crate::types::checker::driver::init` (alongside `inject_builtin_throwables`).
//!
//! Key details:
//! - Property and method bodies are dummies; the runtime semantics are implemented by the
//!   `class_get_attributes` builtin and private metadata slots.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{
    ClassMethod, ClassProperty, Expr, ExprKind, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;
use crate::types::PhpType;

use super::super::Checker;

pub(crate) fn inject_builtin_reflection(
    interface_map: &HashMap<String, super::InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
    trait_names: &HashSet<String>,
) -> Result<(), CompileError> {
    let builtin_name = "ReflectionAttribute";
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
                builtin_reflection_attribute_name_property(),
                builtin_reflection_attribute_args_property(),
            ],
            methods: vec![
                builtin_reflection_attribute_constructor_method(),
                builtin_reflection_attribute_get_name_method(),
                builtin_reflection_attribute_get_arguments_method(),
            ],
            attributes: Vec::new(),
            constants: Vec::new(),
        },
    );

    Ok(())
}

fn builtin_reflection_attribute_name_property() -> ClassProperty {
    ClassProperty {
        name: "__name".to_string(),
        visibility: Visibility::Private,
        type_expr: Some(TypeExpr::Str),
        readonly: false,
        is_final: false,
        is_static: false,
        by_ref: false,
        default: Some(Expr::new(
            ExprKind::StringLiteral(String::new()),
            crate::span::Span::dummy(),
        )),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

fn builtin_reflection_attribute_args_property() -> ClassProperty {
    ClassProperty {
        name: "__args".to_string(),
        visibility: Visibility::Private,
        type_expr: Some(TypeExpr::Named(crate::names::Name::unqualified("array"))),
        readonly: false,
        is_final: false,
        is_static: false,
        by_ref: false,
        default: Some(Expr::new(
            ExprKind::ArrayLiteral(Vec::new()),
            crate::span::Span::dummy(),
        )),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

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
    }
}
