//! Purpose:
//! Injects the private ArrayIterator-compatible view used by AppendIterator::getArrayIterator().
//! Delegates ArrayAccess and Iterator operations back to AppendIterator storage methods.
//!
//! Called from:
//! - `super::append::insert_classes()`.
//!
//! Key details:
//! - The helper class is compiler-internal and intentionally absent from `spl_classes()`.
//! - The helper preserves sparse/removed entries by asking the owner for active slots.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, ClassMethod, ClassProperty, Expr, Stmt, TypeExpr};
use crate::types::traits::FlattenedClass;

use super::common::*;

/// Inserts class into the supplied builtin metadata registry.
pub(super) fn insert_class(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "__ElephcAppendIteratorArrayIterator".to_string(),
        FlattenedClass {
            name: "__ElephcAppendIteratorArrayIterator".to_string(),
            extends: Some("ArrayIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: append_iterator_array_iterator_properties(),
            methods: spl_append_iterator_array_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );
}

/// Appends iterator array iterator properties to the current runtime or metadata collection.
fn append_iterator_array_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("owner", named_type("AppendIterator")),
        storage_property_default("appendPosition", TypeExpr::Int, int_expr(0)),
    ]
}

/// Builds the method list for SPL append iterator array iterator.
fn spl_append_iterator_array_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("owner", named_type("AppendIterator"))],
            Some(TypeExpr::Void),
            append_array_iterator_construct_body(),
        ),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), append_array_iterator_count_body()),
        method_with_body(
            "append",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            append_array_iterator_append_body(),
        ),
        method_with_body(
            "offsetSet",
            vec![param("offset", mixed_type()), param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            append_array_iterator_offset_set_body(),
        ),
        method_with_body(
            "offsetExists",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Bool),
            append_array_iterator_offset_exists_body(),
        ),
        method_with_body(
            "offsetGet",
            vec![param("offset", mixed_type())],
            Some(mixed_type()),
            append_array_iterator_offset_get_body(),
        ),
        method_with_body(
            "offsetUnset",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Void),
            append_array_iterator_offset_unset_body(),
        ),
        method_with_body("getArrayCopy", Vec::new(), Some(array_type()), append_array_iterator_copy_body()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), append_array_iterator_rewind_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), append_array_iterator_next_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), append_array_iterator_valid_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), append_array_iterator_key_body()),
        method_with_body("current", Vec::new(), Some(mixed_type()), append_array_iterator_current_body()),
    ]
}

/// Appends array iterator owner expr to the current runtime or metadata collection.
fn append_array_iterator_owner_expr() -> Expr {
    property_access(this_expr(), "owner")
}

/// Appends array iterator position expr to the current runtime or metadata collection.
fn append_array_iterator_position_expr() -> Expr {
    property_access(this_expr(), "appendPosition")
}

/// Appends array iterator owner call to the current runtime or metadata collection.
fn append_array_iterator_owner_call(method: &str, args: Vec<Expr>) -> Expr {
    method_call(append_array_iterator_owner_expr(), method, args)
}

/// Appends array iterator construct body to the current runtime or metadata collection.
fn append_array_iterator_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "owner", var_expr("owner")),
        property_assign_stmt(this_expr(), "appendPosition", int_expr(0)),
    ]
}

/// Appends array iterator count body to the current runtime or metadata collection.
fn append_array_iterator_count_body() -> Vec<Stmt> {
    return_body(append_array_iterator_owner_call("__elephcStorageCount", Vec::new()))
}

/// Appends array iterator append body to the current runtime or metadata collection.
fn append_array_iterator_append_body() -> Vec<Stmt> {
    vec![expr_stmt(append_array_iterator_owner_call(
        "__elephcStorageAppend",
        vec![var_expr("iterator")],
    ))]
}

/// Appends array iterator offset set body to the current runtime or metadata collection.
fn append_array_iterator_offset_set_body() -> Vec<Stmt> {
    vec![expr_stmt(append_array_iterator_owner_call(
        "__elephcStorageOffsetSet",
        vec![var_expr("offset"), var_expr("iterator")],
    ))]
}

/// Appends array iterator offset exists body to the current runtime or metadata collection.
fn append_array_iterator_offset_exists_body() -> Vec<Stmt> {
    return_body(append_array_iterator_owner_call(
        "__elephcStorageOffsetExists",
        vec![var_expr("offset")],
    ))
}

/// Appends array iterator offset get body to the current runtime or metadata collection.
fn append_array_iterator_offset_get_body() -> Vec<Stmt> {
    return_body(append_array_iterator_owner_call(
        "__elephcStorageOffsetGet",
        vec![var_expr("offset")],
    ))
}

/// Appends array iterator offset unset body to the current runtime or metadata collection.
fn append_array_iterator_offset_unset_body() -> Vec<Stmt> {
    vec![expr_stmt(append_array_iterator_owner_call(
        "__elephcStorageOffsetUnset",
        vec![var_expr("offset")],
    ))]
}

/// Appends array iterator copy body to the current runtime or metadata collection.
fn append_array_iterator_copy_body() -> Vec<Stmt> {
    return_body(append_array_iterator_owner_call(
        "__elephcStorageGetArrayCopy",
        Vec::new(),
    ))
}

/// Appends array iterator rewind body to the current runtime or metadata collection.
fn append_array_iterator_rewind_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "appendPosition", int_expr(0))]
}

/// Appends array iterator next body to the current runtime or metadata collection.
fn append_array_iterator_next_body() -> Vec<Stmt> {
    vec![property_assign_stmt(
        this_expr(),
        "appendPosition",
        binary_expr(append_array_iterator_position_expr(), BinOp::Add, int_expr(1)),
    )]
}

/// Appends array iterator valid body to the current runtime or metadata collection.
fn append_array_iterator_valid_body() -> Vec<Stmt> {
    vec![
        while_stmt(
            binary_expr(
                append_array_iterator_position_expr(),
                BinOp::Lt,
                append_array_iterator_owner_call("__elephcStoragePhysicalCount", Vec::new()),
            ),
            vec![
                if_stmt(
                    append_array_iterator_owner_call(
                        "__elephcStorageIsActive",
                        vec![append_array_iterator_position_expr()],
                    ),
                    return_body(bool_expr(true)),
                    None,
                ),
                property_assign_stmt(
                    this_expr(),
                    "appendPosition",
                    binary_expr(append_array_iterator_position_expr(), BinOp::Add, int_expr(1)),
                ),
            ],
        ),
        return_stmt(bool_expr(false)),
    ]
}

/// Appends array iterator key body to the current runtime or metadata collection.
fn append_array_iterator_key_body() -> Vec<Stmt> {
    return_body(append_array_iterator_owner_call(
        "__elephcStorageKey",
        vec![append_array_iterator_position_expr()],
    ))
}

/// Appends array iterator current body to the current runtime or metadata collection.
fn append_array_iterator_current_body() -> Vec<Stmt> {
    return_body(append_array_iterator_owner_call(
        "__elephcStorageCurrent",
        vec![append_array_iterator_position_expr()],
    ))
}
