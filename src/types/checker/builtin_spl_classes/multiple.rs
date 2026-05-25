//! Purpose:
//! Injects MultipleIterator metadata and composite key/current traversal bodies.
//! Keeps multi-source zip semantics separate from AppendIterator's sequential traversal.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Flags control need-any/need-all validity and numeric/associative output keys.
//! - Iterator/info arrays stay parallel and are rebuilt when detaching.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, Stmt, TypeExpr};
use crate::types::traits::FlattenedClass;

use super::common::*;

pub(super) fn insert_class(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "MultipleIterator".to_string(),
        FlattenedClass {
            name: "MultipleIterator".to_string(),
            extends: None,
            implements: vec!["Iterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: multiple_iterator_properties(),
            methods: spl_multiple_iterator_methods(),
            attributes: Vec::new(),
            constants: multiple_iterator_constants(),
            used_traits: Vec::new(),
        },
    );
}

fn multiple_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("iterators", array_type()),
        storage_property("infos", array_type()),
        storage_property("flags", TypeExpr::Int),
    ]
}

fn spl_multiple_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param_default("flags", TypeExpr::Int, int_expr(1))],
            Some(TypeExpr::Void),
            multiple_construct_body(),
        ),
        method_with_body("getFlags", Vec::new(), Some(TypeExpr::Int), return_body(multiple_flags_expr())),
        method_with_body(
            "setFlags",
            vec![param("flags", TypeExpr::Int)],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "flags", var_expr("flags"))],
        ),
        method_with_body(
            "attachIterator",
            vec![
                param("iterator", named_type("Iterator")),
                param_default(
                    "info",
                    TypeExpr::Nullable(Box::new(TypeExpr::Union(vec![TypeExpr::Str, TypeExpr::Int]))),
                    null_expr(),
                ),
            ],
            Some(TypeExpr::Void),
            multiple_attach_iterator_body(),
        ),
        method_with_body(
            "detachIterator",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            multiple_detach_iterator_body(),
        ),
        method_with_body(
            "containsIterator",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Bool),
            multiple_contains_iterator_body(),
        ),
        method_with_body(
            "countIterators",
            Vec::new(),
            Some(TypeExpr::Int),
            return_body(count_expr(multiple_iterators_expr())),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), multiple_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), multiple_valid_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), multiple_output_body("key")),
        method_with_body("current", Vec::new(), Some(mixed_type()), multiple_output_body("current")),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), multiple_next_body()),
        method_with_body("__debugInfo", Vec::new(), Some(array_type()), multiple_debug_info_body()),
    ]
}

fn multiple_iterator_constants() -> Vec<ClassConst> {
    vec![
        class_const("MIT_NEED_ANY", 0),
        class_const("MIT_NEED_ALL", 1),
        class_const("MIT_KEYS_NUMERIC", 0),
        class_const("MIT_KEYS_ASSOC", 2),
    ]
}

fn multiple_iterators_expr() -> Expr {
    property_access(this_expr(), "iterators")
}

fn multiple_infos_expr() -> Expr {
    property_access(this_expr(), "infos")
}

fn multiple_flags_expr() -> Expr {
    property_access(this_expr(), "flags")
}

fn multiple_iterator_at(index: Expr) -> Expr {
    array_access(multiple_iterators_expr(), index)
}

fn multiple_info_at(index: Expr) -> Expr {
    array_access(multiple_infos_expr(), index)
}

fn multiple_need_all_expr() -> Expr {
    binary_expr(
        binary_expr(multiple_flags_expr(), BinOp::BitAnd, int_expr(1)),
        BinOp::NotEq,
        int_expr(0),
    )
}

fn multiple_assoc_keys_expr() -> Expr {
    binary_expr(
        binary_expr(multiple_flags_expr(), BinOp::BitAnd, int_expr(2)),
        BinOp::NotEq,
        int_expr(0),
    )
}

fn multiple_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "iterators", empty_array_expr()),
        property_assign_stmt(this_expr(), "infos", empty_array_expr()),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
    ]
}

fn multiple_attach_iterator_body() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(multiple_iterator_at(var_expr("i")), BinOp::StrictEq, var_expr("iterator")),
                    vec![
                        property_array_assign_stmt(this_expr(), "infos", var_expr("i"), var_expr("info")),
                        return_void_stmt(),
                    ],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        property_array_push_stmt(this_expr(), "iterators", var_expr("iterator")),
        property_array_push_stmt(this_expr(), "infos", var_expr("info")),
    ]
}

fn multiple_detach_iterator_body() -> Vec<Stmt> {
    vec![
        assign_stmt("newIterators", empty_array_expr()),
        assign_stmt("newInfos", empty_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                assign_stmt("candidate", multiple_iterator_at(var_expr("i"))),
                if_stmt(
                    not_expr(binary_expr(var_expr("candidate"), BinOp::StrictEq, var_expr("iterator"))),
                    vec![
                        array_push_stmt("newIterators", var_expr("candidate")),
                        array_push_stmt("newInfos", multiple_info_at(var_expr("i"))),
                    ],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        property_assign_stmt(this_expr(), "iterators", var_expr("newIterators")),
        property_assign_stmt(this_expr(), "infos", var_expr("newInfos")),
    ]
}

fn multiple_contains_iterator_body() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(multiple_iterator_at(var_expr("i")), BinOp::StrictEq, var_expr("iterator")),
                    return_body(bool_expr(true)),
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(bool_expr(false)),
    ]
}

fn multiple_each_iterator_body(method: &str) -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                typed_assign_stmt("iterator", named_type("Iterator"), multiple_iterator_at(var_expr("i"))),
                expr_stmt(method_call(var_expr("iterator"), method, Vec::new())),
                increment_stmt("i"),
            ],
        ),
    ]
}

fn multiple_rewind_body() -> Vec<Stmt> {
    multiple_each_iterator_body("rewind")
}

fn multiple_next_body() -> Vec<Stmt> {
    multiple_each_iterator_body("next")
}

fn multiple_valid_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(count_expr(multiple_iterators_expr()), BinOp::StrictEq, int_expr(0)),
            return_body(bool_expr(false)),
            None,
        ),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        if_stmt(
            multiple_need_all_expr(),
            vec![
                while_stmt(
                    binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
                    vec![
                        typed_assign_stmt("iterator", named_type("Iterator"), multiple_iterator_at(var_expr("i"))),
                        if_stmt(
                            not_expr(method_call(var_expr("iterator"), "valid", Vec::new())),
                            return_body(bool_expr(false)),
                            None,
                        ),
                        increment_stmt("i"),
                    ],
                ),
                return_stmt(bool_expr(true)),
            ],
            None,
        ),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                typed_assign_stmt("iterator", named_type("Iterator"), multiple_iterator_at(var_expr("i"))),
                if_stmt(
                    method_call(var_expr("iterator"), "valid", Vec::new()),
                    return_body(bool_expr(true)),
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(bool_expr(false)),
    ]
}

fn multiple_output_body(method: &str) -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(count_expr(multiple_iterators_expr()), BinOp::StrictEq, int_expr(0)),
            vec![throw_stmt(new_object_expr(
                "RuntimeException",
                vec![string_expr(&format!("Called {method}() on an invalid iterator"))],
            ))],
            None,
        ),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        if_stmt(
            multiple_need_all_expr(),
            vec![
                while_stmt(
                    binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
                    vec![
                        typed_assign_stmt("iterator", named_type("Iterator"), multiple_iterator_at(var_expr("i"))),
                        if_stmt(
                            not_expr(method_call(var_expr("iterator"), "valid", Vec::new())),
                            vec![throw_stmt(new_object_expr(
                                "RuntimeException",
                                vec![string_expr(&format!(
                                    "Called {method}() with non valid sub iterator"
                                ))],
                            ))],
                            None,
                        ),
                        increment_stmt("i"),
                    ],
                ),
            ],
            None,
        ),
        assign_stmt("out", empty_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                typed_assign_stmt("iterator", named_type("Iterator"), multiple_iterator_at(var_expr("i"))),
                assign_stmt("info", multiple_info_at(var_expr("i"))),
                if_stmt(
                    multiple_assoc_keys_expr(),
                    vec![if_stmt(
                        function_call("is_null", vec![var_expr("info")]),
                        vec![throw_stmt(new_object_expr(
                            "InvalidArgumentException",
                            vec![string_expr("Sub-Iterator is associated with NULL")],
                        ))],
                        None,
                    )],
                    None,
                ),
                typed_assign_stmt("item", mixed_type(), null_expr()),
                if_stmt(
                    method_call(var_expr("iterator"), "valid", Vec::new()),
                    vec![assign_stmt("item", method_call(var_expr("iterator"), method, Vec::new()))],
                    None,
                ),
                if_stmt(
                    multiple_assoc_keys_expr(),
                    vec![array_assign_stmt("out", var_expr("info"), var_expr("item"))],
                    Some(vec![array_assign_stmt("out", var_expr("i"), var_expr("item"))]),
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(var_expr("out")),
    ]
}

fn multiple_debug_info_body() -> Vec<Stmt> {
    return_body(expr(ExprKind::ArrayLiteralAssoc(vec![
        (string_expr("\0MultipleIterator\0iterators"), multiple_iterators_expr()),
        (string_expr("\0MultipleIterator\0infos"), multiple_infos_expr()),
        (string_expr("\0MultipleIterator\0flags"), multiple_flags_expr()),
    ])))
}
