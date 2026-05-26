//! Purpose:
//! Injects CachingIterator metadata and its synthetic cache-management method bodies.
//! Keeps cache flags, full-cache ArrayAccess behavior, and string conversion rules in one module.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Cache capture advances the inner iterator once and stores the current key/value snapshot.
//! - FULL_CACHE gates ArrayAccess and getCache/count behavior like PHP SPL.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, CastType, ClassConst, ClassMethod, ClassProperty, Expr, Stmt, TypeExpr};
use crate::types::traits::FlattenedClass;

use super::common::*;
use super::forwarding::{inner_call, inner_expr};

/// Inserts class into the supplied builtin metadata registry.
pub(super) fn insert_class(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "CachingIterator".to_string(),
        FlattenedClass {
            name: "CachingIterator".to_string(),
            extends: Some("IteratorIterator".to_string()),
            implements: vec![
                "ArrayAccess".to_string(),
                "Countable".to_string(),
                "Stringable".to_string(),
            ],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: caching_iterator_properties(),
            methods: spl_caching_iterator_methods(),
            attributes: Vec::new(),
            constants: caching_iterator_constants(),
            used_traits: Vec::new(),
        },
    );
}

/// Builds the property list for caching iterator.
fn caching_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("flags", TypeExpr::Int),
        storage_property("cache", array_type()),
        storage_property("currentKey", mixed_type()),
        storage_property("currentValue", mixed_type()),
        storage_property("currentValid", TypeExpr::Bool),
        storage_property("cachedHasNext", TypeExpr::Bool),
    ]
}

/// Builds the method list for SPL caching iterator.
fn spl_caching_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", named_type("Iterator")),
                param_default("flags", TypeExpr::Int, int_expr(1)),
            ],
            Some(TypeExpr::Void),
            caching_construct_body(),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), caching_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), return_body(caching_current_valid_expr())),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), caching_next_body()),
        method_with_body("current", Vec::new(), Some(mixed_type()), caching_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), caching_key_body()),
        method_with_body("hasNext", Vec::new(), Some(TypeExpr::Bool), return_body(caching_has_next_expr())),
        method_with_body("__toString", Vec::new(), Some(TypeExpr::Str), caching_to_string_body()),
        method_with_body("getFlags", Vec::new(), Some(TypeExpr::Int), return_body(caching_flags_expr())),
        method_with_body(
            "setFlags",
            vec![param("flags", TypeExpr::Int)],
            Some(TypeExpr::Void),
            caching_set_flags_body(),
        ),
        method_with_body(
            "offsetGet",
            vec![param("key", mixed_type())],
            Some(mixed_type()),
            caching_offset_get_body(),
        ),
        method_with_body(
            "offsetSet",
            vec![param("key", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
            caching_offset_set_body(),
        ),
        method_with_body(
            "offsetUnset",
            vec![param("key", mixed_type())],
            Some(TypeExpr::Void),
            caching_offset_unset_body(),
        ),
        method_with_body(
            "offsetExists",
            vec![param("key", mixed_type())],
            Some(TypeExpr::Bool),
            caching_offset_exists_body(),
        ),
        method_with_body("getCache", Vec::new(), Some(array_type()), caching_get_cache_body()),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), caching_count_body()),
        method_with_body(
            "__elephcCaptureCurrent",
            Vec::new(),
            Some(TypeExpr::Void),
            caching_capture_current_body(),
        ),
    ]
}

/// Provides the Caching iterator constants helper used by the caching module.
fn caching_iterator_constants() -> Vec<ClassConst> {
    vec![
        class_const("CALL_TOSTRING", 1),
        class_const("CATCH_GET_CHILD", 16),
        class_const("TOSTRING_USE_KEY", 2),
        class_const("TOSTRING_USE_CURRENT", 4),
        class_const("TOSTRING_USE_INNER", 8),
        class_const("FULL_CACHE", 256),
    ]
}

/// Builds the AST expression for caching flags.
fn caching_flags_expr() -> Expr {
    property_access(this_expr(), "flags")
}

/// Builds the AST expression for caching cache.
fn caching_cache_expr() -> Expr {
    property_access(this_expr(), "cache")
}

/// Builds the AST expression for caching current key.
fn caching_current_key_expr() -> Expr {
    property_access(this_expr(), "currentKey")
}

/// Builds the AST expression for caching current value.
fn caching_current_value_expr() -> Expr {
    property_access(this_expr(), "currentValue")
}

/// Builds the AST expression for caching current valid.
fn caching_current_valid_expr() -> Expr {
    property_access(this_expr(), "currentValid")
}

/// Builds the AST expression for caching has next.
fn caching_has_next_expr() -> Expr {
    property_access(this_expr(), "cachedHasNext")
}

/// Builds the AST expression for caching flag enabled.
fn caching_flag_enabled_expr(flags: Expr, bit: i64) -> Expr {
    binary_expr(
        binary_expr(flags, BinOp::BitAnd, int_expr(bit)),
        BinOp::NotEq,
        int_expr(0),
    )
}

/// Builds the AST expression for caching full cache.
fn caching_full_cache_expr() -> Expr {
    caching_flag_enabled_expr(caching_flags_expr(), 256)
}

/// Builds the synthetic method body for caching construct.
fn caching_construct_body() -> Vec<Stmt> {
    let mut body = caching_validate_flags_body("CachingIterator::__construct", "flags");
    body.extend(vec![
        property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
        property_assign_stmt(this_expr(), "cache", empty_assoc_array_expr()),
        property_assign_stmt(this_expr(), "currentKey", null_expr()),
        property_assign_stmt(this_expr(), "currentValue", null_expr()),
        property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
        property_assign_stmt(this_expr(), "cachedHasNext", bool_expr(false)),
    ]);
    body
}

/// Builds the synthetic method body for caching validate flags.
fn caching_validate_flags_body(context: &str, var_name: &str) -> Vec<Stmt> {
    let mut body = vec![assign_stmt("stringFlagCount", int_expr(0))];
    for bit in [1, 2, 4, 8] {
        body.push(if_stmt(
            caching_flag_enabled_expr(var_expr(var_name), bit),
            vec![assign_stmt(
                "stringFlagCount",
                binary_expr(var_expr("stringFlagCount"), BinOp::Add, int_expr(1)),
            )],
            None,
        ));
    }
    body.push(if_stmt(
        binary_expr(var_expr("stringFlagCount"), BinOp::Gt, int_expr(1)),
        vec![throw_stmt(new_object_expr(
            "ValueError",
            vec![string_expr(&format!(
                "{context}(): Argument #{} ($flags) must contain only one of CachingIterator::CALL_TOSTRING, CachingIterator::TOSTRING_USE_KEY, CachingIterator::TOSTRING_USE_CURRENT, or CachingIterator::TOSTRING_USE_INNER",
                if context.ends_with("__construct") { 2 } else { 1 }
            ))],
        ))],
        None,
    ));
    body
}

/// Builds the synthetic method body for caching rewind.
fn caching_rewind_body() -> Vec<Stmt> {
    vec![
        expr_stmt(inner_call("rewind")),
        expr_stmt(method_call(this_expr(), "__elephcCaptureCurrent", Vec::new())),
    ]
}

/// Builds the synthetic method body for caching next.
fn caching_next_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(caching_has_next_expr()),
            vec![
                property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
                property_assign_stmt(this_expr(), "cachedHasNext", bool_expr(false)),
                property_assign_stmt(this_expr(), "currentKey", null_expr()),
                property_assign_stmt(this_expr(), "currentValue", null_expr()),
                return_void_stmt(),
            ],
            None,
        ),
        expr_stmt(method_call(this_expr(), "__elephcCaptureCurrent", Vec::new())),
    ]
}

/// Builds the synthetic method body for caching capture current.
fn caching_capture_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(inner_call("valid")),
            vec![
                property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
                property_assign_stmt(this_expr(), "cachedHasNext", bool_expr(false)),
                property_assign_stmt(this_expr(), "currentKey", null_expr()),
                property_assign_stmt(this_expr(), "currentValue", null_expr()),
                return_void_stmt(),
            ],
            None,
        ),
        property_assign_stmt(this_expr(), "currentKey", inner_call("key")),
        property_assign_stmt(this_expr(), "currentValue", inner_call("current")),
        property_assign_stmt(this_expr(), "currentValid", bool_expr(true)),
        if_stmt(
            caching_full_cache_expr(),
            vec![property_array_assign_stmt(
                this_expr(),
                "cache",
                caching_current_key_expr(),
                caching_current_value_expr(),
            )],
            None,
        ),
        expr_stmt(inner_call("next")),
        property_assign_stmt(this_expr(), "cachedHasNext", inner_call("valid")),
    ]
}

/// Builds the synthetic method body for caching current.
fn caching_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(not_expr(caching_current_valid_expr()), null_return_body(), None),
        return_stmt(caching_current_value_expr()),
    ]
}

/// Builds the synthetic method body for caching key.
fn caching_key_body() -> Vec<Stmt> {
    vec![
        if_stmt(not_expr(caching_current_valid_expr()), null_return_body(), None),
        return_stmt(caching_current_key_expr()),
    ]
}

/// Builds the synthetic method body for caching to string.
fn caching_to_string_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(
                binary_expr(caching_flags_expr(), BinOp::BitAnd, int_expr(15)),
                BinOp::StrictEq,
                int_expr(0),
            ),
            vec![throw_stmt(new_object_expr(
                "BadMethodCallException",
                vec![string_expr(
                    "CachingIterator does not fetch string value (see CachingIterator::__construct)",
                )],
            ))],
            None,
        ),
        if_stmt(not_expr(caching_current_valid_expr()), return_body(string_expr("")), None),
        if_stmt(
            caching_flag_enabled_expr(caching_flags_expr(), 2),
            return_body(cast_expr(CastType::String, caching_current_key_expr())),
            None,
        ),
        if_stmt(
            caching_flag_enabled_expr(caching_flags_expr(), 8),
            return_body(cast_expr(CastType::String, inner_expr())),
            None,
        ),
        return_stmt(cast_expr(CastType::String, caching_current_value_expr())),
    ]
}

/// Builds the synthetic method body for caching set flags.
fn caching_set_flags_body() -> Vec<Stmt> {
    let mut body = caching_validate_flags_body("CachingIterator::setFlags", "flags");
    body.push(if_stmt(
        binary_expr(
            caching_flag_enabled_expr(caching_flags_expr(), 1),
            BinOp::And,
            not_expr(caching_flag_enabled_expr(var_expr("flags"), 1)),
        ),
        vec![throw_stmt(new_object_expr(
            "InvalidArgumentException",
            vec![string_expr("Unsetting flag CALL_TO_STRING is not possible")],
        ))],
        None,
    ));
    body.push(property_assign_stmt(this_expr(), "flags", var_expr("flags")));
    body
}

/// Builds the synthetic method body for caching require full cache.
fn caching_require_full_cache_body(mut body: Vec<Stmt>) -> Vec<Stmt> {
    let mut out = vec![if_stmt(
        not_expr(caching_full_cache_expr()),
        vec![throw_stmt(new_object_expr(
            "BadMethodCallException",
            vec![string_expr(
                "CachingIterator does not use a full cache (see CachingIterator::__construct)",
            )],
        ))],
        None,
    )];
    out.append(&mut body);
    out
}

/// Builds the synthetic method body for caching offset get.
fn caching_offset_get_body() -> Vec<Stmt> {
    caching_require_full_cache_body(return_body(array_access(caching_cache_expr(), var_expr("key"))))
}

/// Builds the synthetic method body for caching offset set.
fn caching_offset_set_body() -> Vec<Stmt> {
    caching_require_full_cache_body(vec![property_array_assign_stmt(
        this_expr(),
        "cache",
        var_expr("key"),
        var_expr("value"),
    )])
}

/// Builds the synthetic method body for caching offset unset.
fn caching_offset_unset_body() -> Vec<Stmt> {
    caching_require_full_cache_body(vec![
        assign_stmt("newCache", empty_assoc_array_expr()),
        foreach_stmt(
            caching_cache_expr(),
            Some("cacheKey"),
            "cacheValue",
            vec![if_stmt(
                not_expr(binary_expr(var_expr("cacheKey"), BinOp::StrictEq, var_expr("key"))),
                vec![array_assign_stmt(
                    "newCache",
                    var_expr("cacheKey"),
                    var_expr("cacheValue"),
                )],
                None,
            )],
        ),
        property_assign_stmt(this_expr(), "cache", var_expr("newCache")),
    ])
}

/// Builds the synthetic method body for caching offset exists.
fn caching_offset_exists_body() -> Vec<Stmt> {
    caching_require_full_cache_body(return_body(function_call(
        "array_key_exists",
        vec![var_expr("key"), caching_cache_expr()],
    )))
}

/// Builds the synthetic method body for caching get cache.
fn caching_get_cache_body() -> Vec<Stmt> {
    caching_require_full_cache_body(return_body(caching_cache_expr()))
}

/// Builds the synthetic method body for caching count.
fn caching_count_body() -> Vec<Stmt> {
    caching_require_full_cache_body(return_body(count_expr(caching_cache_expr())))
}
