//! Purpose:
//! Injects recursive filter decorators and ParentIterator metadata.
//! Keeps recursive filtering wrappers separate from RecursiveIteratorIterator traversal state.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Recursive callback children preserve callback and environment state.
//! - Internal narrowing hooks let synthetic bodies express RecursiveIterator returns.

use std::collections::HashMap;

use crate::parser::ast::{ClassMethod, Stmt, TypeExpr};
use crate::types::traits::FlattenedClass;

use super::common::*;
use super::filters::callback_filter_construct_body;
use super::forwarding::{inner_expr, iterator_iterator_construct_body, recursive_inner_return_body};
use super::recursive_array::assume_recursive_iterator_expr;

/// Inserts classes into the supplied builtin metadata registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "RecursiveFilterIterator".to_string(),
        FlattenedClass {
            name: "RecursiveFilterIterator".to_string(),
            extends: Some("FilterIterator".to_string()),
            implements: vec!["RecursiveIterator".to_string()],
            is_abstract: true,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_recursive_filter_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "RecursiveCallbackFilterIterator".to_string(),
        FlattenedClass {
            name: "RecursiveCallbackFilterIterator".to_string(),
            extends: Some("CallbackFilterIterator".to_string()),
            implements: vec!["RecursiveIterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_recursive_callback_filter_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "ParentIterator".to_string(),
        FlattenedClass {
            name: "ParentIterator".to_string(),
            extends: Some("RecursiveFilterIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_parent_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );
}

/// Builds the method list for SPL recursive filter iterator.
fn spl_recursive_filter_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("iterator", named_type("RecursiveIterator"))],
            Some(TypeExpr::Void),
            iterator_iterator_construct_body(),
        ),
        method_with_body("hasChildren", Vec::new(), Some(TypeExpr::Bool), recursive_inner_return_body("hasChildren")),
        method_with_body(
            "getChildren",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            recursive_filter_get_children_body(),
        ),
        method_with_body(
            "__elephcAssumeRecursiveIterator",
            vec![param("iterator", mixed_type())],
            Some(named_type("RecursiveIterator")),
            Vec::new(),
        ),
    ]
}

/// Builds the method list for SPL recursive callback filter iterator.
fn spl_recursive_callback_filter_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", named_type("RecursiveIterator")),
                param("callback", named_type("callable")),
            ],
            Some(TypeExpr::Void),
            callback_filter_construct_body(),
        ),
        method_with_body("hasChildren", Vec::new(), Some(TypeExpr::Bool), recursive_inner_return_body("hasChildren")),
        method_with_body(
            "getChildren",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            recursive_callback_filter_get_children_body(),
        ),
        method_with_body(
            "__elephcAssumeRecursiveIterator",
            vec![param("iterator", mixed_type())],
            Some(named_type("RecursiveIterator")),
            Vec::new(),
        ),
    ]
}

/// Builds the method list for SPL parent iterator.
fn spl_parent_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("iterator", named_type("RecursiveIterator"))],
            Some(TypeExpr::Void),
            iterator_iterator_construct_body(),
        ),
        method_with_body("accept", Vec::new(), Some(TypeExpr::Bool), return_body(method_call(this_expr(), "hasChildren", Vec::new()))),
        method_with_body(
            "getChildren",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            parent_iterator_get_children_body(),
        ),
        method_with_body(
            "__elephcAssumeRecursiveIterator",
            vec![param("iterator", mixed_type())],
            Some(named_type("RecursiveIterator")),
            Vec::new(),
        ),
    ]
}

/// Builds the synthetic method body for recursive filter get children.
fn recursive_filter_get_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt("child", method_call(inner_expr(), "getChildren", Vec::new())),
        if_stmt(
            function_call("is_null", vec![var_expr("child")]),
            return_body(null_expr()),
            None,
        ),
        return_stmt(new_static_expr(vec![assume_recursive_iterator_expr(var_expr("child"))])),
    ]
}

/// Builds the synthetic method body for recursive callback filter get children.
fn recursive_callback_filter_get_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt("child", method_call(inner_expr(), "getChildren", Vec::new())),
        if_stmt(
            function_call("is_null", vec![var_expr("child")]),
            return_body(null_expr()),
            None,
        ),
        assign_stmt(
            "next",
            new_object_expr(
                "RecursiveCallbackFilterIterator",
                vec![
                    assume_recursive_iterator_expr(var_expr("child")),
                    property_access(this_expr(), "callback"),
                ],
            ),
        ),
        expr_stmt(method_call(
            var_expr("next"),
            "__elephcSetCallbackEnv",
            vec![property_access(this_expr(), "callbackEnv")],
        )),
        return_stmt(var_expr("next")),
    ]
}

/// Builds the synthetic method body for parent iterator get children.
fn parent_iterator_get_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt("child", method_call(inner_expr(), "getChildren", Vec::new())),
        if_stmt(
            function_call("is_null", vec![var_expr("child")]),
            return_body(null_expr()),
            None,
        ),
        return_stmt(new_object_expr(
            "ParentIterator",
            vec![assume_recursive_iterator_expr(var_expr("child"))],
        )),
    ]
}
