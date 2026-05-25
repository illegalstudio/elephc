//! Purpose:
//! Builds RecursiveIteratorIterator traversal-state synthetic method bodies.
//! Keeps recursive frame-stack transitions away from class declaration wiring.
//!
//! Called from:
//! - `super::recursive_iterator_iterator` method declarations.
//!
//! Key details:
//! - State arrays track iterators, frame states, and frame depths in parallel.
//! - Advance logic preserves self-first, child-first, and leaves-only traversal modes.

use crate::parser::ast::{BinOp, Expr, Stmt, TypeExpr};

use super::common::*;
use super::recursive_array::assume_recursive_iterator_expr;

fn recursive_iterator_iterator_root_expr() -> Expr {
    property_access(this_expr(), "root")
}

fn recursive_iterator_iterator_mode_expr() -> Expr {
    property_access(this_expr(), "mode")
}

fn recursive_iterator_iterator_iterators_expr() -> Expr {
    property_access(this_expr(), "iterators")
}

fn recursive_iterator_iterator_states_expr() -> Expr {
    property_access(this_expr(), "states")
}

fn recursive_iterator_iterator_depths_expr() -> Expr {
    property_access(this_expr(), "depths")
}

fn recursive_iterator_iterator_depth_expr() -> Expr {
    property_access(this_expr(), "depth")
}

fn recursive_iterator_iterator_slot_expr() -> Expr {
    property_access(this_expr(), "slot")
}

fn recursive_iterator_iterator_current_valid_expr() -> Expr {
    property_access(this_expr(), "currentValid")
}

fn recursive_iterator_iterator_valid_expr() -> Expr {
    recursive_iterator_iterator_current_valid_expr()
}

fn recursive_iterator_iterator_iterator_at_depth(depth: Expr) -> Expr {
    array_access(
        recursive_iterator_iterator_iterators_expr(),
        depth,
    )
}

fn recursive_iterator_iterator_state_at_current_slot() -> Expr {
    array_access(
        recursive_iterator_iterator_states_expr(),
        recursive_iterator_iterator_slot_expr(),
    )
}

fn recursive_iterator_iterator_depth_at_current_slot() -> Expr {
    array_access(
        recursive_iterator_iterator_depths_expr(),
        recursive_iterator_iterator_slot_expr(),
    )
}

fn recursive_iterator_iterator_current_iterator_expr() -> Expr {
    recursive_iterator_iterator_iterator_at_depth(recursive_iterator_iterator_slot_expr())
}

fn recursive_iterator_iterator_slot_for_depth_expr(depth: Expr) -> Expr {
    method_call(this_expr(), "__elephcSlotForDepth", vec![depth])
}

pub(super) fn recursive_iterator_iterator_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "root", var_expr("iterator")),
        property_assign_stmt(this_expr(), "mode", var_expr("mode")),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
        property_assign_stmt(this_expr(), "iterators", empty_array_expr()),
        property_assign_stmt(this_expr(), "states", empty_array_expr()),
        property_assign_stmt(this_expr(), "depths", empty_array_expr()),
        property_assign_stmt(this_expr(), "depth", int_expr(0)),
        property_assign_stmt(this_expr(), "slot", int_expr(0)),
        property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
    ]
}

pub(super) fn recursive_iterator_iterator_rewind_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "iterators", empty_array_expr()),
        property_assign_stmt(this_expr(), "states", empty_array_expr()),
        property_assign_stmt(this_expr(), "depths", empty_array_expr()),
        property_assign_stmt(this_expr(), "depth", int_expr(0)),
        property_assign_stmt(this_expr(), "slot", int_expr(0)),
        property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
        expr_stmt(method_call(
            recursive_iterator_iterator_root_expr(),
            "rewind",
            Vec::new(),
        )),
        if_stmt(
            method_call(recursive_iterator_iterator_root_expr(), "valid", Vec::new()),
            vec![
                property_array_push_stmt(this_expr(), "iterators", recursive_iterator_iterator_root_expr()),
                property_array_push_stmt(this_expr(), "states", int_expr(0)),
                property_array_push_stmt(this_expr(), "depths", int_expr(0)),
                expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new())),
            ],
            None,
        ),
    ]
}

pub(super) fn recursive_iterator_iterator_valid_body() -> Vec<Stmt> {
    return_body(recursive_iterator_iterator_valid_expr())
}

pub(super) fn recursive_iterator_iterator_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(recursive_iterator_iterator_valid_expr()),
            null_return_body(),
            None,
        ),
        return_stmt(method_call(
            recursive_iterator_iterator_current_iterator_expr(),
            "current",
            Vec::new(),
        )),
    ]
}

pub(super) fn recursive_iterator_iterator_key_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(recursive_iterator_iterator_valid_expr()),
            null_return_body(),
            None,
        ),
        return_stmt(method_call(
            recursive_iterator_iterator_current_iterator_expr(),
            "key",
            Vec::new(),
        )),
    ]
}

pub(super) fn recursive_iterator_iterator_next_body() -> Vec<Stmt> {
    vec![if_stmt(
        recursive_iterator_iterator_valid_expr(),
        vec![expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new()))],
        None,
    )]
}

pub(super) fn recursive_iterator_iterator_get_depth_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(recursive_iterator_iterator_valid_expr()),
            return_body(int_expr(0)),
            None,
        ),
        return_stmt(recursive_iterator_iterator_depth_expr()),
    ]
}

pub(super) fn recursive_iterator_iterator_get_inner_iterator_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            recursive_iterator_iterator_valid_expr(),
            return_body(recursive_iterator_iterator_current_iterator_expr()),
            None,
        ),
        return_stmt(recursive_iterator_iterator_root_expr()),
    ]
}

pub(super) fn recursive_iterator_iterator_get_sub_iterator_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(recursive_iterator_iterator_valid_expr()),
            vec![
                if_stmt(
                    binary_expr(var_expr("level"), BinOp::LtEq, int_expr(0)),
                    return_body(recursive_iterator_iterator_root_expr()),
                    None,
                ),
                return_stmt(null_expr()),
            ],
            None,
        ),
        if_stmt(
            binary_expr(var_expr("level"), BinOp::Lt, int_expr(0)),
            return_body(recursive_iterator_iterator_current_iterator_expr()),
            None,
        ),
        if_stmt(
            binary_expr(
                var_expr("level"),
                BinOp::LtEq,
                recursive_iterator_iterator_depth_expr(),
            ),
            return_body(recursive_iterator_iterator_iterator_at_depth(
                recursive_iterator_iterator_slot_for_depth_expr(var_expr("level")),
            )),
            None,
        ),
        return_stmt(null_expr()),
    ]
}

pub(super) fn recursive_iterator_iterator_slot_for_depth_body() -> Vec<Stmt> {
    vec![
        typed_assign_stmt("i", TypeExpr::Int, int_expr(0)),
        typed_assign_stmt("slot", TypeExpr::Int, int_expr(0)),
        typed_assign_stmt("limit", TypeExpr::Int, count_expr(recursive_iterator_iterator_depths_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(
                        array_access(recursive_iterator_iterator_depths_expr(), var_expr("i")),
                        BinOp::StrictEq,
                        var_expr("level"),
                    ),
                    vec![assign_stmt("slot", var_expr("i"))],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(var_expr("slot")),
    ]
}

pub(super) fn recursive_iterator_iterator_advance_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
        property_assign_stmt(
            this_expr(),
            "depth",
            recursive_iterator_iterator_depth_at_current_slot(),
        ),
        assign_stmt("iterator", recursive_iterator_iterator_current_iterator_expr()),
        if_stmt(
            not_expr(method_call(var_expr("iterator"), "valid", Vec::new())),
            recursive_iterator_iterator_pop_invalid_frame_body(),
            None,
        ),
        assign_stmt(
            "state",
            recursive_iterator_iterator_state_at_current_slot(),
        ),
        if_stmt(
            binary_expr(var_expr("state"), BinOp::StrictEq, int_expr(2)),
            vec![
                expr_stmt(method_call(var_expr("iterator"), "next", Vec::new())),
                property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(0)),
                expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new())),
                return_void_stmt(),
            ],
            None,
        ),
        if_stmt(
            binary_expr(
                recursive_iterator_iterator_mode_expr(),
                BinOp::StrictEq,
                int_expr(1),
            ),
            recursive_iterator_iterator_advance_self_first_body(),
            Some(recursive_iterator_iterator_advance_children_first_or_leaves_body()),
        ),
    ]
}

fn recursive_iterator_iterator_pop_invalid_frame_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(recursive_iterator_iterator_depth_expr(), BinOp::StrictEq, int_expr(0)),
            vec![
                property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
                return_void_stmt(),
            ],
            Some(vec![
                typed_assign_stmt(
                    "previousDepth",
                    TypeExpr::Int,
                    binary_expr(recursive_iterator_iterator_depth_expr(), BinOp::Sub, int_expr(1)),
                ),
                property_assign_stmt(
                    this_expr(),
                    "depth",
                    var_expr("previousDepth"),
                ),
                property_assign_stmt(
                    this_expr(),
                    "slot",
                    recursive_iterator_iterator_slot_for_depth_expr(var_expr("previousDepth")),
                ),
                expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new())),
                return_void_stmt(),
            ]),
        ),
    ]
}

fn recursive_iterator_iterator_advance_self_first_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(var_expr("state"), BinOp::StrictEq, int_expr(0)),
            vec![
                property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(1)),
                property_assign_stmt(this_expr(), "currentValid", bool_expr(true)),
                return_void_stmt(),
            ],
            None,
        ),
        assign_stmt(
            "hasChildren",
            method_call(var_expr("iterator"), "hasChildren", Vec::new()),
        ),
        if_stmt(
            var_expr("hasChildren"),
            vec![
                assign_stmt("child", method_call(var_expr("iterator"), "getChildren", Vec::new())),
                if_stmt(
                    not_expr(function_call("is_null", vec![var_expr("child")])),
                    recursive_iterator_iterator_descend_current_child_body(int_expr(2)),
                    None,
                ),
            ],
            None,
        ),
        property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(2)),
        expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new())),
        return_void_stmt(),
    ]
}

fn recursive_iterator_iterator_advance_children_first_or_leaves_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(var_expr("state"), BinOp::StrictEq, int_expr(1)),
            vec![
                property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(2)),
                property_assign_stmt(this_expr(), "currentValid", bool_expr(true)),
                return_void_stmt(),
            ],
            None,
        ),
        assign_stmt(
            "hasChildren",
            method_call(var_expr("iterator"), "hasChildren", Vec::new()),
        ),
        if_stmt(
            var_expr("hasChildren"),
            vec![
                assign_stmt("child", method_call(var_expr("iterator"), "getChildren", Vec::new())),
                if_stmt(
                    not_expr(function_call("is_null", vec![var_expr("child")])),
                    recursive_iterator_iterator_non_self_child_body(),
                    None,
                ),
            ],
            None,
        ),
        property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(2)),
        property_assign_stmt(this_expr(), "currentValid", bool_expr(true)),
        return_void_stmt(),
    ]
}

fn recursive_iterator_iterator_non_self_child_body() -> Vec<Stmt> {
    let mut body = recursive_iterator_iterator_descend_current_child_body(int_expr(2));
    body.push(if_stmt(
        binary_expr(
            recursive_iterator_iterator_mode_expr(),
            BinOp::StrictEq,
            int_expr(0),
        ),
        vec![
            property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(2)),
            expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new())),
            return_void_stmt(),
        ],
        None,
    ));
    body
}

fn recursive_iterator_iterator_descend_current_child_body(parent_state: Expr) -> Vec<Stmt> {
    vec![
        assign_stmt(
            "recursiveChild",
            assume_recursive_iterator_expr(var_expr("child")),
        ),
        expr_stmt(method_call(var_expr("recursiveChild"), "rewind", Vec::new())),
        if_stmt(
            method_call(var_expr("recursiveChild"), "valid", Vec::new()),
            vec![
                if_stmt(
                    binary_expr(
                        recursive_iterator_iterator_mode_expr(),
                        BinOp::StrictEq,
                        int_expr(2),
                    ),
                    vec![property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(1))],
                    Some(vec![property_array_assign_stmt(
                        this_expr(),
                        "states",
                        recursive_iterator_iterator_slot_expr(),
                        parent_state,
                    )]),
                ),
                typed_assign_stmt(
                    "nextDepth",
                    TypeExpr::Int,
                    binary_expr(recursive_iterator_iterator_depth_expr(), BinOp::Add, int_expr(1)),
                ),
                typed_assign_stmt("nextSlot", TypeExpr::Int, count_expr(recursive_iterator_iterator_iterators_expr())),
                property_array_push_stmt(this_expr(), "iterators", var_expr("recursiveChild")),
                property_array_push_stmt(this_expr(), "states", int_expr(0)),
                property_array_push_stmt(this_expr(), "depths", var_expr("nextDepth")),
                property_assign_stmt(this_expr(), "depth", var_expr("nextDepth")),
                property_assign_stmt(this_expr(), "slot", var_expr("nextSlot")),
                expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new())),
                return_void_stmt(),
            ],
            None,
        ),
    ]
}
