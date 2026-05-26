//! Purpose:
//! Injects AppendIterator metadata and its synthetic storage facade methods.
//! Keeps multi-iterator sequencing separate from MultipleIterator and from the helper ArrayIterator wrapper.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - AppendIterator keeps iterator, key, and active-state arrays in parallel.
//! - The public array-iterator view delegates to internal storage facade methods.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, ClassMethod, ClassProperty, Expr, Stmt, TypeExpr};
use crate::types::traits::FlattenedClass;

use super::append_array_iterator;
use super::common::*;
use super::append_storage::*;
use super::forwarding::{inner_call, inner_expr};

/// Inserts classes into the supplied builtin metadata registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "AppendIterator".to_string(),
        FlattenedClass {
            name: "AppendIterator".to_string(),
            extends: Some("IteratorIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: append_iterator_properties(),
            methods: spl_append_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );
    append_array_iterator::insert_class(class_map);
}

/// Appends iterator properties to the current runtime or metadata collection.
fn append_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("iterators", array_type()),
        storage_property("iteratorKeys", array_type()),
        storage_property("iteratorActive", array_type()),
        storage_property("index", TypeExpr::Int),
        storage_property("arrayIterator", named_type("__ElephcAppendIteratorArrayIterator")),
    ]
}

/// Builds the method list for SPL append iterator.
fn spl_append_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body("__construct", Vec::new(), Some(TypeExpr::Void), append_construct_body()),
        method_with_body(
            "append",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            append_append_body(),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), append_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), append_valid_body()),
        method_with_body("current", Vec::new(), Some(mixed_type()), append_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), append_key_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), append_next_body()),
        method_with_body(
            "getInnerIterator",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("Iterator")))),
            append_get_inner_iterator_body(),
        ),
        method_with_body(
            "getIteratorIndex",
            Vec::new(),
            Some(mixed_type()),
            append_get_iterator_index_body(),
        ),
        method_with_body(
            "getArrayIterator",
            Vec::new(),
            Some(named_type("__ElephcAppendIteratorArrayIterator")),
            append_get_array_iterator_body(),
        ),
        method_with_body(
            "__elephcStorageCount",
            Vec::new(),
            Some(TypeExpr::Int),
            append_storage_count_body(),
        ),
        method_with_body(
            "__elephcStoragePhysicalCount",
            Vec::new(),
            Some(TypeExpr::Int),
            return_body(count_expr(append_iterators_expr())),
        ),
        method_with_body(
            "__elephcStorageIsActive",
            vec![param("position", TypeExpr::Int)],
            Some(TypeExpr::Bool),
            return_body(append_active_at_position_expr(var_expr("position"))),
        ),
        method_with_body(
            "__elephcStorageAppend",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            append_storage_append_body(),
        ),
        method_with_body(
            "__elephcStorageOffsetSet",
            vec![param("offset", mixed_type()), param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            append_storage_offset_set_body(),
        ),
        method_with_body(
            "__elephcStorageOffsetExists",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Bool),
            append_storage_offset_exists_body(),
        ),
        method_with_body(
            "__elephcStorageOffsetGet",
            vec![param("offset", mixed_type())],
            Some(mixed_type()),
            append_storage_offset_get_body(),
        ),
        method_with_body(
            "__elephcStorageOffsetUnset",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Void),
            append_storage_offset_unset_body(),
        ),
        method_with_body(
            "__elephcStorageGetArrayCopy",
            Vec::new(),
            Some(array_type()),
            append_storage_get_array_copy_body(),
        ),
        method_with_body(
            "__elephcStorageKey",
            vec![param("position", TypeExpr::Int)],
            Some(mixed_type()),
            return_body(append_key_at_position_expr(var_expr("position"))),
        ),
        method_with_body(
            "__elephcStorageCurrent",
            vec![param("position", TypeExpr::Int)],
            Some(mixed_type()),
            append_storage_current_body(),
        ),
    ]
}

/// Appends array iterator expr to the current runtime or metadata collection.
fn append_array_iterator_expr() -> Expr {
    property_access(this_expr(), "arrayIterator")
}

/// Appends index expr to the current runtime or metadata collection.
fn append_index_expr() -> Expr {
    property_access(this_expr(), "index")
}

/// Appends current key expr to the current runtime or metadata collection.
fn append_current_key_expr() -> Expr {
    append_key_at_position_expr(append_index_expr())
}

/// Appends current iterator expr to the current runtime or metadata collection.
fn append_current_iterator_expr() -> Expr {
    array_access(append_iterators_expr(), append_index_expr())
}

/// Appends construct body to the current runtime or metadata collection.
fn append_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "inner", new_object_expr("EmptyIterator", Vec::new())),
        property_assign_stmt(this_expr(), "iterators", empty_array_expr()),
        property_assign_stmt(this_expr(), "iteratorKeys", empty_array_expr()),
        property_assign_stmt(this_expr(), "iteratorActive", empty_array_expr()),
        property_assign_stmt(this_expr(), "index", int_expr(0)),
        property_assign_stmt(
            this_expr(),
            "arrayIterator",
            new_object_expr("__ElephcAppendIteratorArrayIterator", vec![this_expr()]),
        ),
    ]
}

/// Appends append body to the current runtime or metadata collection.
fn append_append_body() -> Vec<Stmt> {
    append_storage_append_body()
}

/// Appends rewind body to the current runtime or metadata collection.
fn append_rewind_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "index", int_expr(0)),
        property_assign_stmt(this_expr(), "inner", new_object_expr("EmptyIterator", Vec::new())),
        if_stmt(
            binary_expr(count_expr(append_iterators_expr()), BinOp::StrictEq, int_expr(0)),
            vec![return_void_stmt()],
            None,
        ),
        typed_assign_stmt("iterator", named_type("Iterator"), append_current_iterator_expr()),
        property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
        expr_stmt(method_call(var_expr("iterator"), "rewind", Vec::new())),
        expr_stmt(method_call(this_expr(), "valid", Vec::new())),
    ]
}

/// Appends valid body to the current runtime or metadata collection.
fn append_valid_body() -> Vec<Stmt> {
    let mut active_body = vec![
        typed_assign_stmt("iterator", named_type("Iterator"), append_current_iterator_expr()),
        if_stmt(
            method_call(var_expr("iterator"), "valid", Vec::new()),
            vec![
                property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
                return_stmt(bool_expr(true)),
            ],
            None,
        ),
    ];
    active_body.extend(append_advance_index_body());

    vec![
        while_stmt(
            binary_expr(append_index_expr(), BinOp::Lt, count_expr(append_iterators_expr())),
            vec![if_stmt(
                not_expr(append_active_at_position_expr(append_index_expr())),
                append_advance_index_body(),
                Some(active_body),
            )],
        ),
        property_assign_stmt(this_expr(), "inner", new_object_expr("EmptyIterator", Vec::new())),
        return_stmt(bool_expr(false)),
    ]
}

/// Appends advance index body to the current runtime or metadata collection.
fn append_advance_index_body() -> Vec<Stmt> {
    vec![
        append_advance_index_stmt(),
        if_stmt(
            binary_expr(append_index_expr(), BinOp::Lt, count_expr(append_iterators_expr())),
            vec![
                typed_assign_stmt("iterator", named_type("Iterator"), append_current_iterator_expr()),
                property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
                expr_stmt(method_call(var_expr("iterator"), "rewind", Vec::new())),
            ],
            None,
        ),
    ]
}

/// Appends advance index stmt to the current runtime or metadata collection.
fn append_advance_index_stmt() -> Stmt {
    property_assign_stmt(
        this_expr(),
        "index",
        binary_expr(append_index_expr(), BinOp::Add, int_expr(1)),
    )
}

/// Appends current body to the current runtime or metadata collection.
fn append_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(method_call(this_expr(), "valid", Vec::new())),
            null_return_body(),
            None,
        ),
        return_stmt(inner_call("current")),
    ]
}

/// Appends key body to the current runtime or metadata collection.
fn append_key_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(method_call(this_expr(), "valid", Vec::new())),
            null_return_body(),
            None,
        ),
        return_stmt(inner_call("key")),
    ]
}

/// Appends next body to the current runtime or metadata collection.
fn append_next_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(method_call(this_expr(), "valid", Vec::new())),
            vec![return_void_stmt()],
            None,
        ),
        typed_assign_stmt("iterator", named_type("Iterator"), inner_expr()),
        expr_stmt(method_call(var_expr("iterator"), "next", Vec::new())),
        if_stmt(
            not_expr(method_call(var_expr("iterator"), "valid", Vec::new())),
            vec![
                property_assign_stmt(
                    this_expr(),
                    "index",
                    binary_expr(append_index_expr(), BinOp::Add, int_expr(1)),
                ),
                if_stmt(
                    binary_expr(append_index_expr(), BinOp::Lt, count_expr(append_iterators_expr())),
                    vec![
                        typed_assign_stmt("iterator", named_type("Iterator"), append_current_iterator_expr()),
                        property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
                        expr_stmt(method_call(var_expr("iterator"), "rewind", Vec::new())),
                    ],
                    None,
                ),
            ],
            None,
        ),
        expr_stmt(method_call(this_expr(), "valid", Vec::new())),
    ]
}

/// Appends get inner iterator body to the current runtime or metadata collection.
fn append_get_inner_iterator_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(method_call(this_expr(), "valid", Vec::new())),
            null_return_body(),
            None,
        ),
        return_stmt(inner_expr()),
    ]
}

/// Appends get iterator index body to the current runtime or metadata collection.
fn append_get_iterator_index_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(method_call(this_expr(), "valid", Vec::new())),
            null_return_body(),
            None,
        ),
        return_stmt(append_current_key_expr()),
    ]
}

/// Appends get array iterator body to the current runtime or metadata collection.
fn append_get_array_iterator_body() -> Vec<Stmt> {
    return_body(append_array_iterator_expr())
}

