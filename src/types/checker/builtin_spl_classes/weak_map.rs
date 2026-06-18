//! Purpose:
//! Injects `WeakMap` metadata backed by per-instance parallel arrays of object keys and values.
//! Provides object-keyed map storage with ArrayAccess, Countable, and Iterator methods.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Object identity uses strict comparison against stored object handles, matching PHP's
//!   object-keyed map semantics.
//! - This is a STRONG map: stored keys keep their objects alive. PHP `WeakMap` evicts an entry
//!   when its key object is garbage-collected; elephc has no per-object finalizer/weak-reference
//!   registry, so auto-eviction is not implemented (documented gap). For render-one-page usage
//!   this is a potential leak, not a correctness divergence: get/set/count/isset/unset and
//!   iteration behave identically to PHP while the key is live.
//! - `offsetGet` returns `null` for an absent key instead of throwing `Error` like PHP; code
//!   that guards with `isset` first is unaffected (documented gap).

use std::collections::HashMap;

use crate::parser::ast::{BinOp, ClassMethod, ClassProperty, Expr, Stmt, TypeExpr, Visibility};
use crate::types::traits::FlattenedClass;

use super::common::*;

/// Inserts `WeakMap` into the builtin class registry.
pub(super) fn insert_class(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "WeakMap".to_string(),
        FlattenedClass {
            name: "WeakMap".to_string(),
            extends: None,
            implements: vec![
                "Iterator".to_string(),
                "Countable".to_string(),
                "ArrayAccess".to_string(),
            ],
            is_abstract: false,
            is_final: true,
            is_readonly_class: false,
            properties: weak_map_properties(),
            methods: weak_map_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );
}

/// Returns hidden storage properties for object keys, mapped values, and the iterator cursor.
fn weak_map_properties() -> Vec<ClassProperty> {
    vec![
        protected_storage_property("objects", array_type()),
        protected_storage_property("values", array_type()),
        protected_storage_property("position", TypeExpr::Int),
    ]
}

/// Returns all methods supported by the built-in `WeakMap` surface.
fn weak_map_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body("__construct", Vec::new(), Some(TypeExpr::Void), construct_body()),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), count_body()),
        method_with_body(
            "offsetExists",
            vec![param("object", mixed_type())],
            Some(TypeExpr::Bool),
            contains_body(),
        ),
        method_with_body(
            "offsetGet",
            vec![param("object", mixed_type())],
            Some(mixed_type()),
            offset_get_body(),
        ),
        method_with_body(
            "offsetSet",
            vec![param("object", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
            offset_set_body(),
        ),
        method_with_body(
            "offsetUnset",
            vec![param("object", mixed_type())],
            Some(TypeExpr::Void),
            offset_unset_body(),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), valid_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), key_body()),
        method_with_body("current", Vec::new(), Some(mixed_type()), current_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), next_body()),
        protected_method_with_body(
            "__elephcIndexOf",
            vec![param("object", mixed_type())],
            Some(TypeExpr::Int),
            index_of_body(),
        ),
    ]
}

/// Builds a protected concrete synthetic method.
fn protected_method_with_body(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    let mut method = method_with_body(name, params, return_type, body);
    method.visibility = Visibility::Protected;
    method
}

/// Returns `$this->objects`.
fn objects_expr() -> Expr {
    property_access(this_expr(), "objects")
}

/// Returns `$this->values`.
fn values_expr() -> Expr {
    property_access(this_expr(), "values")
}

/// Returns `$this->position`.
fn position_expr() -> Expr {
    property_access(this_expr(), "position")
}

/// Returns `$this->objects[$index]`.
fn object_at(index: Expr) -> Expr {
    array_access(objects_expr(), index)
}

/// Returns `$this->values[$index]`.
fn value_at(index: Expr) -> Expr {
    array_access(values_expr(), index)
}

/// Returns `$this->__elephcIndexOf($object)`.
fn index_of_expr(object: Expr) -> Expr {
    method_call(this_expr(), "__elephcIndexOf", vec![object])
}

/// Initializes the key/value arrays and iterator position.
fn construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "objects", empty_array_expr()),
        property_assign_stmt(this_expr(), "values", empty_array_expr()),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
    ]
}

/// Returns the number of stored entries.
fn count_body() -> Vec<Stmt> {
    return_body(count_expr(objects_expr()))
}

/// Returns true when an object key is present.
fn contains_body() -> Vec<Stmt> {
    return_body(binary_expr(index_of_expr(var_expr("object")), BinOp::GtEq, int_expr(0)))
}

/// Returns the value for an object key, or `null` when absent.
fn offset_get_body() -> Vec<Stmt> {
    vec![
        assign_stmt("index", index_of_expr(var_expr("object"))),
        if_stmt(
            binary_expr(var_expr("index"), BinOp::GtEq, int_expr(0)),
            return_body(value_at(var_expr("index"))),
            None,
        ),
        return_stmt(null_expr()),
    ]
}

/// Stores or updates the value for an object key through ArrayAccess syntax.
fn offset_set_body() -> Vec<Stmt> {
    vec![
        assign_stmt("index", index_of_expr(var_expr("object"))),
        if_stmt(
            binary_expr(var_expr("index"), BinOp::GtEq, int_expr(0)),
            vec![
                property_array_assign_stmt(this_expr(), "values", var_expr("index"), var_expr("value")),
                return_void_stmt(),
            ],
            None,
        ),
        property_array_push_stmt(this_expr(), "objects", var_expr("object")),
        property_array_push_stmt(this_expr(), "values", var_expr("value")),
    ]
}

/// Removes a stored object key and its value when present.
fn offset_unset_body() -> Vec<Stmt> {
    vec![
        typed_assign_stmt("newObjects", array_type(), empty_array_expr()),
        typed_assign_stmt("newValues", array_type(), empty_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(objects_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    not_expr(binary_expr(object_at(var_expr("i")), BinOp::StrictEq, var_expr("object"))),
                    vec![
                        assign_stmt("keptObject", object_at(var_expr("i"))),
                        assign_stmt("keptValue", value_at(var_expr("i"))),
                        array_push_stmt("newObjects", var_expr("keptObject")),
                        array_push_stmt("newValues", var_expr("keptValue")),
                    ],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        property_assign_stmt(this_expr(), "objects", var_expr("newObjects")),
        property_assign_stmt(this_expr(), "values", var_expr("newValues")),
    ]
}

/// Resets iteration to the first stored entry.
fn rewind_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "position", int_expr(0))]
}

/// Returns true when the current iterator position points at a stored entry.
fn valid_body() -> Vec<Stmt> {
    return_body(binary_expr(position_expr(), BinOp::Lt, count_expr(objects_expr())))
}

/// Returns the current object key or `null` when invalid.
fn key_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            method_call(this_expr(), "valid", Vec::new()),
            return_body(object_at(position_expr())),
            None,
        ),
        return_stmt(null_expr()),
    ]
}

/// Returns the current mapped value or `null` when invalid.
fn current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            method_call(this_expr(), "valid", Vec::new()),
            return_body(value_at(position_expr())),
            None,
        ),
        return_stmt(null_expr()),
    ]
}

/// Advances iteration by one entry.
fn next_body() -> Vec<Stmt> {
    vec![property_assign_stmt(
        this_expr(),
        "position",
        binary_expr(position_expr(), BinOp::Add, int_expr(1)),
    )]
}

/// Finds the index of an object key by strict object identity, or `-1` when absent.
fn index_of_body() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(objects_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(object_at(var_expr("i")), BinOp::StrictEq, var_expr("object")),
                    return_body(var_expr("i")),
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(int_expr(-1)),
    ]
}