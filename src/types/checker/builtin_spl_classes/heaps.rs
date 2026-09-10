//! Purpose:
//! Injects SPL heap and priority-queue metadata backed by ordinary object properties.
//! The synthetic method bodies provide PHP-visible ordering, destructive iteration, and flags.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Insert and extraction maintain php-src's physical binary-heap layout through virtual `compare()` calls.
//! - Debug snapshots copy that physical layout without comparison or receiver mutation.
//! - Storage lives in per-instance arrays, allowing normal object deep-free to finalize handles.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, ClassConst, ClassMethod, ClassProperty, Expr, Stmt, TypeExpr, Visibility};
use crate::types::traits::FlattenedClass;

use super::common::*;

/// Inserts SPL heap, max/min heap, and priority queue classes into the builtin registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "SplHeap".to_string(),
        FlattenedClass {
            name: "SplHeap".to_string(),
            span: crate::span::Span::dummy(),
            extends: None,
            implements: vec!["Iterator".to_string(), "Countable".to_string()],
            is_abstract: true,
            is_final: false,
            is_readonly_class: false,
            properties: spl_heap_properties(),
            methods: spl_heap_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "SplMaxHeap".to_string(),
        FlattenedClass {
            name: "SplMaxHeap".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("SplHeap".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: vec![spl_max_heap_compare_method()],
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "SplMinHeap".to_string(),
        FlattenedClass {
            name: "SplMinHeap".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("SplHeap".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: vec![spl_min_heap_compare_method()],
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "SplPriorityQueue".to_string(),
        FlattenedClass {
            name: "SplPriorityQueue".to_string(),
            span: crate::span::Span::dummy(),
            extends: None,
            implements: vec!["Iterator".to_string(), "Countable".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: spl_priority_queue_properties(),
            methods: spl_priority_queue_methods(),
            attributes: Vec::new(),
            constants: spl_priority_queue_constants(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
}

/// Returns the hidden per-instance storage fields used by `SplHeap`.
fn spl_heap_properties() -> Vec<ClassProperty> {
    vec![protected_storage_property("values", array_type())]
}

/// Returns public and protected methods for the abstract `SplHeap` base class.
fn spl_heap_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body("__construct", Vec::new(), Some(TypeExpr::Void), heap_construct_body()),
        method_with_body("insert", vec![param("value", mixed_type())], Some(TypeExpr::Bool), heap_insert_body()),
        method_with_body("extract", Vec::new(), Some(mixed_type()), heap_extract_body()),
        method_with_body("top", Vec::new(), Some(mixed_type()), heap_top_body()),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), heap_count_body()),
        method_with_body("isEmpty", Vec::new(), Some(TypeExpr::Bool), heap_is_empty_body()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), Vec::new()),
        method_with_body("current", Vec::new(), Some(mixed_type()), heap_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), heap_key_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), heap_next_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), heap_valid_body()),
        method_with_body("recoverFromCorruption", Vec::new(), Some(TypeExpr::Bool), return_body(bool_expr(true))),
        method_with_body("isCorrupted", Vec::new(), Some(TypeExpr::Bool), return_body(bool_expr(false))),
        method_with_body("__debugInfo", Vec::new(), Some(array_type()), heap_debug_info_body()),
        protected_abstract_method(
            "compare",
            vec![param("value1", mixed_type()), param("value2", mixed_type())],
            Some(TypeExpr::Int),
        ),
        protected_method_with_body("__elephcBestIndex", Vec::new(), Some(TypeExpr::Int), heap_best_index_body()),
        protected_method_with_body(
            "__elephcRemoveAt",
            vec![param("removeIndex", TypeExpr::Int)],
            Some(TypeExpr::Void),
            heap_remove_at_body(),
        ),
    ]
}

/// Builds the protected `SplMaxHeap::compare()` override.
fn spl_max_heap_compare_method() -> ClassMethod {
    protected_method_with_body(
        "compare",
        vec![param("value1", mixed_type()), param("value2", mixed_type())],
        Some(TypeExpr::Int),
        return_body(binary_expr(var_expr("value1"), BinOp::Spaceship, var_expr("value2"))),
    )
}

/// Builds the protected `SplMinHeap::compare()` override.
fn spl_min_heap_compare_method() -> ClassMethod {
    protected_method_with_body(
        "compare",
        vec![param("value1", mixed_type()), param("value2", mixed_type())],
        Some(TypeExpr::Int),
        return_body(binary_expr(var_expr("value2"), BinOp::Spaceship, var_expr("value1"))),
    )
}

/// Returns the hidden storage fields used by `SplPriorityQueue`.
fn spl_priority_queue_properties() -> Vec<ClassProperty> {
    vec![
        protected_storage_property("values", array_type()),
        protected_storage_property("priorities", array_type()),
        protected_storage_property("extractFlags", TypeExpr::Int),
    ]
}

/// Returns all public and hidden methods for `SplPriorityQueue`.
fn spl_priority_queue_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body("__construct", Vec::new(), Some(TypeExpr::Void), priority_construct_body()),
        method_with_body(
            "compare",
            vec![param("priority1", mixed_type()), param("priority2", mixed_type())],
            Some(TypeExpr::Int),
            return_body(binary_expr(var_expr("priority1"), BinOp::Spaceship, var_expr("priority2"))),
        ),
        method_with_body(
            "insert",
            vec![param("value", mixed_type()), param("priority", mixed_type())],
            Some(TypeExpr::Bool),
            priority_insert_body(),
        ),
        method_with_body("setExtractFlags", vec![param("flags", TypeExpr::Int)], Some(TypeExpr::Void), priority_set_extract_flags_body()),
        method_with_body("getExtractFlags", Vec::new(), Some(TypeExpr::Int), return_body(priority_flags_expr())),
        method_with_body("extract", Vec::new(), Some(mixed_type()), priority_extract_body()),
        method_with_body("top", Vec::new(), Some(mixed_type()), priority_top_body()),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), priority_count_body()),
        method_with_body("isEmpty", Vec::new(), Some(TypeExpr::Bool), priority_is_empty_body()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), Vec::new()),
        method_with_body("current", Vec::new(), Some(mixed_type()), priority_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), priority_key_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), priority_next_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), priority_valid_body()),
        method_with_body("recoverFromCorruption", Vec::new(), Some(TypeExpr::Bool), return_body(bool_expr(true))),
        method_with_body("isCorrupted", Vec::new(), Some(TypeExpr::Bool), return_body(bool_expr(false))),
        method_with_body("__debugInfo", Vec::new(), Some(array_type()), priority_debug_info_body()),
        protected_method_with_body("__elephcBestIndex", Vec::new(), Some(TypeExpr::Int), priority_best_index_body()),
        protected_method_with_body(
            "__elephcOutputAt",
            vec![param("index", TypeExpr::Int)],
            Some(mixed_type()),
            priority_output_at_body(),
        ),
        protected_method_with_body(
            "__elephcRemoveAt",
            vec![param("removeIndex", TypeExpr::Int)],
            Some(TypeExpr::Void),
            priority_remove_at_body(),
        ),
    ]
}

/// Returns public extraction constants for `SplPriorityQueue`.
fn spl_priority_queue_constants() -> Vec<ClassConst> {
    vec![
        class_const("EXTR_DATA", 1),
        class_const("EXTR_PRIORITY", 2),
        class_const("EXTR_BOTH", 3),
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

/// Builds a protected abstract synthetic method.
fn protected_abstract_method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
) -> ClassMethod {
    let mut method = abstract_method(name, params, return_type);
    method.visibility = Visibility::Protected;
    method
}

/// Returns `$this->values`.
fn heap_values_expr() -> Expr {
    property_access(this_expr(), "values")
}

/// Returns `$this->values[$index]`.
fn heap_value_at(index: Expr) -> Expr {
    array_access(heap_values_expr(), index)
}

/// Returns `$this->__elephcBestIndex()`.
fn heap_best_index_expr() -> Expr {
    method_call(this_expr(), "__elephcBestIndex", Vec::new())
}

/// Initializes heap storage on construction.
fn heap_construct_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "values", empty_array_expr())]
}

/// Inserts one value into the persistent php-src-compatible binary-heap layout.
fn heap_insert_body() -> Vec<Stmt> {
    vec![
        typed_assign_stmt("heapValues", array_type(), heap_values_expr()),
        assign_stmt("position", count_expr(var_expr("heapValues"))),
        array_push_stmt("heapValues", var_expr("value")),
        assign_stmt("sifting", bool_expr(true)),
        while_stmt(
            binary_expr(
                binary_expr(var_expr("position"), BinOp::Gt, int_expr(0)),
                BinOp::And,
                var_expr("sifting"),
            ),
            vec![
                assign_stmt("parent", heap_parent_index_expr(var_expr("position"))),
                if_stmt(
                    binary_expr(
                        method_call(
                            this_expr(),
                            "compare",
                            vec![
                                array_access(var_expr("heapValues"), var_expr("parent")),
                                var_expr("value"),
                            ],
                        ),
                        BinOp::Lt,
                        int_expr(0),
                    ),
                    vec![
                        array_assign_stmt(
                            "heapValues",
                            var_expr("position"),
                            array_access(var_expr("heapValues"), var_expr("parent")),
                        ),
                        assign_stmt("position", var_expr("parent")),
                    ],
                    Some(vec![assign_stmt("sifting", bool_expr(false))]),
                ),
            ],
        ),
        array_assign_stmt("heapValues", var_expr("position"), var_expr("value")),
        property_assign_stmt(this_expr(), "values", var_expr("heapValues")),
        return_stmt(bool_expr(true)),
    ]
}

/// Returns the number of live heap elements.
fn heap_count_body() -> Vec<Stmt> {
    return_body(count_expr(heap_values_expr()))
}

/// Returns whether the heap has no live elements.
fn heap_is_empty_body() -> Vec<Stmt> {
    return_body(binary_expr(count_expr(heap_values_expr()), BinOp::StrictEq, int_expr(0)))
}

/// Returns the best heap value without removing it.
fn heap_top_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            method_call(this_expr(), "isEmpty", Vec::new()),
            vec![throw_stmt(new_object_expr(
                "RuntimeException",
                vec![string_expr("Can't peek at an empty heap")],
            ))],
            None,
        ),
        return_stmt(heap_value_at(heap_best_index_expr())),
    ]
}

/// Removes and returns the best heap value.
fn heap_extract_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            method_call(this_expr(), "isEmpty", Vec::new()),
            vec![throw_stmt(new_object_expr(
                "RuntimeException",
                vec![string_expr("Can't extract from an empty heap")],
            ))],
            None,
        ),
        assign_stmt("best", heap_best_index_expr()),
        assign_stmt("value", heap_value_at(var_expr("best"))),
        expr_stmt(method_call(this_expr(), "__elephcRemoveAt", vec![var_expr("best")])),
        return_stmt(var_expr("value")),
    ]
}

/// Returns current iterator value or null for an empty heap.
fn heap_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            method_call(this_expr(), "isEmpty", Vec::new()),
            return_body(null_expr()),
            None,
        ),
        return_stmt(heap_value_at(heap_best_index_expr())),
    ]
}

/// Returns PHP's destructive heap iterator key, which counts down from `count() - 1`.
fn heap_key_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            method_call(this_expr(), "isEmpty", Vec::new()),
            return_body(null_expr()),
            None,
        ),
        return_stmt(binary_expr(count_expr(heap_values_expr()), BinOp::Sub, int_expr(1))),
    ]
}

/// Advances the heap iterator by extracting the current best element.
fn heap_next_body() -> Vec<Stmt> {
    vec![if_stmt(
        not_expr(method_call(this_expr(), "isEmpty", Vec::new())),
        vec![expr_stmt(method_call(this_expr(), "extract", Vec::new()))],
        None,
    )]
}

/// Returns true while destructive iteration still has elements.
fn heap_valid_body() -> Vec<Stmt> {
    return_body(not_expr(method_call(this_expr(), "isEmpty", Vec::new())))
}

/// Copies the persistent physical heap without invoking the virtual comparator.
fn heap_debug_snapshot_statements() -> Vec<Stmt> {
    vec![
        typed_assign_stmt("debugHeap", array_type(), empty_array_expr()),
        assign_stmt("debugIndex", int_expr(0)),
        assign_stmt("debugLimit", count_expr(heap_values_expr())),
        while_stmt(
            binary_expr(var_expr("debugIndex"), BinOp::Lt, var_expr("debugLimit")),
            vec![
                assign_stmt("debugValue", heap_value_at(var_expr("debugIndex"))),
                array_push_stmt("debugHeap", var_expr("debugValue")),
                increment_stmt("debugIndex"),
            ],
        ),
    ]
}

/// Returns php-src's private debug fields and a non-mutating heap snapshot.
fn heap_debug_info_body() -> Vec<Stmt> {
    let mut body = heap_debug_snapshot_statements();
    body.push(return_stmt(expr(
        crate::parser::ast::ExprKind::ArrayLiteralAssoc(vec![
            (private_debug_key("SplHeap", "flags"), int_expr(0)),
            (
                private_debug_key("SplHeap", "isCorrupted"),
                bool_expr(false),
            ),
            (
                private_debug_key("SplHeap", "heap"),
                var_expr("debugHeap"),
            ),
        ]),
    )));
    body
}

/// Returns the root index of the persistent physical heap.
fn heap_best_index_body() -> Vec<Stmt> {
    return_body(int_expr(0))
}

/// Deletes the physical root and restores php-src's heap invariant by sift-down.
fn heap_remove_at_body() -> Vec<Stmt> {
    vec![
        typed_assign_stmt("heapValues", array_type(), heap_values_expr()),
        typed_assign_stmt("newValues", array_type(), empty_array_expr()),
        assign_stmt(
            "limit",
            binary_expr(count_expr(var_expr("heapValues")), BinOp::Sub, int_expr(1)),
        ),
        if_stmt(
            binary_expr(var_expr("limit"), BinOp::Gt, int_expr(0)),
            vec![
                assign_stmt(
                    "bottom",
                    array_access(var_expr("heapValues"), var_expr("limit")),
                ),
                assign_stmt("i", int_expr(0)),
                while_stmt(
                    binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
                    vec![
                        if_stmt(
                            binary_expr(var_expr("i"), BinOp::StrictEq, int_expr(0)),
                            vec![array_push_stmt("newValues", var_expr("bottom"))],
                            Some(vec![array_push_stmt(
                                "newValues",
                                array_access(var_expr("heapValues"), var_expr("i")),
                            )]),
                        ),
                        increment_stmt("i"),
                    ],
                ),
                assign_stmt("parent", int_expr(0)),
                assign_stmt("sifting", bool_expr(true)),
                while_stmt(
                    binary_expr(
                        binary_expr(
                            var_expr("parent"),
                            BinOp::Lt,
                            function_call("intdiv", vec![var_expr("limit"), int_expr(2)]),
                        ),
                        BinOp::And,
                        var_expr("sifting"),
                    ),
                    vec![
                        assign_stmt(
                            "child",
                            binary_expr(
                                binary_expr(var_expr("parent"), BinOp::Mul, int_expr(2)),
                                BinOp::Add,
                                int_expr(1),
                            ),
                        ),
                        if_stmt(
                            binary_expr(
                                binary_expr(
                                    binary_expr(var_expr("child"), BinOp::Add, int_expr(1)),
                                    BinOp::Lt,
                                    var_expr("limit"),
                                ),
                                BinOp::And,
                                binary_expr(
                                    method_call(
                                        this_expr(),
                                        "compare",
                                        vec![
                                            array_access(
                                                var_expr("newValues"),
                                                binary_expr(
                                                    var_expr("child"),
                                                    BinOp::Add,
                                                    int_expr(1),
                                                ),
                                            ),
                                            array_access(
                                                var_expr("newValues"),
                                                var_expr("child"),
                                            ),
                                        ],
                                    ),
                                    BinOp::Gt,
                                    int_expr(0),
                                ),
                            ),
                            vec![increment_stmt("child")],
                            None,
                        ),
                        if_stmt(
                            binary_expr(
                                method_call(
                                    this_expr(),
                                    "compare",
                                    vec![
                                        var_expr("bottom"),
                                        array_access(
                                            var_expr("newValues"),
                                            var_expr("child"),
                                        ),
                                    ],
                                ),
                                BinOp::Lt,
                                int_expr(0),
                            ),
                            vec![
                                array_assign_stmt(
                                    "newValues",
                                    var_expr("parent"),
                                    array_access(
                                        var_expr("newValues"),
                                        var_expr("child"),
                                    ),
                                ),
                                assign_stmt("parent", var_expr("child")),
                            ],
                            Some(vec![assign_stmt("sifting", bool_expr(false))]),
                        ),
                    ],
                ),
                array_assign_stmt("newValues", var_expr("parent"), var_expr("bottom")),
            ],
            None,
        ),
        property_assign_stmt(this_expr(), "values", var_expr("newValues")),
    ]
}

/// Computes the parent slot for a positive binary-heap index.
fn heap_parent_index_expr(index: Expr) -> Expr {
    function_call(
        "intdiv",
        vec![binary_expr(index, BinOp::Sub, int_expr(1)), int_expr(2)],
    )
}

/// Returns `$this->priorities`.
fn priority_priorities_expr() -> Expr {
    property_access(this_expr(), "priorities")
}

/// Returns `$this->extractFlags`.
fn priority_flags_expr() -> Expr {
    property_access(this_expr(), "extractFlags")
}

/// Returns `$this->priorities[$index]`.
fn priority_at(index: Expr) -> Expr {
    array_access(priority_priorities_expr(), index)
}

/// Returns `$this->__elephcBestIndex()`.
fn priority_best_index_expr() -> Expr {
    method_call(this_expr(), "__elephcBestIndex", Vec::new())
}

/// Initializes priority queue storage and default extraction mode.
fn priority_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "values", empty_array_expr()),
        property_assign_stmt(this_expr(), "priorities", empty_array_expr()),
        property_assign_stmt(this_expr(), "extractFlags", int_expr(1)),
    ]
}

/// Inserts a data/priority pair into the persistent physical priority heap.
fn priority_insert_body() -> Vec<Stmt> {
    vec![
        typed_assign_stmt("heapValues", array_type(), heap_values_expr()),
        typed_assign_stmt("heapPriorities", array_type(), priority_priorities_expr()),
        assign_stmt("position", count_expr(var_expr("heapValues"))),
        array_push_stmt("heapValues", var_expr("value")),
        array_push_stmt("heapPriorities", var_expr("priority")),
        assign_stmt("sifting", bool_expr(true)),
        while_stmt(
            binary_expr(
                binary_expr(var_expr("position"), BinOp::Gt, int_expr(0)),
                BinOp::And,
                var_expr("sifting"),
            ),
            vec![
                assign_stmt("parent", heap_parent_index_expr(var_expr("position"))),
                if_stmt(
                    binary_expr(
                        method_call(
                            this_expr(),
                            "compare",
                            vec![
                                array_access(
                                    var_expr("heapPriorities"),
                                    var_expr("parent"),
                                ),
                                var_expr("priority"),
                            ],
                        ),
                        BinOp::Lt,
                        int_expr(0),
                    ),
                    vec![
                        array_assign_stmt(
                            "heapValues",
                            var_expr("position"),
                            array_access(var_expr("heapValues"), var_expr("parent")),
                        ),
                        array_assign_stmt(
                            "heapPriorities",
                            var_expr("position"),
                            array_access(var_expr("heapPriorities"), var_expr("parent")),
                        ),
                        assign_stmt("position", var_expr("parent")),
                    ],
                    Some(vec![assign_stmt("sifting", bool_expr(false))]),
                ),
            ],
        ),
        array_assign_stmt("heapValues", var_expr("position"), var_expr("value")),
        array_assign_stmt(
            "heapPriorities",
            var_expr("position"),
            var_expr("priority"),
        ),
        property_assign_stmt(this_expr(), "values", var_expr("heapValues")),
        property_assign_stmt(this_expr(), "priorities", var_expr("heapPriorities")),
        return_stmt(bool_expr(true)),
    ]
}

/// Stores the selected extraction flags.
fn priority_set_extract_flags_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "extractFlags", var_expr("flags"))]
}

/// Returns the number of queued data values.
fn priority_count_body() -> Vec<Stmt> {
    return_body(count_expr(heap_values_expr()))
}

/// Returns whether the priority queue is empty.
fn priority_is_empty_body() -> Vec<Stmt> {
    return_body(binary_expr(count_expr(heap_values_expr()), BinOp::StrictEq, int_expr(0)))
}

/// Returns the top data value without removing it.
fn priority_top_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            method_call(this_expr(), "isEmpty", Vec::new()),
            vec![throw_stmt(new_object_expr(
                "RuntimeException",
                vec![string_expr("Can't peek at an empty heap")],
            ))],
            None,
        ),
        return_stmt(heap_value_at(priority_best_index_expr())),
    ]
}

/// Removes the top pair and returns data, priority, or both depending on extract flags.
fn priority_extract_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            method_call(this_expr(), "isEmpty", Vec::new()),
            vec![throw_stmt(new_object_expr(
                "RuntimeException",
                vec![string_expr("Can't extract from an empty heap")],
            ))],
            None,
        ),
        assign_stmt("best", priority_best_index_expr()),
        assign_stmt("out", method_call(this_expr(), "__elephcOutputAt", vec![var_expr("best")])),
        expr_stmt(method_call(this_expr(), "__elephcRemoveAt", vec![var_expr("best")])),
        return_stmt(var_expr("out")),
    ]
}

/// Returns the current iterator output without removing it, or null when empty.
fn priority_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            method_call(this_expr(), "isEmpty", Vec::new()),
            return_body(null_expr()),
            None,
        ),
        return_stmt(method_call(this_expr(), "__elephcOutputAt", vec![priority_best_index_expr()])),
    ]
}

/// Returns PHP's destructive priority-queue iterator key, counting down from `count() - 1`.
fn priority_key_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            method_call(this_expr(), "isEmpty", Vec::new()),
            return_body(null_expr()),
            None,
        ),
        return_stmt(binary_expr(count_expr(heap_values_expr()), BinOp::Sub, int_expr(1))),
    ]
}

/// Advances the iterator by extracting the current top pair.
fn priority_next_body() -> Vec<Stmt> {
    vec![if_stmt(
        not_expr(method_call(this_expr(), "isEmpty", Vec::new())),
        vec![expr_stmt(method_call(this_expr(), "extract", Vec::new()))],
        None,
    )]
}

/// Returns true while destructive priority-queue iteration still has pairs.
fn priority_valid_body() -> Vec<Stmt> {
    return_body(not_expr(method_call(this_expr(), "isEmpty", Vec::new())))
}

/// Copies physical heap slots into php-src's nested `{data, priority}` rows.
fn priority_debug_pair_snapshot_statements() -> Vec<Stmt> {
    vec![
        typed_assign_stmt("debugHeap", array_type(), empty_array_expr()),
        assign_stmt("debugIndex", int_expr(0)),
        assign_stmt("debugLimit", count_expr(heap_values_expr())),
        while_stmt(
            binary_expr(var_expr("debugIndex"), BinOp::Lt, var_expr("debugLimit")),
            vec![
                array_push_stmt(
                    "debugHeap",
                    expr(crate::parser::ast::ExprKind::ArrayLiteralAssoc(vec![
                        (
                            string_expr("data"),
                            heap_value_at(var_expr("debugIndex")),
                        ),
                        (
                            string_expr("priority"),
                            priority_at(var_expr("debugIndex")),
                        ),
                    ])),
                ),
                increment_stmt("debugIndex"),
            ],
        ),
    ]
}

/// Returns php-src's private queue fields and a non-mutating pair snapshot.
fn priority_debug_info_body() -> Vec<Stmt> {
    let mut body = priority_debug_pair_snapshot_statements();
    body.push(return_stmt(expr(
        crate::parser::ast::ExprKind::ArrayLiteralAssoc(vec![
            (
                private_debug_key("SplPriorityQueue", "flags"),
                priority_flags_expr(),
            ),
            (
                private_debug_key("SplPriorityQueue", "isCorrupted"),
                bool_expr(false),
            ),
            (
                private_debug_key("SplPriorityQueue", "heap"),
                var_expr("debugHeap"),
            ),
        ]),
    )));
    body
}

/// Builds php-src's NUL-mangled key for one private debug field.
fn private_debug_key(scope: &str, property: &str) -> Expr {
    string_expr(&format!("\0{scope}\0{property}"))
}

/// Returns the root index of the persistent physical priority heap.
fn priority_best_index_body() -> Vec<Stmt> {
    return_body(int_expr(0))
}

/// Builds the visible output for one queue index based on extraction flags.
fn priority_output_at_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(priority_flags_expr(), BinOp::StrictEq, int_expr(2)),
            return_body(priority_at(var_expr("index"))),
            None,
        ),
        if_stmt(
            binary_expr(priority_flags_expr(), BinOp::StrictEq, int_expr(3)),
            return_body(expr(crate::parser::ast::ExprKind::ArrayLiteralAssoc(vec![
                (string_expr("data"), heap_value_at(var_expr("index"))),
                (string_expr("priority"), priority_at(var_expr("index"))),
            ]))),
            None,
        ),
        return_stmt(heap_value_at(var_expr("index"))),
    ]
}

/// Deletes the physical root pair and restores php-src's priority-heap invariant.
fn priority_remove_at_body() -> Vec<Stmt> {
    vec![
        typed_assign_stmt("heapValues", array_type(), heap_values_expr()),
        typed_assign_stmt("heapPriorities", array_type(), priority_priorities_expr()),
        typed_assign_stmt("newValues", array_type(), empty_array_expr()),
        typed_assign_stmt("newPriorities", array_type(), empty_array_expr()),
        assign_stmt(
            "limit",
            binary_expr(count_expr(var_expr("heapValues")), BinOp::Sub, int_expr(1)),
        ),
        if_stmt(
            binary_expr(var_expr("limit"), BinOp::Gt, int_expr(0)),
            vec![
                assign_stmt(
                    "bottomValue",
                    array_access(var_expr("heapValues"), var_expr("limit")),
                ),
                assign_stmt(
                    "bottomPriority",
                    array_access(var_expr("heapPriorities"), var_expr("limit")),
                ),
                assign_stmt("i", int_expr(0)),
                while_stmt(
                    binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
                    vec![
                        if_stmt(
                            binary_expr(var_expr("i"), BinOp::StrictEq, int_expr(0)),
                            vec![
                                array_push_stmt("newValues", var_expr("bottomValue")),
                                array_push_stmt("newPriorities", var_expr("bottomPriority")),
                            ],
                            Some(vec![
                                array_push_stmt(
                                    "newValues",
                                    array_access(var_expr("heapValues"), var_expr("i")),
                                ),
                                array_push_stmt(
                                    "newPriorities",
                                    array_access(var_expr("heapPriorities"), var_expr("i")),
                                ),
                            ]),
                        ),
                        increment_stmt("i"),
                    ],
                ),
                assign_stmt("parent", int_expr(0)),
                assign_stmt("sifting", bool_expr(true)),
                while_stmt(
                    binary_expr(
                        binary_expr(
                            var_expr("parent"),
                            BinOp::Lt,
                            function_call("intdiv", vec![var_expr("limit"), int_expr(2)]),
                        ),
                        BinOp::And,
                        var_expr("sifting"),
                    ),
                    vec![
                        assign_stmt(
                            "child",
                            binary_expr(
                                binary_expr(var_expr("parent"), BinOp::Mul, int_expr(2)),
                                BinOp::Add,
                                int_expr(1),
                            ),
                        ),
                        if_stmt(
                            binary_expr(
                                binary_expr(
                                    binary_expr(var_expr("child"), BinOp::Add, int_expr(1)),
                                    BinOp::Lt,
                                    var_expr("limit"),
                                ),
                                BinOp::And,
                                binary_expr(
                                    method_call(
                                        this_expr(),
                                        "compare",
                                        vec![
                                            array_access(
                                                var_expr("newPriorities"),
                                                binary_expr(
                                                    var_expr("child"),
                                                    BinOp::Add,
                                                    int_expr(1),
                                                ),
                                            ),
                                            array_access(
                                                var_expr("newPriorities"),
                                                var_expr("child"),
                                            ),
                                        ],
                                    ),
                                    BinOp::Gt,
                                    int_expr(0),
                                ),
                            ),
                            vec![increment_stmt("child")],
                            None,
                        ),
                        if_stmt(
                            binary_expr(
                                method_call(
                                    this_expr(),
                                    "compare",
                                    vec![
                                        var_expr("bottomPriority"),
                                        array_access(
                                            var_expr("newPriorities"),
                                            var_expr("child"),
                                        ),
                                    ],
                                ),
                                BinOp::Lt,
                                int_expr(0),
                            ),
                            vec![
                                array_assign_stmt(
                                    "newValues",
                                    var_expr("parent"),
                                    array_access(var_expr("newValues"), var_expr("child")),
                                ),
                                array_assign_stmt(
                                    "newPriorities",
                                    var_expr("parent"),
                                    array_access(
                                        var_expr("newPriorities"),
                                        var_expr("child"),
                                    ),
                                ),
                                assign_stmt("parent", var_expr("child")),
                            ],
                            Some(vec![assign_stmt("sifting", bool_expr(false))]),
                        ),
                    ],
                ),
                array_assign_stmt("newValues", var_expr("parent"), var_expr("bottomValue")),
                array_assign_stmt(
                    "newPriorities",
                    var_expr("parent"),
                    var_expr("bottomPriority"),
                ),
            ],
            None,
        ),
        property_assign_stmt(this_expr(), "values", var_expr("newValues")),
        property_assign_stmt(this_expr(), "priorities", var_expr("newPriorities")),
    ]
}
