//! Purpose:
//! Builds `RecursiveTreeIterator`'s storage and methods: the ASCII-tree prefix, entry and postfix
//! php renders for each element of a `RecursiveIteratorIterator` traversal.
//!
//! Called from:
//! - `super::recursive_iterator_iterator::insert_classes()`.
//!
//! Key details:
//! - Mirrors php-src `spl_iterators.c`: the constructor wraps the iterator in a
//!   `RecursiveCachingIterator` (`CATCH_GET_CHILD` by default) so every level can answer
//!   `hasNext()`, and traverses `SELF_FIRST` with `BYPASS_KEY` unless told otherwise.
//! - The prefix is `PREFIX_LEFT`, one `MID_HAS_NEXT`/`MID_LAST` part per enclosing level, the
//!   `END_HAS_NEXT`/`END_LAST` part of the current level, then `PREFIX_RIGHT`.
//! - The raw element and key come from the parent's `current()`/`key()` through `parent::`.

use crate::parser::ast::{BinOp, CastType, ClassMethod, ClassProperty, Expr, ExprKind, Stmt, TypeExpr};
use crate::synthetic_class::e_parent_call;

use super::common::*;

/// `RecursiveTreeIterator::BYPASS_CURRENT`.
const BYPASS_CURRENT: i64 = 4;
/// `RecursiveTreeIterator::BYPASS_KEY`, the default flags.
const BYPASS_KEY: i64 = 8;
/// `CachingIterator::CATCH_GET_CHILD`, the default caching-iterator flags.
const CATCH_GET_CHILD: i64 = 16;
/// `RecursiveIteratorIterator::SELF_FIRST`, the default mode.
const SELF_FIRST: i64 = 1;

/// The tree's own storage: the six prefix parts, the postfix, and the flags it renders with.
pub(super) fn recursive_tree_iterator_properties() -> Vec<ClassProperty> {
    let parts = ["", "| ", "  ", "|-", "\\-", ""];
    vec![
        storage_property_default(
            "prefixParts",
            string_array_type(),
            expr(ExprKind::ArrayLiteral(parts.iter().map(|part| string_expr(part)).collect())),
        ),
        storage_property_default("postfix", TypeExpr::Str, string_expr("")),
        storage_property_default("treeFlags", TypeExpr::Int, int_expr(BYPASS_KEY)),
    ]
}

/// The tree's methods, in php's declaration order.
pub(super) fn recursive_tree_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", mixed_type()),
                param_default("flags", TypeExpr::Int, int_expr(BYPASS_KEY)),
                param_default("cachingIteratorFlags", TypeExpr::Int, int_expr(CATCH_GET_CHILD)),
                param_default("mode", TypeExpr::Int, int_expr(SELF_FIRST)),
            ],
            Some(TypeExpr::Void),
            construct_body(),
        ),
        method_with_body("key", Vec::new(), Some(mixed_type()), key_body()),
        method_with_body("current", Vec::new(), Some(mixed_type()), current_body()),
        method_with_body("getPrefix", Vec::new(), Some(TypeExpr::Str), get_prefix_body()),
        method_with_body(
            "setPostfix",
            vec![param("postfix", TypeExpr::Str)],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "postfix", var_expr("postfix"))],
        ),
        method_with_body(
            "setPrefixPart",
            vec![param("part", TypeExpr::Int), param("value", TypeExpr::Str)],
            Some(TypeExpr::Void),
            set_prefix_part_body(),
        ),
        method_with_body(
            "getEntry",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(TypeExpr::Str))),
            get_entry_body(),
        ),
        method_with_body(
            "getPostfix",
            Vec::new(),
            Some(TypeExpr::Str),
            return_body(property_access(this_expr(), "postfix")),
        ),
    ]
}

/// `$this->treeFlags = $flags;` then
/// `parent::__construct(new RecursiveCachingIterator($iterator, $cachingIteratorFlags), $mode, $flags);`.
fn construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "treeFlags", var_expr("flags")),
        expr_stmt(e_parent_call(
            "__construct",
            vec![
                new_object_expr(
                    "RecursiveCachingIterator",
                    vec![var_expr("iterator"), var_expr("cachingIteratorFlags")],
                ),
                var_expr("mode"),
                var_expr("flags"),
            ],
        )),
    ]
}

/// `($this->treeFlags & <flag>) != 0`.
fn tree_flag_set(flag: i64) -> Expr {
    binary_expr(
        binary_expr(property_access(this_expr(), "treeFlags"), BinOp::BitAnd, int_expr(flag)),
        BinOp::NotEq,
        int_expr(0),
    )
}

/// `$this->getPrefix() . <middle> . $this->getPostfix()`.
fn decorated(middle: Expr) -> Expr {
    binary_expr(
        binary_expr(method_call(this_expr(), "getPrefix", Vec::new()), BinOp::Concat, middle),
        BinOp::Concat,
        method_call(this_expr(), "getPostfix", Vec::new()),
    )
}

/// `$key = parent::key(); if (BYPASS_KEY) return $key; return prefix . $key . postfix;`.
fn key_body() -> Vec<Stmt> {
    vec![
        assign_stmt("key", e_parent_call("key", Vec::new())),
        if_stmt(tree_flag_set(BYPASS_KEY), return_body(var_expr("key")), None),
        return_stmt(decorated(cast_expr(CastType::String, var_expr("key")))),
    ]
}

/// `if (BYPASS_CURRENT) return parent::current(); return prefix . getEntry() . postfix;`.
fn current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            tree_flag_set(BYPASS_CURRENT),
            return_body(e_parent_call("current", Vec::new())),
            None,
        ),
        return_stmt(decorated(cast_expr(
            CastType::String,
            method_call(this_expr(), "getEntry", Vec::new()),
        ))),
    ]
}

/// `$this->prefixParts[$index]`.
fn prefix_part(index: Expr) -> Expr {
    array_access(property_access(this_expr(), "prefixParts"), index)
}

/// `(($sub = $this->getSubIterator($level)) instanceof RecursiveCachingIterator && $sub->hasNext())
/// ? <has_next> : <last>`, written as statements: `$part` holds the chosen prefix part.
fn level_part(level: Expr, has_next: i64, last: i64) -> Vec<Stmt> {
    vec![
        assign_stmt("part", prefix_part(int_expr(last))),
        assign_stmt("sub", method_call(this_expr(), "getSubIterator", vec![level])),
        if_stmt(
            instanceof_expr(var_expr("sub"), "RecursiveCachingIterator"),
            vec![if_stmt(
                method_call(var_expr("sub"), "hasNext", Vec::new()),
                vec![assign_stmt("part", prefix_part(int_expr(has_next)))],
                None,
            )],
            None,
        ),
        assign_stmt("prefix", binary_expr(var_expr("prefix"), BinOp::Concat, var_expr("part"))),
    ]
}

/// php-src `spl_recursive_tree_iterator_get_prefix`.
fn get_prefix_body() -> Vec<Stmt> {
    let mut per_level = level_part(var_expr("level"), 1, 2);
    per_level.push(increment_stmt("level"));
    let mut body = vec![
        assign_stmt("prefix", prefix_part(int_expr(0))),
        assign_stmt("depth", method_call(this_expr(), "getDepth", Vec::new())),
        assign_stmt("level", int_expr(0)),
        while_stmt(binary_expr(var_expr("level"), BinOp::Lt, var_expr("depth")), per_level),
    ];
    body.extend(level_part(var_expr("depth"), 3, 4));
    body.push(return_stmt(binary_expr(var_expr("prefix"), BinOp::Concat, prefix_part(int_expr(5)))));
    body
}

/// Throws php's `ValueError` for a part outside `PREFIX_LEFT..=PREFIX_RIGHT`, then stores it.
fn set_prefix_part_body() -> Vec<Stmt> {
    let out_of_range = binary_expr(
        binary_expr(var_expr("part"), BinOp::Lt, int_expr(0)),
        BinOp::Or,
        binary_expr(var_expr("part"), BinOp::Gt, int_expr(5)),
    );
    vec![
        if_stmt(
            out_of_range,
            vec![throw_stmt(new_object_expr(
                "ValueError",
                vec![string_expr(
                    "RecursiveTreeIterator::setPrefixPart(): Argument #1 ($part) must be a RecursiveTreeIterator::PREFIX_* constant",
                )],
            ))],
            None,
        ),
        property_array_assign_stmt(this_expr(), "prefixParts", var_expr("part"), var_expr("value")),
    ]
}

/// The current element as php's tree prints it: `Array` for an array (without the conversion
/// warning), otherwise its string form.
fn get_entry_body() -> Vec<Stmt> {
    vec![
        assign_stmt("entry", e_parent_call("current", Vec::new())),
        if_stmt(
            function_call("is_array", vec![var_expr("entry")]),
            return_body(string_expr("Array")),
            None,
        ),
        return_stmt(cast_expr(CastType::String, var_expr("entry"))),
    ]
}
