//! Purpose:
//! Builds and patches checker metadata for PHP builtin exception types.
//! Supplies synthetic declarations or contract validation for classes and interfaces that user code may reference.
//!
//! Called from:
//! - `crate::types::checker::builtin_types`
//! - `crate::types::checker::driver::init`
//!
//! Key details:
//! - Dummy AST members carry type contracts only; runtime behavior is implemented elsewhere.

use crate::names::php_symbol_key;
use crate::parser::ast::{
    ClassMethod, ClassProperty, Expr, ExprKind, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::types::PhpType;

use super::super::Checker;

pub(super) fn builtin_exception_message_property() -> ClassProperty {
    ClassProperty {
        name: "message".to_string(),
        visibility: Visibility::Public,
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

pub(super) fn builtin_exception_constructor_method() -> ClassMethod {
    ClassMethod {
        name: "__construct".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "message".to_string(),
                None,
                Some(Expr::new(
                    ExprKind::StringLiteral(String::new()),
                    crate::span::Span::dummy(),
                )),
                false,
            ),
            (
                "code".to_string(),
                None,
                Some(Expr::new(
                    ExprKind::IntLiteral(0),
                    crate::span::Span::dummy(),
                )),
                false,
            ),
        ],
        variadic: None,
        return_type: None,
        body: vec![
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(Expr::new(ExprKind::This, crate::span::Span::dummy())),
                    property: "message".to_string(),
                    value: Expr::new(
                        ExprKind::Variable("message".to_string()),
                        crate::span::Span::dummy(),
                    ),
                },
                crate::span::Span::dummy(),
            ),
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(Expr::new(ExprKind::This, crate::span::Span::dummy())),
                    property: "code".to_string(),
                    value: Expr::new(
                        ExprKind::Variable("code".to_string()),
                        crate::span::Span::dummy(),
                    ),
                },
                crate::span::Span::dummy(),
            ),
        ],
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

pub(super) fn builtin_exception_code_property() -> ClassProperty {
    ClassProperty {
        name: "code".to_string(),
        visibility: Visibility::Protected,
        type_expr: Some(TypeExpr::Int),
        readonly: false,
        is_final: false,
        is_static: false,
        by_ref: false,
        default: Some(Expr::new(
            ExprKind::IntLiteral(0),
            crate::span::Span::dummy(),
        )),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

pub(super) fn builtin_exception_get_code_method() -> ClassMethod {
    ClassMethod {
        name: "getCode".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        variadic: None,
        return_type: None,
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, crate::span::Span::dummy())),
                    property: "code".to_string(),
                },
                crate::span::Span::dummy(),
            ))),
            crate::span::Span::dummy(),
        )],
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

pub(super) fn builtin_exception_get_message_method() -> ClassMethod {
    ClassMethod {
        name: "getMessage".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        variadic: None,
        return_type: None,
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, crate::span::Span::dummy())),
                    property: "message".to_string(),
                },
                crate::span::Span::dummy(),
            ))),
            crate::span::Span::dummy(),
        )],
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

pub(super) fn builtin_throwable_get_message_method() -> ClassMethod {
    ClassMethod {
        name: "getMessage".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: true,
        is_final: false,
        has_body: false,
        params: Vec::new(),
        variadic: None,
        return_type: None,
        body: Vec::new(),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

pub(crate) fn patch_builtin_exception_signatures(checker: &mut Checker) {
    if let Some(interface_info) = checker.interfaces.get_mut("Throwable") {
        if let Some(sig) = interface_info.methods.get_mut(&php_symbol_key("getMessage")) {
            sig.return_type = PhpType::Str;
        }
    }
    for class_name in ["Exception", "RuntimeException", "JsonException"] {
        if let Some(class_info) = checker.classes.get_mut(class_name) {
            if let Some(sig) = class_info.methods.get_mut("__construct") {
                if let Some(param) = sig.params.get_mut(0) {
                    param.1 = PhpType::Str;
                }
                if let Some(param) = sig.params.get_mut(1) {
                    param.1 = PhpType::Int;
                }
                sig.return_type = PhpType::Void;
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getMessage")) {
                sig.return_type = PhpType::Str;
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getCode")) {
                sig.return_type = PhpType::Int;
            }
        }
    }
}
