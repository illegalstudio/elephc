//! Purpose:
//! Injects the supported `Phar` and `PharData` builtin class metadata.
//! Provides the first OOP archive surface by mapping methods and ArrayAccess onto `phar://` URLs.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Method bodies are synthetic PHP-like AST, so normal checker and EIR lowering own behavior.
//! - Archive writes, deletion, and supported compression controls reuse existing
//!   runtime `phar://` paths or focused internal bridge helpers.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, CastType, ClassConst, ClassMethod, ClassProperty, Expr, TypeExpr};
use crate::types::traits::FlattenedClass;

use super::common::*;

/// Inserts the supported PHAR classes into the builtin metadata registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    insert_phar_like_class(class_map, "Phar");
    insert_phar_like_class(class_map, "PharData");
}

/// Inserts one PHAR-like archive class with the shared ArrayAccess surface.
fn insert_phar_like_class(class_map: &mut HashMap<String, FlattenedClass>, name: &str) {
    class_map.insert(
        name.to_string(),
        FlattenedClass {
            name: name.to_string(),
            extends: None,
            implements: vec!["ArrayAccess".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: phar_properties(),
            methods: phar_methods(),
            attributes: Vec::new(),
            constants: phar_constants(),
            used_traits: Vec::new(),
        },
    );
}

/// Builds the private archive-path property shared by `Phar` and `PharData`.
fn phar_properties() -> Vec<ClassProperty> {
    vec![storage_property("path", TypeExpr::Str)]
}

/// Builds the supported constructor, write helper, and ArrayAccess methods for PHAR objects.
fn phar_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("filename", TypeExpr::Str)],
            Some(TypeExpr::Void),
            phar_construct_body(),
        ),
        method_with_body(
            "offsetExists",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Bool),
            phar_offset_exists_body(),
        ),
        method_with_body(
            "offsetGet",
            vec![param("offset", mixed_type())],
            Some(mixed_type()),
            phar_offset_get_body(),
        ),
        method_with_body(
            "addFromString",
            vec![
                param("localName", TypeExpr::Str),
                param("contents", TypeExpr::Str),
            ],
            Some(TypeExpr::Void),
            phar_add_from_string_body(),
        ),
        method_with_body(
            "compressFiles",
            vec![param("compression", TypeExpr::Int)],
            Some(TypeExpr::Void),
            phar_compress_files_body(),
        ),
        method_with_body(
            "decompressFiles",
            Vec::new(),
            Some(TypeExpr::Bool),
            phar_decompress_files_body(),
        ),
        method_with_body(
            "delete",
            vec![param("localName", TypeExpr::Str)],
            Some(TypeExpr::Bool),
            phar_delete_body(),
        ),
        method_with_body(
            "offsetSet",
            vec![param("offset", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
            phar_offset_set_body(),
        ),
        method_with_body(
            "offsetUnset",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Void),
            phar_offset_unset_body(),
        ),
    ]
}

/// Builds constructor body that stores the archive path on the object.
fn phar_construct_body() -> Vec<crate::parser::ast::Stmt> {
    vec![property_assign_stmt(this_expr(), "path", var_expr("filename"))]
}

/// Builds `offsetExists()` as a `file_get_contents()` false check.
fn phar_offset_exists_body() -> Vec<crate::parser::ast::Stmt> {
    return_body(binary_expr(
        function_call("file_get_contents", vec![phar_entry_url_expr(var_expr("offset"))]),
        BinOp::StrictNotEq,
        bool_expr(false),
    ))
}

/// Builds `offsetGet()` as a `file_get_contents()` read.
fn phar_offset_get_body() -> Vec<crate::parser::ast::Stmt> {
    return_body(function_call(
        "file_get_contents",
        vec![phar_entry_url_expr(var_expr("offset"))],
    ))
}

/// Builds `addFromString()` as a typed `file_put_contents()` archive write.
fn phar_add_from_string_body() -> Vec<crate::parser::ast::Stmt> {
    vec![expr_stmt(function_call(
        "file_put_contents",
        vec![phar_entry_url_expr(var_expr("localName")), var_expr("contents")],
    ))]
}

/// Builds `compressFiles()` as an archive-wide native PHAR compression rewrite.
fn phar_compress_files_body() -> Vec<crate::parser::ast::Stmt> {
    vec![expr_stmt(function_call(
        "__elephc_phar_set_compression",
        vec![property_access(this_expr(), "path"), var_expr("compression")],
    ))]
}

/// Builds `decompressFiles()` as a native PHAR compression reset.
fn phar_decompress_files_body() -> Vec<crate::parser::ast::Stmt> {
    return_body(function_call(
        "__elephc_phar_set_compression",
        vec![property_access(this_expr(), "path"), int_expr(0)],
    ))
}

/// Builds `delete()` as an archive-entry `unlink()`.
fn phar_delete_body() -> Vec<crate::parser::ast::Stmt> {
    return_body(function_call(
        "unlink",
        vec![phar_entry_url_expr(var_expr("localName"))],
    ))
}

/// Builds `offsetSet()` as a `file_put_contents()` write.
fn phar_offset_set_body() -> Vec<crate::parser::ast::Stmt> {
    vec![expr_stmt(function_call(
        "file_put_contents",
        vec![phar_entry_url_expr(var_expr("offset")), var_expr("value")],
    ))]
}

/// Builds `offsetUnset()` as an archive-entry `unlink()`.
fn phar_offset_unset_body() -> Vec<crate::parser::ast::Stmt> {
    vec![expr_stmt(function_call(
        "unlink",
        vec![phar_entry_url_expr(var_expr("offset"))],
    ))]
}

/// Builds the `phar://<archive>/<entry>` URL expression for an ArrayAccess offset.
fn phar_entry_url_expr(offset: Expr) -> Expr {
    let archive_url = binary_expr(
        string_expr("phar://"),
        BinOp::Concat,
        property_access(this_expr(), "path"),
    );
    let archive_prefix = binary_expr(archive_url, BinOp::Concat, string_expr("/"));
    binary_expr(
        archive_prefix,
        BinOp::Concat,
        cast_expr(CastType::String, offset),
    )
}

/// Builds the currently exposed PHAR format, compression, and signature constants.
fn phar_constants() -> Vec<ClassConst> {
    vec![
        class_const("NONE", 0),
        class_const("COMPRESSED", 61_440),
        class_const("GZ", 4_096),
        class_const("BZ2", 8_192),
        class_const("PHAR", 1),
        class_const("TAR", 2),
        class_const("ZIP", 3),
        class_const("MD5", 1),
        class_const("SHA1", 2),
        class_const("SHA256", 3),
        class_const("SHA512", 4),
        class_const("OPENSSL", 16),
    ]
}
