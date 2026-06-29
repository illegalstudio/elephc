//! Purpose:
//! Injects the supported `Phar` and `PharData` builtin class metadata.
//! Provides the OOP archive surface by mapping methods and ArrayAccess onto `phar://` URLs or object storage.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Method bodies are synthetic PHP-like AST, so normal checker and EIR lowering own behavior.
//! - Archive writes, deletion, and supported compression controls reuse existing
//!   runtime `phar://` paths or focused internal bridge helpers.

use std::collections::HashMap;

use crate::parser::ast::{
    BinOp, CastType, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, TypeExpr,
};
use crate::types::traits::FlattenedClass;

use super::common::*;

/// Inserts the supported PHAR classes into the builtin metadata registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    insert_phar_like_class(class_map, "Phar");
    insert_phar_like_class(class_map, "PharData");
    insert_phar_file_info_class(class_map);
}

/// Inserts one PHAR-like archive class with the shared ArrayAccess surface.
fn insert_phar_like_class(class_map: &mut HashMap<String, FlattenedClass>, name: &str) {
    class_map.insert(
        name.to_string(),
        FlattenedClass {
            name: name.to_string(),
            extends: None,
            implements: vec![
                "ArrayAccess".to_string(),
                "Iterator".to_string(),
                "Countable".to_string(),
            ],
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

/// Inserts the PHAR entry info class used by archive ArrayAccess reads.
fn insert_phar_file_info_class(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "PharFileInfo".to_string(),
        FlattenedClass {
            name: "PharFileInfo".to_string(),
            extends: Some("SplFileInfo".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: phar_file_info_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );
}

/// Builds the private object state shared by `Phar` and `PharData`.
fn phar_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("path", TypeExpr::Str),
        storage_property("metadata", mixed_type()),
        storage_property("hasMetadata", TypeExpr::Bool),
        storage_property("stub", TypeExpr::Str),
        storage_property("entries", array_type()),
        storage_property("position", TypeExpr::Int),
    ]
}

/// Builds the supported constructor, metadata/stub helpers, write helpers, and ArrayAccess methods.
fn phar_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("filename", TypeExpr::Str)],
            Some(TypeExpr::Void),
            phar_construct_body(),
        ),
        method_with_body(
            "__toString",
            Vec::new(),
            Some(TypeExpr::Str),
            return_body(phar_path_expr()),
        ),
        method_with_body(
            "getPath",
            Vec::new(),
            Some(TypeExpr::Str),
            return_body(phar_path_expr()),
        ),
        method_with_body(
            "getPathname",
            Vec::new(),
            Some(TypeExpr::Str),
            return_body(phar_path_expr()),
        ),
        method_with_body(
            "getFilename",
            Vec::new(),
            Some(TypeExpr::Str),
            return_body(function_call("basename", vec![phar_path_expr()])),
        ),
        method_with_body(
            "setMetadata",
            vec![param("metadata", mixed_type())],
            Some(TypeExpr::Void),
            phar_set_metadata_body(),
        ),
        method_with_body(
            "getMetadata",
            vec![param_default(
                "unserializeOptions",
                array_type(),
                empty_array_expr(),
            )],
            Some(mixed_type()),
            phar_get_metadata_body(),
        ),
        method_with_body(
            "hasMetadata",
            Vec::new(),
            Some(TypeExpr::Bool),
            return_body(property_access(this_expr(), "hasMetadata")),
        ),
        method_with_body(
            "delMetadata",
            Vec::new(),
            Some(TypeExpr::Bool),
            phar_del_metadata_body(),
        ),
        method_with_body(
            "setStub",
            vec![param("stub", TypeExpr::Str)],
            Some(TypeExpr::Bool),
            phar_set_stub_body(),
        ),
        method_with_body(
            "getStub",
            Vec::new(),
            Some(TypeExpr::Str),
            return_body(property_access(this_expr(), "stub")),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), phar_rewind_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), phar_next_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), phar_valid_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), phar_key_body()),
        method_with_body(
            "current",
            Vec::new(),
            Some(named_type("PharFileInfo")),
            phar_current_body(),
        ),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), phar_count_body()),
        method_with_body(
            "offsetExists",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Bool),
            phar_offset_exists_body(),
        ),
        method_with_body(
            "offsetGet",
            vec![param("offset", mixed_type())],
            Some(named_type("PharFileInfo")),
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
            "compress",
            vec![
                param("compression", TypeExpr::Int),
                param_default("extension", TypeExpr::Str, string_expr("")),
            ],
            Some(named_type("PharData")),
            phar_compress_body(),
        ),
        method_with_body(
            "decompress",
            vec![param_default("extension", TypeExpr::Str, string_expr(""))],
            Some(named_type("PharData")),
            phar_decompress_body(),
        ),
        method_with_body(
            "setSignatureAlgorithm",
            vec![
                param("algo", TypeExpr::Int),
                param_default("privateKey", TypeExpr::Str, string_expr("")),
            ],
            Some(TypeExpr::Void),
            phar_set_signature_algorithm_body(),
        ),
        method_with_body(
            "getSignature",
            Vec::new(),
            Some(mixed_type()),
            phar_get_signature_body(),
        ),
        method_with_body(
            "setZipPassword",
            vec![param("password", TypeExpr::Str)],
            Some(TypeExpr::Bool),
            phar_set_zip_password_body(),
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

/// Builds the entry-level PHAR file info methods.
fn phar_file_info_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("filename", TypeExpr::Str)],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "path", var_expr("filename"))],
        ),
        method_with_body(
            "setMetadata",
            vec![param("metadata", mixed_type())],
            Some(TypeExpr::Void),
            phar_file_info_set_metadata_body(),
        ),
        method_with_body(
            "getMetadata",
            vec![param_default(
                "unserializeOptions",
                array_type(),
                empty_array_expr(),
            )],
            Some(mixed_type()),
            phar_file_info_get_metadata_body(),
        ),
        method_with_body(
            "hasMetadata",
            Vec::new(),
            Some(TypeExpr::Bool),
            phar_file_info_has_metadata_body(),
        ),
        method_with_body(
            "delMetadata",
            Vec::new(),
            Some(TypeExpr::Bool),
            phar_file_info_del_metadata_body(),
        ),
        method_with_body(
            "__toString",
            Vec::new(),
            Some(TypeExpr::Str),
            return_body(property_access(this_expr(), "path")),
        ),
        method_with_body(
            "getPath",
            Vec::new(),
            Some(TypeExpr::Str),
            return_body(function_call(
                "dirname",
                vec![property_access(this_expr(), "path")],
            )),
        ),
        method_with_body(
            "getFilename",
            Vec::new(),
            Some(TypeExpr::Str),
            return_body(function_call(
                "basename",
                vec![property_access(this_expr(), "path")],
            )),
        ),
        method_with_body(
            "getPathname",
            Vec::new(),
            Some(TypeExpr::Str),
            return_body(property_access(this_expr(), "path")),
        ),
        method_with_body(
            "getContent",
            Vec::new(),
            Some(mixed_type()),
            return_body(function_call(
                "file_get_contents",
                vec![property_access(this_expr(), "path")],
            )),
        ),
    ]
}

/// Builds constructor body that stores the archive path and loads any persisted
/// stub and global metadata from the archive.
fn phar_construct_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        property_assign_stmt(this_expr(), "path", var_expr("filename")),
        property_assign_stmt(this_expr(), "metadata", null_expr()),
        property_assign_stmt(this_expr(), "hasMetadata", bool_expr(false)),
        // Load the persisted stub, falling back to the default when none is stored.
        assign_stmt(
            "loadedStub",
            function_call("__elephc_phar_get_stub", vec![var_expr("filename")]),
        ),
        if_stmt(
            binary_expr(var_expr("loadedStub"), BinOp::StrictNotEq, string_expr("")),
            vec![property_assign_stmt(this_expr(), "stub", var_expr("loadedStub"))],
            Some(vec![property_assign_stmt(
                this_expr(),
                "stub",
                string_expr("<?php __HALT_COMPILER(); ?>"),
            )]),
        ),
        // Load and unserialize the persisted global metadata when present.
        assign_stmt(
            "loadedMeta",
            function_call("__elephc_phar_get_metadata", vec![var_expr("filename")]),
        ),
        if_stmt(
            binary_expr(var_expr("loadedMeta"), BinOp::StrictNotEq, string_expr("")),
            vec![
                property_assign_stmt(
                    this_expr(),
                    "metadata",
                    function_call("unserialize", vec![var_expr("loadedMeta")]),
                ),
                property_assign_stmt(this_expr(), "hasMetadata", bool_expr(true)),
            ],
            None,
        ),
        property_assign_stmt(
            this_expr(),
            "entries",
            function_call("__elephc_phar_list_entries", vec![var_expr("filename")]),
        ),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
    ]
}

/// Builds `setMetadata()` as per-object metadata storage.
fn phar_set_metadata_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        property_assign_stmt(this_expr(), "metadata", var_expr("metadata")),
        property_assign_stmt(this_expr(), "hasMetadata", bool_expr(true)),
        // Persist the serialized metadata into the archive.
        expr_stmt(function_call(
            "__elephc_phar_set_metadata",
            vec![
                property_access(this_expr(), "path"),
                function_call("serialize", vec![var_expr("metadata")]),
            ],
        )),
    ]
}

/// Builds `getMetadata()` with PHP's null result before metadata is set.
fn phar_get_metadata_body() -> Vec<crate::parser::ast::Stmt> {
    vec![if_stmt(
        property_access(this_expr(), "hasMetadata"),
        return_body(property_access(this_expr(), "metadata")),
        Some(return_body(null_expr())),
    )]
}

/// Builds `delMetadata()` by clearing the per-object metadata state and the archive's
/// persisted global metadata.
fn phar_del_metadata_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        property_assign_stmt(this_expr(), "metadata", null_expr()),
        property_assign_stmt(this_expr(), "hasMetadata", bool_expr(false)),
        // Clear the persisted metadata by writing an empty blob.
        expr_stmt(function_call(
            "__elephc_phar_set_metadata",
            vec![property_access(this_expr(), "path"), string_expr("")],
        )),
        return_stmt(bool_expr(true)),
    ]
}

/// Reads an entry's persisted serialized metadata via the bridge, keyed by the
/// `PharFileInfo`'s inherited `phar://` pathname.
fn phar_file_info_read_metadata_expr() -> Expr {
    function_call(
        "__elephc_phar_get_file_metadata",
        vec![property_access(this_expr(), "path")],
    )
}

/// Builds `PharFileInfo::setMetadata()` as a write-through to the entry's persisted
/// per-file metadata (no object-local copy; reads go straight back to the archive).
fn phar_file_info_set_metadata_body() -> Vec<crate::parser::ast::Stmt> {
    vec![expr_stmt(function_call(
        "__elephc_phar_set_file_metadata",
        vec![
            property_access(this_expr(), "path"),
            function_call("serialize", vec![var_expr("metadata")]),
        ],
    ))]
}

/// Builds `PharFileInfo::getMetadata()`: reads the persisted blob and unserializes it,
/// returning `null` when the entry carries no metadata.
fn phar_file_info_get_metadata_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        assign_stmt("rawMeta", phar_file_info_read_metadata_expr()),
        if_stmt(
            binary_expr(var_expr("rawMeta"), BinOp::StrictEq, string_expr("")),
            vec![return_stmt(null_expr())],
            None,
        ),
        return_stmt(function_call("unserialize", vec![var_expr("rawMeta")])),
    ]
}

/// Builds `PharFileInfo::hasMetadata()`: true when the entry's persisted metadata blob
/// is non-empty.
fn phar_file_info_has_metadata_body() -> Vec<crate::parser::ast::Stmt> {
    return_body(binary_expr(
        phar_file_info_read_metadata_expr(),
        BinOp::StrictNotEq,
        string_expr(""),
    ))
}

/// Builds `PharFileInfo::delMetadata()`: clears the entry's persisted per-file
/// metadata and reports success.
fn phar_file_info_del_metadata_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        expr_stmt(function_call(
            "__elephc_phar_set_file_metadata",
            vec![property_access(this_expr(), "path"), string_expr("")],
        )),
        return_stmt(bool_expr(true)),
    ]
}

/// Builds `setSignatureAlgorithm()`: `Phar::OPENSSL` (16) routes to RSA-SHA1 signing
/// with the supplied private key; the hash algorithms (MD5/SHA1/SHA256/SHA512) route to
/// the hash-signing bridge. The archive trailer is rewritten in place.
fn phar_set_signature_algorithm_body() -> Vec<crate::parser::ast::Stmt> {
    vec![if_stmt(
        binary_expr(var_expr("algo"), BinOp::StrictEq, int_expr(16)),
        vec![expr_stmt(function_call(
            "__elephc_phar_sign_openssl",
            vec![property_access(this_expr(), "path"), var_expr("privateKey")],
        ))],
        Some(vec![expr_stmt(function_call(
            "__elephc_phar_sign_hash",
            vec![property_access(this_expr(), "path"), var_expr("algo")],
        ))]),
    )]
}

/// Builds `setZipPassword()`: stores the password (a compiler extension) used to
/// read and write traditional-PKWARE (ZipCrypto) encrypted ZIP entries. Once set,
/// later reads decrypt encrypted entries and zip writes encrypt their entries.
fn phar_set_zip_password_body() -> Vec<crate::parser::ast::Stmt> {
    vec![return_stmt(function_call(
        "__elephc_phar_set_zip_password",
        vec![var_expr("password")],
    ))]
}

/// Builds `getSignature()` returning `['hash' => <uppercase hex>, 'hash_type' => <name>]`
/// read from the archive's signature trailer.
fn phar_get_signature_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        assign_stmt(
            "sigHash",
            function_call(
                "__elephc_phar_get_signature_hash",
                vec![property_access(this_expr(), "path")],
            ),
        ),
        assign_stmt(
            "sigType",
            function_call(
                "__elephc_phar_get_signature_type",
                vec![property_access(this_expr(), "path")],
            ),
        ),
        return_stmt(expr(ExprKind::ArrayLiteralAssoc(vec![
            (string_expr("hash"), var_expr("sigHash")),
            (string_expr("hash_type"), var_expr("sigType")),
        ]))),
    ]
}

/// Builds `setStub()` as per-object storage plus a write-through to the archive.
fn phar_set_stub_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        property_assign_stmt(this_expr(), "stub", var_expr("stub")),
        return_stmt(function_call(
            "__elephc_phar_set_stub",
            vec![property_access(this_expr(), "path"), var_expr("stub")],
        )),
    ]
}

/// Builds `rewind()` by resetting the object-local entry position.
fn phar_rewind_body() -> Vec<crate::parser::ast::Stmt> {
    vec![property_assign_stmt(this_expr(), "position", int_expr(0))]
}

/// Builds `next()` by advancing the object-local entry position.
fn phar_next_body() -> Vec<crate::parser::ast::Stmt> {
    vec![property_assign_stmt(
        this_expr(),
        "position",
        binary_expr(phar_position_expr(), BinOp::Add, int_expr(1)),
    )]
}

/// Builds `valid()` over the tracked entry-name list.
fn phar_valid_body() -> Vec<crate::parser::ast::Stmt> {
    return_body(binary_expr(
        phar_position_expr(),
        BinOp::Lt,
        count_expr(phar_entries_expr()),
    ))
}

/// Builds `key()` as the current tracked entry name.
fn phar_key_body() -> Vec<crate::parser::ast::Stmt> {
    return_body(phar_entry_at_position_expr())
}

/// Builds `current()` as a `PharFileInfo` for the current tracked entry.
fn phar_current_body() -> Vec<crate::parser::ast::Stmt> {
    return_body(new_object_expr(
        "PharFileInfo",
        vec![phar_entry_url_expr(phar_entry_at_position_expr())],
    ))
}

/// Builds `count()` over the tracked entry-name list.
fn phar_count_body() -> Vec<crate::parser::ast::Stmt> {
    return_body(count_expr(phar_entries_expr()))
}

/// Builds `offsetExists()` as a `file_get_contents()` false check.
fn phar_offset_exists_body() -> Vec<crate::parser::ast::Stmt> {
    return_body(binary_expr(
        function_call("file_get_contents", vec![phar_entry_url_expr(var_expr("offset"))]),
        BinOp::StrictNotEq,
        bool_expr(false),
    ))
}

/// Builds `offsetGet()` as a `PharFileInfo` entry object.
fn phar_offset_get_body() -> Vec<crate::parser::ast::Stmt> {
    return_body(new_object_expr(
        "PharFileInfo",
        vec![phar_entry_url_expr(var_expr("offset"))],
    ))
}

/// Builds `addFromString()` as a typed `file_put_contents()` archive write.
fn phar_add_from_string_body() -> Vec<crate::parser::ast::Stmt> {
    let mut body = vec![expr_stmt(function_call(
        "file_put_contents",
        vec![phar_entry_url_expr(var_expr("localName")), var_expr("contents")],
    ))];
    body.extend(phar_track_entry_body(var_expr("localName")));
    body
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

/// Builds `compress()` as a whole-archive gzip/bzip2 rewrite: produces a new
/// `<base>.gz` / `<base>.bz2` file and returns a fresh `PharData` for it (`null` on
/// failure or an unsupported compression constant). `Phar::GZ` = 4096, `Phar::BZ2` = 8192.
fn phar_compress_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        assign_stmt("dest", string_expr("")),
        if_stmt(
            binary_expr(var_expr("compression"), BinOp::StrictEq, int_expr(4096)),
            vec![assign_stmt(
                "dest",
                function_call(
                    "__elephc_phar_gzip_archive",
                    vec![property_access(this_expr(), "path")],
                ),
            )],
            Some(vec![if_stmt(
                binary_expr(var_expr("compression"), BinOp::StrictEq, int_expr(8192)),
                vec![assign_stmt(
                    "dest",
                    function_call(
                        "__elephc_phar_bzip2_archive",
                        vec![property_access(this_expr(), "path")],
                    ),
                )],
                None,
            )]),
        ),
        if_stmt(
            binary_expr(var_expr("dest"), BinOp::StrictEq, string_expr("")),
            vec![throw_stmt(new_object_expr(
                "RuntimeException",
                vec![string_expr("Unable to compress phar archive")],
            ))],
            None,
        ),
        return_stmt(new_object_expr("PharData", vec![var_expr("dest")])),
    ]
}

/// Builds `decompress()` as a whole-archive decompression: produces the plain
/// `<base>` archive (stripping a `.gz`/`.bz2` suffix) and returns a fresh `PharData`
/// for it (`null` when the archive is not compressed or the write fails).
fn phar_decompress_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        assign_stmt(
            "dest",
            function_call(
                "__elephc_phar_decompress_archive",
                vec![property_access(this_expr(), "path")],
            ),
        ),
        if_stmt(
            binary_expr(var_expr("dest"), BinOp::StrictEq, string_expr("")),
            vec![throw_stmt(new_object_expr(
                "RuntimeException",
                vec![string_expr("Unable to decompress phar archive")],
            ))],
            None,
        ),
        return_stmt(new_object_expr("PharData", vec![var_expr("dest")])),
    ]
}

/// Builds `delete()` as an archive-entry `unlink()`.
fn phar_delete_body() -> Vec<crate::parser::ast::Stmt> {
    let mut body = vec![assign_stmt(
        "deleted",
        function_call("unlink", vec![phar_entry_url_expr(var_expr("localName"))]),
    )];
    body.push(if_stmt(
        var_expr("deleted"),
        phar_forget_entry_body(var_expr("localName")),
        None,
    ));
    body.push(return_stmt(var_expr("deleted")));
    body
}

/// Builds `offsetSet()` as a `file_put_contents()` write.
fn phar_offset_set_body() -> Vec<crate::parser::ast::Stmt> {
    let mut body = vec![expr_stmt(function_call(
        "file_put_contents",
        vec![phar_entry_url_expr(var_expr("offset")), var_expr("value")],
    ))];
    body.extend(phar_track_entry_body(var_expr("offset")));
    body
}

/// Builds `offsetUnset()` as an archive-entry `unlink()`.
fn phar_offset_unset_body() -> Vec<crate::parser::ast::Stmt> {
    let mut body = vec![expr_stmt(function_call(
        "unlink",
        vec![phar_entry_url_expr(var_expr("offset"))],
    ))];
    body.extend(phar_forget_entry_body(var_expr("offset")));
    body
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

/// Builds statements that append one entry name to the object-local iterator list once.
fn phar_track_entry_body(entry: Expr) -> Vec<crate::parser::ast::Stmt> {
    vec![
        assign_stmt("entryName", cast_expr(CastType::String, entry)),
        assign_stmt("seen", bool_expr(false)),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(phar_entries_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(
                        phar_entry_at_expr(var_expr("i")),
                        BinOp::StrictEq,
                        var_expr("entryName"),
                    ),
                    vec![assign_stmt("seen", bool_expr(true))],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        if_stmt(
            not_expr(var_expr("seen")),
            vec![property_array_push_stmt(this_expr(), "entries", var_expr("entryName"))],
            None,
        ),
    ]
}

/// Builds statements that remove one entry name from the object-local iterator list.
fn phar_forget_entry_body(entry: Expr) -> Vec<crate::parser::ast::Stmt> {
    vec![
        assign_stmt("entryName", cast_expr(CastType::String, entry)),
        assign_stmt("newEntries", empty_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(phar_entries_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    not_expr(binary_expr(
                        phar_entry_at_expr(var_expr("i")),
                        BinOp::StrictEq,
                        var_expr("entryName"),
                    )),
                    vec![array_push_stmt("newEntries", phar_entry_at_expr(var_expr("i")))],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        property_assign_stmt(this_expr(), "entries", var_expr("newEntries")),
    ]
}

/// Returns the archive path stored by the constructor.
fn phar_path_expr() -> Expr {
    property_access(this_expr(), "path")
}

/// Returns the object-local tracked entry-name list.
fn phar_entries_expr() -> Expr {
    property_access(this_expr(), "entries")
}

/// Returns the current object-local iterator position.
fn phar_position_expr() -> Expr {
    property_access(this_expr(), "position")
}

/// Returns the tracked entry name at an arbitrary position.
fn phar_entry_at_expr(index: Expr) -> Expr {
    array_access(phar_entries_expr(), index)
}

/// Returns the tracked entry name at the current iterator position.
fn phar_entry_at_position_expr() -> Expr {
    phar_entry_at_expr(phar_position_expr())
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
