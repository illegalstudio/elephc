//! Purpose:
//! Owns AppendIterator's internal storage facade method bodies.
//! Separates slot/key/active-array mutation from public sequential traversal behavior.
//!
//! Called from:
//! - `super::append` synthetic AppendIterator method declarations.
//!
//! Key details:
//! - Iterator, key, and active arrays remain parallel.
//! - Unset marks entries inactive so the public array-iterator view can skip them.

use crate::parser::ast::{BinOp, Expr, Stmt};

use super::common::*;

/// Appends iterators expr to the current runtime or metadata collection.
pub(super) fn append_iterators_expr() -> Expr {
    property_access(this_expr(), "iterators")
}

/// Appends iterator keys expr to the current runtime or metadata collection.
pub(super) fn append_iterator_keys_expr() -> Expr {
    property_access(this_expr(), "iteratorKeys")
}

/// Appends iterator active expr to the current runtime or metadata collection.
pub(super) fn append_iterator_active_expr() -> Expr {
    property_access(this_expr(), "iteratorActive")
}

/// Appends key at position expr to the current runtime or metadata collection.
pub(super) fn append_key_at_position_expr(position: Expr) -> Expr {
    array_access(append_iterator_keys_expr(), position)
}

/// Appends active at position expr to the current runtime or metadata collection.
pub(super) fn append_active_at_position_expr(position: Expr) -> Expr {
    array_access(append_iterator_active_expr(), position)
}

/// Appends storage append body to the current runtime or metadata collection.
pub(super) fn append_storage_append_body() -> Vec<Stmt> {
    vec![
        property_array_push_stmt(this_expr(), "iteratorKeys", count_expr(append_iterator_keys_expr())),
        property_array_push_stmt(this_expr(), "iterators", var_expr("iterator")),
        property_array_push_stmt(this_expr(), "iteratorActive", bool_expr(true)),
    ]
}

/// Appends storage offset set body to the current runtime or metadata collection.
pub(super) fn append_storage_offset_set_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(var_expr("offset"), BinOp::StrictEq, null_expr()),
            append_storage_append_body(),
            Some(vec![
                assign_stmt("i", int_expr(0)),
                assign_stmt("limit", count_expr(append_iterator_keys_expr())),
                while_stmt(
                    binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
                    vec![
                        if_stmt(
                            binary_expr(append_key_at_position_expr(var_expr("i")), BinOp::StrictEq, var_expr("offset")),
                            vec![
                                property_array_assign_stmt(
                                    this_expr(),
                                    "iterators",
                                    var_expr("i"),
                                    var_expr("iterator"),
                                ),
                                property_array_assign_stmt(
                                    this_expr(),
                                    "iteratorActive",
                                    var_expr("i"),
                                    bool_expr(true),
                                ),
                                return_void_stmt(),
                            ],
                            None,
                        ),
                        increment_stmt("i"),
                    ],
                ),
                property_array_push_stmt(this_expr(), "iteratorKeys", var_expr("offset")),
                property_array_push_stmt(this_expr(), "iterators", var_expr("iterator")),
                property_array_push_stmt(this_expr(), "iteratorActive", bool_expr(true)),
            ]),
        ),
    ]
}

/// Appends storage count body to the current runtime or metadata collection.
pub(super) fn append_storage_count_body() -> Vec<Stmt> {
    vec![
        assign_stmt("count", int_expr(0)),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(append_iterators_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    append_active_at_position_expr(var_expr("i")),
                    vec![assign_stmt(
                        "count",
                        binary_expr(var_expr("count"), BinOp::Add, int_expr(1)),
                    )],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(var_expr("count")),
    ]
}

/// Appends storage offset exists body to the current runtime or metadata collection.
pub(super) fn append_storage_offset_exists_body() -> Vec<Stmt> {
    let mut body = vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(append_iterator_keys_expr())),
    ];
    body.push(while_stmt(
        binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
        vec![
            if_stmt(
                binary_expr(
                    binary_expr(
                        append_key_at_position_expr(var_expr("i")),
                        BinOp::StrictEq,
                        var_expr("offset"),
                    ),
                    BinOp::And,
                    append_active_at_position_expr(var_expr("i")),
                ),
                return_body(bool_expr(true)),
                None,
            ),
            increment_stmt("i"),
        ],
    ));
    body.push(return_stmt(bool_expr(false)));
    body
}

/// Appends storage offset get body to the current runtime or metadata collection.
pub(super) fn append_storage_offset_get_body() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(append_iterator_keys_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(
                        binary_expr(
                            append_key_at_position_expr(var_expr("i")),
                            BinOp::StrictEq,
                            var_expr("offset"),
                        ),
                        BinOp::And,
                        append_active_at_position_expr(var_expr("i")),
                    ),
                    return_body(array_access(append_iterators_expr(), var_expr("i"))),
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(null_expr()),
    ]
}

/// Appends storage offset unset body to the current runtime or metadata collection.
pub(super) fn append_storage_offset_unset_body() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(append_iterator_keys_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(append_key_at_position_expr(var_expr("i")), BinOp::StrictEq, var_expr("offset")),
                    vec![
                        property_array_assign_stmt(
                            this_expr(),
                            "iteratorActive",
                            var_expr("i"),
                            bool_expr(false),
                        ),
                        return_void_stmt(),
                    ],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
    ]
}

/// Appends storage get array copy body to the current runtime or metadata collection.
pub(super) fn append_storage_get_array_copy_body() -> Vec<Stmt> {
    vec![
        assign_stmt("out", empty_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(append_iterator_keys_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    append_active_at_position_expr(var_expr("i")),
                    vec![array_assign_stmt(
                        "out",
                        append_key_at_position_expr(var_expr("i")),
                        array_access(append_iterators_expr(), var_expr("i")),
                    )],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(var_expr("out")),
    ]
}

/// Appends storage current body to the current runtime or metadata collection.
pub(super) fn append_storage_current_body() -> Vec<Stmt> {
    vec![
        assign_stmt("i", var_expr("position")),
        return_stmt(array_access(append_iterators_expr(), var_expr("i"))),
    ]
}
