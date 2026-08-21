//! Purpose:
//! Injects file and directory SPL iterator metadata and synthetic method bodies.
//! Builds Phase 8 filesystem classes from existing path, stat, stream, scandir, and glob builtins.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Directory iterators snapshot entry names into array-backed iterator state.
//! - SplFileObject iterates a file through the existing `file()` line-array builtin.
//! - Recursive wrappers keep typed recursive-inner storage for child traversal.

use std::collections::HashMap;

use crate::parser::ast::{
    BinOp, CastType, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, Stmt, TypeExpr,
    Visibility,
};
use crate::types::traits::FlattenedClass;

use super::common::*;

const SPL_FILE_DROP_NEW_LINE: i64 = 1;
const SPL_FILE_READ_AHEAD: i64 = 2;
const SPL_FILE_SKIP_EMPTY: i64 = 4;
const SPL_FILE_READ_CSV: i64 = 8;

const FS_CURRENT_AS_FILEINFO: i64 = 0;
const FS_CURRENT_AS_SELF: i64 = 16;
const FS_CURRENT_AS_PATHNAME: i64 = 32;
const FS_CURRENT_MODE_MASK: i64 = 240;
const FS_KEY_AS_PATHNAME: i64 = 0;
const FS_KEY_AS_FILENAME: i64 = 256;
const FS_KEY_MODE_MASK: i64 = 3840;
const FS_NEW_CURRENT_AND_KEY: i64 = 256;
const FS_SKIP_DOTS: i64 = 4096;
const FS_UNIX_PATHS: i64 = 8192;
const FS_FOLLOW_SYMLINKS: i64 = 16384;

/// Inserts Phase 8 filesystem SPL classes into the supplied metadata registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "SplFileInfo".to_string(),
        FlattenedClass {
            name: "SplFileInfo".to_string(),
            span: crate::span::Span::dummy(),
            extends: None,
            implements: vec!["Stringable".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: spl_file_info_properties(),
            methods: spl_file_info_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "SplFileObject".to_string(),
        FlattenedClass {
            name: "SplFileObject".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("SplFileInfo".to_string()),
            implements: vec!["RecursiveIterator".to_string(), "SeekableIterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: spl_file_object_properties(),
            methods: spl_file_object_methods(),
            attributes: Vec::new(),
            constants: spl_file_object_constants(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "SplTempFileObject".to_string(),
        FlattenedClass {
            name: "SplTempFileObject".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("SplFileObject".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: spl_temp_file_object_properties(),
            methods: spl_temp_file_object_methods(),
            attributes: Vec::new(),
            constants: spl_file_object_constants(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "DirectoryIterator".to_string(),
        FlattenedClass {
            name: "DirectoryIterator".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("SplFileInfo".to_string()),
            implements: vec!["Iterator".to_string(), "SeekableIterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: directory_iterator_properties(),
            methods: directory_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "FilesystemIterator".to_string(),
        FlattenedClass {
            name: "FilesystemIterator".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("DirectoryIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: filesystem_iterator_methods(),
            attributes: Vec::new(),
            constants: filesystem_iterator_constants(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "GlobIterator".to_string(),
        FlattenedClass {
            name: "GlobIterator".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("FilesystemIterator".to_string()),
            implements: vec!["Countable".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: glob_iterator_properties(),
            methods: glob_iterator_methods(),
            attributes: Vec::new(),
            constants: filesystem_iterator_constants(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "RecursiveDirectoryIterator".to_string(),
        FlattenedClass {
            name: "RecursiveDirectoryIterator".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("FilesystemIterator".to_string()),
            implements: vec!["RecursiveIterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: recursive_directory_iterator_methods(),
            attributes: Vec::new(),
            constants: filesystem_iterator_constants(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "RecursiveCachingIterator".to_string(),
        FlattenedClass {
            name: "RecursiveCachingIterator".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("CachingIterator".to_string()),
            implements: vec!["RecursiveIterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: recursive_caching_iterator_properties(),
            methods: recursive_caching_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
}

/// Builds shared filesystem iterator constants.
fn filesystem_iterator_constants() -> Vec<ClassConst> {
    vec![
        class_const("CURRENT_AS_PATHNAME", FS_CURRENT_AS_PATHNAME),
        class_const("CURRENT_AS_FILEINFO", FS_CURRENT_AS_FILEINFO),
        class_const("CURRENT_AS_SELF", FS_CURRENT_AS_SELF),
        class_const("CURRENT_MODE_MASK", FS_CURRENT_MODE_MASK),
        class_const("KEY_AS_PATHNAME", FS_KEY_AS_PATHNAME),
        class_const("KEY_AS_FILENAME", FS_KEY_AS_FILENAME),
        class_const("KEY_MODE_MASK", FS_KEY_MODE_MASK),
        class_const("NEW_CURRENT_AND_KEY", FS_NEW_CURRENT_AND_KEY),
        class_const("SKIP_DOTS", FS_SKIP_DOTS),
        class_const("UNIX_PATHS", FS_UNIX_PATHS),
        class_const("FOLLOW_SYMLINKS", FS_FOLLOW_SYMLINKS),
    ]
}

/// Builds SplFileObject line-reading constants.
fn spl_file_object_constants() -> Vec<ClassConst> {
    vec![
        class_const("DROP_NEW_LINE", SPL_FILE_DROP_NEW_LINE),
        class_const("READ_AHEAD", SPL_FILE_READ_AHEAD),
        class_const("SKIP_EMPTY", SPL_FILE_SKIP_EMPTY),
        class_const("READ_CSV", SPL_FILE_READ_CSV),
    ]
}

/// Builds SplFileInfo storage properties.
fn spl_file_info_properties() -> Vec<ClassProperty> {
    vec![
        protected_storage_property("path", TypeExpr::Str),
        protected_storage_property("fileClass", TypeExpr::Str),
        protected_storage_property("infoClass", TypeExpr::Str),
    ]
}

/// Builds SplFileObject storage properties.
fn spl_file_object_properties() -> Vec<ClassProperty> {
    vec![
        protected_storage_property("backingPath", TypeExpr::Str),
        protected_storage_property("stream", mixed_type()),
        protected_storage_property("lines", array_type()),
        protected_storage_property("lineNumber", TypeExpr::Int),
        protected_storage_property("flags", TypeExpr::Int),
        protected_storage_property("delimiter", TypeExpr::Str),
        protected_storage_property("enclosure", TypeExpr::Str),
        protected_storage_property("escape", TypeExpr::Str),
        protected_storage_property("maxLineLen", TypeExpr::Int),
        // READ_CSV records, and whether they still describe `lines` under the current controls.
        // A RECORD is not a line: a quoted field may hold newlines, so one record can span
        // several entries of `lines`, and the mapping has to be built once rather than guessed
        // per `current()` call.
        protected_storage_property("csvRecords", array_type()),
        // 1 for a record that came from a line holding nothing but its terminator. SKIP_EMPTY
        // (with DROP_NEW_LINE) steps OVER those without renumbering the rest, so the flag is
        // recorded per record rather than the record being dropped.
        protected_storage_property("csvBlank", array_type()),
        // Whether any line has been consumed through `fgets()`/`fscanf()` yet. It exists for
        // `fscanf()` alone: measured on `php -n` 8.5.6, the FIRST `fscanf()` of a fresh object
        // leaves `key()` at 0 while every later one advances it, so on a three-line file the
        // keys run 0, 1, 2 where `fgets()` gives 1, 2, 3. Mixing the two confirms it is the
        // first READ that is special and not the method: `fgets()` then `fscanf()` gives 1
        // then 2.
        protected_storage_property("hasReadLine", TypeExpr::Bool),
    ]
}

/// Builds SplTempFileObject-only storage properties.
fn spl_temp_file_object_properties() -> Vec<ClassProperty> {
    vec![
        protected_storage_property("tempMaxMemory", TypeExpr::Int),
        protected_storage_property("tempBuffer", TypeExpr::Str),
        protected_storage_property("tempPosition", TypeExpr::Int),
        protected_storage_property("tempSpilled", TypeExpr::Bool),
    ]
}

/// Builds directory iterator storage properties shared by directory subclasses.
fn directory_iterator_properties() -> Vec<ClassProperty> {
    vec![
        protected_storage_property("directory", TypeExpr::Str),
        protected_storage_property("entries", array_type()),
        protected_storage_property("position", TypeExpr::Int),
        protected_storage_property("fsFlags", TypeExpr::Int),
        protected_storage_property("entriesArePathnames", TypeExpr::Bool),
    ]
}

/// Builds GlobIterator storage properties.
fn glob_iterator_properties() -> Vec<ClassProperty> {
    vec![protected_storage_property("pattern", TypeExpr::Str)]
}

/// Builds RecursiveCachingIterator storage properties.
fn recursive_caching_iterator_properties() -> Vec<ClassProperty> {
    vec![storage_property("recursiveInner", named_type("RecursiveIterator"))]
}

/// Builds SplFileInfo methods.
fn spl_file_info_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("filename", TypeExpr::Str)],
            Some(TypeExpr::Void),
            spl_file_info_construct_body(),
        ),
        method_with_body("__toString", Vec::new(), Some(TypeExpr::Str), return_body(file_path_expr())),
        method_with_body("getPath", Vec::new(), Some(TypeExpr::Str), return_body(function_call("dirname", vec![file_path_arg_expr()]))),
        method_with_body("getFilename", Vec::new(), Some(TypeExpr::Str), return_body(function_call("basename", vec![file_path_arg_expr()]))),
        method_with_body(
            "getExtension",
            Vec::new(),
            Some(TypeExpr::Str),
            return_body(function_call("pathinfo", vec![file_path_arg_expr(), int_expr(4)])),
        ),
        method_with_body(
            "getBasename",
            vec![param_default("suffix", TypeExpr::Str, string_expr(""))],
            Some(TypeExpr::Str),
            return_body(function_call("basename", vec![file_path_arg_expr(), var_expr("suffix")])),
        ),
        method_with_body("getPathname", Vec::new(), Some(TypeExpr::Str), return_body(file_path_expr())),
        method_with_body("getPerms", Vec::new(), Some(mixed_type()), return_body(function_call("fileperms", vec![file_path_arg_expr()]))),
        method_with_body("getInode", Vec::new(), Some(mixed_type()), return_body(function_call("fileinode", vec![file_path_arg_expr()]))),
        method_with_body("getSize", Vec::new(), Some(TypeExpr::Int), return_body(function_call("filesize", vec![file_path_arg_expr()]))),
        method_with_body("getOwner", Vec::new(), Some(mixed_type()), return_body(function_call("fileowner", vec![file_path_arg_expr()]))),
        method_with_body("getGroup", Vec::new(), Some(mixed_type()), return_body(function_call("filegroup", vec![file_path_arg_expr()]))),
        method_with_body("getATime", Vec::new(), Some(mixed_type()), return_body(function_call("fileatime", vec![file_path_arg_expr()]))),
        method_with_body("getMTime", Vec::new(), Some(TypeExpr::Int), return_body(function_call("filemtime", vec![file_path_arg_expr()]))),
        method_with_body("getCTime", Vec::new(), Some(mixed_type()), return_body(function_call("filectime", vec![file_path_arg_expr()]))),
        method_with_body("getType", Vec::new(), Some(mixed_type()), return_body(function_call("filetype", vec![file_path_arg_expr()]))),
        method_with_body("isWritable", Vec::new(), Some(TypeExpr::Bool), return_body(function_call("is_writable", vec![file_path_arg_expr()]))),
        method_with_body("isWriteable", Vec::new(), Some(TypeExpr::Bool), return_body(function_call("is_writeable", vec![file_path_arg_expr()]))),
        method_with_body("isReadable", Vec::new(), Some(TypeExpr::Bool), return_body(function_call("is_readable", vec![file_path_arg_expr()]))),
        method_with_body("isExecutable", Vec::new(), Some(TypeExpr::Bool), return_body(function_call("is_executable", vec![file_path_arg_expr()]))),
        method_with_body("isFile", Vec::new(), Some(TypeExpr::Bool), return_body(function_call("is_file", vec![file_path_arg_expr()]))),
        method_with_body("isDir", Vec::new(), Some(TypeExpr::Bool), return_body(function_call("is_dir", vec![file_path_arg_expr()]))),
        method_with_body("isLink", Vec::new(), Some(TypeExpr::Bool), return_body(function_call("is_link", vec![file_path_arg_expr()]))),
        method_with_body("getLinkTarget", Vec::new(), Some(mixed_type()), return_body(function_call("readlink", vec![file_path_arg_expr()]))),
        method_with_body("getRealPath", Vec::new(), Some(mixed_type()), return_body(function_call("realpath", vec![file_path_arg_expr()]))),
        method_with_body(
            "getFileInfo",
            vec![param_default("class", TypeExpr::Nullable(Box::new(TypeExpr::Str)), null_expr())],
            Some(named_type("SplFileInfo")),
            return_body(new_dynamic_object_expr(
                null_coalesce_expr(var_expr("class"), property_access(this_expr(), "infoClass")),
                "SplFileInfo",
                "SplFileInfo",
                vec![file_path_arg_expr()],
            )),
        ),
        method_with_body(
            "getPathInfo",
            vec![param_default("class", TypeExpr::Nullable(Box::new(TypeExpr::Str)), null_expr())],
            Some(named_type("SplFileInfo")),
            return_body(new_dynamic_object_expr(
                null_coalesce_expr(var_expr("class"), property_access(this_expr(), "infoClass")),
                "SplFileInfo",
                "SplFileInfo",
                vec![function_call("dirname", vec![file_path_arg_expr()])],
            )),
        ),
        method_with_body(
            "openFile",
            vec![
                param_default("mode", TypeExpr::Str, string_expr("r")),
                param_default("useIncludePath", TypeExpr::Bool, bool_expr(false)),
                param_default("context", mixed_type(), null_expr()),
            ],
            Some(named_type("SplFileObject")),
            return_body(new_dynamic_object_expr(
                property_access(this_expr(), "fileClass"),
                "SplFileObject",
                "SplFileObject",
                vec![
                    file_path_arg_expr(),
                    var_expr("mode"),
                    var_expr("useIncludePath"),
                    var_expr("context"),
                ],
            )),
        ),
        method_with_body(
            "setFileClass",
            vec![param_default("class", TypeExpr::Str, string_expr("SplFileObject"))],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "fileClass", var_expr("class"))],
        ),
        method_with_body(
            "setInfoClass",
            vec![param_default("class", TypeExpr::Str, string_expr("SplFileInfo"))],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "infoClass", var_expr("class"))],
        ),
    ]
}

/// Builds SplFileObject methods.
fn spl_file_object_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("filename", TypeExpr::Str),
                param_default("mode", TypeExpr::Str, string_expr("r")),
                param_default("useIncludePath", TypeExpr::Bool, bool_expr(false)),
                param_default("context", mixed_type(), null_expr()),
            ],
            Some(TypeExpr::Void),
            spl_file_object_construct_body(var_expr("filename"), var_expr("mode")),
        ),
        method_with_body("current", Vec::new(), Some(mixed_type()), spl_file_object_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), return_body(file_line_number_expr())),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), spl_file_object_next_body()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), spl_file_object_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), spl_file_object_valid_body()),
        method_with_body("eof", Vec::new(), Some(TypeExpr::Bool), return_body(function_call("feof", vec![file_stream_expr()]))),
        method_with_body("fgets", Vec::new(), Some(mixed_type()), spl_file_object_fgets_body()),
        method_with_body(
            "fscanf",
            vec![param("format", TypeExpr::Str)],
            Some(mixed_type()),
            spl_file_object_fscanf_body(),
        ),
        // php documents `getCurrentLine()` as an ALIAS of `fgets()`, and it behaves like one:
        // measured on `php -n` 8.5.6 over `"aa\nbb\n"`, it CONSUMES the line, advances
        // `key()`, and throws once `feof()` holds. Answering the cached current line instead
        // left the stream where it was, so `getCurrentLine()` then `fgetc()` read `a` where
        // php reads `b`.
        method_with_body("getCurrentLine", Vec::new(), Some(mixed_type()), spl_file_object_fgets_body()),
        method_with_body("fgetc", Vec::new(), Some(mixed_type()), return_body(function_call("fgetc", vec![file_stream_expr()]))),
        method_with_body(
            "fread",
            vec![param("length", TypeExpr::Int)],
            Some(TypeExpr::Str),
            return_body(function_call("fread", vec![file_stream_expr(), var_expr("length")])),
        ),
        method_with_body("fwrite", vec![param("data", TypeExpr::Str)], Some(TypeExpr::Int), spl_file_object_fwrite_body()),
        method_with_body("fflush", Vec::new(), Some(TypeExpr::Bool), return_body(function_call("fflush", vec![file_stream_expr()]))),
        method_with_body("flock", vec![param("operation", TypeExpr::Int)], Some(TypeExpr::Bool), return_body(function_call("flock", vec![file_stream_expr(), var_expr("operation")]))),
        method_with_body("ftruncate", vec![param("size", TypeExpr::Int)], Some(TypeExpr::Bool), spl_file_object_ftruncate_body()),
        method_with_body("fstat", Vec::new(), Some(mixed_type()), return_body(function_call("fstat", vec![file_stream_expr()]))),
        method_with_body("ftell", Vec::new(), Some(TypeExpr::Int), return_body(function_call("ftell", vec![file_stream_expr()]))),
        method_with_body(
            "fseek",
            vec![
                param("offset", TypeExpr::Int),
                param_default("whence", TypeExpr::Int, int_expr(0)),
            ],
            Some(TypeExpr::Int),
            spl_file_object_fseek_body(),
        ),
        method_with_body("seek", vec![param("line", TypeExpr::Int)], Some(TypeExpr::Void), vec![property_assign_stmt(this_expr(), "lineNumber", var_expr("line"))]),
        method_with_body("getFlags", Vec::new(), Some(TypeExpr::Int), return_body(file_object_flags_expr())),
        method_with_body(
            "setFlags",
            vec![param("flags", TypeExpr::Int)],
            Some(TypeExpr::Void),
            vec![
                property_assign_stmt(this_expr(), "flags", var_expr("flags")),
                spl_file_object_csv_refresh_stmt(),
            ],
        ),
        method_with_body("getMaxLineLen", Vec::new(), Some(TypeExpr::Int), return_body(property_access(this_expr(), "maxLineLen"))),
        method_with_body("setMaxLineLen", vec![param("maxLength", TypeExpr::Int)], Some(TypeExpr::Void), vec![property_assign_stmt(this_expr(), "maxLineLen", var_expr("maxLength"))]),
        method_with_body(
            "setCsvControl",
            vec![
                param_default("separator", TypeExpr::Str, string_expr(",")),
                param_default("enclosure", TypeExpr::Str, string_expr("\"")),
                param_default("escape", TypeExpr::Str, string_expr("\\")),
            ],
            Some(TypeExpr::Void),
            spl_file_object_set_csv_control_body(),
        ),
        method_with_body("getCsvControl", Vec::new(), Some(array_type()), spl_file_object_get_csv_control_body()),
        // Compiler-synthesized READ_CSV helpers; not part of php's SplFileObject surface.
        protected_method_with_body(
            "__elephcCsvBuild",
            Vec::new(),
            Some(TypeExpr::Void),
            spl_file_object_csv_build_body(),
        ),
        protected_method_with_body(
            "__elephcCsvSkipBlank",
            Vec::new(),
            Some(TypeExpr::Void),
            spl_file_object_csv_skip_blank_body(),
        ),
        // The three controls default to NULL rather than to `","` / `'"'` / `"\\"`, because
        // php resolves an omitted one against the object's own `setCsvControl()` state, not
        // against a literal: `$f->setCsvControl(";"); $f->fgetcsv()` splits on `;`.
        method_with_body(
            "fgetcsv",
            vec![
                param_default("separator", nullable_string_type(), null_expr()),
                param_default("enclosure", nullable_string_type(), null_expr()),
                param_default("escape", nullable_string_type(), null_expr()),
            ],
            Some(mixed_type()),
            spl_file_object_fgetcsv_body(),
        ),
        method_with_body(
            "fputcsv",
            vec![
                param("fields", string_array_type()),
                param_default("separator", nullable_string_type(), null_expr()),
                param_default("enclosure", nullable_string_type(), null_expr()),
                param_default("escape", nullable_string_type(), null_expr()),
                param_default("eol", TypeExpr::Str, string_expr("\n")),
            ],
            Some(TypeExpr::Int),
            spl_file_object_fputcsv_body(),
        ),
        method_with_body("hasChildren", Vec::new(), Some(TypeExpr::Bool), return_body(bool_expr(false))),
        method_with_body(
            "getChildren",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            null_return_body(),
        ),
    ]
}

/// Builds SplTempFileObject methods.
fn spl_temp_file_object_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param_default("maxMemory", TypeExpr::Int, int_expr(2_097_152))],
            Some(TypeExpr::Void),
            spl_temp_file_object_construct_body(),
        ),
        method_with_body("eof", Vec::new(), Some(TypeExpr::Bool), spl_temp_file_object_eof_body()),
        method_with_body("fgets", Vec::new(), Some(mixed_type()), spl_temp_file_object_fgets_body()),
        method_with_body("fgetc", Vec::new(), Some(mixed_type()), spl_temp_file_object_fgetc_body()),
        method_with_body(
            "fread",
            vec![param("length", TypeExpr::Int)],
            Some(TypeExpr::Str),
            spl_temp_file_object_fread_body(),
        ),
        method_with_body("fwrite", vec![param("data", TypeExpr::Str)], Some(TypeExpr::Int), spl_temp_file_object_fwrite_body()),
        method_with_body("fflush", Vec::new(), Some(TypeExpr::Bool), spl_temp_file_object_fflush_body()),
        method_with_body("ftruncate", vec![param("size", TypeExpr::Int)], Some(TypeExpr::Bool), spl_temp_file_object_ftruncate_body()),
        method_with_body("fstat", Vec::new(), Some(mixed_type()), spl_temp_file_object_fstat_body()),
        method_with_body("ftell", Vec::new(), Some(TypeExpr::Int), spl_temp_file_object_ftell_body()),
        method_with_body(
            "fseek",
            vec![
                param("offset", TypeExpr::Int),
                param_default("whence", TypeExpr::Int, int_expr(0)),
            ],
            Some(TypeExpr::Int),
            spl_temp_file_object_fseek_body(),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), spl_temp_file_object_rewind_body()),
        protected_method_with_body(
            "__elephcSpillToFile",
            Vec::new(),
            Some(TypeExpr::Void),
            spl_temp_file_object_spill_body(),
        ),
    ]
}

/// Builds DirectoryIterator methods.
fn directory_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("directory", TypeExpr::Str)],
            Some(TypeExpr::Void),
            directory_construct_body(var_expr("directory"), int_expr(0), false, false),
        ),
        method_with_body(
            "current",
            Vec::new(),
            Some(mixed_type()),
            return_body(this_expr()),
        ),
        method_with_body("key", Vec::new(), Some(mixed_type()), return_body(directory_position_expr())),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), directory_next_body()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), directory_rewind_body()),
        method_with_body("seek", vec![param("offset", TypeExpr::Int)], Some(TypeExpr::Void), directory_seek_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), directory_valid_body()),
        method_with_body("isDot", Vec::new(), Some(TypeExpr::Bool), return_body(directory_is_dot_expr())),
        method_with_body("__toString", Vec::new(), Some(TypeExpr::Str), return_body(function_call("basename", vec![file_path_arg_expr()]))),
        protected_method_with_body("__elephcRefreshPath", Vec::new(), Some(TypeExpr::Void), directory_refresh_path_body()),
    ]
}

/// Builds FilesystemIterator methods.
fn filesystem_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("directory", TypeExpr::Str),
                param_default("flags", TypeExpr::Int, int_expr(FS_SKIP_DOTS)),
            ],
            Some(TypeExpr::Void),
            directory_construct_body(var_expr("directory"), var_expr("flags"), true, false),
        ),
        method_with_body("current", Vec::new(), Some(mixed_type()), filesystem_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), filesystem_key_body()),
        method_with_body("getFlags", Vec::new(), Some(TypeExpr::Int), return_body(filesystem_flags_expr())),
        method_with_body("setFlags", vec![param("flags", TypeExpr::Int)], Some(TypeExpr::Void), filesystem_set_flags_body()),
    ]
}

/// Builds GlobIterator methods.
fn glob_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("pattern", TypeExpr::Str),
                param_default("flags", TypeExpr::Int, int_expr(FS_CURRENT_AS_FILEINFO)),
            ],
            Some(TypeExpr::Void),
            glob_iterator_construct_body(),
        ),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), return_body(count_expr(directory_entries_expr()))),
        method_with_body("setFlags", vec![param("flags", TypeExpr::Int)], Some(TypeExpr::Void), vec![property_assign_stmt(this_expr(), "fsFlags", var_expr("flags"))]),
    ]
}

/// Builds RecursiveDirectoryIterator methods.
fn recursive_directory_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("directory", TypeExpr::Str),
                param_default("flags", TypeExpr::Int, int_expr(FS_CURRENT_AS_FILEINFO)),
            ],
            Some(TypeExpr::Void),
            directory_construct_body(var_expr("directory"), var_expr("flags"), true, false),
        ),
        method_with_body("hasChildren", Vec::new(), Some(TypeExpr::Bool), recursive_directory_has_children_body()),
        method_with_body(
            "getChildren",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            recursive_directory_get_children_body(),
        ),
    ]
}

/// Builds RecursiveCachingIterator methods.
fn recursive_caching_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", named_type("RecursiveIterator")),
                param_default("flags", TypeExpr::Int, int_expr(1)),
            ],
            Some(TypeExpr::Void),
            recursive_caching_construct_body(),
        ),
        method_with_body("hasChildren", Vec::new(), Some(TypeExpr::Bool), recursive_caching_has_children_body()),
        method_with_body(
            "getChildren",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            recursive_caching_get_children_body(),
        ),
        method_with_body(
            "__elephcAssumeRecursiveIterator",
            vec![param("iterator", mixed_type())],
            Some(named_type("RecursiveIterator")),
            return_body(var_expr("iterator")),
        ),
    ]
}

/// Builds a protected synthetic method.
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

/// Returns `$this->path`.
fn file_path_expr() -> Expr {
    property_access(this_expr(), "path")
}

/// Returns a copied `$this->path` for filesystem builtins that may consume string temporaries.
fn file_path_arg_expr() -> Expr {
    string_copy_expr(file_path_expr())
}

/// Returns `$this->backingPath`.
fn file_backing_path_expr() -> Expr {
    property_access(this_expr(), "backingPath")
}

/// Returns a copied `$this->backingPath` for stream-backed file storage.
fn file_backing_path_arg_expr() -> Expr {
    string_copy_expr(file_backing_path_expr())
}

/// Builds a string copy expression by concatenating an empty string.
fn string_copy_expr(value: Expr) -> Expr {
    binary_expr(value, BinOp::Concat, string_expr(""))
}

/// Returns the `?string` type the CSV control parameters declare.
///
/// The null is not a value a caller passes for its own sake: it is how an OMITTED control is
/// told apart from a spelled one, so the body can fall back on `setCsvControl()` state.
fn nullable_string_type() -> TypeExpr {
    TypeExpr::Nullable(Box::new(TypeExpr::Str))
}

/// Returns `$this->lines`.
fn file_lines_expr() -> Expr {
    property_access(this_expr(), "lines")
}

/// Returns `$this->lineNumber`.
fn file_line_number_expr() -> Expr {
    property_access(this_expr(), "lineNumber")
}

/// Returns `$this->flags` for SplFileObject.
fn file_object_flags_expr() -> Expr {
    property_access(this_expr(), "flags")
}

/// Returns `$this->stream` for SplFileObject.
fn file_stream_expr() -> Expr {
    property_access(this_expr(), "stream")
}

/// Returns `$this->tempMaxMemory` for SplTempFileObject.
fn temp_max_memory_expr() -> Expr {
    property_access(this_expr(), "tempMaxMemory")
}

/// Returns `$this->tempBuffer` for SplTempFileObject.
fn temp_buffer_expr() -> Expr {
    property_access(this_expr(), "tempBuffer")
}

/// Returns a copied `$this->tempBuffer` for string builtins that may consume temporaries.
fn temp_buffer_arg_expr() -> Expr {
    string_copy_expr(temp_buffer_expr())
}

/// Returns `$this->tempPosition` for SplTempFileObject.
fn temp_position_expr() -> Expr {
    property_access(this_expr(), "tempPosition")
}

/// Returns `$this->tempSpilled` for SplTempFileObject.
fn temp_spilled_expr() -> Expr {
    property_access(this_expr(), "tempSpilled")
}

/// Returns `$this->directory`.
fn directory_path_expr() -> Expr {
    property_access(this_expr(), "directory")
}

/// Returns `$this->entries`.
fn directory_entries_expr() -> Expr {
    property_access(this_expr(), "entries")
}

/// Returns `$this->position`.
fn directory_position_expr() -> Expr {
    property_access(this_expr(), "position")
}

/// Returns `$this->fsFlags`.
fn filesystem_flags_expr() -> Expr {
    property_access(this_expr(), "fsFlags")
}

/// Returns `$this->entriesArePathnames`.
fn entries_are_pathnames_expr() -> Expr {
    property_access(this_expr(), "entriesArePathnames")
}

/// Returns the directory entry at the current position.
fn directory_current_entry_expr() -> Expr {
    array_access(directory_entries_expr(), directory_position_expr())
}

/// Builds `$directory . "/" . $entry`.
fn path_join_expr(directory: Expr, entry: Expr) -> Expr {
    binary_expr(binary_expr(directory, BinOp::Concat, string_expr("/")), BinOp::Concat, entry)
}

/// Tests whether a flag bit is set in `flags`.
fn flag_enabled_expr(flags: Expr, bit: i64) -> Expr {
    binary_expr(
        binary_expr(flags, BinOp::BitAnd, int_expr(bit)),
        BinOp::NotEq,
        int_expr(0),
    )
}

/// Tests whether a flag mask resolves to `value`.
fn flag_mode_is_expr(flags: Expr, mask: i64, value: i64) -> Expr {
    binary_expr(
        binary_expr(flags, BinOp::BitAnd, int_expr(mask)),
        BinOp::StrictEq,
        int_expr(value),
    )
}

/// Tests whether `entry` is not "." or "..".
fn not_dot_name_expr(entry: Expr) -> Expr {
    binary_expr(
        binary_expr(entry.clone(), BinOp::StrictNotEq, string_expr(".")),
        BinOp::And,
        binary_expr(entry, BinOp::StrictNotEq, string_expr("..")),
    )
}

/// Tests whether the current directory entry is a dot entry.
fn directory_is_dot_expr() -> Expr {
    not_expr(not_dot_name_expr(function_call("basename", vec![file_path_arg_expr()])))
}

/// Returns the current file line expression.
fn file_current_line_expr() -> Expr {
    array_access(file_lines_expr(), file_line_number_expr())
}

/// Returns true when the file object is positioned at a valid line.
fn file_object_valid_expr() -> Expr {
    binary_expr(file_line_number_expr(), BinOp::Lt, count_expr(file_lines_expr()))
}

/// Tests whether an expression has PHP runtime type "array".
fn gettype_is_array_expr(value: Expr) -> Expr {
    binary_expr(
        function_call("gettype", vec![value]),
        BinOp::StrictEq,
        string_expr("array"),
    )
}

/// Builds the SplFileInfo constructor body.
fn spl_file_info_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "path", string_copy_expr(var_expr("filename"))),
        property_assign_stmt(this_expr(), "fileClass", string_expr("SplFileObject")),
        property_assign_stmt(this_expr(), "infoClass", string_expr("SplFileInfo")),
    ]
}

/// Builds the shared SplFileObject initialization body for a path and stream mode.
fn spl_file_object_construct_body(path: Expr, mode: Expr) -> Vec<Stmt> {
    spl_file_object_construct_body_with_backing(path.clone(), path, mode)
}

/// Builds SplFileObject initialization with separate logical and backing paths.
fn spl_file_object_construct_body_with_backing(path: Expr, backing_path: Expr, mode: Expr) -> Vec<Stmt> {
    let mut body = vec![
        property_assign_stmt(this_expr(), "path", string_copy_expr(path.clone())),
        property_assign_stmt(this_expr(), "backingPath", string_copy_expr(backing_path.clone())),
        property_assign_stmt(
            this_expr(),
            "stream",
            function_call("fopen", vec![string_copy_expr(backing_path.clone()), mode]),
        ),
        property_assign_stmt(this_expr(), "fileClass", string_expr("SplFileObject")),
        property_assign_stmt(this_expr(), "infoClass", string_expr("SplFileInfo")),
        property_assign_stmt(this_expr(), "lineNumber", int_expr(0)),
        property_assign_stmt(this_expr(), "hasReadLine", bool_expr(false)),
        property_assign_stmt(this_expr(), "flags", int_expr(0)),
        property_assign_stmt(this_expr(), "delimiter", string_expr(",")),
        property_assign_stmt(this_expr(), "enclosure", string_expr("\"")),
        property_assign_stmt(this_expr(), "escape", string_expr("\\")),
        property_assign_stmt(this_expr(), "maxLineLen", int_expr(0)),
        property_assign_stmt(this_expr(), "csvRecords", empty_array_expr()),
    ];
    body.extend(file_object_load_lines_body(string_copy_expr(backing_path)));
    body
}

/// Builds statements that reload SplFileObject line storage from a filesystem path.
fn file_object_load_lines_body(path: Expr) -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "lines", empty_array_expr()),
        foreach_stmt(
            function_call("file", vec![path]),
            None,
            "line",
            vec![property_array_push_stmt(this_expr(), "lines", var_expr("line"))],
        ),
        spl_file_object_csv_refresh_stmt(),
    ]
}

/// Builds the SplTempFileObject constructor body.
fn spl_temp_file_object_construct_body() -> Vec<Stmt> {
    let body = vec![
        if_stmt(
            binary_expr(var_expr("maxMemory"), BinOp::Lt, int_expr(0)),
            vec![assign_stmt("path", string_expr("php://memory"))],
            Some(vec![assign_stmt(
                "path",
                binary_expr(string_expr("php://temp/maxmemory:"), BinOp::Concat, var_expr("maxMemory")),
            )]),
        ),
        property_assign_stmt(this_expr(), "path", string_copy_expr(var_expr("path"))),
        property_assign_stmt(this_expr(), "backingPath", string_expr("")),
        property_assign_stmt(this_expr(), "stream", null_expr()),
        property_assign_stmt(this_expr(), "lines", empty_array_expr()),
        property_assign_stmt(this_expr(), "fileClass", string_expr("SplFileObject")),
        property_assign_stmt(this_expr(), "infoClass", string_expr("SplFileInfo")),
        property_assign_stmt(this_expr(), "lineNumber", int_expr(0)),
        property_assign_stmt(this_expr(), "hasReadLine", bool_expr(false)),
        property_assign_stmt(this_expr(), "flags", int_expr(0)),
        property_assign_stmt(this_expr(), "delimiter", string_expr(",")),
        property_assign_stmt(this_expr(), "enclosure", string_expr("\"")),
        property_assign_stmt(this_expr(), "escape", string_expr("\\")),
        property_assign_stmt(this_expr(), "maxLineLen", int_expr(0)),
        property_assign_stmt(this_expr(), "tempMaxMemory", var_expr("maxMemory")),
        property_assign_stmt(this_expr(), "tempBuffer", string_expr("")),
        property_assign_stmt(this_expr(), "tempPosition", int_expr(0)),
        property_assign_stmt(this_expr(), "tempSpilled", bool_expr(false)),
    ];
    body
}

/// Builds SplTempFileObject eof() with memory-buffer and spilled-stream paths.
fn spl_temp_file_object_eof_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            temp_spilled_expr(),
            return_body(function_call("feof", vec![file_stream_expr()])),
            None,
        ),
        return_stmt(binary_expr(
            temp_position_expr(),
            BinOp::GtEq,
            function_call("strlen", vec![temp_buffer_arg_expr()]),
        )),
    ]
}

/// Builds SplTempFileObject fgets() over the memory buffer until spill occurs.
fn spl_temp_file_object_fgets_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            temp_spilled_expr(),
            return_body(function_call("fgets", vec![file_stream_expr()])),
            None,
        ),
        assign_stmt("line", string_expr("")),
        while_stmt(
            binary_expr(
                temp_position_expr(),
                BinOp::Lt,
                function_call("strlen", vec![temp_buffer_arg_expr()]),
            ),
            vec![
                assign_stmt(
                    "ch",
                    function_call("substr", vec![temp_buffer_arg_expr(), temp_position_expr(), int_expr(1)]),
                ),
                property_assign_stmt(
                    this_expr(),
                    "tempPosition",
                    binary_expr(temp_position_expr(), BinOp::Add, int_expr(1)),
                ),
                assign_stmt("line", binary_expr(var_expr("line"), BinOp::Concat, var_expr("ch"))),
                if_stmt(
                    binary_expr(var_expr("ch"), BinOp::StrictEq, string_expr("\n")),
                    spl_temp_file_object_return_line_body(),
                    None,
                ),
            ],
        ),
        if_stmt(
            binary_expr(function_call("strlen", vec![var_expr("line")]), BinOp::Gt, int_expr(0)),
            vec![property_assign_stmt(
                this_expr(),
                "lineNumber",
                binary_expr(file_line_number_expr(), BinOp::Add, int_expr(1)),
            )],
            None,
        ),
        return_stmt(var_expr("line")),
    ]
}

/// Builds statements that increment the line number and return `$line`.
fn spl_temp_file_object_return_line_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(
            this_expr(),
            "lineNumber",
            binary_expr(file_line_number_expr(), BinOp::Add, int_expr(1)),
        ),
        return_stmt(var_expr("line")),
    ]
}

/// Builds SplTempFileObject fgetc() over the memory buffer until spill occurs.
fn spl_temp_file_object_fgetc_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            temp_spilled_expr(),
            return_body(function_call("fgetc", vec![file_stream_expr()])),
            None,
        ),
        if_stmt(
            binary_expr(
                temp_position_expr(),
                BinOp::GtEq,
                function_call("strlen", vec![temp_buffer_arg_expr()]),
            ),
            return_body(bool_expr(false)),
            None,
        ),
        assign_stmt(
            "ch",
            function_call("substr", vec![temp_buffer_arg_expr(), temp_position_expr(), int_expr(1)]),
        ),
        property_assign_stmt(
            this_expr(),
            "tempPosition",
            binary_expr(temp_position_expr(), BinOp::Add, int_expr(1)),
        ),
        return_stmt(var_expr("ch")),
    ]
}

/// Builds SplTempFileObject fread() with memory-buffer slicing before spill.
fn spl_temp_file_object_fread_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            temp_spilled_expr(),
            return_body(function_call("fread", vec![file_stream_expr(), var_expr("length")])),
            None,
        ),
        assign_stmt(
            "chunk",
            function_call("substr", vec![temp_buffer_arg_expr(), temp_position_expr(), var_expr("length")]),
        ),
        property_assign_stmt(
            this_expr(),
            "tempPosition",
            binary_expr(
                temp_position_expr(),
                BinOp::Add,
                function_call("strlen", vec![var_expr("chunk")]),
            ),
        ),
        return_stmt(var_expr("chunk")),
    ]
}

/// Builds SplTempFileObject fwrite() with threshold-based spill to a temp file.
fn spl_temp_file_object_fwrite_body() -> Vec<Stmt> {
    let mut body = vec![
        if_stmt(
            temp_spilled_expr(),
            spl_temp_file_object_spilled_fwrite_body(),
            None,
        ),
        assign_stmt("bytes", function_call("strlen", vec![var_expr("data")])),
        assign_stmt("writeEnd", binary_expr(temp_position_expr(), BinOp::Add, var_expr("bytes"))),
        assign_stmt("tail", string_expr("")),
        if_stmt(
            binary_expr(
                var_expr("writeEnd"),
                BinOp::Lt,
                function_call("strlen", vec![temp_buffer_arg_expr()]),
            ),
            vec![assign_stmt(
                "tail",
                function_call("substr", vec![temp_buffer_arg_expr(), var_expr("writeEnd")]),
            )],
            None,
        ),
        property_assign_stmt(
            this_expr(),
            "tempBuffer",
            binary_expr(
                binary_expr(
                    function_call("substr", vec![temp_buffer_arg_expr(), int_expr(0), temp_position_expr()]),
                    BinOp::Concat,
                    var_expr("data"),
                ),
                BinOp::Concat,
                var_expr("tail"),
            ),
        ),
        property_assign_stmt(this_expr(), "tempPosition", var_expr("writeEnd")),
    ];
    body.extend(spl_temp_file_object_reload_lines_from_buffer_body());
    body.push(if_stmt(
        spl_temp_file_object_should_spill_expr(),
        vec![expr_stmt(method_call(this_expr(), "__elephcSpillToFile", Vec::new()))],
        None,
    ));
    body.push(return_stmt(var_expr("bytes")));
    body
}

/// Builds the spilled-stream fwrite() branch for SplTempFileObject.
fn spl_temp_file_object_spilled_fwrite_body() -> Vec<Stmt> {
    let mut body = vec![assign_stmt(
        "bytes",
        function_call("fwrite", vec![file_stream_expr(), var_expr("data")]),
    )];
    body.extend(file_object_load_lines_body(file_backing_path_arg_expr()));
    body.push(return_stmt(var_expr("bytes")));
    body
}

/// Builds SplTempFileObject fflush() with no-op success for memory-only storage.
fn spl_temp_file_object_fflush_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            temp_spilled_expr(),
            return_body(function_call("fflush", vec![file_stream_expr()])),
            None,
        ),
        return_stmt(bool_expr(true)),
    ]
}

/// Builds SplTempFileObject ftruncate() with memory-buffer truncation before spill.
fn spl_temp_file_object_ftruncate_body() -> Vec<Stmt> {
    let mut body = vec![
        if_stmt(
            temp_spilled_expr(),
            spl_temp_file_object_spilled_ftruncate_body(),
            None,
        ),
        if_stmt(
            binary_expr(var_expr("size"), BinOp::Lt, function_call("strlen", vec![temp_buffer_arg_expr()])),
            vec![property_assign_stmt(
                this_expr(),
                "tempBuffer",
                function_call("substr", vec![temp_buffer_arg_expr(), int_expr(0), var_expr("size")]),
            )],
            None,
        ),
        if_stmt(
            binary_expr(temp_position_expr(), BinOp::Gt, var_expr("size")),
            vec![property_assign_stmt(this_expr(), "tempPosition", var_expr("size"))],
            None,
        ),
    ];
    body.extend(spl_temp_file_object_reload_lines_from_buffer_body());
    body.push(return_stmt(bool_expr(true)));
    body
}

/// Builds the spilled-stream ftruncate() branch for SplTempFileObject.
fn spl_temp_file_object_spilled_ftruncate_body() -> Vec<Stmt> {
    let mut body = vec![assign_stmt(
        "ok",
        function_call("ftruncate", vec![file_stream_expr(), var_expr("size")]),
    )];
    body.extend(file_object_load_lines_body(file_backing_path_arg_expr()));
    body.push(return_stmt(var_expr("ok")));
    body
}

/// Builds SplTempFileObject fstat() with a small memory-backed stat array before spill.
fn spl_temp_file_object_fstat_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            temp_spilled_expr(),
            return_body(function_call("fstat", vec![file_stream_expr()])),
            None,
        ),
        assign_stmt("size", function_call("strlen", vec![temp_buffer_arg_expr()])),
        return_stmt(spl_temp_file_object_memory_fstat_expr()),
    ]
}

/// Builds a PHP-like fstat array for memory-backed SplTempFileObject storage.
fn spl_temp_file_object_memory_fstat_expr() -> Expr {
    expr(ExprKind::ArrayLiteralAssoc(vec![
        (int_expr(0), int_expr(0)),
        (string_expr("dev"), int_expr(0)),
        (int_expr(1), int_expr(0)),
        (string_expr("ino"), int_expr(0)),
        (int_expr(2), int_expr(0)),
        (string_expr("mode"), int_expr(0)),
        (int_expr(3), int_expr(0)),
        (string_expr("nlink"), int_expr(0)),
        (int_expr(4), int_expr(0)),
        (string_expr("uid"), int_expr(0)),
        (int_expr(5), int_expr(0)),
        (string_expr("gid"), int_expr(0)),
        (int_expr(6), int_expr(0)),
        (string_expr("rdev"), int_expr(0)),
        (int_expr(7), var_expr("size")),
        (string_expr("size"), var_expr("size")),
        (int_expr(8), int_expr(0)),
        (string_expr("atime"), int_expr(0)),
        (int_expr(9), int_expr(0)),
        (string_expr("mtime"), int_expr(0)),
        (int_expr(10), int_expr(0)),
        (string_expr("ctime"), int_expr(0)),
        (int_expr(11), int_expr(0)),
        (string_expr("blksize"), int_expr(0)),
        (int_expr(12), int_expr(0)),
        (string_expr("blocks"), int_expr(0)),
    ]))
}

/// Builds SplTempFileObject ftell() over the memory cursor until spill occurs.
fn spl_temp_file_object_ftell_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            temp_spilled_expr(),
            return_body(function_call("ftell", vec![file_stream_expr()])),
            None,
        ),
        return_stmt(temp_position_expr()),
    ]
}

/// Builds SplTempFileObject fseek() over the memory cursor until spill occurs.
fn spl_temp_file_object_fseek_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            temp_spilled_expr(),
            return_body(function_call(
                "fseek",
                vec![file_stream_expr(), var_expr("offset"), var_expr("whence")],
            )),
            None,
        ),
        if_stmt(
            binary_expr(var_expr("whence"), BinOp::StrictEq, int_expr(1)),
            vec![assign_stmt("newPosition", binary_expr(temp_position_expr(), BinOp::Add, var_expr("offset")))],
            Some(vec![if_stmt(
                binary_expr(var_expr("whence"), BinOp::StrictEq, int_expr(2)),
                vec![assign_stmt(
                    "newPosition",
                    binary_expr(
                        function_call("strlen", vec![temp_buffer_arg_expr()]),
                        BinOp::Add,
                        var_expr("offset"),
                    ),
                )],
                Some(vec![assign_stmt("newPosition", var_expr("offset"))]),
            )]),
        ),
        if_stmt(
            binary_expr(var_expr("newPosition"), BinOp::Lt, int_expr(0)),
            vec![assign_stmt("newPosition", int_expr(0))],
            None,
        ),
        property_assign_stmt(this_expr(), "tempPosition", var_expr("newPosition")),
        return_stmt(int_expr(0)),
    ]
}

/// Builds SplTempFileObject rewind() for memory and spilled-stream storage.
fn spl_temp_file_object_rewind_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            temp_spilled_expr(),
            vec![expr_stmt(function_call("rewind", vec![file_stream_expr()]))],
            None,
        ),
        property_assign_stmt(this_expr(), "tempPosition", int_expr(0)),
        property_assign_stmt(this_expr(), "lineNumber", int_expr(0)),
    ]
}

/// Builds the hidden spill helper that moves the memory buffer into a temp file.
fn spl_temp_file_object_spill_body() -> Vec<Stmt> {
    let mut body = vec![
        if_stmt(temp_spilled_expr(), vec![return_void_stmt()], None),
        property_assign_stmt(
            this_expr(),
            "backingPath",
            function_call("tempnam", vec![function_call("sys_get_temp_dir", Vec::new()), string_expr("elephc")]),
        ),
        expr_stmt(function_call("file_put_contents", vec![file_backing_path_arg_expr(), temp_buffer_arg_expr()])),
        property_assign_stmt(
            this_expr(),
            "stream",
            function_call("fopen", vec![file_backing_path_arg_expr(), string_expr("r+")]),
        ),
        property_assign_stmt(this_expr(), "tempSpilled", bool_expr(true)),
        expr_stmt(function_call("fseek", vec![file_stream_expr(), temp_position_expr()])),
    ];
    body.extend(file_object_load_lines_body(file_backing_path_arg_expr()));
    body
}

/// Returns true when the in-memory temp buffer has crossed its configured spill threshold.
fn spl_temp_file_object_should_spill_expr() -> Expr {
    binary_expr(
        binary_expr(temp_max_memory_expr(), BinOp::GtEq, int_expr(0)),
        BinOp::And,
        binary_expr(
            function_call("strlen", vec![temp_buffer_arg_expr()]),
            BinOp::Gt,
            temp_max_memory_expr(),
        ),
    )
}

/// Builds statements that refresh inherited line storage from the memory buffer.
fn spl_temp_file_object_reload_lines_from_buffer_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "lines", empty_array_expr()),
        if_stmt(
            binary_expr(function_call("strlen", vec![temp_buffer_arg_expr()]), BinOp::StrictEq, int_expr(0)),
            Vec::new(),
            Some(vec![foreach_stmt(
                function_call("explode", vec![string_expr("\n"), temp_buffer_arg_expr()]),
                None,
                "line",
                vec![property_array_push_stmt(this_expr(), "lines", var_expr("line"))],
            )]),
        ),
        spl_file_object_csv_refresh_stmt(),
    ]
}

/// Builds SplFileObject current(), reading a CSV RECORD under READ_CSV.
///
/// It used to `explode()` the raw line on the delimiter, which is not CSV at all: an enclosure
/// was ordinary text, so `a,"b,c",d` came back as four fields with quotes attached; the line's
/// own `"\n"` stayed glued to the last one; a blank line answered `[""]` where php answers
/// `[null]`; and a quoted field containing a newline was cut in half. The record list is built
/// from the same line storage the rest of the class uses, but a RECORD may span several lines.
fn spl_file_object_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            flag_enabled_expr(file_object_flags_expr(), SPL_FILE_READ_CSV),
            vec![
                return_stmt(array_access(
                    property_access(this_expr(), "csvRecords"),
                    file_line_number_expr(),
                )),
            ],
            None,
        ),
        return_stmt(file_current_line_expr()),
    ]
}

/// Builds the READ_CSV record cache, one record per `fgetcsv()` php would perform.
///
/// Lines are accumulated until the enclosures balance, so a quoted field holding newlines
/// stays one field of one record; `str_getcsv()` then applies the very rules the compiled
/// `fgetcsv()` applies, including php's `[null]` for a line that was nothing but its
/// terminator. The cache is rebuilt only when the controls or the line storage changed.
fn spl_file_object_csv_build_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "csvRecords", empty_array_expr()),
        property_assign_stmt(this_expr(), "csvBlank", empty_array_expr()),
        assign_stmt("pending", string_expr("")),
        assign_stmt("inside", bool_expr(false)),
        foreach_stmt(
            file_lines_expr(),
            None,
            "line",
            {
                let mut body = vec![assign_stmt(
                    "pending",
                    binary_expr(var_expr("pending"), BinOp::Concat, var_expr("line")),
                )];
                body.extend(spl_file_object_csv_scan_line_body());
                body.push(if_stmt(
                    not_expr(var_expr("inside")),
                    vec![
                        property_array_push_stmt(
                            this_expr(),
                            "csvRecords",
                            spl_file_object_csv_parse_expr(var_expr("pending")),
                        ),
                        property_array_push_stmt(
                            this_expr(),
                            "csvBlank",
                            spl_file_object_csv_blank_expr(var_expr("pending")),
                        ),
                        assign_stmt("pending", string_expr("")),
                    ],
                    None,
                ));
                body
            },
        ),
        // A file that ends inside an enclosure still yields what it has: php's reader stops at
        // end of input rather than discarding the half-record it accumulated.
        if_stmt(
            binary_expr(var_expr("pending"), BinOp::StrictNotEq, string_expr("")),
            vec![
                property_array_push_stmt(
                    this_expr(),
                    "csvRecords",
                    spl_file_object_csv_parse_expr(var_expr("pending")),
                ),
                property_array_push_stmt(this_expr(), "csvBlank", int_expr(0)),
            ],
            // Otherwise there is ONE more record than there are terminated lines. php reads
            // until a read fails, so a file whose last byte is a newline — and an EMPTY file —
            // yield a final `[null]`. SKIP_EMPTY turns that last one into `false` instead.
            Some(vec![
                assign_stmt("tail", string_expr("")),
                foreach_stmt(
                    file_lines_expr(),
                    None,
                    "line",
                    vec![assign_stmt("tail", string_copy_expr(var_expr("line")))],
                ),
                if_stmt(
                    binary_expr(
                        binary_expr(var_expr("tail"), BinOp::StrictEq, string_expr("")),
                        BinOp::Or,
                        binary_expr(
                            function_call(
                                "substr",
                                vec![
                                    string_copy_expr(var_expr("tail")),
                                    int_expr(-1),
                                    int_expr(1),
                                ],
                            ),
                            BinOp::StrictEq,
                            string_expr("\n"),
                        ),
                    ),
                    vec![
                        // The trailing record is never SKIPPED, only reshaped: php answers
                        // `false` there under SKIP_EMPTY and `[null]` without it.
                        property_array_push_stmt(this_expr(), "csvBlank", int_expr(0)),
                        if_stmt(
                        flag_enabled_expr(file_object_flags_expr(), SPL_FILE_SKIP_EMPTY),
                        vec![property_array_push_stmt(
                            this_expr(),
                            "csvRecords",
                            bool_expr(false),
                        )],
                        // The controls are spelled out even though an empty subject cannot use
                        // them: omitting `$escape` is what raises the 8.4 deprecation, and a
                        // notice the user's program never asked for would come out of it.
                        Some(vec![property_array_push_stmt(
                            this_expr(),
                            "csvRecords",
                            function_call(
                                "str_getcsv",
                                vec![
                                    string_expr(""),
                                    string_expr(","),
                                    string_expr("\""),
                                    string_expr("\\"),
                                ],
                            ),
                        )]),
                    ),
                    ],
                    None,
                ),
            ]),
        ),
    ]
}

/// Builds the byte scan that updates `$inside` across one more line of `$pending`.
///
/// A record ends at a line terminator only when no enclosure is open. The scan mirrors the
/// parser's own states: inside an enclosure the escape character shields the byte after it,
/// so `"a\"b"` reads as closed, and a doubled `""` closes and reopens for a net zero.
///
/// It is INLINED into the caller rather than living behind a method, because `current()` — the
/// only reader — cannot call one: a method call from inside `SplFileObject::current()` faults
/// at run time whatever the callee does, `getCsvControl()` included. See the module note.
fn spl_file_object_csv_scan_line_body() -> Vec<Stmt> {
    vec![
        assign_stmt("index", int_expr(0)),
        assign_stmt("length", function_call("strlen", vec![string_copy_expr(var_expr("line"))])),
        while_stmt(
            binary_expr(var_expr("index"), BinOp::Lt, var_expr("length")),
            vec![
                assign_stmt(
                    "byte",
                    function_call(
                        "substr",
                        vec![
                            string_copy_expr(var_expr("line")),
                            var_expr("index"),
                            int_expr(1),
                        ],
                    ),
                ),
                if_stmt(
                    binary_expr(
                        binary_expr(
                            var_expr("inside"),
                            BinOp::And,
                            binary_expr(
                                string_copy_expr(property_access(this_expr(), "escape")),
                                BinOp::StrictNotEq,
                                string_expr(""),
                            ),
                        ),
                        BinOp::And,
                        binary_expr(
                            var_expr("byte"),
                            BinOp::StrictEq,
                            string_copy_expr(property_access(this_expr(), "escape")),
                        ),
                    ),
                    // The escape shields whatever follows, so step over it as data.
                    vec![assign_stmt(
                        "index",
                        binary_expr(var_expr("index"), BinOp::Add, int_expr(1)),
                    )],
                    Some(vec![if_stmt(
                        binary_expr(
                            var_expr("byte"),
                            BinOp::StrictEq,
                            string_copy_expr(property_access(this_expr(), "enclosure")),
                        ),
                        vec![assign_stmt("inside", not_expr(var_expr("inside")))],
                        None,
                    )]),
                ),
                assign_stmt(
                    "index",
                    binary_expr(var_expr("index"), BinOp::Add, int_expr(1)),
                ),
            ],
        ),
    ]
}

/// Returns 1 when the accumulated buffer was nothing but a line terminator.
///
/// That is php's BLANK line — the one `fgetcsv()` answers `[null]` for — and the only thing
/// SKIP_EMPTY steps over. A line of spaces is NOT blank: `" \n"` is a one-field record.
fn spl_file_object_csv_blank_expr(buffer: Expr) -> Expr {
    let is = |terminator: &str| {
        binary_expr(
            string_copy_expr(buffer.clone()),
            BinOp::StrictEq,
            string_expr(terminator),
        )
    };
    expr(crate::parser::ast::ExprKind::Ternary {
        condition: Box::new(binary_expr(
            binary_expr(is(""), BinOp::Or, is("\n")),
            BinOp::Or,
            binary_expr(is("\r"), BinOp::Or, is("\r\n")),
        )),
        then_expr: Box::new(int_expr(1)),
        else_expr: Box::new(int_expr(0)),
    })
}

/// Returns `str_getcsv($buffer, $this->delimiter, $this->enclosure, $this->escape)`.
fn spl_file_object_csv_parse_expr(buffer: Expr) -> Expr {
    function_call(
        "str_getcsv",
        vec![
            string_copy_expr(buffer),
            string_copy_expr(property_access(this_expr(), "delimiter")),
            string_copy_expr(property_access(this_expr(), "enclosure")),
            string_copy_expr(property_access(this_expr(), "escape")),
        ],
    )
}

/// Builds the statement that rebuilds the READ_CSV record list, if the flag is on.
///
/// The rebuild is EAGER, at every point that moves the line storage or the control characters,
/// rather than lazy inside `current()`: the records were parsed with those bytes and cannot
/// survive a change to either, and `current()` is the one method that cannot call another.
fn spl_file_object_csv_refresh_stmt() -> Stmt {
    if_stmt(
        flag_enabled_expr(file_object_flags_expr(), SPL_FILE_READ_CSV),
        vec![expr_stmt(method_call(
            this_expr(),
            "__elephcCsvBuild",
            Vec::new(),
        ))],
        None,
    )
}

/// Builds SplFileObject next().
fn spl_file_object_next_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(
            this_expr(),
            "lineNumber",
            binary_expr(file_line_number_expr(), BinOp::Add, int_expr(1)),
        ),
        spl_file_object_csv_skip_blank_stmt(),
    ]
}

/// Builds SplFileObject rewind().
fn spl_file_object_rewind_body() -> Vec<Stmt> {
    vec![
        expr_stmt(function_call("rewind", vec![file_stream_expr()])),
        property_assign_stmt(this_expr(), "lineNumber", int_expr(0)),
        spl_file_object_csv_skip_blank_stmt(),
    ]
}

/// Builds the call that steps the cursor over blank CSV records.
fn spl_file_object_csv_skip_blank_stmt() -> Stmt {
    expr_stmt(method_call(this_expr(), "__elephcCsvSkipBlank", Vec::new()))
}

/// Builds the cursor advance that SKIP_EMPTY performs over blank records.
///
/// php only honors SKIP_EMPTY together with DROP_NEW_LINE, and it steps OVER the blank record
/// rather than removing it: the keys of the records that follow are unchanged, so a file whose
/// second line is empty iterates 0, 2, 3. Nothing happens without READ_CSV — the plain line
/// path keeps its own behaviour.
fn spl_file_object_csv_skip_blank_body() -> Vec<Stmt> {
    let mask = SPL_FILE_READ_CSV | SPL_FILE_SKIP_EMPTY | SPL_FILE_DROP_NEW_LINE;
    vec![
        if_stmt(
            not_expr(flag_mode_is_expr(file_object_flags_expr(), mask, mask)),
            vec![return_void_stmt()],
            None,
        ),
        while_stmt(
            binary_expr(
                binary_expr(
                    file_line_number_expr(),
                    BinOp::Lt,
                    count_expr(property_access(this_expr(), "csvBlank")),
                ),
                BinOp::And,
                binary_expr(
                    array_access(property_access(this_expr(), "csvBlank"), file_line_number_expr()),
                    BinOp::StrictEq,
                    int_expr(1),
                ),
            ),
            vec![property_assign_stmt(
                this_expr(),
                "lineNumber",
                binary_expr(file_line_number_expr(), BinOp::Add, int_expr(1)),
            )],
        ),
    ]
}

/// Builds SplFileObject valid().
///
/// Under READ_CSV the bound is the RECORD count, not the line count: a quoted field holding
/// newlines makes one record out of several lines, so iterating to `count($this->lines)` walked
/// off the end of the record list and answered null for the tail.
fn spl_file_object_valid_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            flag_enabled_expr(file_object_flags_expr(), SPL_FILE_READ_CSV),
            vec![return_stmt(binary_expr(
                file_line_number_expr(),
                BinOp::Lt,
                count_expr(property_access(this_expr(), "csvRecords")),
            ))],
            None,
        ),
        return_stmt(file_object_valid_expr()),
    ]
}

/// Builds SplFileObject fgets().
fn spl_file_object_fgets_body() -> Vec<Stmt> {
    let mut body = vec![spl_file_object_read_guard_stmt()];
    body.extend(spl_file_object_read_line_stmts());
    body.push(property_assign_stmt(
        this_expr(),
        "lineNumber",
        binary_expr(file_line_number_expr(), BinOp::Add, int_expr(1)),
    ));
    body.push(property_assign_stmt(this_expr(), "hasReadLine", bool_expr(true)));
    body.push(return_stmt(var_expr("line")));
    body
}

/// Builds the read that both `fgets()` and `fscanf()` perform, into `$line`.
///
/// A read that comes back `false` becomes `""`. php's own reader answers the EMPTY STRING for
/// the read that first reaches end of file — measured on `php -n` 8.5.6, `fgets()` on
/// `"a\nbb\n"` gives `'a\n'`, `'bb\n'`, then `''`, and only the call AFTER that one fails. The
/// `false` this backend used to return is not a value php ever produces here.
///
/// The `(string)` cast carries that rule AND the representation: it makes `$line` a STRING
/// rather than `string|false`, and `(string) false` is `""` — php's own answer. Keeping the
/// union instead left `fscanf()` handing a boxed Mixed to `sscanf()`'s declared `string`
/// parameter.
fn spl_file_object_read_line_stmts() -> Vec<Stmt> {
    vec![assign_stmt(
        "line",
        cast_expr(
            CastType::String,
            function_call("fgets", vec![file_stream_expr()]),
        ),
    )]
}

/// Builds php's refusal to read a file object already positioned at end of file.
///
/// `php -n` 8.5.6 throws `RuntimeException: Cannot read from file <path>` — the path as the
/// constructor received it — from `fgets()` and `fscanf()` once `feof()` holds. `feof()` only
/// becomes true after a read has hit the end, which is why the empty-string read above happens
/// FIRST and this guard fires on the call after it.
fn spl_file_object_read_guard_stmt() -> Stmt {
    if_stmt(
        function_call("feof", vec![file_stream_expr()]),
        vec![throw_stmt(new_object_expr(
            "RuntimeException",
            vec![binary_expr(
                string_expr("Cannot read from file "),
                BinOp::Concat,
                file_path_expr(),
            )],
        ))],
        None,
    )
}

/// Builds SplFileObject fscanf().
///
/// One line through the shared scanf engine, so the method and the free function cannot drift.
/// The line-number rule is php's, and it is not `fgets()`'s: the FIRST read of a fresh object
/// leaves `key()` where it was, and only later reads advance it.
fn spl_file_object_fscanf_body() -> Vec<Stmt> {
    let mut body = vec![spl_file_object_read_guard_stmt()];
    body.extend(spl_file_object_read_line_stmts());
    body.push(if_stmt(
        property_access(this_expr(), "hasReadLine"),
        vec![property_assign_stmt(
            this_expr(),
            "lineNumber",
            binary_expr(file_line_number_expr(), BinOp::Add, int_expr(1)),
        )],
        None,
    ));
    body.push(property_assign_stmt(this_expr(), "hasReadLine", bool_expr(true)));
    body.push(return_stmt(function_call(
        "sscanf",
        vec![var_expr("line"), var_expr("format")],
    )));
    body
}

/// Builds SplFileObject fwrite().
fn spl_file_object_fwrite_body() -> Vec<Stmt> {
    let mut body = vec![
        assign_stmt("bytes", function_call("fwrite", vec![file_stream_expr(), var_expr("data")])),
    ];
    body.extend(file_object_load_lines_body(file_backing_path_arg_expr()));
    body.push(return_stmt(var_expr("bytes")));
    body
}

/// Builds SplFileObject ftruncate().
fn spl_file_object_ftruncate_body() -> Vec<Stmt> {
    let mut body = vec![assign_stmt(
        "ok",
        function_call("ftruncate", vec![file_stream_expr(), var_expr("size")]),
    )];
    body.extend(file_object_load_lines_body(file_backing_path_arg_expr()));
    body.push(return_stmt(var_expr("ok")));
    body
}

/// Builds SplFileObject fseek().
fn spl_file_object_fseek_body() -> Vec<Stmt> {
    return_body(function_call(
        "fseek",
        vec![file_stream_expr(), var_expr("offset"), var_expr("whence")],
    ))
}

/// Builds SplFileObject setCsvControl().
fn spl_file_object_set_csv_control_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "delimiter", var_expr("separator")),
        property_assign_stmt(this_expr(), "enclosure", var_expr("enclosure")),
        property_assign_stmt(this_expr(), "escape", var_expr("escape")),
        spl_file_object_csv_refresh_stmt(),
    ]
}

/// Builds SplFileObject getCsvControl().
fn spl_file_object_get_csv_control_body() -> Vec<Stmt> {
    return_body(expr(crate::parser::ast::ExprKind::ArrayLiteral(vec![
        property_access(this_expr(), "delimiter"),
        property_access(this_expr(), "enclosure"),
        property_access(this_expr(), "escape"),
    ])))
}

/// Returns `$<name> ?? $this-><property>`, the fallback php applies to an omitted CSV control.
///
/// php-src reads the object's `setCsvControl()` state for anything the call left out, so the
/// parameter defaults are null and the STATE supplies the byte. Spelling `","` in the signature
/// instead made `$f->setCsvControl(";"); $f->fgetcsv()` split on a comma.
fn csv_control_or_state_expr(name: &str, property: &str) -> Expr {
    null_coalesce_expr(
        var_expr(name),
        string_copy_expr(property_access(this_expr(), property)),
    )
}

/// Builds SplFileObject fgetcsv().
fn spl_file_object_fgetcsv_body() -> Vec<Stmt> {
    vec![
        assign_stmt(
            "row",
            function_call(
                "fgetcsv",
                vec![
                    file_stream_expr(),
                    int_expr(0),
                    csv_control_or_state_expr("separator", "delimiter"),
                    csv_control_or_state_expr("enclosure", "enclosure"),
                    csv_control_or_state_expr("escape", "escape"),
                ],
            ),
        ),
        property_assign_stmt(
            this_expr(),
            "lineNumber",
            binary_expr(file_line_number_expr(), BinOp::Add, int_expr(1)),
        ),
        return_stmt(var_expr("row")),
    ]
}

/// Builds SplFileObject fputcsv().
///
/// `$eol` is FORWARDED. The method declared it and then called the function with five
/// arguments, so `$f->fputcsv(["a", "b"], ",", '"', "\\", "")` — php's "write no terminator",
/// which answers 3 — still wrote a newline and answered 4, and a custom terminator was
/// silently discarded.
fn spl_file_object_fputcsv_body() -> Vec<Stmt> {
    let mut body = vec![
        assign_stmt(
            "bytes",
            function_call(
                "fputcsv",
                vec![
                    file_stream_expr(),
                    var_expr("fields"),
                    csv_control_or_state_expr("separator", "delimiter"),
                    csv_control_or_state_expr("enclosure", "enclosure"),
                    csv_control_or_state_expr("escape", "escape"),
                    string_copy_expr(var_expr("eol")),
                ],
            ),
        ),
    ];
    body.extend(file_object_load_lines_body(file_backing_path_arg_expr()));
    body.push(return_stmt(var_expr("bytes")));
    body
}

/// Builds a directory constructor body.
fn directory_construct_body(directory: Expr, flags: Expr, filter_dots: bool, entries_are_paths: bool) -> Vec<Stmt> {
    let mut body = vec![
        property_assign_stmt(this_expr(), "directory", string_copy_expr(directory.clone())),
        property_assign_stmt(this_expr(), "fsFlags", flags.clone()),
        property_assign_stmt(this_expr(), "entriesArePathnames", bool_expr(entries_are_paths)),
    ];
    body.extend(directory_rebuild_entries_body(directory, flags, filter_dots));
    body.extend(vec![
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        expr_stmt(method_call(this_expr(), "__elephcRefreshPath", Vec::new())),
    ]);
    body
}

/// Builds statements that populate the directory entry snapshot.
fn directory_rebuild_entries_body(directory: Expr, flags: Expr, filter_dots: bool) -> Vec<Stmt> {
    if !filter_dots {
        return vec![
            property_assign_stmt(this_expr(), "entries", empty_array_expr()),
            foreach_stmt(
                function_call("scandir", vec![string_copy_expr(directory)]),
                None,
                "entry",
                vec![property_array_push_stmt(this_expr(), "entries", var_expr("entry"))],
            ),
        ];
    }
    vec![
        property_assign_stmt(this_expr(), "entries", empty_array_expr()),
        foreach_stmt(
            function_call("scandir", vec![string_copy_expr(directory)]),
            None,
            "entry",
            vec![if_stmt(
                binary_expr(
                    not_expr(flag_enabled_expr(flags, FS_SKIP_DOTS)),
                    BinOp::Or,
                    not_dot_name_expr(var_expr("entry")),
                ),
                vec![property_array_push_stmt(this_expr(), "entries", var_expr("entry"))],
                None,
            )],
        ),
    ]
}

/// Builds DirectoryIterator refresh-path helper body.
fn directory_refresh_path_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(directory_position_expr(), BinOp::Lt, count_expr(directory_entries_expr())),
            vec![
                if_stmt(
                    entries_are_pathnames_expr(),
                    vec![
                        property_assign_stmt(this_expr(), "path", string_copy_expr(directory_current_entry_expr())),
                        return_void_stmt(),
                    ],
                    None,
                ),
                property_assign_stmt(
                    this_expr(),
                    "path",
                    path_join_expr(
                        string_copy_expr(directory_path_expr()),
                        string_copy_expr(directory_current_entry_expr()),
                    ),
                ),
                return_void_stmt(),
            ],
            None,
        ),
        property_assign_stmt(this_expr(), "path", string_expr("")),
    ]
}

/// Builds DirectoryIterator next().
fn directory_next_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(
            this_expr(),
            "position",
            binary_expr(directory_position_expr(), BinOp::Add, int_expr(1)),
        ),
        expr_stmt(method_call(this_expr(), "__elephcRefreshPath", Vec::new())),
    ]
}

/// Builds DirectoryIterator rewind().
fn directory_rewind_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        expr_stmt(method_call(this_expr(), "__elephcRefreshPath", Vec::new())),
    ]
}

/// Builds DirectoryIterator seek().
fn directory_seek_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "position", var_expr("offset")),
        expr_stmt(method_call(this_expr(), "__elephcRefreshPath", Vec::new())),
    ]
}

/// Builds DirectoryIterator valid().
fn directory_valid_body() -> Vec<Stmt> {
    return_body(binary_expr(directory_position_expr(), BinOp::Lt, count_expr(directory_entries_expr())))
}

/// Builds FilesystemIterator current().
fn filesystem_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            flag_mode_is_expr(filesystem_flags_expr(), FS_CURRENT_MODE_MASK, FS_CURRENT_AS_PATHNAME),
            return_body(file_path_expr()),
            None,
        ),
        if_stmt(
            flag_mode_is_expr(filesystem_flags_expr(), FS_CURRENT_MODE_MASK, FS_CURRENT_AS_SELF),
            return_body(this_expr()),
            None,
        ),
        return_stmt(new_object_expr("SplFileInfo", vec![file_path_arg_expr()])),
    ]
}

/// Builds FilesystemIterator key().
fn filesystem_key_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            flag_mode_is_expr(filesystem_flags_expr(), FS_KEY_MODE_MASK, FS_KEY_AS_FILENAME),
            return_body(function_call("basename", vec![file_path_arg_expr()])),
            None,
        ),
        return_stmt(file_path_expr()),
    ]
}

/// Builds FilesystemIterator setFlags().
fn filesystem_set_flags_body() -> Vec<Stmt> {
    let mut body = vec![
        property_assign_stmt(this_expr(), "fsFlags", var_expr("flags")),
        property_assign_stmt(this_expr(), "entriesArePathnames", bool_expr(false)),
    ];
    body.extend(directory_rebuild_entries_body(directory_path_expr(), var_expr("flags"), true));
    body.extend(vec![
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        expr_stmt(method_call(this_expr(), "__elephcRefreshPath", Vec::new())),
    ]);
    body
}

/// Builds GlobIterator constructor.
fn glob_iterator_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "pattern", string_copy_expr(var_expr("pattern"))),
        property_assign_stmt(
            this_expr(),
            "directory",
            function_call("dirname", vec![string_copy_expr(var_expr("pattern"))]),
        ),
        property_assign_stmt(this_expr(), "fsFlags", var_expr("flags")),
        property_assign_stmt(this_expr(), "entriesArePathnames", bool_expr(true)),
        property_assign_stmt(this_expr(), "entries", empty_array_expr()),
        foreach_stmt(
            function_call("glob", vec![string_copy_expr(var_expr("pattern"))]),
            None,
            "entry",
            vec![property_array_push_stmt(this_expr(), "entries", var_expr("entry"))],
        ),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        expr_stmt(method_call(this_expr(), "__elephcRefreshPath", Vec::new())),
    ]
}

/// Builds RecursiveDirectoryIterator hasChildren().
fn recursive_directory_has_children_body() -> Vec<Stmt> {
    return_body(binary_expr(
        binary_expr(
            function_call("is_dir", vec![file_path_arg_expr()]),
            BinOp::And,
            binary_expr(
                flag_enabled_expr(filesystem_flags_expr(), FS_FOLLOW_SYMLINKS),
                BinOp::Or,
                not_expr(function_call("is_link", vec![file_path_arg_expr()])),
            ),
        ),
        BinOp::And,
        not_expr(directory_is_dot_expr()),
    ))
}

/// Builds RecursiveDirectoryIterator getChildren().
fn recursive_directory_get_children_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(method_call(this_expr(), "hasChildren", Vec::new())),
            return_body(null_expr()),
            None,
        ),
        return_stmt(new_object_expr(
            "RecursiveDirectoryIterator",
            vec![file_path_arg_expr(), filesystem_flags_expr()],
        )),
    ]
}

/// Builds RecursiveCachingIterator constructor.
fn recursive_caching_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
        property_assign_stmt(this_expr(), "recursiveInner", var_expr("iterator")),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
        property_assign_stmt(this_expr(), "cache", empty_assoc_array_expr()),
        property_assign_stmt(this_expr(), "currentKey", null_expr()),
        property_assign_stmt(this_expr(), "currentValue", null_expr()),
        property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
        property_assign_stmt(this_expr(), "cachedHasNext", bool_expr(false)),
    ]
}

/// Builds RecursiveCachingIterator getChildren().
fn recursive_caching_get_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt("value", method_call(this_expr(), "current", Vec::new())),
        if_stmt(
            instanceof_expr(var_expr("value"), "RecursiveIterator"),
            return_body(new_object_expr(
                "RecursiveCachingIterator",
                vec![
                    method_call(this_expr(), "__elephcAssumeRecursiveIterator", vec![var_expr("value")]),
                    method_call(this_expr(), "getFlags", Vec::new()),
                ],
            )),
            None,
        ),
        if_stmt(
            gettype_is_array_expr(var_expr("value")),
            return_body(new_object_expr(
                "RecursiveCachingIterator",
                vec![
                    new_object_expr("RecursiveArrayIterator", vec![var_expr("value")]),
                    method_call(this_expr(), "getFlags", Vec::new()),
                ],
            )),
            None,
        ),
        return_stmt(null_expr()),
    ]
}

/// Builds RecursiveCachingIterator hasChildren().
fn recursive_caching_has_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt("value", method_call(this_expr(), "current", Vec::new())),
        return_stmt(binary_expr(
            instanceof_expr(var_expr("value"), "RecursiveIterator"),
            BinOp::Or,
            gettype_is_array_expr(var_expr("value")),
        )),
    ]
}
