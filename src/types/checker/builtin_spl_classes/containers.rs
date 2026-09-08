//! Purpose:
//! Injects SPL phase-4 container class metadata and the internal SplFixedArray iterator helper.
//! Keeps runtime-backed container declarations separate from phase-5 iterator decorators.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Runtime-backed methods stay bodyless so codegen intrinsics own their behavior.
//! - Small serialization/debug helpers are synthetic PHP-like method bodies.

use std::collections::HashMap;

use crate::parser::ast::{
    BinOp, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, Stmt, TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;

use super::common::{
    array_access, array_push_stmt, array_type, assign_stmt, binary_expr, bool_expr, class_const,
    class_method, count_expr, expr, expr_stmt, foreach_stmt, function_call, if_stmt,
    increment_stmt, int_expr, method, method_call, method_with_body, mixed_type, named_type,
    new_object_expr, not_expr, null_expr, param, param_default, property_access,
    property_assign_stmt, return_body, return_stmt, storage_property, storage_property_default,
    string_expr, this_expr, var_expr, while_stmt,
};

/// Inserts classes into the supplied builtin metadata registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "SplDoublyLinkedList".to_string(),
        FlattenedClass {
            name: "SplDoublyLinkedList".to_string(),
            span: crate::span::Span::dummy(),
            extends: None,
            implements: vec![
                "Iterator".to_string(),
                "Countable".to_string(),
                "ArrayAccess".to_string(),
            ],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_doubly_linked_list_methods(),
            attributes: Vec::new(),
            constants: spl_doubly_linked_list_constants(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "SplStack".to_string(),
        FlattenedClass {
            name: "SplStack".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("SplDoublyLinkedList".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "SplQueue".to_string(),
        FlattenedClass {
            name: "SplQueue".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("SplDoublyLinkedList".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: vec![
                method("enqueue", vec![param("value", mixed_type())], Some(TypeExpr::Void)),
                method("dequeue", Vec::new(), Some(mixed_type())),
            ],
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "SplFixedArray".to_string(),
        FlattenedClass {
            name: "SplFixedArray".to_string(),
            span: crate::span::Span::dummy(),
            extends: None,
            implements: vec![
                "IteratorAggregate".to_string(),
                "ArrayAccess".to_string(),
                "Countable".to_string(),
                "JsonSerializable".to_string(),
            ],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_fixed_array_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    insert_internal_iterator(class_map);
}

/// Inserts only the compiler-owned iterator used by DatePeriod and aggregate SPL classes.
pub(super) fn insert_internal_iterator(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.entry("InternalIterator".to_string()).or_insert_with(|| FlattenedClass {
        name: "InternalIterator".to_string(),
        span: crate::span::Span::dummy(),
        extends: None,
        implements: vec!["Iterator".to_string()],
        is_abstract: false,
        is_final: true,
        is_readonly_class: false,
        properties: internal_iterator_properties(),
        methods: spl_internal_iterator_methods(),
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    });
}

/// Builds the method list for SPL internal iterator.
fn spl_internal_iterator_methods() -> Vec<ClassMethod> {
    let mut construct = method_with_body(
        "__construct",
        vec![
            param("owner", mixed_type()),
            param_default(
                "onCurrent",
                TypeExpr::Nullable(Box::new(named_type("Closure"))),
                null_expr(),
            ),
            param_default(
                "onValid",
                TypeExpr::Nullable(Box::new(named_type("Closure"))),
                null_expr(),
            ),
            param_default(
                "onNext",
                TypeExpr::Nullable(Box::new(named_type("Closure"))),
                null_expr(),
            ),
            param_default(
                "onRewind",
                TypeExpr::Nullable(Box::new(named_type("Closure"))),
                null_expr(),
            ),
        ],
        Some(TypeExpr::Void),
        internal_iterator_construct_body(),
    );
    construct.visibility = Visibility::Private;

    vec![
        construct,
        method_with_body("current", Vec::new(), Some(mixed_type()), internal_iterator_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), internal_iterator_key_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), internal_iterator_next_body()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), internal_iterator_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), internal_iterator_valid_body()),
    ]
}

/// Builds the property list for internal iterator.
fn internal_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("owner", mixed_type()),
        storage_property("position", TypeExpr::Int),
        storage_property_default("rewindCalled", TypeExpr::Bool, bool_expr(false)),
        storage_property_default(
            "onCurrent",
            TypeExpr::Nullable(Box::new(named_type("Closure"))),
            null_expr(),
        ),
        storage_property_default(
            "onValid",
            TypeExpr::Nullable(Box::new(named_type("Closure"))),
            null_expr(),
        ),
        storage_property_default(
            "onNext",
            TypeExpr::Nullable(Box::new(named_type("Closure"))),
            null_expr(),
        ),
        storage_property_default(
            "onRewind",
            TypeExpr::Nullable(Box::new(named_type("Closure"))),
            null_expr(),
        ),
    ]
}

/// Builds the method list for SPL doubly linked list.
fn spl_doubly_linked_list_methods() -> Vec<ClassMethod> {
    vec![
        method(
            "add",
            vec![param("index", TypeExpr::Int), param("value", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method("pop", Vec::new(), Some(mixed_type())),
        method("shift", Vec::new(), Some(mixed_type())),
        method("push", vec![param("value", mixed_type())], Some(TypeExpr::Void)),
        method(
            "unshift",
            vec![param("value", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method("top", Vec::new(), Some(mixed_type())),
        method("bottom", Vec::new(), Some(mixed_type())),
        method("count", Vec::new(), Some(TypeExpr::Int)),
        method("isEmpty", Vec::new(), Some(TypeExpr::Bool)),
        method(
            "setIteratorMode",
            vec![param("mode", TypeExpr::Int)],
            Some(TypeExpr::Void),
        ),
        method("getIteratorMode", Vec::new(), Some(TypeExpr::Int)),
        method(
            "offsetExists",
            vec![param("index", mixed_type())],
            Some(TypeExpr::Bool),
        ),
        method(
            "offsetGet",
            vec![param("index", mixed_type())],
            Some(mixed_type()),
        ),
        method(
            "offsetSet",
            vec![param("index", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method(
            "offsetUnset",
            vec![param("index", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method("rewind", Vec::new(), Some(TypeExpr::Void)),
        method("current", Vec::new(), Some(mixed_type())),
        method("key", Vec::new(), Some(mixed_type())),
        method("prev", Vec::new(), Some(TypeExpr::Void)),
        method("next", Vec::new(), Some(TypeExpr::Void)),
        method("valid", Vec::new(), Some(TypeExpr::Bool)),
        method("serialize", Vec::new(), Some(TypeExpr::Str)),
        method(
            "unserialize",
            vec![param("data", TypeExpr::Str)],
            Some(TypeExpr::Void),
        ),
        method_with_body(
            "__serialize",
            Vec::new(),
            Some(array_type()),
            dll_serialize_array_body(),
        ),
        method_with_body(
            "__unserialize",
            vec![param("data", array_type())],
            Some(TypeExpr::Void),
            dll_unserialize_body(),
        ),
        method_with_body(
            "__debugInfo",
            Vec::new(),
            Some(array_type()),
            dll_debug_info_body(),
        ),
    ]
}

/// Builds the method list for SPL fixed array.
fn spl_fixed_array_methods() -> Vec<ClassMethod> {
    vec![
        method(
            "__construct",
            vec![param_default("size", TypeExpr::Int, int_expr(0))],
            Some(TypeExpr::Void),
        ),
        method_with_body("__wakeup", Vec::new(), Some(TypeExpr::Void), Vec::new()),
        class_method(
            "fromArray",
            true,
            vec![
                param("array", array_type()),
                param_default("preserveKeys", TypeExpr::Bool, bool_expr(true)),
            ],
            Some(named_type("SplFixedArray")),
        ),
        method_with_body(
            "__serialize",
            Vec::new(),
            Some(array_type()),
            vec![return_stmt(method_call(this_expr(), "toArray", Vec::new()))],
        ),
        method("__unserialize", vec![param("data", array_type())], Some(TypeExpr::Void)),
        method("count", Vec::new(), Some(TypeExpr::Int)),
        method_with_body(
            "getIterator",
            Vec::new(),
            Some(named_type("Iterator")),
            fixed_array_get_iterator_body(),
        ),
        method("toArray", Vec::new(), Some(array_type())),
        method("getSize", Vec::new(), Some(TypeExpr::Int)),
        method(
            "setSize",
            vec![param("size", TypeExpr::Int)],
            Some(TypeExpr::Void),
        ),
        method(
            "offsetExists",
            vec![param("index", mixed_type())],
            Some(TypeExpr::Bool),
        ),
        method(
            "offsetGet",
            vec![param("index", mixed_type())],
            Some(mixed_type()),
        ),
        method(
            "offsetSet",
            vec![param("index", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method(
            "offsetUnset",
            vec![param("index", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method("jsonSerialize", Vec::new(), Some(array_type())),
    ]
}

/// Provides the SPL doubly linked list constants helper used by the containers module.
fn spl_doubly_linked_list_constants() -> Vec<ClassConst> {
    vec![
        class_const("IT_MODE_LIFO", 2),
        class_const("IT_MODE_FIFO", 0),
        class_const("IT_MODE_DELETE", 1),
        class_const("IT_MODE_KEEP", 0),
    ]
}

/// Builds the AST expression for internal iterator owner.
fn internal_iterator_owner_expr() -> Expr {
    property_access(this_expr(), "owner")
}

/// Builds the AST expression for internal iterator position.
fn internal_iterator_position_expr() -> Expr {
    property_access(this_expr(), "position")
}

/// Builds the synthetic method body for internal iterator construct.
fn internal_iterator_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "owner", var_expr("owner")),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        property_assign_stmt(this_expr(), "rewindCalled", bool_expr(false)),
        property_assign_stmt(this_expr(), "onCurrent", var_expr("onCurrent")),
        property_assign_stmt(this_expr(), "onValid", var_expr("onValid")),
        property_assign_stmt(this_expr(), "onNext", var_expr("onNext")),
        property_assign_stmt(this_expr(), "onRewind", var_expr("onRewind")),
    ]
}

/// Builds the lazy rewind guard shared by every observable iterator operation.
fn internal_iterator_ensure_rewound_body() -> Vec<Stmt> {
    vec![if_stmt(
        binary_expr(
            property_access(this_expr(), "rewindCalled"),
            BinOp::StrictEq,
            bool_expr(false),
        ),
        vec![expr_stmt(method_call(this_expr(), "rewind", Vec::new()))],
        None,
    )]
}

/// Builds the inline lazy-rewind statement used by observable iterator methods.
fn internal_iterator_ensure_rewound_stmt() -> Stmt {
    internal_iterator_ensure_rewound_body()
        .into_iter()
        .next()
        .expect("lazy rewind body always contains one guard")
}

/// Builds the synthetic method body for the internal iterator key.
fn internal_iterator_key_body() -> Vec<Stmt> {
    vec![
        internal_iterator_ensure_rewound_stmt(),
        return_stmt(internal_iterator_position_expr()),
    ]
}

/// Builds the synthetic method body for internal iterator current.
fn internal_iterator_current_body() -> Vec<Stmt> {
    vec![
        internal_iterator_ensure_rewound_stmt(),
        if_stmt(
            binary_expr(
                property_access(this_expr(), "onValid"),
                BinOp::StrictNotEq,
                null_expr(),
            ),
            vec![return_stmt(expr(ExprKind::ExprCall {
                callee: Box::new(property_access(this_expr(), "onCurrent")),
                args: Vec::new(),
            }))],
            None,
        ),
        if_stmt(
            binary_expr(
                property_access(this_expr(), "onCurrent"),
                BinOp::StrictNotEq,
                null_expr(),
            ),
            vec![
                assign_stmt(
                    "value",
                    array_access(
                        internal_iterator_owner_expr(),
                        internal_iterator_position_expr(),
                    ),
                ),
                return_stmt(expr(ExprKind::ExprCall {
                    callee: Box::new(property_access(this_expr(), "onCurrent")),
                    args: vec![var_expr("value")],
                })),
            ],
            None,
        ),
        return_stmt(method_call(
            internal_iterator_owner_expr(),
            "offsetGet",
            vec![internal_iterator_position_expr()],
        )),
    ]
}

/// Builds the synthetic method body for internal iterator next.
fn internal_iterator_next_body() -> Vec<Stmt> {
    vec![
        internal_iterator_ensure_rewound_stmt(),
        property_assign_stmt(
            this_expr(),
            "position",
            binary_expr(
                internal_iterator_position_expr(),
                BinOp::Add,
                int_expr(1),
            ),
        ),
        if_stmt(
            binary_expr(
                property_access(this_expr(), "onValid"),
                BinOp::StrictNotEq,
                null_expr(),
            ),
            vec![expr_stmt(expr(ExprKind::ExprCall {
                callee: Box::new(property_access(this_expr(), "onNext")),
                args: Vec::new(),
            }))],
            Some(vec![
        if_stmt(
            binary_expr(
                property_access(this_expr(), "onCurrent"),
                BinOp::StrictNotEq,
                null_expr(),
            ),
            vec![if_stmt(
                binary_expr(
                    internal_iterator_position_expr(),
                    BinOp::Lt,
                    count_expr(internal_iterator_owner_expr()),
                ),
                vec![expr_stmt(method_call(this_expr(), "current", Vec::new()))],
                Some(vec![expr_stmt(expr(ExprKind::ExprCall {
                    callee: Box::new(property_access(this_expr(), "onCurrent")),
                    args: vec![null_expr()],
                }))]),
            )],
            None,
        ),
            ]),
        ),
    ]
}

/// Builds the synthetic method body for internal iterator rewind.
fn internal_iterator_rewind_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        property_assign_stmt(this_expr(), "rewindCalled", bool_expr(true)),
        if_stmt(
            binary_expr(
                property_access(this_expr(), "onRewind"),
                BinOp::StrictNotEq,
                null_expr(),
            ),
            vec![expr_stmt(expr(ExprKind::ExprCall {
                callee: Box::new(property_access(this_expr(), "onRewind")),
                args: Vec::new(),
            }))],
            Some(vec![
        if_stmt(
            binary_expr(
                binary_expr(
                    property_access(this_expr(), "onCurrent"),
                    BinOp::StrictNotEq,
                    null_expr(),
                ),
                BinOp::And,
                binary_expr(
                    count_expr(internal_iterator_owner_expr()),
                    BinOp::Gt,
                    int_expr(0),
                ),
            ),
            vec![expr_stmt(method_call(this_expr(), "current", Vec::new()))],
            None,
        ),
            ]),
        ),
    ]
}

/// Builds the synthetic method body for internal iterator valid.
fn internal_iterator_valid_body() -> Vec<Stmt> {
    vec![
        internal_iterator_ensure_rewound_stmt(),
        if_stmt(
            binary_expr(
                property_access(this_expr(), "onValid"),
                BinOp::StrictNotEq,
                null_expr(),
            ),
            vec![return_stmt(expr(ExprKind::ExprCall {
                callee: Box::new(property_access(this_expr(), "onValid")),
                args: vec![internal_iterator_position_expr()],
            }))],
            None,
        ),
        return_stmt(binary_expr(
            internal_iterator_position_expr(),
            BinOp::Lt,
            function_call("count", vec![internal_iterator_owner_expr()]),
        )),
    ]
}


/// Builds the synthetic method body for fixed array get iterator.
fn fixed_array_get_iterator_body() -> Vec<Stmt> {
    return_body(new_object_expr(
        "InternalIterator",
        vec![this_expr()],
    ))
}

/// Provides the Dll items snapshot prelude helper used by the containers module.
fn dll_items_snapshot_prelude() -> Vec<Stmt> {
    vec![
        assign_stmt("items", expr(ExprKind::ArrayLiteral(Vec::new()))),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", method_call(this_expr(), "count", Vec::new())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                array_push_stmt("items", method_call(this_expr(), "offsetGet", vec![var_expr("i")])),
                increment_stmt("i"),
            ],
        ),
    ]
}

/// Builds the synthetic method body for dll serialize array.
fn dll_serialize_array_body() -> Vec<Stmt> {
    let mut body = dll_items_snapshot_prelude();
    body.push(return_stmt(expr(ExprKind::ArrayLiteral(vec![
        method_call(this_expr(), "getIteratorMode", Vec::new()),
        var_expr("items"),
        expr(ExprKind::ArrayLiteral(Vec::new())),
    ]))));
    body
}

/// Builds the synthetic method body for dll debug info.
fn dll_debug_info_body() -> Vec<Stmt> {
    let mut body = vec![
        assign_stmt("mode", method_call(this_expr(), "getIteratorMode", Vec::new())),
        expr_stmt(method_call(this_expr(), "setIteratorMode", vec![int_expr(0)])),
    ];
    body.extend(dll_items_snapshot_prelude());
    body.push(expr_stmt(method_call(
        this_expr(),
        "setIteratorMode",
        vec![var_expr("mode")],
    )));
    body.push(return_stmt(expr(ExprKind::ArrayLiteralAssoc(vec![
        (
            string_expr("\0SplDoublyLinkedList\0flags"),
            var_expr("mode"),
        ),
        (
            string_expr("\0SplDoublyLinkedList\0dllist"),
            var_expr("items"),
        ),
    ]))));
    body
}

/// Builds the synthetic method body for dll unserialize.
fn dll_unserialize_body() -> Vec<Stmt> {
    vec![
        expr_stmt(method_call(
            this_expr(),
            "setIteratorMode",
            vec![array_access(var_expr("data"), int_expr(0))],
        )),
        while_stmt(
            not_expr(method_call(this_expr(), "isEmpty", Vec::new())),
            vec![expr_stmt(method_call(this_expr(), "pop", Vec::new()))],
        ),
        foreach_stmt(
            array_access(var_expr("data"), int_expr(1)),
            None,
            "value",
            vec![expr_stmt(method_call(this_expr(), "push", vec![var_expr("value")]))],
        ),
    ]
}
