//! Purpose:
//! Injects the supported `ZipArchive` builtin class metadata (read surface).
//! Maps php's `ext/zip` reading API onto the `zip://` wrapper and the ZIP
//! central-directory stat records the elephc-phar bridge serializes.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Method bodies are synthetic PHP-like AST, so normal checker and EIR lowering
//!   own behavior — there is no dedicated ZipArchive runtime.
//! - `open()` seeds `$records` with the bridge's NUL-joined stat records and
//!   `$names` with just the entry names; every accessor then reads that state, and
//!   entry BYTES come from the `zip://archive#entry` wrapper. One archive read at
//!   open, one wrapper read per `getFromName()`, exactly as php's libzip does.
//! - The read accessors are silent on failure, which is what php measured: only
//!   `open()` reports anything, and it does so through its return value.

use std::collections::HashMap;

use crate::parser::ast::{
    BinOp, CastType, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;

use super::common::*;

/// `ZipArchive::CREATE` — create the archive when it does not exist.
const ZIP_CREATE: i64 = 1;
/// `ZipArchive::EXCL` — fail when the archive already exists.
const ZIP_EXCL: i64 = 2;
/// `ZipArchive::OVERWRITE` — start from an empty archive, whatever is on disk.
const ZIP_OVERWRITE: i64 = 8;
/// `ZipArchive::FL_NOCASE` — match an entry name case-insensitively.
const ZIP_FL_NOCASE: i64 = 1;
/// `ZipArchive::ER_NOENT` — no such file.
const ZIP_ER_NOENT: i64 = 9;
/// `ZipArchive::ER_EXISTS` — the file exists and `EXCL` was asked for.
const ZIP_ER_EXISTS: i64 = 10;
/// `ZipArchive::ER_NOZIP` — the file exists but is not a ZIP archive.
const ZIP_ER_NOZIP: i64 = 19;

/// php's `ValueError` for an empty `$filename`, measured verbatim on `php -n` 8.5.6.
const EMPTY_FILENAME_MESSAGE: &str =
    "ZipArchive::open(): Argument #1 ($filename) must not be empty";

/// How many `\0`-joined fields one serialized stat record holds.
///
/// The entry NAME is the last one, so a bounded split leaves a name containing a
/// `\0` intact instead of scattering it across extra elements.
const RECORD_FIELDS: i64 = 8;

/// Field positions inside one serialized record; see `zip_stat_records` in the bridge.
const FIELD_INDEX: i64 = 0;
/// CRC-32 of the entry's original bytes.
const FIELD_CRC: i64 = 1;
/// Uncompressed byte length.
const FIELD_SIZE: i64 = 2;
/// Stored byte length.
const FIELD_COMP_SIZE: i64 = 3;
/// ZIP compression method.
const FIELD_COMP_METHOD: i64 = 4;
/// ZIP encryption method.
const FIELD_ENCRYPTION_METHOD: i64 = 5;
/// Unix modification timestamp, already converted from the DOS fields.
const FIELD_MTIME: i64 = 6;
/// The entry name, last so a `\0` inside it survives the bounded split.
const FIELD_NAME: i64 = 7;

/// Inserts the supported ZIP classes into the builtin metadata registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "ZipArchive".to_string(),
        FlattenedClass {
            name: "ZipArchive".to_string(),
            span: crate::span::Span::dummy(),
            extends: None,
            implements: vec!["Countable".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: zip_properties(),
            methods: zip_methods(),
            attributes: Vec::new(),
            constants: zip_constants(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
}

/// Builds a php-visible readable property.
fn public_property(name: &str, type_expr: TypeExpr, default: Expr) -> ClassProperty {
    storage_property_with_visibility(name, Some(type_expr), Some(default), Visibility::Public)
}

/// Builds the php-visible properties plus the private state the accessors read.
///
/// Measured on `php -n` 8.5.6: `filename`, `numFiles`, `status`, `statusSys`,
/// `comment` and `lastId` are readable properties, not methods, and `numFiles`
/// returns to `0` and `filename` to `""` after `close()`.
fn zip_properties() -> Vec<ClassProperty> {
    vec![
        public_property("filename", TypeExpr::Str, string_expr("")),
        public_property("numFiles", TypeExpr::Int, int_expr(0)),
        public_property("status", TypeExpr::Int, int_expr(0)),
        public_property("statusSys", TypeExpr::Int, int_expr(0)),
        public_property("comment", TypeExpr::Str, string_expr("")),
        public_property("lastId", TypeExpr::Int, int_expr(-1)),
        // The path exactly as the caller wrote it: it is what the `zip://` URLs are
        // built from, and a resolved one would break a relative open after a chdir.
        storage_property_with_default("path", TypeExpr::Str, Some(string_expr(""))),
        storage_property_with_default("records", array_type(), Some(empty_array_expr())),
        storage_property_with_default("names", array_type(), Some(empty_array_expr())),
        storage_property_with_default("password", TypeExpr::Str, Some(string_expr(""))),
        // `OVERWRITE` makes php DELETE the archive at close when nothing was added.
        storage_property_with_default("emptyOnClose", TypeExpr::Bool, Some(bool_expr(false))),
    ]
}

/// Builds every `ZipArchive` constant php publishes that the read surface can honour.
///
/// Measured with `(new ReflectionClass("ZipArchive"))->getConstants()` on
/// `php -n` 8.5.6. The open flags, the error codes, the lookup flags, the
/// compression methods and the encryption methods are all here; the ones naming
/// behaviour this class does not implement are deliberately absent rather than
/// present and inert.
fn zip_constants() -> Vec<ClassConst> {
    vec![
        class_const("CREATE", ZIP_CREATE),
        class_const("EXCL", ZIP_EXCL),
        class_const("CHECKCONS", 4),
        class_const("OVERWRITE", ZIP_OVERWRITE),
        class_const("RDONLY", 16),
        class_const("FL_NOCASE", ZIP_FL_NOCASE),
        class_const("FL_NODIR", 2),
        class_const("FL_COMPRESSED", 4),
        class_const("FL_UNCHANGED", 8),
        class_const("FL_ENCRYPTED", 32),
        class_const("FL_ENC_GUESS", 0),
        class_const("FL_ENC_RAW", 64),
        class_const("FL_ENC_STRICT", 128),
        class_const("FL_LOCAL", 256),
        class_const("FL_CENTRAL", 512),
        class_const("FL_ENC_UTF_8", 2048),
        class_const("FL_ENC_CP437", 4096),
        class_const("CM_DEFAULT", -1),
        class_const("CM_STORE", 0),
        class_const("CM_SHRINK", 1),
        class_const("CM_REDUCE_1", 2),
        class_const("CM_REDUCE_2", 3),
        class_const("CM_REDUCE_3", 4),
        class_const("CM_REDUCE_4", 5),
        class_const("CM_IMPLODE", 6),
        class_const("CM_DEFLATE", 8),
        class_const("CM_DEFLATE64", 9),
        class_const("CM_PKWARE_IMPLODE", 10),
        class_const("CM_BZIP2", 12),
        class_const("CM_LZMA", 14),
        class_const("CM_TERSE", 18),
        class_const("CM_LZ77", 19),
        class_const("CM_LZMA2", 33),
        class_const("CM_ZSTD", 93),
        class_const("CM_XZ", 95),
        class_const("CM_WAVPACK", 97),
        class_const("CM_PPMD", 98),
        class_const("EM_NONE", 0),
        class_const("EM_TRAD_PKWARE", 1),
        class_const("EM_AES_128", 257),
        class_const("EM_AES_192", 258),
        class_const("EM_AES_256", 259),
        class_const("EM_UNKNOWN", 65535),
        class_const("ER_OK", 0),
        class_const("ER_MULTIDISK", 1),
        class_const("ER_RENAME", 2),
        class_const("ER_CLOSE", 3),
        class_const("ER_SEEK", 4),
        class_const("ER_READ", 5),
        class_const("ER_WRITE", 6),
        class_const("ER_CRC", 7),
        class_const("ER_ZIPCLOSED", 8),
        class_const("ER_NOENT", ZIP_ER_NOENT),
        class_const("ER_EXISTS", ZIP_ER_EXISTS),
        class_const("ER_OPEN", 11),
        class_const("ER_TMPOPEN", 12),
        class_const("ER_ZLIB", 13),
        class_const("ER_MEMORY", 14),
        class_const("ER_CHANGED", 15),
        class_const("ER_COMPNOTSUPP", 16),
        class_const("ER_EOF", 17),
        class_const("ER_INVAL", 18),
        class_const("ER_NOZIP", ZIP_ER_NOZIP),
        class_const("ER_INTERNAL", 20),
        class_const("ER_INCONS", 21),
        class_const("ER_REMOVE", 22),
        class_const("ER_DELETED", 23),
        class_const("ER_ENCRNOTSUPP", 24),
        class_const("ER_RDONLY", 25),
        class_const("ER_NOPASSWD", 26),
        class_const("ER_WRONGPASSWD", 27),
        class_const("ER_OPNOTSUPP", 28),
        class_const("ER_INUSE", 29),
        class_const("ER_TELL", 30),
        class_const("ER_COMPRESSED_DATA", 31),
        class_const("ER_CANCELLED", 32),
        class_const("ER_DATA_LENGTH", 33),
        class_const("ER_NOT_ALLOWED", 34),
        class_const("ER_TRUNCATED_ZIP", 35),
    ]
}

/// Builds the supported open/close, lookup, stat, and read methods.
fn zip_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "open",
            vec![
                param("filename", TypeExpr::Str),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(mixed_type()),
            zip_open_body(),
        ),
        method_with_body("close", Vec::new(), Some(TypeExpr::Bool), zip_close_body()),
        method_with_body(
            "count",
            Vec::new(),
            Some(TypeExpr::Int),
            return_body(property_access(this_expr(), "numFiles")),
        ),
        method_with_body(
            "getNameIndex",
            vec![
                param("index", TypeExpr::Int),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(mixed_type()),
            zip_get_name_index_body(),
        ),
        method_with_body(
            "locateName",
            vec![
                param("name", TypeExpr::Str),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(mixed_type()),
            zip_locate_name_body(),
        ),
        method_with_body(
            "statIndex",
            vec![
                param("index", TypeExpr::Int),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(mixed_type()),
            zip_stat_index_body(),
        ),
        method_with_body(
            "statName",
            vec![
                param("name", TypeExpr::Str),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(mixed_type()),
            zip_stat_name_body(),
        ),
        method_with_body(
            "getFromIndex",
            vec![
                param("index", TypeExpr::Int),
                param_default("len", TypeExpr::Int, int_expr(0)),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(mixed_type()),
            zip_get_from_index_body(),
        ),
        method_with_body(
            "getFromName",
            vec![
                param("name", TypeExpr::Str),
                param_default("len", TypeExpr::Int, int_expr(0)),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(mixed_type()),
            zip_get_from_name_body(),
        ),
        method_with_body(
            "getStream",
            vec![param("name", TypeExpr::Str)],
            Some(mixed_type()),
            zip_get_stream_body(),
        ),
        method_with_body(
            "getStreamName",
            vec![
                param("name", TypeExpr::Str),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(mixed_type()),
            zip_get_stream_body(),
        ),
        method_with_body(
            "getStreamIndex",
            vec![
                param("index", TypeExpr::Int),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(mixed_type()),
            zip_get_stream_index_body(),
        ),
        method_with_body(
            "setPassword",
            vec![param("password", TypeExpr::Str)],
            Some(TypeExpr::Bool),
            zip_set_password_body(),
        ),
        method_with_body(
            "getStatusString",
            Vec::new(),
            Some(TypeExpr::Str),
            return_body(string_expr("No error")),
        ),
        method_with_body(
            "extractTo",
            vec![
                param("directory", TypeExpr::Str),
                param_default("files", mixed_type(), null_expr()),
            ],
            Some(TypeExpr::Bool),
            zip_extract_to_body(),
        ),
        // Compiler-internal: php has no such method, and the leading `__elephc`
        // keeps it out of any name a program could reach.
        method_with_body(
            "__elephcZipCleanPath",
            vec![param("name", TypeExpr::Str)],
            Some(TypeExpr::Str),
            zip_clean_path_body(),
        ),
    ]
}

/// `extractTo($directory, $files = null)`.
///
/// Measured on `php -n` 8.5.6:
///
/// ```text
/// extractTo("ex1")                       => bool(true), the whole archive
/// extractTo("ex2", "f.txt")              => bool(true), that one entry
/// extractTo("ex3", ["f.txt","sub/n.txt"])=> bool(true), those two
/// extractTo("/no/such/root/x")           => bool(false)  and NO warning
/// extractTo("ex4", "nope.txt")           => bool(false)
/// extractTo("ex9", ["f.txt","nope.txt"]) => bool(false)
/// extractTo("ex9", [])                   => bool(false)
/// extractTo("")                          => bool(false)
/// ```
///
/// An existing file is overwritten, and the extracted file carries the ENTRY's
/// mtime, not the extraction time. A directory member (`dd/`) becomes a directory,
/// not an empty file.
fn zip_extract_to_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        if_stmt(
            binary_expr(var_expr("directory"), BinOp::StrictEq, string_expr("")),
            vec![return_stmt(bool_expr(false))],
            None,
        ),
        // `$files` is php's `array|string|null`: absent means the whole archive, a
        // string means that one entry, an array means those.
        typed_assign_stmt("wanted", array_type(), empty_array_expr()),
        if_stmt(
            binary_expr(var_expr("files"), BinOp::StrictEq, null_expr()),
            vec![assign_stmt("wanted", property_access(this_expr(), "names"))],
            // The array arm copies element by element rather than casting: it keeps
            // `$wanted` a plain list of strings whatever the caller passed.
            Some(vec![if_stmt(
                function_call("is_array", vec![var_expr("files")]),
                vec![foreach_stmt(
                    var_expr("files"),
                    None,
                    "one",
                    vec![array_push_stmt(
                        "wanted",
                        cast_expr(CastType::String, var_expr("one")),
                    )],
                )],
                Some(vec![array_push_stmt(
                    "wanted",
                    cast_expr(CastType::String, var_expr("files")),
                )]),
            )]),
        ),
        if_stmt(
            binary_expr(count_expr(var_expr("wanted")), BinOp::StrictEq, int_expr(0)),
            vec![return_stmt(bool_expr(false))],
            None,
        ),
        make_directory_guard("directory"),
        foreach_stmt(
            var_expr("wanted"),
            None,
            "requested",
            vec![
                assign_stmt(
                    "index",
                    method_call(
                        this_expr(),
                        "locateName",
                        vec![cast_expr(CastType::String, var_expr("requested"))],
                    ),
                ),
                // php bails on the FIRST name it cannot find, keeping whatever it
                // already wrote.
                missing_entry_guard("index"),
                assign_stmt(
                    "entry",
                    array_access(property_access(this_expr(), "names"), var_expr("index")),
                ),
                assign_stmt(
                    "clean",
                    method_call(
                        this_expr(),
                        "__elephcZipCleanPath",
                        vec![var_expr("entry")],
                    ),
                ),
                assign_stmt(
                    "target",
                    binary_expr(
                        binary_expr(var_expr("directory"), BinOp::Concat, string_expr("/")),
                        BinOp::Concat,
                        var_expr("clean"),
                    ),
                ),
                if_stmt(
                    binary_expr(var_expr("clean"), BinOp::StrictEq, string_expr("")),
                    // A name that normalizes to nothing at all names no file to write.
                    vec![assign_stmt("target", var_expr("directory"))],
                    Some(vec![if_stmt(
                        binary_expr(
                            function_call(
                                "substr",
                                vec![var_expr("entry"), int_expr(-1)],
                            ),
                            BinOp::StrictEq,
                            string_expr("/"),
                        ),
                        // A directory member becomes a DIRECTORY, not an empty file.
                        vec![make_directory_guard("target")],
                        Some(vec![
                            assign_stmt(
                                "parent",
                                function_call("dirname", vec![var_expr("target")]),
                            ),
                            make_directory_guard("parent"),
                            assign_stmt(
                                "bytes",
                                suppress_expr(function_call(
                                    "file_get_contents",
                                    vec![entry_url_expr(var_expr("entry"))],
                                )),
                            ),
                            if_stmt(
                                binary_expr(
                                    var_expr("bytes"),
                                    BinOp::StrictEq,
                                    bool_expr(false),
                                ),
                                vec![return_stmt(bool_expr(false))],
                                None,
                            ),
                            assign_stmt(
                                "written",
                                suppress_expr(function_call(
                                    "file_put_contents",
                                    vec![
                                        var_expr("target"),
                                        cast_expr(CastType::String, var_expr("bytes")),
                                    ],
                                )),
                            ),
                            if_stmt(
                                binary_expr(
                                    var_expr("written"),
                                    BinOp::StrictEq,
                                    bool_expr(false),
                                ),
                                vec![return_stmt(bool_expr(false))],
                                None,
                            ),
                            // The extracted file carries the ENTRY's mtime.
                            assign_stmt(
                                "stat",
                                method_call(this_expr(), "statIndex", vec![var_expr("index")]),
                            ),
                            expr_stmt(suppress_expr(function_call(
                                "touch",
                                vec![
                                    var_expr("target"),
                                    cast_expr(
                                        CastType::Int,
                                        array_access(var_expr("stat"), string_expr("mtime")),
                                    ),
                                ],
                            ))),
                        ]),
                    )]),
                ),
            ],
        ),
        return_stmt(bool_expr(true)),
    ]
}

/// `__elephcZipCleanPath($name)`: the path php actually extracts an entry to.
///
/// php refuses to let an archive write outside the destination, and it does so by
/// NORMALIZING the stored name rather than by rejecting it. Measured on `php -n`
/// 8.5.6 by extracting an archive whose entries carry these names:
///
/// ```text
/// "../up.txt"           => "up.txt"
/// "a/../b.txt"          => "b.txt"
/// "a/b/../c.txt"        => "a/c.txt"      (`..` pops ONE segment, it does not reset)
/// "a/b/../../../d.txt"  => "d.txt"        (popping an empty stack is a no-op)
/// "./dot.txt"           => "dot.txt"
/// "/abs.txt"            => "abs.txt"
/// "..//e.txt"           => "e.txt"
/// "x/./y.txt"           => "x/y.txt"
/// "f..g.txt"            => "f..g.txt"     (only a WHOLE `..` segment counts)
/// "a/..b/h.txt"         => "a/..b/h.txt"
/// "..\\win.txt"         => "..\\win.txt"  (a backslash is not a separator here)
/// ```
///
/// Every one of those landed inside the destination directory; not one escaped it.
fn zip_clean_path_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        typed_assign_stmt("parts", array_type(), empty_array_expr()),
        foreach_stmt(
            function_call("explode", vec![string_expr("/"), var_expr("name")]),
            None,
            "segment",
            vec![if_stmt(
                binary_expr(
                    binary_expr(var_expr("segment"), BinOp::StrictNotEq, string_expr("")),
                    BinOp::And,
                    binary_expr(var_expr("segment"), BinOp::StrictNotEq, string_expr(".")),
                ),
                vec![if_stmt(
                    binary_expr(var_expr("segment"), BinOp::StrictEq, string_expr("..")),
                    vec![
                        assign_stmt("depth", count_expr(var_expr("parts"))),
                        // Popping an empty stack is a no-op, which is why
                        // `a/b/../../../d.txt` still lands at `d.txt`.
                        if_stmt(
                            binary_expr(var_expr("depth"), BinOp::Gt, int_expr(0)),
                            vec![assign_stmt(
                                "parts",
                                function_call(
                                    "array_slice",
                                    vec![
                                        var_expr("parts"),
                                        int_expr(0),
                                        binary_expr(var_expr("depth"), BinOp::Sub, int_expr(1)),
                                    ],
                                ),
                            )],
                            None,
                        ),
                    ],
                    Some(vec![array_push_stmt("parts", var_expr("segment"))]),
                )],
                None,
            )],
        ),
        return_stmt(function_call(
            "implode",
            vec![string_expr("/"), var_expr("parts")],
        )),
    ]
}

/// Creates the directory named by `$var` when it is not one already, or returns `false`.
fn make_directory_guard(var: &str) -> crate::parser::ast::Stmt {
    if_stmt(
        not_expr(function_call("is_dir", vec![var_expr(var)])),
        vec![if_stmt(
            not_expr(suppress_expr(function_call(
                "mkdir",
                vec![var_expr(var), int_expr(0o777), bool_expr(true)],
            ))),
            vec![return_stmt(bool_expr(false))],
            None,
        )],
        None,
    )
}

/// `open($filename, $flags = 0)`.
///
/// Measured on `php -n` 8.5.6, one line per branch below:
///
/// ```text
/// open("a.zip")                          => bool(true)   numFiles 3
/// open("a.zip", RDONLY)                  => bool(true)
/// open("ghost.zip")                      => int(9)   ER_NOENT
/// open("ghost.zip", RDONLY)              => int(9)   ER_NOENT
/// open("new.zip", CREATE)                => bool(true)   numFiles 0, no file created
/// open("a.zip", CREATE)                  => bool(true)   numFiles 3, opens the existing one
/// open("a.zip", CREATE|EXCL)             => int(10)  ER_EXISTS
/// open("a.zip", OVERWRITE)               => bool(true)   numFiles 0
/// open("notzip.txt")                     => int(19)  ER_NOZIP
/// open("")                               => ValueError: ZipArchive::open(): Argument #1
///                                           ($filename) must not be empty
/// ```
fn zip_open_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        // php raises before it looks at the filesystem at all.
        if_stmt(
            binary_expr(var_expr("filename"), BinOp::StrictEq, string_expr("")),
            vec![throw_stmt(new_object_expr(
                "ValueError",
                vec![string_expr(EMPTY_FILENAME_MESSAGE)],
            ))],
            None,
        ),
        assign_stmt(
            "exists",
            function_call("is_file", vec![var_expr("filename")]),
        ),
        // EXCL is checked first: php answers ER_EXISTS even when CREATE is also set.
        if_stmt(
            binary_expr(
                var_expr("exists"),
                BinOp::And,
                binary_expr(
                    binary_expr(var_expr("flags"), BinOp::BitAnd, int_expr(ZIP_EXCL)),
                    BinOp::StrictNotEq,
                    int_expr(0),
                ),
            ),
            vec![return_stmt(int_expr(ZIP_ER_EXISTS))],
            None,
        ),
        // Without CREATE or OVERWRITE a missing archive is simply absent.
        if_stmt(
            binary_expr(
                not_expr(var_expr("exists")),
                BinOp::And,
                binary_expr(
                    binary_expr(
                        var_expr("flags"),
                        BinOp::BitAnd,
                        int_expr(ZIP_CREATE | ZIP_OVERWRITE),
                    ),
                    BinOp::StrictEq,
                    int_expr(0),
                ),
            ),
            vec![return_stmt(int_expr(ZIP_ER_NOENT))],
            None,
        ),
        assign_stmt("raw", empty_array_expr()),
        // OVERWRITE ignores whatever is on disk, so the archive is never even parsed.
        if_stmt(
            binary_expr(
                var_expr("exists"),
                BinOp::And,
                binary_expr(
                    binary_expr(var_expr("flags"), BinOp::BitAnd, int_expr(ZIP_OVERWRITE)),
                    BinOp::StrictEq,
                    int_expr(0),
                ),
            ),
            vec![
                assign_stmt(
                    "raw",
                    function_call("__elephc_zip_stat_entries", vec![var_expr("filename")]),
                ),
                // An EMPTY serialization means the bridge found no ZIP at all; an
                // archive holding no entries still carries its count record.
                if_stmt(
                    binary_expr(
                        count_expr(var_expr("raw")),
                        BinOp::StrictEq,
                        int_expr(0),
                    ),
                    vec![return_stmt(int_expr(ZIP_ER_NOZIP))],
                    None,
                ),
            ],
            None,
        ),
        property_assign_stmt(this_expr(), "path", var_expr("filename")),
        // php reports the RESOLVED path; a path that does not exist yet has none, and
        // php then reports what it was given.
        assign_stmt(
            "resolved",
            function_call("realpath", vec![var_expr("filename")]),
        ),
        if_stmt(
            binary_expr(var_expr("resolved"), BinOp::StrictEq, bool_expr(false)),
            vec![property_assign_stmt(
                this_expr(),
                "filename",
                var_expr("filename"),
            )],
            Some(vec![property_assign_stmt(
                this_expr(),
                "filename",
                var_expr("resolved"),
            )]),
        ),
        property_assign_stmt(this_expr(), "records", empty_array_expr()),
        property_assign_stmt(this_expr(), "names", empty_array_expr()),
        assign_stmt("position", int_expr(0)),
        // Element 0 is the bridge's count record, which the entry list skips.
        foreach_stmt(
            var_expr("raw"),
            None,
            "record",
            vec![
                if_stmt(
                    binary_expr(var_expr("position"), BinOp::Gt, int_expr(0)),
                    vec![
                        assign_stmt(
                            "fields",
                            function_call(
                                "explode",
                                vec![
                                    nul_expr(),
                                    var_expr("record"),
                                    int_expr(RECORD_FIELDS),
                                ],
                            ),
                        ),
                        property_array_push_stmt(this_expr(), "records", var_expr("record")),
                        property_array_push_stmt(
                            this_expr(),
                            "names",
                            array_access(var_expr("fields"), int_expr(FIELD_NAME)),
                        ),
                    ],
                    None,
                ),
                increment_stmt("position"),
            ],
        ),
        property_assign_stmt(
            this_expr(),
            "numFiles",
            count_expr(property_access(this_expr(), "names")),
        ),
        property_assign_stmt(this_expr(), "status", int_expr(0)),
        property_assign_stmt(this_expr(), "statusSys", int_expr(0)),
        property_assign_stmt(
            this_expr(),
            "emptyOnClose",
            binary_expr(
                binary_expr(var_expr("flags"), BinOp::BitAnd, int_expr(ZIP_OVERWRITE)),
                BinOp::StrictNotEq,
                int_expr(0),
            ),
        ),
        return_stmt(bool_expr(true)),
    ]
}

/// `close()`.
///
/// Measured: `close()` answers `true`, `numFiles` returns to `0` and `filename` to
/// `""`. An archive opened with `OVERWRITE` and closed without a single addition is
/// REMOVED from disk — libzip deletes an archive that would hold nothing — and
/// `CREATE` on a missing file likewise leaves no file behind.
fn zip_close_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        if_stmt(
            binary_expr(
                property_access(this_expr(), "emptyOnClose"),
                BinOp::And,
                function_call("is_file", vec![property_access(this_expr(), "path")]),
            ),
            vec![expr_stmt(suppress_expr(function_call(
                "unlink",
                vec![property_access(this_expr(), "path")],
            )))],
            None,
        ),
        property_assign_stmt(this_expr(), "records", empty_array_expr()),
        property_assign_stmt(this_expr(), "names", empty_array_expr()),
        property_assign_stmt(this_expr(), "numFiles", int_expr(0)),
        property_assign_stmt(this_expr(), "filename", string_expr("")),
        property_assign_stmt(this_expr(), "path", string_expr("")),
        property_assign_stmt(this_expr(), "emptyOnClose", bool_expr(false)),
        return_stmt(bool_expr(true)),
    ]
}

/// `getNameIndex($index, $flags = 0)`: the stored name, or `false` out of range.
fn zip_get_name_index_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        index_out_of_range_guard(),
        return_stmt(array_access(
            property_access(this_expr(), "names"),
            var_expr("index"),
        )),
    ]
}

/// `locateName($name, $flags = 0)`: the index, or `false`.
///
/// Measured: an exact match wins outright, and `FL_NOCASE` is what makes
/// `locateName("F.TXT", ZipArchive::FL_NOCASE)` find `f.txt` where the plain call
/// answers `false`.
fn zip_locate_name_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        assign_stmt("total", count_expr(property_access(this_expr(), "names"))),
        assign_stmt("i", int_expr(0)),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("total")),
            vec![
                if_stmt(
                    binary_expr(
                        array_access(property_access(this_expr(), "names"), var_expr("i")),
                        BinOp::StrictEq,
                        var_expr("name"),
                    ),
                    vec![return_stmt(var_expr("i"))],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        if_stmt(
            binary_expr(
                binary_expr(var_expr("flags"), BinOp::BitAnd, int_expr(ZIP_FL_NOCASE)),
                BinOp::StrictNotEq,
                int_expr(0),
            ),
            vec![
                assign_stmt("j", int_expr(0)),
                while_stmt(
                    binary_expr(var_expr("j"), BinOp::Lt, var_expr("total")),
                    vec![
                        if_stmt(
                            binary_expr(
                                function_call(
                                    "strcasecmp",
                                    vec![
                                        array_access(
                                            property_access(this_expr(), "names"),
                                            var_expr("j"),
                                        ),
                                        var_expr("name"),
                                    ],
                                ),
                                BinOp::StrictEq,
                                int_expr(0),
                            ),
                            vec![return_stmt(var_expr("j"))],
                            None,
                        ),
                        increment_stmt("j"),
                    ],
                ),
            ],
            None,
        ),
        return_stmt(bool_expr(false)),
    ]
}

/// `statIndex($index, $flags = 0)`: php's eight-key array, in php's key order.
///
/// Measured on `php -n` 8.5.6 for a deflated 12-byte `f.txt`:
///
/// ```text
/// ["name" => "f.txt", "index" => 0, "crc" => 2936552237, "size" => 12,
///  "mtime" => 1786887576, "comp_size" => 14, "comp_method" => 8,
///  "encryption_method" => 0]
/// ```
///
/// The order is the one `var_dump()` prints, so the keys are inserted in it.
fn zip_stat_index_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        index_out_of_range_guard(),
        assign_stmt(
            "fields",
            function_call(
                "explode",
                vec![
                    nul_expr(),
                    array_access(property_access(this_expr(), "records"), var_expr("index")),
                    int_expr(RECORD_FIELDS),
                ],
            ),
        ),
        assign_stmt("stat", empty_assoc_array_expr()),
        array_assign_stmt(
            "stat",
            string_expr("name"),
            array_access(var_expr("fields"), int_expr(FIELD_NAME)),
        ),
        stat_int_field("index", FIELD_INDEX),
        stat_int_field("crc", FIELD_CRC),
        stat_int_field("size", FIELD_SIZE),
        stat_int_field("mtime", FIELD_MTIME),
        stat_int_field("comp_size", FIELD_COMP_SIZE),
        stat_int_field("comp_method", FIELD_COMP_METHOD),
        stat_int_field("encryption_method", FIELD_ENCRYPTION_METHOD),
        return_stmt(var_expr("stat")),
    ]
}

/// `statName($name, $flags = 0)`: the same array, found by name.
fn zip_stat_name_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        assign_stmt(
            "found",
            method_call(
                this_expr(),
                "locateName",
                vec![var_expr("name"), var_expr("flags")],
            ),
        ),
        missing_entry_guard("found"),
        return_stmt(method_call(
            this_expr(),
            "statIndex",
            vec![var_expr("found")],
        )),
    ]
}

/// `getFromIndex($index, $len = 0, $flags = 0)`: the entry bytes, or `false`.
///
/// The read goes through the `zip://` wrapper, which is the same bridge libzip's
/// own read would use, and it is SUPPRESSED: php answers a failed read with a bare
/// `false` and no diagnostic, unlike `file_get_contents()` on the same URL.
fn zip_get_from_index_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        index_out_of_range_guard(),
        return_stmt(suppress_expr(function_call(
            "file_get_contents",
            vec![entry_url_expr(array_access(
                property_access(this_expr(), "names"),
                var_expr("index"),
            ))],
        ))),
    ]
}

/// `getFromName($name, $len = 0, $flags = 0)`: the entry bytes, or `false`.
fn zip_get_from_name_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        assign_stmt(
            "found",
            method_call(
                this_expr(),
                "locateName",
                vec![var_expr("name"), var_expr("flags")],
            ),
        ),
        missing_entry_guard("found"),
        return_stmt(method_call(
            this_expr(),
            "getFromIndex",
            vec![var_expr("found")],
        )),
    ]
}

/// `getStream($name)` / `getStreamName($name, $flags = 0)`: a readable stream, or `false`.
///
/// Measured: `getStream("nope")` answers a bare `bool(false)` with no warning, so
/// the entry is located BEFORE any open is attempted.
fn zip_get_stream_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        assign_stmt(
            "found",
            method_call(this_expr(), "locateName", vec![var_expr("name")]),
        ),
        missing_entry_guard("found"),
        return_stmt(method_call(
            this_expr(),
            "getStreamIndex",
            vec![var_expr("found")],
        )),
    ]
}

/// `getStreamIndex($index, $flags = 0)`: a readable stream, or `false`.
fn zip_get_stream_index_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        index_out_of_range_guard(),
        return_stmt(suppress_expr(function_call(
            "fopen",
            vec![
                entry_url_expr(array_access(
                    property_access(this_expr(), "names"),
                    var_expr("index"),
                )),
                string_expr("r"),
            ],
        ))),
    ]
}

/// `setPassword($password)`: arms the ZipCrypto password the bridge reads with.
///
/// This is the same password `Phar` uses for an encrypted zip-based archive, so it
/// reaches the bridge through the existing helper rather than a second one.
fn zip_set_password_body() -> Vec<crate::parser::ast::Stmt> {
    vec![
        property_assign_stmt(this_expr(), "password", var_expr("password")),
        expr_stmt(function_call(
            "__elephc_phar_set_zip_password",
            vec![var_expr("password")],
        )),
        return_stmt(bool_expr(true)),
    ]
}

/// Returns `false` when `$index` names no entry, the way every indexed accessor does.
fn index_out_of_range_guard() -> crate::parser::ast::Stmt {
    if_stmt(
        binary_expr(
            binary_expr(var_expr("index"), BinOp::Lt, int_expr(0)),
            BinOp::Or,
            binary_expr(
                var_expr("index"),
                BinOp::GtEq,
                count_expr(property_access(this_expr(), "names")),
            ),
        ),
        vec![return_stmt(bool_expr(false))],
        None,
    )
}

/// Returns `false` when a `locateName()` result in `$name_var` found nothing.
fn missing_entry_guard(name_var: &str) -> crate::parser::ast::Stmt {
    if_stmt(
        binary_expr(var_expr(name_var), BinOp::StrictEq, bool_expr(false)),
        vec![return_stmt(bool_expr(false))],
        None,
    )
}

/// Inserts one integer stat field under its php key, cast the way php reports it.
fn stat_int_field(key: &str, field: i64) -> crate::parser::ast::Stmt {
    array_assign_stmt(
        "stat",
        string_expr(key),
        cast_expr(
            CastType::Int,
            array_access(var_expr("fields"), int_expr(field)),
        ),
    )
}

/// Builds `"zip://" . $this->path . "#" . <entry>`.
fn entry_url_expr(entry: Expr) -> Expr {
    binary_expr(
        binary_expr(
            binary_expr(
                string_expr("zip://"),
                BinOp::Concat,
                property_access(this_expr(), "path"),
            ),
            BinOp::Concat,
            string_expr("#"),
        ),
        BinOp::Concat,
        entry,
    )
}

/// The one-byte `"\0"` separator the serialized stat records are joined with.
fn nul_expr() -> Expr {
    string_expr("\0")
}

/// Wraps `value` in `@`, so a failed read answers `false` without a diagnostic.
fn suppress_expr(value: Expr) -> Expr {
    expr(ExprKind::ErrorSuppress(Box::new(value)))
}
