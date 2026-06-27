//! Purpose:
//! Injects `SplObjectStorage` metadata backed by per-instance arrays.
//! Provides object identity storage, info payloads, ArrayAccess, Countable, and Iterator methods.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Object identity uses strict comparison against stored object handles.
//! - The parallel arrays are ordinary object properties, so object deep-free finalizes each instance.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, ClassMethod, ClassProperty, Expr, Stmt, TypeExpr, Visibility};
use crate::types::traits::FlattenedClass;

use super::common::*;

/// Inserts `SplObjectStorage` into the builtin class registry.
pub(super) fn insert_class(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "SplObjectStorage".to_string(),
        FlattenedClass {
            name: "SplObjectStorage".to_string(),
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
            properties: spl_object_storage_properties(),
            methods: spl_object_storage_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
}

/// Returns hidden storage properties for objects, attached info values, and cursor position.
fn spl_object_storage_properties() -> Vec<ClassProperty> {
    vec![
        protected_storage_property("objects", array_type()),
        protected_storage_property("infos", array_type()),
        protected_storage_property("position", TypeExpr::Int),
    ]
}

/// Returns all methods supported by the built-in `SplObjectStorage` surface.
fn spl_object_storage_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body("__construct", Vec::new(), Some(TypeExpr::Void), construct_body()),
        method_with_body(
            "attach",
            vec![param("object", mixed_type()), param("info", mixed_type())],
            Some(TypeExpr::Void),
            attach_body(),
        ),
        method_with_body("detach", vec![param("object", mixed_type())], Some(TypeExpr::Void), detach_body()),
        method_with_body("contains", vec![param("object", mixed_type())], Some(TypeExpr::Bool), contains_body()),
        method_with_body("addAll", vec![param("storage", named_type("SplObjectStorage"))], Some(TypeExpr::Void), add_all_body()),
        method_with_body("removeAll", vec![param("storage", named_type("SplObjectStorage"))], Some(TypeExpr::Void), remove_all_body()),
        method_with_body(
            "removeAllExcept",
            vec![param("storage", named_type("SplObjectStorage"))],
            Some(TypeExpr::Void),
            remove_all_except_body(),
        ),
        method_with_body("getInfo", Vec::new(), Some(mixed_type()), get_info_body()),
        method_with_body("setInfo", vec![param("info", mixed_type())], Some(TypeExpr::Void), set_info_body()),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), count_body()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), valid_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), key_body()),
        method_with_body("current", Vec::new(), Some(mixed_type()), current_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), next_body()),
        method_with_body("seek", vec![param("offset", TypeExpr::Int)], Some(TypeExpr::Void), seek_body()),
        method_with_body("offsetExists", vec![param("object", mixed_type())], Some(TypeExpr::Bool), contains_body()),
        method_with_body("offsetGet", vec![param("object", mixed_type())], Some(mixed_type()), offset_get_body()),
        method_with_body(
            "offsetSet",
            vec![param("object", mixed_type()), param("info", mixed_type())],
            Some(TypeExpr::Void),
            offset_set_body(),
        ),
        method_with_body("offsetUnset", vec![param("object", mixed_type())], Some(TypeExpr::Void), detach_body()),
        method_with_body("getHash", vec![param("object", mixed_type())], Some(TypeExpr::Str), get_hash_body()),
        method_with_body("serialize", Vec::new(), Some(TypeExpr::Str), return_body(string_expr(""))),
        method_with_body("unserialize", vec![param("data", TypeExpr::Str)], Some(TypeExpr::Void), construct_body()),
        method_with_body("__serialize", Vec::new(), Some(array_type()), serialize_array_body()),
        method_with_body("__unserialize", vec![param("data", array_type())], Some(TypeExpr::Void), construct_body()),
        method_with_body("__debugInfo", Vec::new(), Some(array_type()), debug_info_body()),
        protected_method_with_body(
            "__elephcIndexOf",
            vec![param("object", mixed_type())],
            Some(TypeExpr::Int),
            index_of_body(),
        ),
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

/// Returns `$this->objects`.
fn objects_expr() -> Expr {
    property_access(this_expr(), "objects")
}

/// Returns `$this->infos`.
fn infos_expr() -> Expr {
    property_access(this_expr(), "infos")
}

/// Returns `$this->position`.
fn position_expr() -> Expr {
    property_access(this_expr(), "position")
}

/// Returns `$this->objects[$index]`.
fn object_at(index: Expr) -> Expr {
    array_access(objects_expr(), index)
}

/// Returns `$this->infos[$index]`.
fn info_at(index: Expr) -> Expr {
    array_access(infos_expr(), index)
}

/// Returns `$this->__elephcIndexOf($object)`.
fn index_of_expr(object: Expr) -> Expr {
    method_call(this_expr(), "__elephcIndexOf", vec![object])
}

/// Initializes the storage arrays and iterator position.
fn construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "objects", empty_array_expr()),
        property_assign_stmt(this_expr(), "infos", empty_array_expr()),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
    ]
}

/// Attaches a new object or updates info for an existing object.
fn attach_body() -> Vec<Stmt> {
    vec![
        assign_stmt("index", index_of_expr(var_expr("object"))),
        if_stmt(
            binary_expr(var_expr("index"), BinOp::GtEq, int_expr(0)),
            vec![
                property_array_assign_stmt(this_expr(), "infos", var_expr("index"), var_expr("info")),
                return_void_stmt(),
            ],
            None,
        ),
        property_array_push_stmt(this_expr(), "objects", var_expr("object")),
        property_array_push_stmt(this_expr(), "infos", var_expr("info")),
    ]
}

/// Detaches a stored object if present.
fn detach_body() -> Vec<Stmt> {
    vec![
        typed_assign_stmt("newObjects", array_type(), empty_array_expr()),
        typed_assign_stmt("newInfos", array_type(), empty_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(objects_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    not_expr(binary_expr(object_at(var_expr("i")), BinOp::StrictEq, var_expr("object"))),
                    vec![
                        assign_stmt("keptObject", object_at(var_expr("i"))),
                        assign_stmt("keptInfo", info_at(var_expr("i"))),
                        array_push_stmt("newObjects", var_expr("keptObject")),
                        array_push_stmt("newInfos", var_expr("keptInfo")),
                    ],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        property_assign_stmt(this_expr(), "objects", var_expr("newObjects")),
        property_assign_stmt(this_expr(), "infos", var_expr("newInfos")),
    ]
}

/// Returns true when an object is attached.
fn contains_body() -> Vec<Stmt> {
    return_body(binary_expr(index_of_expr(var_expr("object")), BinOp::GtEq, int_expr(0)))
}

/// Attaches every object/info pair from another storage instance.
fn add_all_body() -> Vec<Stmt> {
    vec![foreach_stmt(
        var_expr("storage"),
        None,
        "object",
        vec![expr_stmt(method_call(
            this_expr(),
            "attach",
            vec![var_expr("object"), array_access(var_expr("storage"), var_expr("object"))],
        ))],
    )]
}

/// Detaches every object found in another storage instance.
fn remove_all_body() -> Vec<Stmt> {
    vec![foreach_stmt(
        var_expr("storage"),
        None,
        "object",
        vec![expr_stmt(method_call(this_expr(), "detach", vec![var_expr("object")]))],
    )]
}

/// Keeps only objects that are also present in another storage instance.
fn remove_all_except_body() -> Vec<Stmt> {
    vec![
        typed_assign_stmt("newObjects", array_type(), empty_array_expr()),
        typed_assign_stmt("newInfos", array_type(), empty_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(objects_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                assign_stmt("object", object_at(var_expr("i"))),
                if_stmt(
                    method_call(var_expr("storage"), "contains", vec![var_expr("object")]),
                    vec![
                        assign_stmt("info", info_at(var_expr("i"))),
                        array_push_stmt("newObjects", var_expr("object")),
                        array_push_stmt("newInfos", var_expr("info")),
                    ],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        property_assign_stmt(this_expr(), "objects", var_expr("newObjects")),
        property_assign_stmt(this_expr(), "infos", var_expr("newInfos")),
    ]
}

/// Returns the info payload for the current iterator position.
fn get_info_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            method_call(this_expr(), "valid", Vec::new()),
            return_body(info_at(position_expr())),
            None,
        ),
        return_stmt(null_expr()),
    ]
}

/// Updates the info payload for the current iterator position when valid.
fn set_info_body() -> Vec<Stmt> {
    vec![if_stmt(
        method_call(this_expr(), "valid", Vec::new()),
        vec![property_array_assign_stmt(this_expr(), "infos", position_expr(), var_expr("info"))],
        None,
    )]
}

/// Returns the number of attached objects.
fn count_body() -> Vec<Stmt> {
    return_body(count_expr(objects_expr()))
}

/// Resets iteration to the first storage entry.
fn rewind_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "position", int_expr(0))]
}

/// Returns true when the current iterator position points at a stored object.
fn valid_body() -> Vec<Stmt> {
    return_body(binary_expr(position_expr(), BinOp::Lt, count_expr(objects_expr())))
}

/// Returns the current numeric iterator key or null when invalid.
fn key_body() -> Vec<Stmt> {
    vec![
        if_stmt(method_call(this_expr(), "valid", Vec::new()), return_body(position_expr()), None),
        return_stmt(null_expr()),
    ]
}

/// Returns the current object or null when invalid.
fn current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            method_call(this_expr(), "valid", Vec::new()),
            return_body(object_at(position_expr())),
            None,
        ),
        return_stmt(null_expr()),
    ]
}

/// Advances iteration by one entry.
fn next_body() -> Vec<Stmt> {
    vec![property_assign_stmt(
        this_expr(),
        "position",
        binary_expr(position_expr(), BinOp::Add, int_expr(1)),
    )]
}

/// Moves the iterator position to a requested offset.
fn seek_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "position", var_expr("offset"))]
}

/// Returns info attached to a specific object or null when absent.
fn offset_get_body() -> Vec<Stmt> {
    vec![
        assign_stmt("index", index_of_expr(var_expr("object"))),
        if_stmt(
            binary_expr(var_expr("index"), BinOp::GtEq, int_expr(0)),
            return_body(info_at(var_expr("index"))),
            None,
        ),
        return_stmt(null_expr()),
    ]
}

/// Stores info for a specific object through ArrayAccess syntax.
fn offset_set_body() -> Vec<Stmt> {
    vec![expr_stmt(method_call(
        this_expr(),
        "attach",
        vec![var_expr("object"), var_expr("info")],
    ))]
}

/// Returns a stable decimal hash for objects currently attached to this storage.
fn get_hash_body() -> Vec<Stmt> {
    vec![
        assign_stmt("index", index_of_expr(var_expr("object"))),
        if_stmt(
            binary_expr(var_expr("index"), BinOp::GtEq, int_expr(0)),
            return_body(binary_expr(string_expr(""), BinOp::Concat, var_expr("index"))),
            None,
        ),
        return_stmt(string_expr("")),
    ]
}

/// Returns an array snapshot suitable for `__serialize()`.
fn serialize_array_body() -> Vec<Stmt> {
    return_body(expr(crate::parser::ast::ExprKind::ArrayLiteralAssoc(vec![
        (string_expr("objects"), objects_expr()),
        (string_expr("infos"), infos_expr()),
    ])))
}

/// Returns a small debug array exposing stored objects and info values.
fn debug_info_body() -> Vec<Stmt> {
    serialize_array_body()
}

/// Finds the index of an object by strict object identity, or `-1` when absent.
fn index_of_body() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(objects_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(object_at(var_expr("i")), BinOp::StrictEq, var_expr("object")),
                    return_body(var_expr("i")),
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(int_expr(-1)),
    ]
}
