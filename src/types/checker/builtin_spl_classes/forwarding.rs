//! Purpose:
//! Injects simple forwarding iterator decorators: IteratorIterator, LimitIterator, NoRewindIterator, and InfiniteIterator.
//! Exposes shared inner-iterator helpers used by filters, caching, append, and recursive decorators.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Decorators store an `inner` iterator and synthesize PHP-like forwarding methods.
//! - Limited visibility keeps the forwarding helpers local to SPL metadata construction.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, ClassMethod, ClassProperty, Expr, Stmt, TypeExpr};
use crate::types::traits::FlattenedClass;

use super::common::*;

/// Inserts classes into the supplied builtin metadata registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "IteratorIterator".to_string(),
        FlattenedClass {
            name: "IteratorIterator".to_string(),
            extends: None,
            implements: vec!["OuterIterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: iterator_iterator_properties(),
            methods: spl_iterator_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "LimitIterator".to_string(),
        FlattenedClass {
            name: "LimitIterator".to_string(),
            extends: Some("IteratorIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: limit_iterator_properties(),
            methods: spl_limit_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "NoRewindIterator".to_string(),
        FlattenedClass {
            name: "NoRewindIterator".to_string(),
            extends: Some("IteratorIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_no_rewind_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "InfiniteIterator".to_string(),
        FlattenedClass {
            name: "InfiniteIterator".to_string(),
            extends: Some("IteratorIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_infinite_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );
}

/// Builds the property list for iterator iterator.
fn iterator_iterator_properties() -> Vec<ClassProperty> {
    vec![storage_property("inner", named_type("Iterator"))]
}

/// Builds the property list for limit iterator.
fn limit_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("position", TypeExpr::Int),
        storage_property("offset", TypeExpr::Int),
        storage_property("limit", TypeExpr::Int),
    ]
}

/// Builds the method list for SPL iterator iterator.
fn spl_iterator_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", named_type("Traversable")),
                param_default(
                    "class",
                    TypeExpr::Nullable(Box::new(TypeExpr::Str)),
                    null_expr(),
                ),
            ],
            Some(TypeExpr::Void),
            iterator_iterator_construct_body(),
        ),
        method_with_body("current", Vec::new(), Some(mixed_type()), inner_return_body("current")),
        method_with_body("key", Vec::new(), Some(mixed_type()), inner_return_body("key")),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), inner_void_body("next")),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), inner_void_body("rewind")),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), inner_return_body("valid")),
        method_with_body(
            "getInnerIterator",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("Iterator")))),
            return_body(inner_expr()),
        ),
    ]
}

/// Builds the method list for SPL limit iterator.
fn spl_limit_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", named_type("Iterator")),
                param_default("offset", TypeExpr::Int, int_expr(0)),
                param_default("limit", TypeExpr::Int, int_expr(-1)),
            ],
            Some(TypeExpr::Void),
            limit_iterator_construct_body(),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), limit_rewind_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), limit_next_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), limit_valid_body()),
        method_with_body(
            "seek",
            vec![param("offset", TypeExpr::Int)],
            Some(TypeExpr::Void),
            limit_seek_body(),
        ),
        method_with_body("getPosition", Vec::new(), Some(TypeExpr::Int), return_body(limit_position_expr())),
    ]
}

/// Builds the method list for SPL no rewind iterator.
fn spl_no_rewind_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            iterator_iterator_construct_body(),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), Vec::new()),
    ]
}

/// Builds the method list for SPL infinite iterator.
fn spl_infinite_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            iterator_iterator_construct_body(),
        ),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), infinite_next_body()),
    ]
}

/// Builds the synthetic method body for iterator iterator construct.
pub(super) fn iterator_iterator_construct_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "inner", var_expr("iterator"))]
}

/// Builds the AST expression for inner.
pub(super) fn inner_expr() -> Expr {
    property_access(this_expr(), "inner")
}

/// Provides the Inner call helper used by the forwarding module.
pub(super) fn inner_call(method: &str) -> Expr {
    method_call(inner_expr(), method, Vec::new())
}

/// Builds the synthetic method body for inner return.
fn inner_return_body(method: &str) -> Vec<Stmt> {
    return_body(inner_call(method))
}

/// Builds the synthetic method body for inner void.
pub(super) fn inner_void_body(method: &str) -> Vec<Stmt> {
    vec![expr_stmt(inner_call(method))]
}

/// Builds the synthetic method body for recursive inner return.
pub(super) fn recursive_inner_return_body(method: &str) -> Vec<Stmt> {
    return_body(method_call(inner_expr(), method, Vec::new()))
}

/// Builds the AST expression for limit position.
fn limit_position_expr() -> Expr {
    property_access(this_expr(), "position")
}

/// Builds the AST expression for limit offset.
fn limit_offset_expr() -> Expr {
    property_access(this_expr(), "offset")
}

/// Builds the AST expression for limit bound.
fn limit_bound_expr() -> Expr {
    property_access(this_expr(), "limit")
}

/// Builds the synthetic method body for limit iterator construct.
fn limit_iterator_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
        property_assign_stmt(this_expr(), "offset", var_expr("offset")),
        property_assign_stmt(this_expr(), "limit", var_expr("limit")),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
    ]
}

/// Builds the synthetic method body for limit rewind.
fn limit_rewind_body() -> Vec<Stmt> {
    vec![
        expr_stmt(inner_call("rewind")),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        while_stmt(
            binary_expr(limit_position_expr(), BinOp::Lt, limit_offset_expr()),
            vec![
                if_stmt(not_expr(inner_call("valid")), vec![return_void_stmt()], None),
                expr_stmt(inner_call("next")),
                property_assign_stmt(
                    this_expr(),
                    "position",
                    binary_expr(limit_position_expr(), BinOp::Add, int_expr(1)),
                ),
            ],
        ),
    ]
}

/// Builds the synthetic method body for limit next.
fn limit_next_body() -> Vec<Stmt> {
    vec![
        expr_stmt(inner_call("next")),
        property_assign_stmt(
            this_expr(),
            "position",
            binary_expr(limit_position_expr(), BinOp::Add, int_expr(1)),
        ),
    ]
}

/// Builds the synthetic method body for limit valid.
fn limit_valid_body() -> Vec<Stmt> {
    vec![
        if_stmt(not_expr(inner_call("valid")), return_body(bool_expr(false)), None),
        if_stmt(
            binary_expr(limit_bound_expr(), BinOp::Lt, int_expr(0)),
            return_body(bool_expr(true)),
            None,
        ),
        return_stmt(binary_expr(
            binary_expr(limit_position_expr(), BinOp::Sub, limit_offset_expr()),
            BinOp::Lt,
            limit_bound_expr(),
        )),
    ]
}

/// Builds the synthetic method body for limit seek.
fn limit_seek_body() -> Vec<Stmt> {
    vec![
        expr_stmt(method_call(this_expr(), "rewind", Vec::new())),
        while_stmt(
            binary_expr(limit_position_expr(), BinOp::Lt, var_expr("offset")),
            vec![
                if_stmt(
                    not_expr(method_call(this_expr(), "valid", Vec::new())),
                    vec![return_void_stmt()],
                    None,
                ),
                expr_stmt(method_call(this_expr(), "next", Vec::new())),
            ],
        ),
    ]
}

/// Builds the synthetic method body for infinite next.
fn infinite_next_body() -> Vec<Stmt> {
    vec![
        expr_stmt(inner_call("next")),
        if_stmt(not_expr(inner_call("valid")), inner_void_body("rewind"), None),
    ]
}
