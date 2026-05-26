//! Purpose:
//! Injects FilterIterator and CallbackFilterIterator metadata.
//! Keeps callback-aware filter behavior separate from plain forwarding decorators.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - CallbackFilterIterator stores callable state and exposes an internal callback trampoline method.
//! - Filter movement skips rejected inner elements after rewind and next.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, CastType, ClassMethod, ClassProperty, Stmt, TypeExpr};
use crate::types::traits::FlattenedClass;

use super::common::*;
use super::forwarding::{inner_call, inner_expr, inner_void_body, iterator_iterator_construct_body};

/// Inserts classes into the supplied builtin metadata registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "FilterIterator".to_string(),
        FlattenedClass {
            name: "FilterIterator".to_string(),
            extends: Some("IteratorIterator".to_string()),
            implements: Vec::new(),
            is_abstract: true,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_filter_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "CallbackFilterIterator".to_string(),
        FlattenedClass {
            name: "CallbackFilterIterator".to_string(),
            extends: Some("FilterIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: callback_filter_iterator_properties(),
            methods: spl_callback_filter_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );
}

/// Builds the property list for callback filter iterator.
fn callback_filter_iterator_properties() -> Vec<ClassProperty> {
    vec![
        protected_storage_property_untyped("callback"),
        protected_storage_property("callbackEnv", TypeExpr::Ptr(None)),
    ]
}

/// Builds the method list for SPL filter iterator.
fn spl_filter_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            iterator_iterator_construct_body(),
        ),
        abstract_method("accept", Vec::new(), Some(TypeExpr::Bool)),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), filter_rewind_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), filter_next_body()),
    ]
}

/// Builds the method list for SPL callback filter iterator.
fn spl_callback_filter_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", named_type("Iterator")),
                param("callback", named_type("callable")),
            ],
            Some(TypeExpr::Void),
            callback_filter_construct_body(),
        ),
        method_with_body(
            "__elephcAcceptCallback",
            vec![
                param("current", mixed_type()),
                param("key", mixed_type()),
                param("iterator", named_type("Iterator")),
            ],
            Some(TypeExpr::Bool),
            Vec::new(),
        ),
        method_with_body("accept", Vec::new(), Some(TypeExpr::Bool), callback_filter_accept_body()),
        method_with_body(
            "__elephcSetCallbackEnv",
            vec![param("env", TypeExpr::Ptr(None))],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "callbackEnv", var_expr("env"))],
        ),
    ]
}

/// Builds the synthetic method body for filter rewind.
fn filter_rewind_body() -> Vec<Stmt> {
    let mut body = inner_void_body("rewind");
    body.extend(filter_skip_rejected_body());
    body
}

/// Builds the synthetic method body for filter next.
fn filter_next_body() -> Vec<Stmt> {
    let mut body = inner_void_body("next");
    body.extend(filter_skip_rejected_body());
    body
}

/// Builds the synthetic method body for filter skip rejected.
fn filter_skip_rejected_body() -> Vec<Stmt> {
    vec![while_stmt(
        binary_expr(
            inner_call("valid"),
            BinOp::And,
            not_expr(method_call(this_expr(), "accept", Vec::new())),
        ),
        inner_void_body("next"),
    )]
}

/// Builds the synthetic method body for callback filter construct.
pub(super) fn callback_filter_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
        property_assign_stmt(this_expr(), "callback", var_expr("callback")),
    ]
}

/// Builds the synthetic method body for callback filter accept.
fn callback_filter_accept_body() -> Vec<Stmt> {
    return_body(cast_expr(
        CastType::Bool,
        method_call(
            this_expr(),
            "__elephcAcceptCallback",
            vec![
                inner_call("current"),
                inner_call("key"),
                inner_expr(),
            ],
        ),
    ))
}
