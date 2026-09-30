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
        // Where each line ENDED, as the descriptor saw it. php's iteration reads through the
        // stream, so `current()` leaves it past the line it answered and `ftell()` reports that;
        // this object reads every line once and restores the position, so the stream never moves
        // on its own. These are absolute offsets, which is what lets SKIP_EMPTY filter them
        // beside `lines` without renumbering anything.
        protected_storage_property("lineEnds", array_type()),
        // Whether the stream was ALREADY at its end when the last line came back. php drives its
        // iteration from the stream, so a file whose last byte is a newline gives one more read
        // before the end — but `php://temp` reports EOF the moment a line read drains it. The
        // plain-line loader reads this from a local; the CSV builder re-runs from `setFlags()`
        // long after that local is gone, so the answer lives here.
        protected_storage_property("loadedAtEnd", TypeExpr::Bool),
        protected_storage_property("lineNumber", TypeExpr::Int),
        protected_storage_property("flags", TypeExpr::Int),
        protected_storage_property("delimiter", TypeExpr::Str),
        protected_storage_property("enclosure", TypeExpr::Str),
        protected_storage_property("escape", TypeExpr::Str),
        // Whether `setCsvControl()` has ever been given an `$escape`. php 8.4 deprecates a CSV
        // call that omits it, and this is the state that silences the notice: MEASURED on
        // `php -n` 8.5.6, `$f->setCsvControl(",", '"', "\\"); $f->fgetcsv();` is silent, and a
        // LATER `setCsvControl(";")` deprecates ITSELF without turning the flag back off — so it
        // is sticky-on, per object, and never inherited by a second SplFileObject on the same
        // file.
        protected_storage_property("escapeProvided", TypeExpr::Bool),
        protected_storage_property("maxLineLen", TypeExpr::Int),
        // What a `seek()` past the last line left behind. php's `seek()` walks the stream line by
        // line, so a seek to or beyond the end consumes the read-ahead and the object reports
        // `valid() === false` afterwards even though `key()` names a line. MEASURED on a
        // four-line file (five elements): `seek(4)` answers key 4, current `""`, valid FALSE;
        // `seek(99)` answers key 4, current FALSE, valid FALSE. 0 = neither happened.
        protected_storage_property("seekState", TypeExpr::Int),
        // Whether the ITERATOR has positioned this object. php's iteration reads one line AHEAD,
        // so `eof()` turns true while the LAST element is still current, and stays false for a
        // fresh object that has only been constructed. Reading lines by hand does not read ahead
        // at all — MEASURED, two `fgets()` over `"a\nb\n"` leave `eof()` false on a plain file —
        // so only `rewind()` and `next()` set this, and `eof()` falls back to the stream when it
        // is unset.
        protected_storage_property("iterStarted", TypeExpr::Bool),
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
///
/// There are none. The class IS an `SplFileObject` on `php://temp`, so it holds exactly its
/// parent's state — and declaring storage of its own would put that storage FIRST in the
/// flattened layout, leaving every inherited method reading its parent's slot index against a
/// different object. `feof($this->stream)` segfaulted on the four properties that used to be
/// here.
fn spl_temp_file_object_properties() -> Vec<ClassProperty> {
    Vec::new()
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

/// Which of php's two wordings a failing `SplFileInfo` getter uses.
#[derive(Clone, Copy)]
enum SplStatKind {
    /// `stat failed for <path>` — every getter that follows symlinks.
    Stat,
    /// `Lstat failed for <path>` — `getType()`, which does not.
    Lstat,
}

/// Builds a stat-backed `SplFileInfo` getter that THROWS when the file is not there.
///
/// php refuses to answer these at all for a path it cannot stat: MEASURED on `php -n` 8.5.6, all
/// nine raise `RuntimeException: SplFileInfo::<name>(): stat failed for <path>` — `getType()`
/// with `Lstat` instead. elephc answered `0` or `false` in silence, so a program that trusted
/// `getSize()` got a size of zero for a file that was not there.
fn spl_file_info_stat_getter_body(name: &str, builtin: &str, kind: SplStatKind) -> Vec<Stmt> {
    let tail = match kind {
        SplStatKind::Stat => "(): stat failed for ",
        SplStatKind::Lstat => "(): Lstat failed for ",
    };
    vec![
        assign_stmt(
            "__splStat",
            suppress_expr(function_call(builtin, vec![file_path_arg_expr()])),
        ),
        if_stmt(
            binary_expr(var_expr("__splStat"), BinOp::StrictEq, bool_expr(false)),
            vec![throw_stmt(new_object_expr(
                "RuntimeException",
                vec![binary_expr(
                    string_expr(&format!("SplFileInfo::{name}{tail}")),
                    BinOp::Concat,
                    file_path_arg_expr(),
                )],
            ))],
            None,
        ),
        return_stmt(var_expr("__splStat")),
    ]
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
        method_with_body("getPath", Vec::new(), Some(TypeExpr::Str), spl_file_info_get_path_body()),
        method_with_body("getFilename", Vec::new(), Some(TypeExpr::Str), spl_file_info_get_filename_body()),
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
        method_with_body(
            "getPerms",
            Vec::new(),
            Some(mixed_type()),
            spl_file_info_stat_getter_body("getPerms", "fileperms", SplStatKind::Stat),
        ),
        method_with_body(
            "getInode",
            Vec::new(),
            Some(mixed_type()),
            spl_file_info_stat_getter_body("getInode", "fileinode", SplStatKind::Stat),
        ),
        method_with_body(
            "getSize",
            Vec::new(),
            Some(TypeExpr::Int),
            spl_file_info_stat_getter_body("getSize", "filesize", SplStatKind::Stat),
        ),
        method_with_body(
            "getOwner",
            Vec::new(),
            Some(mixed_type()),
            spl_file_info_stat_getter_body("getOwner", "fileowner", SplStatKind::Stat),
        ),
        method_with_body(
            "getGroup",
            Vec::new(),
            Some(mixed_type()),
            spl_file_info_stat_getter_body("getGroup", "filegroup", SplStatKind::Stat),
        ),
        method_with_body(
            "getATime",
            Vec::new(),
            Some(mixed_type()),
            spl_file_info_stat_getter_body("getATime", "fileatime", SplStatKind::Stat),
        ),
        method_with_body(
            "getMTime",
            Vec::new(),
            Some(TypeExpr::Int),
            spl_file_info_stat_getter_body("getMTime", "filemtime", SplStatKind::Stat),
        ),
        method_with_body(
            "getCTime",
            Vec::new(),
            Some(mixed_type()),
            spl_file_info_stat_getter_body("getCTime", "filectime", SplStatKind::Stat),
        ),
        method_with_body(
            "getType",
            Vec::new(),
            Some(mixed_type()),
            spl_file_info_stat_getter_body("getType", "filetype", SplStatKind::Lstat),
        ),
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
        method_with_body("eof", Vec::new(), Some(TypeExpr::Bool), spl_file_object_eof_body()),
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
        method_with_body("fgetc", Vec::new(), Some(mixed_type()), spl_file_object_manual_read_body(function_call("fgetc", vec![file_stream_expr()]))),
        method_with_body(
            "fread",
            vec![param("length", TypeExpr::Int)],
            Some(TypeExpr::Str),
            spl_file_object_manual_read_body(function_call("fread", vec![file_stream_expr(), var_expr("length")])),
        ),
        method_with_body(
            "fwrite",
            vec![
                param("data", TypeExpr::Str),
                // php's own second parameter, read off reflection:
                // `fwrite(string $data, ?int $length = null)`. elephc declared ONE, so
                // `$f->fwrite("abcdef", 3)` was refused at compile time and a subclass carrying
                // php's real signature could not override it at all.
                param_default("length", nullable_int_type(), null_expr()),
            ],
            Some(TypeExpr::Int),
            spl_file_object_fwrite_body(),
        ),
        method_with_body("fflush", Vec::new(), Some(TypeExpr::Bool), return_body(function_call("fflush", vec![file_stream_expr()]))),
        // MEASURED and left at ONE parameter on purpose. php's is
        // `flock(int $operation, &$wouldBlock = null)`, so a subclass carrying php's real
        // signature is refused here with `Cannot change parameter count when overriding method`.
        // Declaring the second parameter does NOT fix that: the body would then hand the builtin
        // a by-reference slot, and the backend refuses a 3-argument `flock` outright —
        // `flock would_block output for non-local arguments` — even from a branch a run never
        // takes, so the ordinary `$f->flock(LOCK_SH)` stopped compiling. The out-parameter is
        // unreachable anyway: `flock($h, LOCK_SH, $w)` over `$w = null` answers `by-ref integer
        // output written into a null slot` for the plain BUILTIN too. Two backend gaps stand
        // between this declaration and php's, and neither is a signature.
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
        method_with_body(
            "seek",
            vec![param("line", TypeExpr::Int)],
            Some(TypeExpr::Void),
            spl_file_object_seek_body(),
        ),
        method_with_body("getFlags", Vec::new(), Some(TypeExpr::Int), return_body(file_object_flags_expr())),
        method_with_body(
            "setFlags",
            vec![param("flags", TypeExpr::Int)],
            Some(TypeExpr::Void),
            {
                // The blank-line filter depends on the FLAGS, and `setFlags()` may turn them on
                // long after the lines were read — so the lines are read again from the stream,
                // which is cheap and is also the only way a later `setFlags(0)` gets them back.
                let mut body = vec![property_assign_stmt(this_expr(), "flags", var_expr("flags"))];
                body.extend(file_object_load_lines_body(file_backing_path_arg_expr()));
                body
            },
        ),
        method_with_body("getMaxLineLen", Vec::new(), Some(TypeExpr::Int), return_body(property_access(this_expr(), "maxLineLen"))),
        method_with_body("setMaxLineLen", vec![param("maxLength", TypeExpr::Int)], Some(TypeExpr::Void), vec![property_assign_stmt(this_expr(), "maxLineLen", var_expr("maxLength"))]),
        method_with_body(
            "setCsvControl",
            vec![
                param_default("separator", TypeExpr::Str, string_expr(",")),
                param_default("enclosure", TypeExpr::Str, string_expr("\"")),
                // `?string = null` rather than php's `string = "\\"`: the 8.4 deprecation fires on
                // an OMITTED `$escape`, and elephc refuses `func_num_args()` in a method whose
                // parameters have defaults — "elephc cannot tell a passed argument from a
                // defaulted one" — so the sentinel is what tells them apart. Same device the
                // `fgetcsv` / `fputcsv` controls below already use, for the same reason.
                param_default("escape", nullable_string_type(), null_expr()),
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
    let mut methods = vec![method_with_body(
        "__construct",
        vec![param_default("maxMemory", TypeExpr::Int, int_expr(2_097_152))],
        Some(TypeExpr::Void),
        spl_temp_file_object_construct_body(),
    )];
    methods.extend(spl_temp_file_object_stream_methods());
    methods
}

/// Re-declares the parent's stream methods, with the parent's own bodies.
///
/// They are IDENTICAL to `SplFileObject`'s and inheritance ought to supply them. It does not:
/// a program whose only file object is an `SplTempFileObject` gets a NULL vtable slot for every
/// method the subclass does not declare, and `$t->ftell()` jumps to address zero. MEASURED —
/// `lldb` stops at `frame #0: 0x0000000000000000`, and the emitted assembly carries no
/// `_eir_SplFileObject__ftell` at all. Constructing an `SplFileObject` ANYWHERE in the same
/// program makes the crash disappear, which is the signature of a body that was never emitted
/// rather than a dispatch that went wrong.
///
/// Declaring them here is what keeps this class working while that gap is open. Each entry is
/// the same expression `spl_file_object_methods` uses, so there is one behaviour, not two.
fn spl_temp_file_object_stream_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body("eof", Vec::new(), Some(TypeExpr::Bool), spl_file_object_eof_body()),
        method_with_body("fgets", Vec::new(), Some(mixed_type()), spl_file_object_fgets_body()),
        method_with_body("getCurrentLine", Vec::new(), Some(mixed_type()), spl_file_object_fgets_body()),
        method_with_body("fgetc", Vec::new(), Some(mixed_type()), spl_file_object_manual_read_body(function_call("fgetc", vec![file_stream_expr()]))),
        method_with_body(
            "fread",
            vec![param("length", TypeExpr::Int)],
            Some(TypeExpr::Str),
            spl_file_object_manual_read_body(function_call("fread", vec![file_stream_expr(), var_expr("length")])),
        ),
        method_with_body(
            "fwrite",
            vec![
                param("data", TypeExpr::Str),
                // php's own second parameter, read off reflection:
                // `fwrite(string $data, ?int $length = null)`. elephc declared ONE, so
                // `$f->fwrite("abcdef", 3)` was refused at compile time and a subclass carrying
                // php's real signature could not override it at all.
                param_default("length", nullable_int_type(), null_expr()),
            ],
            Some(TypeExpr::Int),
            spl_file_object_fwrite_body(),
        ),
        method_with_body("fflush", Vec::new(), Some(TypeExpr::Bool), return_body(function_call("fflush", vec![file_stream_expr()]))),
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
        // MEASURED and left at ONE parameter on purpose. php's is
        // `flock(int $operation, &$wouldBlock = null)`, so a subclass carrying php's real
        // signature is refused here with `Cannot change parameter count when overriding method`.
        // Declaring the second parameter does NOT fix that: the body would then hand the builtin
        // a by-reference slot, and the backend refuses a 3-argument `flock` outright —
        // `flock would_block output for non-local arguments` — even from a branch a run never
        // takes, so the ordinary `$f->flock(LOCK_SH)` stopped compiling. The out-parameter is
        // unreachable anyway: `flock($h, LOCK_SH, $w)` over `$w = null` answers `by-ref integer
        // output written into a null slot` for the plain BUILTIN too. Two backend gaps stand
        // between this declaration and php's, and neither is a signature.
        method_with_body("flock", vec![param("operation", TypeExpr::Int)], Some(TypeExpr::Bool), return_body(function_call("flock", vec![file_stream_expr(), var_expr("operation")]))),
    ]
}

/// Builds DirectoryIterator methods.
fn directory_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("directory", TypeExpr::Str)],
            Some(TypeExpr::Void),
            directory_construct_body("DirectoryIterator", var_expr("directory"), int_expr(0), false, false),
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
            directory_construct_body("FilesystemIterator", var_expr("directory"), var_expr("flags"), true, false),
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
            directory_construct_body("RecursiveDirectoryIterator", var_expr("directory"), var_expr("flags"), true, false),
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

/// Splits a path the way php's `SplFileInfo` does, into `$__p` (the part before the last
/// separator) and `$__n` (the rest).
///
/// NOT `dirname()`/`basename()`, which is what elephc used and what makes it wrong in nine of the
/// fourteen shapes below. MEASURED on `php -n` 8.5.6:
///
/// ```text
/// path          getPath   getFilename     dirname   basename
/// 'a.txt'       ''        'a.txt'         '.'       'a.txt'
/// 'fi2'         ''        'fi2'           '.'       'fi2'
/// 'fi2/a.txt'   'fi2'     'a.txt'         'fi2'     'a.txt'
/// './a.txt'     '.'       'a.txt'         '.'       'a.txt'
/// '/a.txt'      ''        '/a.txt'        '/'       'a.txt'
/// '/'           ''        '/'             '/'       ''
/// '.'           ''        '.'             '.'       '.'
/// 'a//b.txt'    'a/'      'b.txt'         'a'       'b.txt'
/// 'dir/'        ''        'dir'           '.'       'dir'
/// ```
///
/// One rule accounts for all of them, and for `getFilename` too — php stores a `path_len` and
/// takes the filename from just past it, so the two answers are two halves of one split. Trailing
/// separators come off first; a separator at index 0 means there is no path part at all, which is
/// why `/a.txt` keeps its slash in the FILENAME.
fn spl_file_info_path_split_stmts() -> Vec<Stmt> {
    vec![
        assign_stmt(
            "__raw",
            string_copy_expr(file_path_arg_expr()),
        ),
        assign_stmt(
            "__p",
            function_call("rtrim", vec![var_expr("__raw"), string_expr("/")]),
        ),
        assign_stmt(
            "__i",
            function_call("strrpos", vec![var_expr("__p"), string_expr("/")]),
        ),
    ]
}

/// Builds SplFileInfo getPath(): everything before the last separator, or the empty string.
fn spl_file_info_get_path_body() -> Vec<Stmt> {
    let mut body = spl_file_info_path_split_stmts();
    body.push(if_stmt(
        binary_expr(
            binary_expr(var_expr("__i"), BinOp::StrictEq, bool_expr(false)),
            BinOp::Or,
            binary_expr(var_expr("__i"), BinOp::StrictEq, int_expr(0)),
        ),
        vec![return_stmt(string_expr(""))],
        None,
    ));
    body.push(return_stmt(function_call(
        "substr",
        vec![var_expr("__p"), int_expr(0), var_expr("__i")],
    )));
    body
}

/// Builds SplFileInfo getFilename(): the other half of the same split.
fn spl_file_info_get_filename_body() -> Vec<Stmt> {
    let mut body = spl_file_info_path_split_stmts();
    // A path that was ALL separators keeps its original spelling: php answers `/` for `/`.
    body.push(if_stmt(
        binary_expr(var_expr("__p"), BinOp::StrictEq, string_expr("")),
        vec![return_stmt(var_expr("__raw"))],
        None,
    ));
    body.push(if_stmt(
        binary_expr(
            binary_expr(var_expr("__i"), BinOp::StrictEq, bool_expr(false)),
            BinOp::Or,
            binary_expr(var_expr("__i"), BinOp::StrictEq, int_expr(0)),
        ),
        vec![return_stmt(var_expr("__p"))],
        None,
    ));
    body.push(return_stmt(function_call(
        "substr",
        vec![
            var_expr("__p"),
            binary_expr(var_expr("__i"), BinOp::Add, int_expr(1)),
        ],
    )));
    body
}

/// Returns `?int`.
fn nullable_int_type() -> TypeExpr {
    TypeExpr::Nullable(Box::new(TypeExpr::Int))
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

/// Suppresses a call's own diagnostics, so the caller can report the failure its own way.
fn suppress_expr(value: Expr) -> Expr {
    expr(ExprKind::ErrorSuppress(Box::new(value)))
}

/// The message php throws when a file object cannot open its stream.
///
/// The reason is approximated from `file_exists()`, the way `DirectoryIterator` already does:
/// the synthesized body is PHP and has no `errno`, and `error_get_last()` — which would carry
/// php's own text — is not implemented. The two reasons this distinguishes are the two that
/// happen: a path that is not there, and one that is but cannot be opened. MEASURED on
/// `php -n` 8.5.6 for both.
fn spl_open_failure_message(backing_path: Expr) -> Expr {
    binary_expr(
        binary_expr(
            binary_expr(
                string_expr("SplFileObject::__construct("),
                BinOp::Concat,
                string_copy_expr(backing_path.clone()),
            ),
            BinOp::Concat,
            string_expr("): Failed to open stream: "),
        ),
        BinOp::Concat,
        expr(ExprKind::Ternary {
            condition: Box::new(function_call("file_exists", vec![string_copy_expr(backing_path)])),
            then_expr: Box::new(string_expr("Permission denied")),
            else_expr: Box::new(string_expr("No such file or directory")),
        }),
    )
}

/// Builds SplFileObject initialization with separate logical and backing paths.
fn spl_file_object_construct_body_with_backing(path: Expr, backing_path: Expr, mode: Expr) -> Vec<Stmt> {
    let mut body = vec![
        // php refuses a DIRECTORY before it opens anything, and says so as a LogicException.
        // elephc opened it, warned three times about bytes it could not read, and handed back a
        // live object — MEASURED: `new SplFileObject(".")` threw in php and answered an object here.
        if_stmt(
            function_call("is_dir", vec![string_copy_expr(backing_path.clone())]),
            vec![throw_stmt(new_object_expr(
                "LogicException",
                vec![string_expr("Cannot use SplFileObject with directories")],
            ))],
            None,
        ),
        property_assign_stmt(this_expr(), "path", string_copy_expr(path.clone())),
        property_assign_stmt(this_expr(), "backingPath", string_copy_expr(backing_path.clone())),
        // php opens the stream ITSELF and THROWS instead of warning. Suppressing the open is what
        // removes the three warnings php never prints — `fopen()`, then `file()` on the same
        // missing path, then `foreach` over the `false` that came back.
        property_assign_stmt(
            this_expr(),
            "stream",
            suppress_expr(function_call("fopen", vec![string_copy_expr(backing_path.clone()), mode])),
        ),
        // `new SplFileObject($p)` is php's way of saying "open this or fail loudly". elephc failed
        // QUIETLY: the object came back with a `false` stream and every later call read nothing.
        if_stmt(
            binary_expr(file_stream_expr(), BinOp::StrictEq, bool_expr(false)),
            vec![throw_stmt(new_object_expr(
                "RuntimeException",
                vec![spl_open_failure_message(backing_path.clone())],
            ))],
            None,
        ),
        property_assign_stmt(this_expr(), "fileClass", string_expr("SplFileObject")),
        property_assign_stmt(this_expr(), "infoClass", string_expr("SplFileInfo")),
        property_assign_stmt(this_expr(), "lineNumber", int_expr(0)),
        property_assign_stmt(this_expr(), "hasReadLine", bool_expr(false)),
        property_assign_stmt(this_expr(), "iterStarted", bool_expr(false)),
        // `seekState` was read by `current()` and `valid()` but only WRITTEN by `rewind()` and
        // `seek()`, so `(new SplFileObject($p))->current()` — php's own first-line idiom, with no
        // rewind — died on an uninitialized typed property.
        property_assign_stmt(this_expr(), "seekState", int_expr(0)),
        property_assign_stmt(this_expr(), "flags", int_expr(0)),
        property_assign_stmt(this_expr(), "delimiter", string_expr(",")),
        property_assign_stmt(this_expr(), "enclosure", string_expr("\"")),
        property_assign_stmt(this_expr(), "escape", string_expr("\\")),
        property_assign_stmt(this_expr(), "escapeProvided", bool_expr(false)),
        property_assign_stmt(this_expr(), "maxLineLen", int_expr(0)),
        property_assign_stmt(this_expr(), "csvRecords", empty_array_expr()),
    ];
    body.extend(file_object_load_lines_body(string_copy_expr(backing_path)));
    body
}

/// Builds statements that reload SplFileObject line storage from THE STREAM IT HOLDS.
///
/// Not from the path. `file($this->backingPath)` re-opened the file by name, and a stream that
/// has no name to re-open — `php://memory`, `php://temp`, everything `SplTempFileObject` is built
/// on — came back EMPTY every time. MEASURED on `php -n` 8.5.6: after `$t = new
/// SplTempFileObject(); $t->fwrite("temp\n"); $t->rewind();`, php's `current()` answers
/// `"temp\n"` and elephc answered `""` for `php://memory` and dropped the newline for a temp
/// object — two different wrong answers from the same cause.
///
/// The read costs what `fgets()` costs, which is no longer the reason to avoid it: a 900 KB file
/// of 100 000 lines went from 538 ms to 20 ms when the line reader learned to fill the stream's
/// own buffer. The position is saved and restored, because reloading is not a seek the program
/// asked for.
fn file_object_load_lines_body(_path: Expr) -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "lines", empty_array_expr()),
        property_assign_stmt(this_expr(), "lineEnds", empty_array_expr()),
        assign_stmt("__splPos", function_call("ftell", vec![file_stream_expr()])),
        expr_stmt(function_call("rewind", vec![file_stream_expr()])),
        // Whether the stream was already at its end when the LAST line came back. That is the
        // question php's iteration asks, and the answer is not a property of the bytes: the same
        // `"a\nb\n"` leaves a plain file short of the end and a `php://temp` stream past it.
        assign_stmt("__splAtEnd", bool_expr(false)),
        while_stmt(
            binary_expr(
                assign_expr("__splLine", function_call("fgets", vec![file_stream_expr()])),
                BinOp::StrictNotEq,
                bool_expr(false),
            ),
            vec![
                property_array_push_stmt(this_expr(), "lines", var_expr("__splLine")),
                property_array_push_stmt(
                    this_expr(),
                    "lineEnds",
                    function_call("ftell", vec![file_stream_expr()]),
                ),
                assign_stmt("__splAtEnd", function_call("feof", vec![file_stream_expr()])),
            ],
        ),
        expr_stmt(function_call(
            "fseek",
            vec![file_stream_expr(), var_expr("__splPos")],
        )),
        property_assign_stmt(this_expr(), "loadedAtEnd", var_expr("__splAtEnd")),
        spl_file_object_skip_empty_stmt(),
        spl_file_object_trailing_line_stmt(),
        spl_file_object_csv_refresh_stmt(),
    ]
}

/// Drops the blank lines php steps over when DROP_NEW_LINE and SKIP_EMPTY are BOTH set.
///
/// php only honours `SKIP_EMPTY` together with `DROP_NEW_LINE` — alone it changes nothing but the
/// final element. MEASURED on `"a\n\nb\n"`: with both flags php yields `'a'`, `'b'`, `false`,
/// and the keys are CONSECUTIVE — 0 and 1, not 0 and 2 — so the blank is removed from the
/// sequence rather than stepped over in place.
///
/// The removal happens before the trailing element is appended, so a file that ends in a newline
/// still gets one.
fn spl_file_object_skip_empty_stmt() -> Stmt {
    let mask = SPL_FILE_SKIP_EMPTY | SPL_FILE_DROP_NEW_LINE;
    if_stmt(
        binary_expr(
            binary_expr(
                binary_expr(file_object_flags_expr(), BinOp::BitAnd, int_expr(mask)),
                BinOp::StrictEq,
                int_expr(mask),
            ),
            BinOp::And,
            // READ_CSV has its own rule, and it is the OPPOSITE one: php steps over the blank
            // RECORD without renumbering, so `0, 2, 3` — measured. Removing the blank line here
            // would renumber the records that follow it.
            not_expr(flag_enabled_expr(file_object_flags_expr(), SPL_FILE_READ_CSV)),
        ),
        vec![
            assign_stmt("__splKept", empty_array_expr()),
            assign_stmt("__splKeptEnds", empty_array_expr()),
            foreach_stmt(
                file_lines_expr(),
                Some("__splIdx"),
                "__splLine",
                vec![if_stmt(
                    binary_expr(
                        function_call("rtrim", vec![var_expr("__splLine"), string_expr("\n")]),
                        BinOp::StrictNotEq,
                        string_expr(""),
                    ),
                    vec![
                        array_push_stmt("__splKept", var_expr("__splLine")),
                        // Absolute, so a surviving line keeps the offset it always had.
                        array_push_stmt(
                            "__splKeptEnds",
                            array_access(
                                property_access(this_expr(), "lineEnds"),
                                var_expr("__splIdx"),
                            ),
                        ),
                    ],
                    None,
                )],
            ),
            property_assign_stmt(this_expr(), "lines", var_expr("__splKept")),
            property_assign_stmt(this_expr(), "lineEnds", var_expr("__splKeptEnds")),
        ],
        None,
    )
}

/// Builds `$name = <value>` as an EXPRESSION, for a `while` that reads and tests in one step.
fn assign_expr(name: &str, value: Expr) -> Expr {
    expr(ExprKind::Assignment {
        target: Box::new(var_expr(name)),
        value: Box::new(value),
        result_target: None,
        prelude: Vec::new(),
        conditional_value_temp: None,
    })
}

/// Appends the FINAL empty line php's iteration yields, when the file has one.
///
/// php drives the iteration from the stream, not from a line array: after the last `\n` the
/// stream is not yet at end of file, so one more round answers `''`. `file()` reports no such
/// element, so the array-backed model stopped an iteration early — MEASURED on `php -n` 8.5.6:
///
/// ```text
/// ""       iterates 1 time  ['']
/// "a"      iterates 1 time  ['a']
/// "a\n"    iterates 2 times ['a\n', '']
/// "a\nb\n"  iterates 3 times ['a\n', 'b\n', '']
/// ```
///
/// So the rule is: a file that is EMPTY, or whose last byte is a newline, has one more line than
/// `file()` reports. The last stored line is tested rather than the file re-read, because these
/// lines keep their newline — `DROP_NEW_LINE` is applied by `current()`, not by the loader.
///
/// "after the last `\n` the stream is not yet at end of file" is a claim about the STREAM, and
/// one kind of stream disagrees. `php://temp` reports EOF the moment a line read drains it, so
/// an `SplTempFileObject` holding that same `"a\nb\n"` yields TWO elements, not three, and
/// `"a\n"` yields one. Hence the loader's own reading of `feof()`, taken after the last line
/// arrived rather than after the read that failed — by then every kind of stream says true.
fn spl_file_object_trailing_line_stmt() -> Stmt {
    let last_line = array_access(
        file_lines_expr(),
        binary_expr(count_expr(file_lines_expr()), BinOp::Sub, int_expr(1)),
    );
    if_stmt(
        binary_expr(
            not_expr(var_expr("__splAtEnd")),
            BinOp::And,
            binary_expr(
                binary_expr(count_expr(file_lines_expr()), BinOp::StrictEq, int_expr(0)),
                BinOp::Or,
                binary_expr(
                    function_call("substr", vec![last_line, int_expr(-1)]),
                    BinOp::StrictEq,
                    string_expr("\n"),
                ),
            ),
        ),
        vec![
            property_array_push_stmt(this_expr(), "lines", string_expr("")),
            // It consumed nothing, so it ends where the line before it did. Read from the
            // RECORDED ends, not from `ftell()`: the loader has already restored the position it
            // started from by the time this runs, so a live probe answers where the reading
            // began rather than where it stopped.
            property_array_push_stmt(
                this_expr(),
                "lineEnds",
                expr(crate::parser::ast::ExprKind::Ternary {
                    condition: Box::new(binary_expr(
                        count_expr(property_access(this_expr(), "lineEnds")),
                        BinOp::Gt,
                        int_expr(0),
                    )),
                    then_expr: Box::new(function_call(
                        "intval",
                        vec![array_access(
                            property_access(this_expr(), "lineEnds"),
                            binary_expr(
                                count_expr(property_access(this_expr(), "lineEnds")),
                                BinOp::Sub,
                                int_expr(1),
                            ),
                        )],
                    )),
                    else_expr: Box::new(int_expr(0)),
                }),
            ),
        ],
        None,
    )
}

/// Builds the SplTempFileObject constructor body.
///
/// php's `SplTempFileObject` IS an `SplFileObject` on `php://temp`, and elephc modelled it with a
/// hand-written in-memory buffer instead — thirteen method bodies of its own. They disagreed with
/// php in several places at once. MEASURED on `php -n` 8.5.6:
///
/// ```text
/// $t = new SplTempFileObject();
/// $t->eof()                         php: false   elephc: true
/// …write "a\nbb\nccc\n", fseek(0)
/// $t->current()                     php: "a\n"   elephc: "ccc"
/// iterating it                      php keeps the newlines, elephc dropped them
/// ```
///
/// Opening the stream php opens deletes all of that: everything below is inherited.
///
/// The LOGICAL path is what php reports — `php://temp`, or `php://memory` for a negative
/// `$maxMemory` — while the BACKING path carries the threshold, which is what actually gets
/// opened. `getPathname()` answers the first; the stream comes from the second.
fn spl_temp_file_object_construct_body() -> Vec<Stmt> {
    // php-src keys the NAME off `ZEND_NUM_ARGS()`, not off the value: no argument names
    // `php://temp`, an explicit one names `php://temp/maxmemory:N`, and a negative one names
    // `php://memory`. A synthesized body cannot count arguments, so the DEFAULT VALUE stands in
    // for "no argument" — the two disagree only for an explicit `new SplTempFileObject(2097152)`,
    // which asks for the default in longhand. MEASURED for all three shapes on `php -n` 8.5.6.
    let mut body = vec![if_stmt(
        binary_expr(var_expr("maxMemory"), BinOp::Lt, int_expr(0)),
        vec![
            assign_stmt("path", string_expr("php://memory")),
            assign_stmt("backing", string_expr("php://memory")),
        ],
        Some(vec![
            assign_stmt(
                "backing",
                binary_expr(
                    string_expr("php://temp/maxmemory:"),
                    BinOp::Concat,
                    cast_expr(CastType::String, var_expr("maxMemory")),
                ),
            ),
            if_stmt(
                binary_expr(var_expr("maxMemory"), BinOp::StrictEq, int_expr(2_097_152)),
                vec![assign_stmt("path", string_expr("php://temp"))],
                Some(vec![assign_stmt("path", string_copy_expr(var_expr("backing")))]),
            ),
        ]),
    )];
    body.extend(spl_file_object_construct_body_with_backing(
        var_expr("path"),
        var_expr("backing"),
        string_expr("w+"),
    ));
    body
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
        // php READS the line to answer this, so the descriptor ends past it — MEASURED, the
        // `current()` after `seek(1)` answers `ftell() === 8`. The index-backed model answers the
        // same line without moving anything, so the move is made here.
        if_stmt(
            binary_expr(
                count_expr(property_access(this_expr(), "lineEnds")),
                BinOp::Gt,
                file_line_number_expr(),
            ),
            vec![expr_stmt(function_call(
                "fseek",
                vec![
                    file_stream_expr(),
                    // The element is a `mixed` slot; `fseek` takes an int.
                    function_call(
                        "intval",
                        vec![array_access(
                            property_access(this_expr(), "lineEnds"),
                            file_line_number_expr(),
                        )],
                    ),
                ],
            ))],
            None,
        ),
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
        // See `spl_file_object_seek_body`: a seek past every line answers `false`, not the last
        // element — php has read the stream out by then.
        if_stmt(
            binary_expr(
                property_access(this_expr(), "seekState"),
                BinOp::StrictEq,
                int_expr(2),
            ),
            vec![return_stmt(bool_expr(false))],
            None,
        ),
        // `SKIP_EMPTY` does not SHORTEN the iteration — it changes what the last element IS.
        // MEASURED on `php -n` 8.5.6 over seven file shapes: with the flag set, the element php
        // ends on is `false`, where without it the same element is `""`. The count is the same
        // either way, which is what an earlier reading of this got backwards.
        if_stmt(
            binary_expr(
                flag_enabled_expr(file_object_flags_expr(), SPL_FILE_SKIP_EMPTY),
                BinOp::And,
                binary_expr(file_current_line_expr(), BinOp::StrictEq, string_expr("")),
            ),
            vec![return_stmt(bool_expr(false))],
            None,
        ),
        return_stmt(spl_file_object_drop_new_line_expr(file_current_line_expr())),
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
        // The line list carries the FINAL empty line php's plain iteration yields after a
        // trailing newline. php's csv iteration has no such record — a four-record fixture must
        // not answer five — and `file()` never produces an empty last element on its own, so
        // dropping one here is unambiguous. Done in the builder rather than by ordering the
        // load, because `setFlags(READ_CSV)` rebuilds from `lines` long after construction.
        assign_stmt("csvLines", file_lines_expr()),
        if_stmt(
            binary_expr(
                binary_expr(count_expr(var_expr("csvLines")), BinOp::Gt, int_expr(0)),
                BinOp::And,
                binary_expr(
                    array_access(
                        var_expr("csvLines"),
                        binary_expr(count_expr(var_expr("csvLines")), BinOp::Sub, int_expr(1)),
                    ),
                    BinOp::StrictEq,
                    string_expr(""),
                ),
            ),
            vec![assign_stmt(
                "csvLines",
                function_call(
                    "array_slice",
                    vec![
                        var_expr("csvLines"),
                        int_expr(0),
                        binary_expr(count_expr(var_expr("csvLines")), BinOp::Sub, int_expr(1)),
                    ],
                ),
            )],
            None,
        ),
        foreach_stmt(
            var_expr("csvLines"),
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
                        // Not for a stream that was already at its end: see `loadedAtEnd`. The
                        // plain-line path drops the same element for the same reason, and the
                        // builder above has already dropped the trailing empty LINE this record
                        // would have paired with.
                        not_expr(property_access(this_expr(), "loadedAtEnd")),
                        BinOp::And,
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

/// Builds the note a HAND-DRIVEN read leaves: this object is back on the stream's own answer.
///
/// php's `eof()` is the stream's, and only the ITERATOR reads a line ahead of the one it yields.
/// MEASURED on `php -n` 8.5.6: `rewind()` then one `fgets()` over `"a\nb\n"` in an
/// `SplTempFileObject` reports `eof()` FALSE — the stream has read two of its four bytes — while
/// the same object one `next()` into its iteration reports TRUE. Without this the earlier
/// `rewind()` was still speaking for a cursor the caller had stopped using.
fn spl_file_object_manual_read_stmt() -> Stmt {
    property_assign_stmt(this_expr(), "iterStarted", bool_expr(false))
}

/// Builds a one-expression method body that reads the stream by hand.
fn spl_file_object_manual_read_body(call: Expr) -> Vec<Stmt> {
    vec![spl_file_object_manual_read_stmt(), return_stmt(call)]
}

/// Builds SplFileObject eof(), which answers the ITERATION's read-ahead before the stream.
///
/// php drives its iteration from the stream and reads one line AHEAD, so `eof()` is already true
/// while the last element is still current. MEASURED on `php -n` 8.5.6 over `"a\nb\n"`: a plain
/// file yields three elements and reports `eof()` false, false, TRUE; the same content in an
/// `SplTempFileObject` yields two and reports false, TRUE. elephc iterates a line ARRAY, whose
/// cursor cannot see the stream, and answered `false` at every step of both.
///
/// Reading by hand is the case that keeps the stream in the answer: `fgets()` does not read
/// ahead, and two of them over that same plain file leave `eof()` FALSE — so the fallback is not
/// a default, it is the other half of the rule. `iterStarted` is what tells the two apart, which
/// is also why a freshly constructed object reports `false` even for an EMPTY file.
fn spl_file_object_eof_body() -> Vec<Stmt> {
    let at_last = |collection: Expr| {
        return_stmt(binary_expr(
            file_line_number_expr(),
            BinOp::GtEq,
            binary_expr(count_expr(collection), BinOp::Sub, int_expr(1)),
        ))
    };
    vec![
        if_stmt(
            not_expr(property_access(this_expr(), "iterStarted")),
            vec![return_stmt(function_call("feof", vec![file_stream_expr()]))],
            None,
        ),
        // `valid()` and `current()` already branch this way, and `eof()` has to make the same
        // choice: counting lines while the cursor counts RECORDS goes wrong as soon as one
        // quoted field holds a newline.
        if_stmt(
            flag_enabled_expr(file_object_flags_expr(), SPL_FILE_READ_CSV),
            vec![at_last(property_access(this_expr(), "csvRecords"))],
            None,
        ),
        at_last(file_lines_expr()),
    ]
}

/// Builds SplFileObject next().
fn spl_file_object_next_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(
            this_expr(),
            "lineNumber",
            binary_expr(file_line_number_expr(), BinOp::Add, int_expr(1)),
        ),
        property_assign_stmt(this_expr(), "iterStarted", bool_expr(true)),
        spl_file_object_csv_skip_blank_stmt(),
    ]
}

/// Builds SplFileObject rewind().
fn spl_file_object_rewind_body() -> Vec<Stmt> {
    vec![
        expr_stmt(function_call("rewind", vec![file_stream_expr()])),
        property_assign_stmt(this_expr(), "lineNumber", int_expr(0)),
        property_assign_stmt(this_expr(), "seekState", int_expr(0)),
        property_assign_stmt(this_expr(), "iterStarted", bool_expr(true)),
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

/// Builds SplFileObject seek(), which php bounds at both ends.
///
/// A NEGATIVE line is a `ValueError` in php, not a silent rewind — elephc stored it and answered
/// `key() === -1`. Past the end, php clamps the key to the last element and leaves the object
/// invalid, because the walk consumed the stream getting there. Both MEASURED on `php -n` 8.5.6.
fn spl_file_object_seek_body() -> Vec<Stmt> {
    let last_index = binary_expr(count_expr(file_lines_expr()), BinOp::Sub, int_expr(1));
    vec![
        if_stmt(
            binary_expr(var_expr("line"), BinOp::Lt, int_expr(0)),
            vec![throw_stmt(new_object_expr(
                "ValueError",
                vec![string_expr(
                    "SplFileObject::seek(): Argument #1 ($line) must be greater than or equal to 0",
                )],
            ))],
            None,
        ),
        property_assign_stmt(this_expr(), "seekState", int_expr(0)),
        if_stmt(
            binary_expr(var_expr("line"), BinOp::GtEq, count_expr(file_lines_expr())),
            vec![
                // Past every line: php has read the stream out and answers `false` for the value.
                property_assign_stmt(this_expr(), "seekState", int_expr(2)),
                property_assign_stmt(this_expr(), "lineNumber", last_index.clone()),
            ],
            Some(vec![
                property_assign_stmt(this_expr(), "lineNumber", var_expr("line")),
                if_stmt(
                    binary_expr(var_expr("line"), BinOp::GtEq, last_index),
                    vec![property_assign_stmt(this_expr(), "seekState", int_expr(1))],
                    None,
                ),
            ]),
        ),
        // An empty listing has no last index to clamp to; php answers key 0 there.
        if_stmt(
            binary_expr(file_line_number_expr(), BinOp::Lt, int_expr(0)),
            vec![property_assign_stmt(this_expr(), "lineNumber", int_expr(0))],
            None,
        ),
        // php's `seek()` walks the stream, so the descriptor ends at the START of the line it
        // selected — MEASURED, `seek(1)` alone answers `ftell() === 4` on `"one\ntwo\nthree\n"`.
        spl_file_object_seek_stream_to_line_start_stmt(),
    ]
}

/// Puts the descriptor at the start of `$this->lineNumber`, which is where line `n - 1` ended.
fn spl_file_object_seek_stream_to_line_start_stmt() -> Stmt {
    if_stmt(
        binary_expr(file_line_number_expr(), BinOp::Lt, int_expr(1)),
        vec![expr_stmt(function_call(
            "fseek",
            vec![file_stream_expr(), int_expr(0)],
        ))],
        Some(vec![if_stmt(
            binary_expr(
                count_expr(property_access(this_expr(), "lineEnds")),
                BinOp::GtEq,
                file_line_number_expr(),
            ),
            vec![expr_stmt(function_call(
                "fseek",
                vec![
                    file_stream_expr(),
                    // The element is a `mixed` slot; `fseek` takes an int.
                    function_call(
                        "intval",
                        vec![array_access(
                            property_access(this_expr(), "lineEnds"),
                            binary_expr(file_line_number_expr(), BinOp::Sub, int_expr(1)),
                        )],
                    ),
                ],
            ))],
            None,
        )]),
    )
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
        // See `spl_file_object_seek_body`: a seek that reached the last line consumed the stream.
        if_stmt(
            binary_expr(
                property_access(this_expr(), "seekState"),
                BinOp::Gt,
                int_expr(0),
            ),
            vec![return_stmt(bool_expr(false))],
            None,
        ),
        // `SKIP_EMPTY` WITH `READ_AHEAD` removes the trailing element entirely, where the flag
        // alone only changes it to `false` (see `spl_file_object_current_body`). php reads the
        // next line before answering `valid()`, so with the read-ahead on it is already at end of
        // file and the element is never yielded. MEASURED over the whole 6 shapes × 8 flags
        // matrix on `php -n` 8.5.6 — the two earlier readings of this each had half of it,
        // because neither varied `READ_AHEAD`.
        if_stmt(
            binary_expr(
                binary_expr(
                    binary_expr(
                        file_object_flags_expr(),
                        BinOp::BitAnd,
                        int_expr(SPL_FILE_SKIP_EMPTY | SPL_FILE_READ_AHEAD),
                    ),
                    BinOp::StrictEq,
                    int_expr(SPL_FILE_SKIP_EMPTY | SPL_FILE_READ_AHEAD),
                ),
                BinOp::And,
                binary_expr(
                    binary_expr(count_expr(file_lines_expr()), BinOp::Gt, int_expr(0)),
                    BinOp::And,
                    binary_expr(
                        array_access(
                            file_lines_expr(),
                            binary_expr(count_expr(file_lines_expr()), BinOp::Sub, int_expr(1)),
                        ),
                        BinOp::StrictEq,
                        string_expr(""),
                    ),
                ),
            ),
            vec![return_stmt(binary_expr(
                file_line_number_expr(),
                BinOp::Lt,
                binary_expr(count_expr(file_lines_expr()), BinOp::Sub, int_expr(1)),
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
    vec![spl_file_object_manual_read_stmt(), assign_stmt(
        "line",
        spl_file_object_drop_new_line_expr(cast_expr(
            CastType::String,
            function_call("fgets", vec![file_stream_expr()]),
        )),
    )]
}

/// Wraps a line in the trim `DROP_NEW_LINE` performs, leaving it alone when the flag is clear.
///
/// MEASURED on `php -n` 8.5.6: the flag removes the TRAILING terminator and nothing else — a line
/// reading `"a\rb\n"` becomes `"a\rb"`, keeping the interior carriage return, while `"c\r\n"`
/// becomes `"c"`. That is `rtrim($line, "\r\n")`, and it is safe as a trailing trim because a line
/// never holds an interior `\n` to eat.
fn spl_file_object_drop_new_line_expr(line: Expr) -> Expr {
    expr(crate::parser::ast::ExprKind::Ternary {
        condition: Box::new(flag_enabled_expr(
            file_object_flags_expr(),
            SPL_FILE_DROP_NEW_LINE,
        )),
        then_expr: Box::new(function_call(
            "rtrim",
            vec![line.clone(), string_expr("\r\n")],
        )),
        else_expr: Box::new(line),
    })
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
/// `$length` is FORWARDED rather than reimplemented: php's rule here is the plain builtin's own,
/// and they agree byte for byte. MEASURED on `php -n` 8.5.6 over `"abcdef"` — `null` writes 6,
/// `0` writes 0, `3` writes 3, `99` writes 6, and a NEGATIVE length writes 0 without a
/// diagnostic. An absent `$length` still calls the builtin with two arguments, so the builtin's
/// own default stays the one in force.
fn spl_file_object_fwrite_body() -> Vec<Stmt> {
    let mut body = vec![if_stmt(
        binary_expr(var_expr("length"), BinOp::StrictEq, null_expr()),
        vec![assign_stmt(
            "bytes",
            function_call("fwrite", vec![file_stream_expr(), var_expr("data")]),
        )],
        Some(vec![assign_stmt(
            "bytes",
            function_call(
                "fwrite",
                vec![file_stream_expr(), var_expr("data"), var_expr("length")],
            ),
        )]),
    )];
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

/// Builds php 8.4's `$escape` deprecation for one SPL CSV method.
///
/// The two wordings are php's own, MEASURED byte for byte on `php -n` 8.5.6 rather than guessed
/// from each other: `setCsvControl()` has NO comma after "provided" and stops at "change", while
/// `fgetcsv()` / `fputcsv()` put commas around "as its default value will change" and add the
/// "either explicitly or via SplFileObject::setCsvControl()" tail. php names the DECLARING class,
/// so an `SplTempFileObject` still reports `SplFileObject::fgetcsv()` — which is what this
/// spelling gives, since both classes share these bodies.
///
/// `__rt_diag_warning` supplies the blank line before, php's ` in <file> on line <n>` after, and
/// the `@` suppression, so the argument is the complete line and nothing else.
///
/// VERSION-GATED, like the notice the CSV builtins raise: php 8.4 introduced it and 8.2 / 8.3
/// print nothing, so a `--php-version=8.3` build that emitted it would be noisier than the
/// interpreter it imitates. The gate is here rather than in the emitted PHP because the profile
/// is already fixed when these bodies are built — `pipeline::compile` records it before the
/// parse — so the statement simply is not there at all below 8.4.
fn spl_csv_escape_deprecation_stmts(method: &str, via_set_csv_control: bool) -> Vec<Stmt> {
    if crate::codegen::compile_php_version() < crate::php_version::PhpVersion::Php84 {
        return Vec::new();
    }
    let message = if via_set_csv_control {
        format!(
            "Deprecated: SplFileObject::{method}(): the $escape parameter must be provided, as \
             its default value will change, either explicitly or via \
             SplFileObject::setCsvControl()\n"
        )
    } else {
        format!(
            "Deprecated: SplFileObject::{method}(): the $escape parameter must be provided as \
             its default value will change\n"
        )
    };
    vec![expr_stmt(function_call(
        "__elephc_deprecated",
        vec![string_expr(&message)],
    ))]
}

/// Guards one CSV read or write with php 8.4's `$escape` deprecation.
///
/// Empty below 8.4, so the branch itself is absent rather than merely never taken.
fn spl_csv_escape_deprecation_guard(method: &str) -> Vec<Stmt> {
    let notice = spl_csv_escape_deprecation_stmts(method, true);
    if notice.is_empty() {
        return Vec::new();
    }
    vec![if_stmt(spl_csv_escape_omitted_expr(), notice, None)]
}

/// Returns `$escape === null && !$this->escapeProvided` — when a CSV read or write deprecates.
///
/// Both halves are needed. MEASURED on `php -n` 8.5.6: `$f->fgetcsv()` on a fresh object
/// deprecates, and the same call after `$f->setCsvControl(",", '"', "\\")` is silent.
fn spl_csv_escape_omitted_expr() -> Expr {
    binary_expr(
        binary_expr(var_expr("escape"), BinOp::StrictEq, null_expr()),
        BinOp::And,
        not_expr(property_access(this_expr(), "escapeProvided")),
    )
}

/// Builds SplFileObject setCsvControl().
///
/// An omitted `$escape` RESETS the stored one rather than keeping it: MEASURED, `setCsvControl(",",
/// '"', "#")` then `setCsvControl(";")` answers `getCsvControl() === [";", "\"", "\\"]`, so the
/// `"#"` is gone. The `escapeProvided` flag does NOT come back off with it — the `fgetcsv()` after
/// that pair is silent — which is why the flag is only ever set, never cleared.
fn spl_file_object_set_csv_control_body() -> Vec<Stmt> {
    let mut omitted = vec![property_assign_stmt(this_expr(), "escape", string_expr("\\"))];
    omitted.extend(spl_csv_escape_deprecation_stmts("setCsvControl", false));
    vec![
        property_assign_stmt(this_expr(), "delimiter", var_expr("separator")),
        property_assign_stmt(this_expr(), "enclosure", var_expr("enclosure")),
        if_stmt(
            binary_expr(var_expr("escape"), BinOp::StrictEq, null_expr()),
            omitted,
            Some(vec![
                property_assign_stmt(this_expr(), "escape", var_expr("escape")),
                property_assign_stmt(this_expr(), "escapeProvided", bool_expr(true)),
            ]),
        ),
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
    let mut body = spl_csv_escape_deprecation_guard("fgetcsv");
    body.extend(vec![
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
        // -- the trailing record php reads out of its LINE model --
        //
        // MEASURED on `php -n` 8.5.6, four calls per shape: a file ending in a newline answers
        // `a+b`, `c+d`, `[NULL]`, `false`; one that does not stops at `false`; an
        // `SplTempFileObject` stops at `false`; and an EMPTY file answers `[NULL]` first. The
        // BUILTIN is right about the descriptor — a plain `fgetcsv($h, …)` answers `false` at
        // each of those — and php's method does not read the descriptor. It reads the line model,
        // which has one more element after a trailing newline and none after a temp stream, and
        // `count($this->lines)` already carries both rules.
        if_stmt(
            binary_expr(
                binary_expr(var_expr("row"), BinOp::StrictEq, bool_expr(false)),
                BinOp::And,
                binary_expr(
                    file_line_number_expr(),
                    BinOp::Lt,
                    count_expr(file_lines_expr()),
                ),
            ),
            vec![assign_stmt(
                "row",
                function_call(
                    "str_getcsv",
                    vec![
                        string_expr(""),
                        csv_control_or_state_expr("separator", "delimiter"),
                        csv_control_or_state_expr("enclosure", "enclosure"),
                        // Spelled out even though an empty subject cannot use it: omitting
                        // `$escape` is what raises the 8.4 deprecation, and a notice the user's
                        // program never asked for would come out of it.
                        csv_control_or_state_expr("escape", "escape"),
                    ],
                ),
            )],
            None,
        ),
        // php's key() after a CSV read names the record it just read, not the one after it —
        // the FIRST read of a fresh object leaves the key where it was and only later reads
        // advance it. MEASURED on `php -n` 8.5.6 over a three-line file: `fgetcsv()` answers
        // keys 0, 1, 2, 3 where `fgets()` answers 1, 2, 3. elephc advanced unconditionally and
        // was one ahead at every step. `hasReadLine` is the same flag `fscanf()` already uses
        // for the same rule.
        if_stmt(
            property_access(this_expr(), "hasReadLine"),
            vec![property_assign_stmt(
                this_expr(),
                "lineNumber",
                binary_expr(file_line_number_expr(), BinOp::Add, int_expr(1)),
            )],
            None,
        ),
        property_assign_stmt(this_expr(), "hasReadLine", bool_expr(true)),
        return_stmt(var_expr("row")),
    ]);
    body
}

/// Builds SplFileObject fputcsv().
///
/// `$eol` is FORWARDED. The method declared it and then called the function with five
/// arguments, so `$f->fputcsv(["a", "b"], ",", '"', "\\", "")` — php's "write no terminator",
/// which answers 3 — still wrote a newline and answered 4, and a custom terminator was
/// silently discarded.
fn spl_file_object_fputcsv_body() -> Vec<Stmt> {
    let mut body = spl_csv_escape_deprecation_guard("fputcsv");
    body.extend(vec![
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
    ]);
    body.extend(file_object_load_lines_body(file_backing_path_arg_expr()));
    body.push(return_stmt(var_expr("bytes")));
    body
}

/// Builds a directory constructor body.
fn directory_construct_body(
    class_name: &str,
    directory: Expr,
    flags: Expr,
    filter_dots: bool,
    entries_are_paths: bool,
) -> Vec<Stmt> {
    let mut body = directory_openable_guard_stmts(class_name, directory.clone());
    body.extend(vec![
        property_assign_stmt(this_expr(), "directory", string_copy_expr(directory.clone())),
        property_assign_stmt(this_expr(), "fsFlags", flags.clone()),
        property_assign_stmt(this_expr(), "entriesArePathnames", bool_expr(entries_are_paths)),
    ]);
    body.extend(directory_rebuild_entries_body(directory, flags, filter_dots));
    body.extend(vec![
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        expr_stmt(method_call(this_expr(), "__elephcRefreshPath", Vec::new())),
    ]);
    body
}

/// Builds php's two refusals for a directory a constructor cannot open.
///
/// The class NAMES ITSELF in both messages — `RecursiveDirectoryIterator` says its own name, not
/// the `FilesystemIterator` it extends — so the name travels in rather than being read from
/// `static::class`, which these synthesized bodies have no need of otherwise.
fn directory_openable_guard_stmts(class_name: &str, directory: Expr) -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(
                string_copy_expr(directory.clone()),
                BinOp::StrictEq,
                string_expr(""),
            ),
            vec![throw_stmt(new_object_expr(
                "ValueError",
                vec![string_expr(&format!(
                    "{}::__construct(): Argument #1 ($directory) must not be empty",
                    class_name
                ))],
            ))],
            None,
        ),
        if_stmt(
            not_expr(function_call("is_dir", vec![string_copy_expr(directory.clone())])),
            vec![throw_stmt(new_object_expr(
                "UnexpectedValueException",
                vec![binary_expr(
                    binary_expr(
                        binary_expr(
                            string_expr(&format!("{}::__construct(", class_name)),
                            BinOp::Concat,
                            string_copy_expr(directory.clone()),
                        ),
                        BinOp::Concat,
                        string_expr("): Failed to open directory: "),
                    ),
                    BinOp::Concat,
                    expr(crate::parser::ast::ExprKind::Ternary {
                        condition: Box::new(function_call(
                            "file_exists",
                            vec![string_copy_expr(directory)],
                        )),
                        // php reports the reason `opendir` would: a path that EXISTS but is not a
                        // directory is `Not a directory`, and one that does not is `No such file
                        // or directory`.
                        then_expr: Box::new(string_expr("Not a directory")),
                        else_expr: Box::new(string_expr("No such file or directory")),
                    }),
                )],
            ))],
            None,
        ),
    ]
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
