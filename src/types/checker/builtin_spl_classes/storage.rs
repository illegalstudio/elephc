//! Purpose:
//! Injects storage-oriented SPL iterator metadata: EmptyIterator, ArrayIterator, RecursiveArrayIterator, and ArrayObject.
//! Owns synthetic PHP-like bodies for array key/value snapshots and ArrayAccess behavior.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - ArrayIterator and ArrayObject store parallel key/value arrays to preserve PHP keys.
//! - RecursiveArrayIterator exposes internal narrowing hooks for recursive traversal codegen.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, ClassMethod, ClassProperty, Expr, Stmt, TypeExpr};
use crate::types::traits::FlattenedClass;

use super::common::*;

pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "EmptyIterator".to_string(),
        FlattenedClass {
            name: "EmptyIterator".to_string(),
            extends: None,
            implements: vec!["Iterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_empty_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "ArrayIterator".to_string(),
        FlattenedClass {
            name: "ArrayIterator".to_string(),
            extends: None,
            implements: vec![
                "Iterator".to_string(),
                "ArrayAccess".to_string(),
                "SeekableIterator".to_string(),
                "Countable".to_string(),
            ],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: array_iterator_properties(),
            methods: spl_array_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "ArrayObject".to_string(),
        FlattenedClass {
            name: "ArrayObject".to_string(),
            extends: None,
            implements: vec![
                "IteratorAggregate".to_string(),
                "ArrayAccess".to_string(),
                "Countable".to_string(),
            ],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: array_object_properties(),
            methods: spl_array_object_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );
}

fn spl_empty_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body("current", Vec::new(), Some(mixed_type()), null_return_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), null_return_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), Vec::new()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), Vec::new()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), return_body(bool_expr(false))),
    ]
}

fn array_iterator_properties() -> Vec<ClassProperty> {
    vec![
        protected_storage_property("keys", array_type()),
        protected_storage_property("values", array_type()),
        protected_storage_property("position", TypeExpr::Int),
        protected_storage_property("flags", TypeExpr::Int),
    ]
}

fn array_object_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("keys", array_type()),
        storage_property("values", array_type()),
        storage_property("flags", TypeExpr::Int),
    ]
}

fn spl_array_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param_default("array", array_type(), empty_array_expr()),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(TypeExpr::Void),
            array_iterator_construct_body(),
        ),
        method_with_body("current", Vec::new(), Some(mixed_type()), array_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), array_key_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), array_next_body()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), array_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), array_valid_body()),
        method_with_body(
            "seek",
            vec![param("offset", TypeExpr::Int)],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "position", var_expr("offset"))],
        ),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), array_count_body()),
        method_with_body(
            "offsetExists",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Bool),
            array_offset_exists_body(),
        ),
        method_with_body(
            "offsetGet",
            vec![param("offset", mixed_type())],
            Some(mixed_type()),
            array_offset_get_body(),
        ),
        method_with_body(
            "offsetSet",
            vec![param("offset", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
            array_offset_set_body(),
        ),
        method_with_body(
            "offsetUnset",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Void),
            array_offset_unset_body(),
        ),
        method_with_body(
            "append",
            vec![param("value", mixed_type())],
            Some(TypeExpr::Void),
            array_append_body(),
        ),
        method_with_body("getArrayCopy", Vec::new(), Some(array_type()), array_copy_body()),
    ]
}

fn spl_array_object_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param_default("array", array_type(), empty_array_expr()),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(TypeExpr::Void),
            array_object_construct_body(),
        ),
        method_with_body("getIterator", Vec::new(), Some(named_type("ArrayIterator")), array_object_get_iterator_body()),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), array_count_body()),
        method_with_body(
            "offsetExists",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Bool),
            array_offset_exists_body(),
        ),
        method_with_body(
            "offsetGet",
            vec![param("offset", mixed_type())],
            Some(mixed_type()),
            array_offset_get_body(),
        ),
        method_with_body(
            "offsetSet",
            vec![param("offset", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
            array_offset_set_body(),
        ),
        method_with_body(
            "offsetUnset",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Void),
            array_offset_unset_body(),
        ),
        method_with_body(
            "append",
            vec![param("value", mixed_type())],
            Some(TypeExpr::Void),
            array_append_body(),
        ),
        method_with_body("getArrayCopy", Vec::new(), Some(array_type()), array_copy_body()),
    ]
}

fn keys_expr() -> Expr {
    property_access(this_expr(), "keys")
}

fn values_expr() -> Expr {
    property_access(this_expr(), "values")
}

fn position_expr() -> Expr {
    property_access(this_expr(), "position")
}

fn key_at(index: Expr) -> Expr {
    array_access(keys_expr(), index)
}

fn value_at(index: Expr) -> Expr {
    array_access(values_expr(), index)
}

fn array_iterator_construct_body() -> Vec<Stmt> {
    let mut body = array_object_construct_body();
    body.insert(2, property_assign_stmt(this_expr(), "position", int_expr(0)));
    body
}

fn array_object_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "keys", function_call("array_keys", vec![var_expr("array")])),
        property_assign_stmt(
            this_expr(),
            "values",
            function_call("array_values", vec![var_expr("array")]),
        ),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
    ]
}

fn array_current_body() -> Vec<Stmt> {
    return_body(value_at(position_expr()))
}

fn array_key_body() -> Vec<Stmt> {
    return_body(key_at(position_expr()))
}

fn array_next_body() -> Vec<Stmt> {
    vec![property_assign_stmt(
        this_expr(),
        "position",
        binary_expr(position_expr(), BinOp::Add, int_expr(1)),
    )]
}

fn array_rewind_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "position", int_expr(0))]
}

fn array_valid_body() -> Vec<Stmt> {
    return_body(binary_expr(position_expr(), BinOp::Lt, count_expr(values_expr())))
}

fn array_count_body() -> Vec<Stmt> {
    return_body(count_expr(values_expr()))
}

fn array_append_body() -> Vec<Stmt> {
    vec![
        property_array_push_stmt(this_expr(), "keys", count_expr(keys_expr())),
        property_array_push_stmt(this_expr(), "values", var_expr("value")),
    ]
}

fn array_offset_exists_body() -> Vec<Stmt> {
    let mut body = array_search_prelude();
    body.push(while_stmt(
        binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
        vec![
            if_stmt(
                binary_expr(key_at(var_expr("i")), BinOp::StrictEq, var_expr("offset")),
                return_body(bool_expr(true)),
                None,
            ),
            increment_stmt("i"),
        ],
    ));
    body.push(return_stmt(bool_expr(false)));
    body
}

fn array_offset_get_body() -> Vec<Stmt> {
    let mut body = array_search_prelude();
    body.push(while_stmt(
        binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
        vec![
            if_stmt(
                binary_expr(key_at(var_expr("i")), BinOp::StrictEq, var_expr("offset")),
                return_body(value_at(var_expr("i"))),
                None,
            ),
            increment_stmt("i"),
        ],
    ));
    body.push(return_stmt(null_expr()));
    body
}

fn array_offset_set_body() -> Vec<Stmt> {
    let mut body = vec![if_stmt(
        binary_expr(var_expr("offset"), BinOp::StrictEq, null_expr()),
        vec![
            expr_stmt(method_call(this_expr(), "append", vec![var_expr("value")])),
            return_void_stmt(),
        ],
        None,
    )];
    body.extend(array_search_prelude());
    body.push(while_stmt(
        binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
        vec![
            if_stmt(
                binary_expr(key_at(var_expr("i")), BinOp::StrictEq, var_expr("offset")),
                vec![
                    property_array_assign_stmt(this_expr(), "values", var_expr("i"), var_expr("value")),
                    return_void_stmt(),
                ],
                None,
            ),
            increment_stmt("i"),
        ],
    ));
    body.push(property_array_push_stmt(this_expr(), "keys", var_expr("offset")));
    body.push(property_array_push_stmt(this_expr(), "values", var_expr("value")));
    body
}

fn array_offset_unset_body() -> Vec<Stmt> {
    vec![
        assign_stmt("newKeys", empty_array_expr()),
        assign_stmt("newValues", empty_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(keys_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    not_expr(binary_expr(key_at(var_expr("i")), BinOp::StrictEq, var_expr("offset"))),
                    vec![
                        array_push_stmt("newKeys", key_at(var_expr("i"))),
                        array_push_stmt("newValues", value_at(var_expr("i"))),
                    ],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        property_assign_stmt(this_expr(), "keys", var_expr("newKeys")),
        property_assign_stmt(this_expr(), "values", var_expr("newValues")),
    ]
}

fn array_copy_body() -> Vec<Stmt> {
    vec![
        assign_stmt("out", empty_assoc_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(keys_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                array_assign_stmt("out", key_at(var_expr("i")), value_at(var_expr("i"))),
                increment_stmt("i"),
            ],
        ),
        return_stmt(var_expr("out")),
    ]
}

fn array_object_get_iterator_body() -> Vec<Stmt> {
    vec![
        assign_stmt("it", new_object_expr("ArrayIterator", vec![empty_array_expr()])),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(keys_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                expr_stmt(method_call(
                    var_expr("it"),
                    "offsetSet",
                    vec![key_at(var_expr("i")), value_at(var_expr("i"))],
                )),
                increment_stmt("i"),
            ],
        ),
        return_stmt(var_expr("it")),
    ]
}

fn array_search_prelude() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(keys_expr())),
    ]
}
