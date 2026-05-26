//! Purpose:
//! Injects RecursiveArrayIterator metadata and array-child discovery bodies.
//! Keeps recursive array storage behavior separate from plain ArrayIterator and ArrayObject metadata.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Array children are wrapped as RecursiveArrayIterator; RecursiveIterator values are passed through.
//! - The internal narrowing helper is shared with recursive decorator modules.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, ClassMethod, Expr, Stmt, TypeExpr};
use crate::types::traits::FlattenedClass;

use super::common::*;

/// Inserts class into the supplied builtin metadata registry.
pub(super) fn insert_class(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "RecursiveArrayIterator".to_string(),
        FlattenedClass {
            name: "RecursiveArrayIterator".to_string(),
            extends: Some("ArrayIterator".to_string()),
            implements: vec!["RecursiveIterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_recursive_array_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );
}

/// Builds the method list for SPL recursive array iterator.
fn spl_recursive_array_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param_default("array", mixed_type(), empty_array_expr()),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(TypeExpr::Void),
            recursive_array_iterator_construct_body(),
        ),
        method_with_body("hasChildren", Vec::new(), Some(TypeExpr::Bool), recursive_array_has_children_body()),
        method_with_body(
            "getChildren",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            recursive_array_get_children_body(),
        ),
        method_with_body(
            "__elephcAssumeRecursiveIterator",
            vec![param("iterator", mixed_type())],
            Some(named_type("RecursiveIterator")),
            Vec::new(),
        ),
    ]
}

/// Builds the synthetic method body for recursive array iterator construct.
fn recursive_array_iterator_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "keys", empty_array_expr()),
        property_assign_stmt(this_expr(), "values", empty_array_expr()),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
        foreach_stmt(
            var_expr("array"),
            Some("key"),
            "value",
            vec![
                property_array_push_stmt(this_expr(), "keys", var_expr("key")),
                property_array_push_stmt(this_expr(), "values", var_expr("value")),
            ],
        ),
    ]
}

/// Builds the AST expression for gettype is array.
fn gettype_is_array_expr(value: Expr) -> Expr {
    binary_expr(
        function_call("gettype", vec![value]),
        BinOp::StrictEq,
        string_expr("array"),
    )
}

/// Builds the AST expression for recursive current.
fn recursive_current_expr() -> Expr {
    method_call(this_expr(), "current", Vec::new())
}

/// Builds the AST expression for assume recursive iterator.
pub(super) fn assume_recursive_iterator_expr(value: Expr) -> Expr {
    method_call(this_expr(), "__elephcAssumeRecursiveIterator", vec![value])
}

/// Builds the synthetic method body for recursive array has children.
fn recursive_array_has_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt("value", recursive_current_expr()),
        return_stmt(binary_expr(
            instanceof_expr(var_expr("value"), "RecursiveIterator"),
            BinOp::Or,
            gettype_is_array_expr(var_expr("value")),
        )),
    ]
}

/// Builds the synthetic method body for recursive array get children.
fn recursive_array_get_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt("value", recursive_current_expr()),
        if_stmt(
            instanceof_expr(var_expr("value"), "RecursiveIterator"),
            return_body(assume_recursive_iterator_expr(var_expr("value"))),
            None,
        ),
        if_stmt(
            gettype_is_array_expr(var_expr("value")),
            return_body(new_object_expr("RecursiveArrayIterator", vec![var_expr("value")])),
            None,
        ),
        return_stmt(null_expr()),
    ]
}
