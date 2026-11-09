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

use crate::fast_hash::FastMap as HashMap;

use crate::parser::ast::{BinOp, CastType, ClassMethod, ClassProperty, Expr, Stmt, TypeExpr};
use crate::types::traits::FlattenedClass;

use super::common::*;

/// Inserts classes into the supplied builtin metadata registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "EmptyIterator".to_string(),
        FlattenedClass {
            name: "EmptyIterator".to_string(),
            span: crate::span::Span::dummy(),
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
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "ArrayIterator".to_string(),
        FlattenedClass {
            name: "ArrayIterator".to_string(),
            span: crate::span::Span::dummy(),
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
            constants: array_flag_constants(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "ArrayObject".to_string(),
        FlattenedClass {
            name: "ArrayObject".to_string(),
            span: crate::span::Span::dummy(),
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
            constants: array_flag_constants(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
}

/// php's `ArrayObject`/`ArrayIterator` flag constants (Laravel's `AsArrayObject` cast passes
/// `ArrayObject::ARRAY_AS_PROPS`).
fn array_flag_constants() -> Vec<crate::parser::ast::ClassConst> {
    vec![class_const("STD_PROP_LIST", 1), class_const("ARRAY_AS_PROPS", 2)]
}

/// Builds the method list for SPL empty iterator.
fn spl_empty_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body("current", Vec::new(), Some(mixed_type()), null_return_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), null_return_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), Vec::new()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), Vec::new()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), return_body(bool_expr(false))),
    ]
}

/// Builds the property list for array iterator.
fn array_iterator_properties() -> Vec<ClassProperty> {
    vec![
        protected_storage_property("keys", array_type()),
        protected_storage_property("values", array_type()),
        protected_storage_property("position", TypeExpr::Int),
        protected_storage_property("flags", TypeExpr::Int),
    ]
}

/// Builds the property list for array object.
///
/// One native array, as php's `ArrayObject` wraps one: keys, order, the next integer key after an
/// `unset()` and every access are the array's own. The previous split into parallel `keys` and
/// `values` lists answered `$o[] = 'n'` on `['a' => 1]` with key `1` (php: `0`), searched linearly
/// on every offset access, and leaked its copies.
fn array_object_properties() -> Vec<ClassProperty> {
    vec![
        // php's own `array`, packed or hash: the synthetic `array_type()` is a list ABI that
        // would read every string key as `0`.
        storage_property("storage", named_type("array")),
        storage_property("flags", TypeExpr::Int),
    ]
}

/// Builds the method list for SPL array iterator.
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
        method_with_body("getFlags", Vec::new(), Some(TypeExpr::Int), return_body(property_access(this_expr(), "flags"))),
        method_with_body(
            "setFlags",
            vec![param("flags", TypeExpr::Int)],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "flags", var_expr("flags"))],
        ),
        // php's `[flags, storage, member properties, null]`, the shape Symfony's var-exporter
        // `Hydrator` rebuilds an ArrayIterator from.
        method_with_body("__serialize", Vec::new(), Some(named_type("array")), array_iterator_serialize_body()),
        method_with_body(
            "__unserialize",
            vec![param("data", named_type("array"))],
            Some(TypeExpr::Void),
            array_iterator_unserialize_body(),
        ),
    ]
}

/// Builds `ArrayIterator::__serialize()`: the `getArrayCopy()` walk, then php's four-slot array.
fn array_iterator_serialize_body() -> Vec<Stmt> {
    let mut body = array_copy_body();
    body.pop();
    body.push(return_stmt(expr(crate::parser::ast::ExprKind::ArrayLiteral(vec![
        property_access(this_expr(), "flags"),
        var_expr("out"),
        empty_array_expr(),
        null_expr(),
    ]))));
    body
}

/// Builds `ArrayIterator::__unserialize($data)`: flags from slot 0 and the storage from slot 1,
/// split into the parallel key/value lists the constructor builds, with the cursor rewound.
fn array_iterator_unserialize_body() -> Vec<Stmt> {
    vec![
        assign_stmt(
            "array",
            cast_expr(CastType::Array, null_coalesce_expr(array_access(var_expr("data"), int_expr(1)), empty_array_expr())),
        ),
        property_assign_stmt(this_expr(), "keys", function_call("array_keys", vec![var_expr("array")])),
        property_assign_stmt(this_expr(), "values", function_call("array_values", vec![var_expr("array")])),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        property_assign_stmt(
            this_expr(),
            "flags",
            cast_expr(CastType::Int, null_coalesce_expr(array_access(var_expr("data"), int_expr(0)), int_expr(0))),
        ),
    ]
}

/// Builds the method list for SPL array object.
fn spl_array_object_methods() -> Vec<ClassMethod> {
    let storage = || property_access(this_expr(), "storage");
    let element = || array_access(storage(), var_expr("offset"));
    vec![
        method_with_body(
            "__construct",
            vec![
                param_default("array", array_type(), empty_array_expr()),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(TypeExpr::Void),
            vec![
                property_assign_stmt(this_expr(), "storage", cast_expr(CastType::Array, var_expr("array"))),
                property_assign_stmt(this_expr(), "flags", var_expr("flags")),
            ],
        ),
        method_with_body(
            "getIterator",
            Vec::new(),
            Some(named_type("ArrayIterator")),
            return_body(new_object_expr("ArrayIterator", vec![storage()])),
        ),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), return_body(count_expr(storage()))),
        method_with_body(
            "offsetExists",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Bool),
            return_body(function_call("array_key_exists", vec![var_expr("offset"), storage()])),
        ),
        method_with_body(
            "offsetGet",
            vec![param("offset", mixed_type())],
            Some(mixed_type()),
            return_body(element()),
        ),
        method_with_body(
            "offsetSet",
            vec![param("offset", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
            vec![if_stmt(
                binary_expr(var_expr("offset"), BinOp::StrictEq, null_expr()),
                vec![property_array_push_stmt(this_expr(), "storage", var_expr("value"))],
                Some(vec![property_array_assign_stmt(
                    this_expr(),
                    "storage",
                    var_expr("offset"),
                    var_expr("value"),
                )]),
            )],
        ),
        method_with_body(
            "offsetUnset",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Void),
            vec![expr_stmt(function_call("unset", vec![element()]))],
        ),
        method_with_body(
            "append",
            vec![param("value", mixed_type())],
            Some(TypeExpr::Void),
            vec![property_array_push_stmt(this_expr(), "storage", var_expr("value"))],
        ),
        method_with_body("getArrayCopy", Vec::new(), Some(array_type()), return_body(storage())),
        // `$this->tracedRequests->exchangeArray([])` (Symfony's `TraceableHttpClient::reset()`):
        // the storage is replaced and the previous contents are returned.
        method_with_body(
            "exchangeArray",
            vec![param("array", named_type("array"))],
            Some(named_type("array")),
            vec![
                assign_stmt("previous", storage()),
                property_assign_stmt(this_expr(), "storage", cast_expr(CastType::Array, var_expr("array"))),
                return_stmt(var_expr("previous")),
            ],
        ),
        method_with_body("getFlags", Vec::new(), Some(TypeExpr::Int), return_body(property_access(this_expr(), "flags"))),
        method_with_body(
            "setFlags",
            vec![param("flags", TypeExpr::Int)],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "flags", var_expr("flags"))],
        ),
        // php's `[flags, storage, member properties, iterator class]` (Symfony's var-exporter
        // `Hydrator` rebuilds an ArrayObject through `__unserialize()`).
        method_with_body(
            "__serialize",
            Vec::new(),
            Some(named_type("array")),
            return_body(expr(crate::parser::ast::ExprKind::ArrayLiteral(vec![
                property_access(this_expr(), "flags"),
                storage(),
                empty_array_expr(),
                null_expr(),
            ]))),
        ),
        method_with_body(
            "__unserialize",
            vec![param("data", named_type("array"))],
            Some(TypeExpr::Void),
            vec![
                property_assign_stmt(
                    this_expr(),
                    "flags",
                    cast_expr(CastType::Int, null_coalesce_expr(array_access(var_expr("data"), int_expr(0)), int_expr(0))),
                ),
                property_assign_stmt(
                    this_expr(),
                    "storage",
                    cast_expr(CastType::Array, null_coalesce_expr(array_access(var_expr("data"), int_expr(1)), empty_array_expr())),
                ),
            ],
        ),
    ]
}

/// Builds the AST expression for keys.
fn keys_expr() -> Expr {
    property_access(this_expr(), "keys")
}

/// Builds the AST expression for values.
fn values_expr() -> Expr {
    property_access(this_expr(), "values")
}

/// Builds the AST expression for position.
fn position_expr() -> Expr {
    property_access(this_expr(), "position")
}

/// Provides the Key at helper used by the storage module.
fn key_at(index: Expr) -> Expr {
    array_access(keys_expr(), index)
}

/// Provides the Value at helper used by the storage module.
fn value_at(index: Expr) -> Expr {
    array_access(values_expr(), index)
}

/// Builds the synthetic method body for array iterator construct.
fn array_iterator_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "keys", function_call("array_keys", vec![var_expr("array")])),
        property_assign_stmt(
            this_expr(),
            "values",
            function_call("array_values", vec![var_expr("array")]),
        ),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
    ]
}

/// Builds the synthetic method body for array current.
fn array_current_body() -> Vec<Stmt> {
    return_body(value_at(position_expr()))
}

/// Builds the synthetic method body for array key.
fn array_key_body() -> Vec<Stmt> {
    return_body(key_at(position_expr()))
}

/// Builds the synthetic method body for array next.
fn array_next_body() -> Vec<Stmt> {
    vec![property_assign_stmt(
        this_expr(),
        "position",
        binary_expr(position_expr(), BinOp::Add, int_expr(1)),
    )]
}

/// Builds the synthetic method body for array rewind.
fn array_rewind_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "position", int_expr(0))]
}

/// Builds the synthetic method body for array valid.
fn array_valid_body() -> Vec<Stmt> {
    return_body(binary_expr(position_expr(), BinOp::Lt, count_expr(values_expr())))
}

/// Builds the synthetic method body for array count.
fn array_count_body() -> Vec<Stmt> {
    return_body(count_expr(values_expr()))
}

/// Builds the synthetic method body for array append.
fn array_append_body() -> Vec<Stmt> {
    vec![
        property_array_push_stmt(this_expr(), "keys", count_expr(keys_expr())),
        property_array_push_stmt(this_expr(), "values", var_expr("value")),
    ]
}

/// Builds the synthetic method body for array offset exists.
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

/// Builds the synthetic method body for array offset get.
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

/// Builds the synthetic method body for array offset set.
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

/// Builds the synthetic method body for array offset unset.
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

/// Builds the synthetic method body for array copy.
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

/// Provides the Array search prelude helper used by the storage module.
fn array_search_prelude() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(keys_expr())),
    ]
}
