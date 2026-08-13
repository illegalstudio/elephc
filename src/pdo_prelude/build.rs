//! Purpose:
//! Builds the PDO standard-library surface as AST — the `elephc_pdo` extern block and every
//! PDO class, for SQLite, PostgreSQL, MySQL/MariaDB and the optional system-client drivers.
//! It replaces the PHP source this module used to carry as a raw string.
//!
//! Called from:
//! - `crate::pdo_prelude::inject_if_used_for_version`, after include resolution and before
//!   name resolution.
//!
//! Key details:
//! - TRANSCRIBED, not rewritten: every declaration here was generated from the parse of the
//!   PHP it replaces (`synthetic_class::transcribe`), and the migration oracle
//!   (`ELEPHC_ORACLE_PHP` / `ELEPHC_ORACLE_WHICH=pdo`) compares the built AST against that
//!   parse node by node. Edit the shape here only with the same comparison in hand.
//! - Method-local variables keep their `$_` prefix from the PHP: the checker resolves a
//!   method-body variable's type against top-level variables of the same name, so a user
//!   global like `$stmt` would otherwise clash with a plain method-local `$stmt`.
//! - EVERY VARIATION IS EXPRESSED HERE. No PHP profile ships `PDO_PRELUDE_SRC` unmodified:
//!   `prelude_source_for_version` rewrites 22 fragments and removes 31 marked blocks for the
//!   version and the optional-driver set, and all of that is now conditionals in this module
//!   rather than edits to a string. `built_declarations_match_the_php_for_every_profile`
//!   checks it across all 63 cells of the (profile x driver) matrix, node by node.
//!
//! # WHAT THE VARIATIONS ACTUALLY COST, measured rather than counted
//!
//! Fifty-three rewrites sounds like fifty-three conditionals. It is not: a rewrite edits a
//! FRAGMENT, and the unit a builder can select is a DECLARATION. Transcribing all seven
//! profiles and all seven optional drivers separately and diffing the results declaration by
//! declaration (`pdo_variance.py`, `pdo_drv_variance.py`) puts the real number at SIX of 188:
//!
//! - `PDO` — 5 bodies across versions (8.0-8.1, 8.2-8.3, 8.4, 8.5, 8.6), and every optional
//!   driver adds constants to it.
//! - `PDOStatement` — 3 bodies (8.0, 8.1-8.4, 8.5-8.6).
//! - `PDORow` — 2 bodies (8.0, then the rest).
//! - the braced `namespace Pdo` block — 8.4 and up only; DBLIB/FIREBIRD/IBM/ODBC change it.
//! - `PDO_ODBC_TYPE` — exists only with ODBC.
//! - `pdo_drivers()` — one body with SQLSRV, another without.
//!
//! The other 182 are identical everywhere. The driver deltas seen so far are ADDITIVE
//! (constant blocks inserted at a marked point), not rewrites of existing members.

#![allow(dead_code)]
#![allow(clippy::too_many_lines)]

use crate::pdo_prelude::OptionalDrivers;
use crate::php_version::PhpVersion;
use crate::parser::ast::{BinOp, CType, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    MethodBuilder,
    attr,
    s_const,
    class,
    closure,
    e_array,
    e_array_assoc,
    e_assign,
    e_binop,
    e_bool,
    e_call,
    e_cast,
    e_class_const,
    e_closure_call,
    e_const,
    e_dyn_prop,
    e_index,
    e_instance_of,
    e_int,
    e_method_call,
    e_neg,
    e_new,
    e_new_dynamic,
    e_new_fq,
    e_new_static,
    e_not,
    e_null,
    e_null_coalesce,
    e_parent_call,
    e_post_inc,
    e_self_call,
    e_self_class,
    e_self_static_prop,
    e_spread,
    e_static_call,
    e_static_class,
    e_str,
    e_ternary,
    e_this,
    e_this_prop,
    e_var,
    extern_fn,
    function,
    internal_declarations,
    method,
    s_array_assign,
    s_array_push,
    s_assign,
    s_break,
    s_continue,
    s_echo,
    s_expr,
    s_for,
    s_foreach,
    s_if,
    s_namespace,
    s_prop_array_assign,
    s_prop_array_push,
    s_prop_assign,
    s_return,
    s_return_void,
    s_self_static_prop_assign,
    s_throw,
    s_typed_assign,
    s_while,
    t_array,
    t_class,
    t_mixed,
    t_nullable,
    t_union,
};

/// The `#[\\Deprecated]` PHP 8.5 puts on a driver's legacy `PDO::<DRIVER>_ATTR_*` alias.
///
/// The constants exist from the moment the driver does; only the annotation is version-gated,
/// and only for the four drivers that grew a namespaced replacement. An EMPTY group list is
/// exactly what `constant` produces, so one call serves both profiles.
fn deprecated_alias(
    php_version: PhpVersion,
    driver: &str,
    namespaced: &str,
) -> Vec<crate::parser::ast::AttributeGroup> {
    if php_version >= PhpVersion::Php85 {
        // ONE backslash in each: `\Deprecated` is root-anchored so a namespace cannot capture
        // it, and the message names `Pdo\Dblib` as PHP would write it. Rust's escape plus the
        // generator's own quoting makes doubling them very easy and invisible until the
        // rendered message reads `Pdo\\Dblib`.
        vec![attr(
            "\\Deprecated",
            vec![e_str(&format!("use Pdo\\{driver}::{namespaced} instead"))],
        )]
    } else {
        vec![]
    }
}

/// `__construct` — lifted out of `decl_class_pdoexception` so it builds in its own stack frame.
fn pdoexception_construct() -> MethodBuilder {
    method("__construct")
        .param_default("message", TypeExpr::Str, e_str(""))
        .param_default("code", TypeExpr::Int, e_int(0))
        .param_default("previous", t_nullable(t_class("Throwable")), e_null())
        .body(vec![
            s_prop_assign(e_this(), "message", e_var("message")),
            s_prop_assign(e_this(), "code", e_var("code")),
            s_prop_assign(e_this(), "previous", e_var("previous")),
        ])
}

/// `__elephcFromErrorInfo` — lifted out of `decl_class_pdoexception` so it builds in its own stack frame.
fn pdoexception_elephcfromerrorinfo() -> MethodBuilder {
    method("__elephcFromErrorInfo")
        .private()
        .static_()
        .param("message", TypeExpr::Str)
        .param_default("errorInfo", t_nullable(t_array()), e_null())
        .param_default("previous", t_nullable(t_class("Throwable")), e_null())
        .returns(t_class("PDOException"))
        .body(vec![
            s_assign("_error", e_new("PDOException", vec![e_var("message"), e_int(0), e_var("previous")])),
            s_prop_assign(e_var("_error"), "errorInfo", e_var("errorInfo")),
            s_if(
                e_call("is_array", vec![e_var("errorInfo")]),
                vec![
                    s_if(
                        e_binop(e_call("count", vec![e_var("errorInfo")]), BinOp::Gt, e_int(0)),
                        vec![
                            s_assign("_sqlState", e_index(e_var("errorInfo"), e_int(0))),
                            s_if(
                                e_call("is_string", vec![e_var("_sqlState")]),
                                vec![
                                    s_prop_assign(e_var("_error"), "sqlStateCode", e_cast(CastType::String, e_var("_sqlState"))),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("count", vec![e_var("errorInfo")]), BinOp::Gt, e_int(1)),
                        vec![
                            s_assign("_driverCode", e_index(e_var("errorInfo"), e_int(1))),
                            s_if(
                                e_call("is_int", vec![e_var("_driverCode")]),
                                vec![
                                    s_prop_assign(e_var("_error"), "code", e_cast(CastType::Int, e_var("_driverCode"))),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_error")),
        ])
}

/// `getCode` — lifted out of `decl_class_pdoexception` so it builds in its own stack frame.
fn pdoexception_getcode() -> MethodBuilder {
    method("getCode")
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::Int]))
        .body(vec![
            s_if(
                e_binop(e_this_prop("sqlStateCode"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_return(e_this_prop("sqlStateCode")),
                ],
                vec![],
                None,
            ),
            s_return(e_this_prop("code")),
        ])
}

/// `getPrevious` — lifted out of `decl_class_pdoexception` so it builds in its own stack frame.
fn pdoexception_getprevious() -> MethodBuilder {
    method("getPrevious")
        .returns(t_nullable(t_class("Throwable")))
        .body(vec![
            s_return(e_this_prop("previous")),
        ])
}

/// `create` — lifted out of `decl_class_elephcpdosqliteblobstream` so it builds in its own stack frame.
fn elephcpdosqliteblobstream_create() -> MethodBuilder {
    method("create")
        .static_()
        .param("conn", TypeExpr::Int)
        .param("table", TypeExpr::Str)
        .param("column", TypeExpr::Str)
        .param("rowid", TypeExpr::Int)
        .param("dbname", TypeExpr::Str)
        .param("flags", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_assign("_size", e_call("elephc_pdo_blob_size", vec![e_var("conn"), e_var("table"), e_var("column"), e_var("rowid"), e_var("dbname")])),
            s_if(
                e_binop(e_var("_size"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_self_static_prop("registered")),
                vec![
                    s_self_static_prop_assign("registered", e_call("stream_wrapper_register", vec![e_str("elephcpdosqliteblob"), e_self_class()])),
                    s_if(
                        e_not(e_self_static_prop("registered")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_self_static_prop_assign("pendingConn", e_var("conn")),
            s_self_static_prop_assign("pendingTable", e_var("table")),
            s_self_static_prop_assign("pendingColumn", e_var("column")),
            s_self_static_prop_assign("pendingRowid", e_var("rowid")),
            s_self_static_prop_assign("pendingDbname", e_var("dbname")),
            s_self_static_prop_assign("pendingSize", e_var("_size")),
            s_self_static_prop_assign("pendingWritable", e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2)), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(1)), BinOp::StrictEq, e_int(0)))),
            s_return(e_call("fopen", vec![e_str("elephcpdosqliteblob://open"), e_ternary(e_self_static_prop("pendingWritable"), e_str("r+"), e_str("r"))])),
        ])
}

/// `stream_open` — lifted out of `decl_class_elephcpdosqliteblobstream` so it builds in its own stack frame.
fn elephcpdosqliteblobstream_stream_open() -> MethodBuilder {
    method("stream_open")
        .param_untyped("path")
        .param_untyped("mode")
        .param_untyped("options")
        .param_by_ref("openedPath", None)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_unusedPath", e_var("path")),
            s_assign("_unusedMode", e_var("mode")),
            s_assign("_unusedOptions", e_var("options")),
            s_prop_assign(e_this(), "conn", e_self_static_prop("pendingConn")),
            s_prop_assign(e_this(), "table", e_self_static_prop("pendingTable")),
            s_prop_assign(e_this(), "column", e_self_static_prop("pendingColumn")),
            s_prop_assign(e_this(), "rowid", e_self_static_prop("pendingRowid")),
            s_prop_assign(e_this(), "dbname", e_self_static_prop("pendingDbname")),
            s_prop_assign(e_this(), "size", e_self_static_prop("pendingSize")),
            s_prop_assign(e_this(), "writable", e_self_static_prop("pendingWritable")),
            s_prop_assign(e_this(), "position", e_int(0)),
            s_return(e_bool(true)),
        ])
}

/// `stream_read` — lifted out of `decl_class_elephcpdosqliteblobstream` so it builds in its own stack frame.
fn elephcpdosqliteblobstream_stream_read() -> MethodBuilder {
    method("stream_read")
        .param("count", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("count"), BinOp::LtEq, e_int(0)), BinOp::Or, e_binop(e_this_prop("position"), BinOp::GtEq, e_this_prop("size"))),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_assign("_length", e_call("elephc_pdo_blob_read_at", vec![e_this_prop("conn"), e_this_prop("table"), e_this_prop("column"), e_this_prop("rowid"), e_this_prop("dbname"), e_this_prop("position"), e_var("count")])),
            s_if(
                e_binop(e_var("_length"), BinOp::LtEq, e_int(0)),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_assign("_chunk", e_call("__elephc_ptr_read_string", vec![e_call("elephc_pdo_blob_data_ptr", vec![]), e_var("_length")])),
            s_prop_assign(e_this(), "position", e_binop(e_this_prop("position"), BinOp::Add, e_var("_length"))),
            s_return(e_var("_chunk")),
        ])
}

/// `stream_write` — lifted out of `decl_class_elephcpdosqliteblobstream` so it builds in its own stack frame.
fn elephcpdosqliteblobstream_stream_write() -> MethodBuilder {
    method("stream_write")
        .param("chunk", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_this_prop("writable")),
                vec![
                    s_return(e_neg(e_int(1))),
                ],
                vec![],
                None,
            ),
            s_assign("_count", e_call("strlen", vec![e_var("chunk")])),
            s_if(
                e_binop(e_binop(e_this_prop("position"), BinOp::Add, e_var("_count")), BinOp::Gt, e_this_prop("size")),
                vec![
                    s_return(e_neg(e_int(1))),
                ],
                vec![],
                None,
            ),
            s_assign("_written", e_call("elephc_pdo_blob_write_at", vec![e_this_prop("conn"), e_this_prop("table"), e_this_prop("column"), e_this_prop("rowid"), e_this_prop("dbname"), e_this_prop("position"), e_var("chunk"), e_var("_count")])),
            s_if(
                e_binop(e_var("_written"), BinOp::StrictNotEq, e_var("_count")),
                vec![
                    s_return(e_neg(e_int(1))),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "position", e_binop(e_this_prop("position"), BinOp::Add, e_var("_written"))),
            s_return(e_var("_written")),
        ])
}

/// `stream_tell` — lifted out of `decl_class_elephcpdosqliteblobstream` so it builds in its own stack frame.
fn elephcpdosqliteblobstream_stream_tell() -> MethodBuilder {
    method("stream_tell")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_this_prop("position")),
        ])
}

/// `stream_eof` — lifted out of `decl_class_elephcpdosqliteblobstream` so it builds in its own stack frame.
fn elephcpdosqliteblobstream_stream_eof() -> MethodBuilder {
    method("stream_eof")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_this_prop("position"), BinOp::GtEq, e_this_prop("size"))),
        ])
}

/// `stream_seek` — lifted out of `decl_class_elephcpdosqliteblobstream` so it builds in its own stack frame.
fn elephcpdosqliteblobstream_stream_seek() -> MethodBuilder {
    method("stream_seek")
        .param("offset", TypeExpr::Int)
        .param("whence", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_size", e_this_prop("size")),
            s_if(
                e_binop(e_var("whence"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_assign("_target", e_var("offset")),
                ],
                vec![
                (e_binop(e_var("whence"), BinOp::StrictEq, e_int(1)), vec![
                    s_assign("_target", e_binop(e_this_prop("position"), BinOp::Add, e_var("offset"))),
                ]),
                (e_binop(e_var("whence"), BinOp::StrictEq, e_int(2)), vec![
                    s_assign("_target", e_binop(e_var("_size"), BinOp::Add, e_var("offset"))),
                ]),
            ],
                Some(vec![
                s_return(e_bool(false)),
            ]),
            ),
            s_if(
                e_binop(e_var("_target"), BinOp::Lt, e_int(0)),
                vec![
                    s_prop_assign(e_this(), "position", e_int(0)),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_target"), BinOp::Gt, e_var("_size")),
                vec![
                    s_prop_assign(e_this(), "position", e_var("_size")),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "position", e_var("_target")),
            s_return(e_bool(true)),
        ])
}

/// `stream_stat` — lifted out of `decl_class_elephcpdosqliteblobstream` so it builds in its own stack frame.
fn elephcpdosqliteblobstream_stream_stat() -> MethodBuilder {
    method("stream_stat")
        .returns(t_array())
        .body(vec![
            s_return(e_array_assoc(vec![(e_str("size"), e_this_prop("size"))])),
        ])
}

/// `stream_flush` — lifted out of `decl_class_elephcpdosqliteblobstream` so it builds in its own stack frame.
fn elephcpdosqliteblobstream_stream_flush() -> MethodBuilder {
    method("stream_flush")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_bool(true)),
        ])
}

/// `stream_close` — lifted out of `decl_class_elephcpdosqliteblobstream` so it builds in its own stack frame.
fn elephcpdosqliteblobstream_stream_close() -> MethodBuilder {
    method("stream_close")
        .returns(TypeExpr::Void)
}

/// `create` — lifted out of `decl_class_elephcpdopgsqllobstream` so it builds in its own stack frame.
fn elephcpdopgsqllobstream_create() -> MethodBuilder {
    method("create")
        .static_()
        .param("owner", t_class("PDO"))
        .param("conn", TypeExpr::Int)
        .param("oid", TypeExpr::Str)
        .param("mode", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_not(e_method_call(e_var("owner"), "inTransaction", vec![])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_size", e_call("elephc_pdo_lob_size", vec![e_var("conn"), e_var("oid")])),
            s_if(
                e_binop(e_var("_size"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_self_static_prop("registered")),
                vec![
                    s_self_static_prop_assign("registered", e_call("stream_wrapper_register", vec![e_str("elephcpdopgsqllob"), e_self_class()])),
                    s_if(
                        e_not(e_self_static_prop("registered")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_self_static_prop_assign("pendingConn", e_var("conn")),
            s_self_static_prop_assign("pendingOid", e_var("oid")),
            s_self_static_prop_assign("pendingSize", e_var("_size")),
            s_self_static_prop_assign("pendingWritable", e_binop(e_binop(e_call("strpos", vec![e_var("mode"), e_str("+")]), BinOp::StrictNotEq, e_bool(false)), BinOp::Or, e_binop(e_call("strpos", vec![e_var("mode"), e_str("w")]), BinOp::StrictNotEq, e_bool(false)))),
            s_self_static_prop_assign("pendingOwner", e_var("owner")),
            s_return(e_call("fopen", vec![e_str("elephcpdopgsqllob://open"), e_ternary(e_self_static_prop("pendingWritable"), e_str("r+"), e_str("r"))])),
        ])
}

/// `stream_open` — lifted out of `decl_class_elephcpdopgsqllobstream` so it builds in its own stack frame.
fn elephcpdopgsqllobstream_stream_open() -> MethodBuilder {
    method("stream_open")
        .param_untyped("path")
        .param_untyped("mode")
        .param_untyped("options")
        .param_by_ref("openedPath", None)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_unusedPath", e_var("path")),
            s_assign("_unusedMode", e_var("mode")),
            s_assign("_unusedOptions", e_var("options")),
            s_prop_assign(e_this(), "conn", e_self_static_prop("pendingConn")),
            s_prop_assign(e_this(), "oid", e_self_static_prop("pendingOid")),
            s_prop_assign(e_this(), "size", e_self_static_prop("pendingSize")),
            s_prop_assign(e_this(), "writable", e_self_static_prop("pendingWritable")),
            s_prop_assign(e_this(), "owner", e_self_static_prop("pendingOwner")),
            s_prop_assign(e_this(), "position", e_int(0)),
            s_return(e_bool(true)),
        ])
}

/// `stream_read` — lifted out of `decl_class_elephcpdopgsqllobstream` so it builds in its own stack frame.
fn elephcpdopgsqllobstream_stream_read() -> MethodBuilder {
    method("stream_read")
        .param("count", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_binop(e_this_prop("owner"), BinOp::StrictEq, e_null()), BinOp::Or, e_not(e_method_call(e_this_prop("owner"), "inTransaction", vec![]))),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("count"), BinOp::LtEq, e_int(0)), BinOp::Or, e_binop(e_this_prop("position"), BinOp::GtEq, e_this_prop("size"))),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_assign("_requested", e_var("count")),
            s_if(
                e_binop(e_binop(e_this_prop("position"), BinOp::Add, e_var("_requested")), BinOp::Gt, e_this_prop("size")),
                vec![
                    s_assign("_requested", e_binop(e_this_prop("size"), BinOp::Sub, e_this_prop("position"))),
                ],
                vec![],
                None,
            ),
            s_assign("_length", e_call("elephc_pdo_lob_read_at", vec![e_this_prop("conn"), e_this_prop("oid"), e_this_prop("position"), e_var("_requested")])),
            s_if(
                e_binop(e_var("_length"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_assign("_chunk", e_str("")),
            s_if(
                e_binop(e_var("_length"), BinOp::Gt, e_int(0)),
                vec![
                    s_assign("_chunk", e_call("__elephc_ptr_read_string", vec![e_call("elephc_pdo_blob_data_ptr", vec![]), e_var("_length")])),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "position", e_binop(e_this_prop("position"), BinOp::Add, e_call("strlen", vec![e_var("_chunk")]))),
            s_return(e_var("_chunk")),
        ])
}

/// `stream_write` — lifted out of `decl_class_elephcpdopgsqllobstream` so it builds in its own stack frame.
fn elephcpdopgsqllobstream_stream_write() -> MethodBuilder {
    method("stream_write")
        .param("chunk", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_binop(e_binop(e_not(e_this_prop("writable")), BinOp::Or, e_binop(e_this_prop("owner"), BinOp::StrictEq, e_null())), BinOp::Or, e_not(e_method_call(e_this_prop("owner"), "inTransaction", vec![]))),
                vec![
                    s_return(e_neg(e_int(1))),
                ],
                vec![],
                None,
            ),
            s_assign("_count", e_call("strlen", vec![e_var("chunk")])),
            s_assign("_written", e_call("elephc_pdo_lob_write_at", vec![e_this_prop("conn"), e_this_prop("oid"), e_this_prop("position"), e_var("chunk"), e_var("_count")])),
            s_if(
                e_binop(e_var("_written"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_neg(e_int(1))),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "position", e_binop(e_this_prop("position"), BinOp::Add, e_var("_written"))),
            s_if(
                e_binop(e_this_prop("position"), BinOp::Gt, e_this_prop("size")),
                vec![
                    s_prop_assign(e_this(), "size", e_this_prop("position")),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_written")),
        ])
}

/// `stream_tell` — lifted out of `decl_class_elephcpdopgsqllobstream` so it builds in its own stack frame.
fn elephcpdopgsqllobstream_stream_tell() -> MethodBuilder {
    method("stream_tell")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_this_prop("position")),
        ])
}

/// `stream_eof` — lifted out of `decl_class_elephcpdopgsqllobstream` so it builds in its own stack frame.
fn elephcpdopgsqllobstream_stream_eof() -> MethodBuilder {
    method("stream_eof")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_this_prop("position"), BinOp::GtEq, e_this_prop("size"))),
        ])
}

/// `stream_seek` — lifted out of `decl_class_elephcpdopgsqllobstream` so it builds in its own stack frame.
fn elephcpdopgsqllobstream_stream_seek() -> MethodBuilder {
    method("stream_seek")
        .param("offset", TypeExpr::Int)
        .param("whence", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_binop(e_this_prop("owner"), BinOp::StrictEq, e_null()), BinOp::Or, e_not(e_method_call(e_this_prop("owner"), "inTransaction", vec![]))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("whence"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_assign("_target", e_var("offset")),
                ],
                vec![
                (e_binop(e_var("whence"), BinOp::StrictEq, e_int(1)), vec![
                    s_assign("_target", e_binop(e_this_prop("position"), BinOp::Add, e_var("offset"))),
                ]),
                (e_binop(e_var("whence"), BinOp::StrictEq, e_int(2)), vec![
                    s_assign("_target", e_binop(e_this_prop("size"), BinOp::Add, e_var("offset"))),
                ]),
            ],
                Some(vec![
                s_return(e_bool(false)),
            ]),
            ),
            s_if(
                e_binop(e_var("_target"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "position", e_var("_target")),
            s_return(e_bool(true)),
        ])
}

/// `stream_stat` — lifted out of `decl_class_elephcpdopgsqllobstream` so it builds in its own stack frame.
fn elephcpdopgsqllobstream_stream_stat() -> MethodBuilder {
    method("stream_stat")
        .returns(t_array())
        .body(vec![
            s_return(e_array_assoc(vec![(e_str("size"), e_this_prop("size"))])),
        ])
}

/// `stream_flush` — lifted out of `decl_class_elephcpdopgsqllobstream` so it builds in its own stack frame.
fn elephcpdopgsqllobstream_stream_flush() -> MethodBuilder {
    method("stream_flush")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_bool(true)),
        ])
}

/// `stream_close` — lifted out of `decl_class_elephcpdopgsqllobstream` so it builds in its own stack frame.
fn elephcpdopgsqllobstream_stream_close() -> MethodBuilder {
    method("stream_close")
        .returns(TypeExpr::Void)
}

/// `resolveDsnUri` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_resolvedsnuri() -> MethodBuilder {
    method("resolveDsnUri")
        .protected()
        .static_()
        .param("dsn", TypeExpr::Str)
        .param("operation", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_call("str_starts_with", vec![e_var("dsn"), e_str("uri:")])),
                vec![
                    s_return(e_binop(e_var("dsn"), BinOp::Concat, e_str(""))),
                ],
                vec![],
                None,
            ),
            s_assign("_uri", e_call("substr", vec![e_var("dsn"), e_int(4)])),
            s_if(
                e_call("str_starts_with", vec![e_var("_uri"), e_str("file://")]),
                vec![
                    s_assign("_uri", e_call("substr", vec![e_var("_uri"), e_int(7)])),
                ],
                vec![],
                None,
            ),
            s_assign("_uriHandle", e_call("fopen", vec![e_var("_uri"), e_str("rb")])),
            s_if(
                e_binop(e_var("_uriHandle"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_throw(e_new("PDOException", vec![e_binop(e_var("operation"), BinOp::Concat, e_str("(): Argument #1 ($dsn) must be a valid data source URI"))])),
                ],
                vec![],
                None,
            ),
            s_assign("_uriLine", e_call("fgets", vec![e_var("_uriHandle")])),
            s_expr(e_call("fclose", vec![e_var("_uriHandle")])),
            s_if(
                e_binop(e_var("_uriLine"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_throw(e_new("PDOException", vec![e_binop(e_var("operation"), BinOp::Concat, e_str("(): Argument #1 ($dsn) must be a valid data source URI"))])),
                ],
                vec![],
                None,
            ),
            s_assign("_resolved", e_call("rtrim", vec![e_cast(CastType::String, e_var("_uriLine")), e_str("\r\n")])),
            s_if(
                e_binop(e_call("strpos", vec![e_var("_resolved"), e_str(":")]), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_throw(e_new("PDOException", vec![e_binop(e_var("operation"), BinOp::Concat, e_str("(): Argument #1 ($dsn) must be a valid data source name (via URI)"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_resolved")),
        ])
}

/// `resolveDsnAlias` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_resolvedsnalias() -> MethodBuilder {
    method("resolveDsnAlias")
        .protected()
        .static_()
        .param("dsn", TypeExpr::Str)
        .param("operation", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_call("strpos", vec![e_var("dsn"), e_str(":")]), BinOp::StrictNotEq, e_bool(false)),
                vec![
                    s_return(e_binop(e_var("dsn"), BinOp::Concat, e_str(""))),
                ],
                vec![],
                None,
            ),
            s_assign("_key", e_binop(e_str("pdo.dsn."), BinOp::Concat, e_var("dsn"))),
            s_if(
                e_binop(e_call("elephc_pdo_ini_dsn_defined", vec![e_var("dsn")]), BinOp::StrictNotEq, e_int(1)),
                vec![
                    s_throw(e_new("PDOException", vec![e_binop(e_var("operation"), BinOp::Concat, e_str("(): Argument #1 ($dsn) must be a valid data source name"))])),
                ],
                vec![],
                None,
            ),
            s_assign("_resolved", e_call("elephc_pdo_ini_dsn_value", vec![e_var("dsn")])),
            s_if(
                e_binop(e_call("strpos", vec![e_var("_resolved"), e_str(":")]), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_throw(e_new("PDOException", vec![e_binop(e_binop(e_str("invalid data source name (via INI: "), BinOp::Concat, e_var("_key")), BinOp::Concat, e_str(")"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_resolved")),
        ])
}

/// `checkDsnIsSupported` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_checkdsnissupported() -> MethodBuilder {
    method("checkDsnIsSupported")
        .protected()
        .param("dsn", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_call("strpos", vec![e_var("dsn"), e_str(":")]), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_throw(e_new("PDOException", vec![e_str("PDO::__construct(): Argument #1 ($dsn) must be a valid data source name")])),
                ],
                vec![],
                None,
            ),
            s_assign("_driver", e_call("substr", vec![e_var("dsn"), e_int(0), e_cast(CastType::Int, e_call("strpos", vec![e_var("dsn"), e_str(":")]))])),
            s_assign("_driverFound", e_bool(false)),
            s_assign("_driverCount", e_call("elephc_pdo_available_driver_count", vec![])),
            s_for(Some(s_assign("_driverIndex", e_int(0))), Some(e_binop(e_var("_driverIndex"), BinOp::Lt, e_var("_driverCount"))), Some(s_expr(e_post_inc("_driverIndex"))), vec![
                s_if(
                    e_binop(e_call("elephc_pdo_available_driver_name", vec![e_var("_driverIndex")]), BinOp::StrictEq, e_var("_driver")),
                    vec![
                        s_assign("_driverFound", e_bool(true)),
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_if(
                e_not(e_var("_driverFound")),
                vec![
                    s_throw(e_new("PDOException", vec![e_str("could not find driver")])),
                ],
                vec![],
                None,
            ),
        ])
}

/// `dblibErrorInfo` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_dbliberrorinfo() -> MethodBuilder {
    method("dblibErrorInfo")
        .private()
        .param("message", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            s_assign("_sqlstate", e_call("elephc_pdo_sqlstate", vec![e_this_prop("conn")])),
            s_assign("_native", e_call("elephc_pdo_errcode", vec![e_this_prop("conn")])),
            s_assign("_osCode", e_call("elephc_pdo_dblib_os_errcode", vec![e_this_prop("conn")])),
            s_assign("_severity", e_call("elephc_pdo_dblib_severity", vec![e_this_prop("conn")])),
            s_assign("_formatted", e_binop(e_binop(e_binop(e_binop(e_binop(e_var("message"), BinOp::Concat, e_str(" [")), BinOp::Concat, e_var("_native")), BinOp::Concat, e_str("] (severity ")), BinOp::Concat, e_var("_severity")), BinOp::Concat, e_str(") []"))),
            s_assign("_info", e_array(vec![e_var("_sqlstate"), e_var("_native"), e_var("_formatted"), e_var("_osCode"), e_var("_severity")])),
            s_assign("_osMessage", e_call("elephc_pdo_dblib_os_errmsg", vec![e_this_prop("conn")])),
            s_if(
                e_binop(e_var("_osMessage"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_array_push("_info", e_var("_osMessage")),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_info")),
        ])
}

/// `fail` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_fail() -> MethodBuilder {
    method("fail")
        .private()
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_this_prop("errMode"), BinOp::Eq, e_int(0)),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("_sqlstate", e_call("elephc_pdo_sqlstate", vec![e_this_prop("conn")])),
            s_assign("_native", e_call("elephc_pdo_errcode", vec![e_this_prop("conn")])),
            s_assign("_errorInfo", e_array(vec![e_var("_sqlstate"), e_var("_native"), e_var("message")])),
            s_if(
                e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("dblib")),
                vec![
                    s_assign("_errorInfo", e_method_call(e_this(), "dblibErrorInfo", vec![e_var("message")])),
                    s_assign("message", e_cast(CastType::String, e_index(e_var("_errorInfo"), e_int(2)))),
                ],
                vec![],
                None,
            ),
            s_assign("_full", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("SQLSTATE["), BinOp::Concat, e_var("_sqlstate")), BinOp::Concat, e_str("]: ")), BinOp::Concat, e_call("__elephc_pdo_sqlstate_description", vec![e_var("_sqlstate")])), BinOp::Concat, e_str(": ")), BinOp::Concat, e_var("_native")), BinOp::Concat, e_str(" ")), BinOp::Concat, e_var("message"))),
            s_if(
                e_binop(e_this_prop("errMode"), BinOp::Eq, e_int(2)),
                vec![
                    s_throw(e_static_call("PDOException", "__elephcFromErrorInfo", vec![e_var("_full"), e_var("_errorInfo")])),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("fwrite", vec![e_const("STDERR"), e_binop(e_binop(e_str("PDO error: "), BinOp::Concat, e_var("_full")), BinOp::Concat, e_str("\n"))])),
        ])
}

/// `throwAuthorizerError` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_throwauthorizererror() -> MethodBuilder {
    method("throwAuthorizerError")
        .private()
        .param("operation", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("_authorizerError", e_call("elephc_pdo_take_authorizer_error", vec![e_this_prop("conn")])),
            s_if(
                e_binop(e_var("_authorizerError"), BinOp::Eq, e_int(0)),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_authorizerError"), BinOp::Eq, e_int(1)),
                vec![
                    s_throw(e_new("ValueError", vec![e_binop(e_var("operation"), BinOp::Concat, e_str("(): Return value of the authorizer callback must be one of Pdo\\Sqlite::OK, Pdo\\Sqlite::DENY, or Pdo\\Sqlite::IGNORE"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_authorizerError"), BinOp::Eq, e_int(2)),
                vec![
                    s_throw(e_new("Error", vec![e_binop(e_var("operation"), BinOp::Concat, e_str("(): SQLite authorizer callback raised an exception"))])),
                ],
                vec![],
                None,
            ),
            s_assign("_returnedType", e_str("object")),
            s_if(
                e_binop(e_var("_authorizerError"), BinOp::Eq, e_int(10)),
                vec![
                    s_assign("_returnedType", e_str("null")),
                ],
                vec![
                (e_binop(e_var("_authorizerError"), BinOp::Eq, e_int(11)), vec![
                    s_assign("_returnedType", e_str("float")),
                ]),
                (e_binop(e_var("_authorizerError"), BinOp::Eq, e_int(12)), vec![
                    s_assign("_returnedType", e_str("string")),
                ]),
                (e_binop(e_var("_authorizerError"), BinOp::Eq, e_int(13)), vec![
                    s_assign("_returnedType", e_str("bool")),
                ]),
                (e_binop(e_var("_authorizerError"), BinOp::Eq, e_int(14)), vec![
                    s_assign("_returnedType", e_str("array")),
                ]),
            ],
                None,
            ),
            s_throw(e_new("TypeError", vec![e_binop(e_binop(e_binop(e_var("operation"), BinOp::Concat, e_str("(): Return value of the authorizer callback must be of type int, ")), BinOp::Concat, e_var("_returnedType")), BinOp::Concat, e_str(" returned"))])),
        ])
}

/// `failCode` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_failcode() -> MethodBuilder {
    method("failCode")
        .private()
        .param("sqlstate", TypeExpr::Str)
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_this_prop("errMode"), BinOp::Eq, e_int(0)),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("_full", e_call("__elephc_pdo_impl_error_message", vec![e_var("sqlstate"), e_var("message")])),
            s_if(
                e_binop(e_this_prop("errMode"), BinOp::Eq, e_int(2)),
                vec![
                    s_throw(e_static_call("PDOException", "__elephcFromErrorInfo", vec![e_var("_full"), e_array(vec![e_var("sqlstate"), e_int(0)])])),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("fwrite", vec![e_const("STDERR"), e_binop(e_binop(e_str("PDO error: "), BinOp::Concat, e_var("_full")), BinOp::Concat, e_str("\n"))])),
        ])
}

/// `checkErrMode` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_checkerrmode() -> MethodBuilder {
    method("checkErrMode")
        .private()
        .param("mode", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_binop(e_binop(e_var("mode"), BinOp::NotEq, e_int(0)), BinOp::And, e_binop(e_var("mode"), BinOp::NotEq, e_int(1))), BinOp::And, e_binop(e_var("mode"), BinOp::NotEq, e_int(2))),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("Error mode must be one of the PDO::ERRMODE_* constants")])),
                ],
                vec![],
                None,
            ),
        ])
}

/// `checkDefaultFetchMode` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_checkdefaultfetchmode() -> MethodBuilder {
    method("checkDefaultFetchMode")
        .private()
        .param("mode", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("mode"), BinOp::Eq, e_int(0)),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("Fetch mode must be a bitmask of PDO::FETCH_* constants")])),
                ],
                vec![],
                None,
            ),
        ])
}

/// `checkAttrCase` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_checkattrcase() -> MethodBuilder {
    method("checkAttrCase")
        .private()
        .param("mode", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_binop(e_binop(e_var("mode"), BinOp::NotEq, e_int(0)), BinOp::And, e_binop(e_var("mode"), BinOp::NotEq, e_int(1))), BinOp::And, e_binop(e_var("mode"), BinOp::NotEq, e_int(2))),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("Case folding mode must be one of the PDO::CASE_* constants")])),
                ],
                vec![],
                None,
            ),
        ])
}

/// `attrValueTypeName` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_attrvaluetypename() -> MethodBuilder {
    method("attrValueTypeName")
        .private()
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_call("is_int", vec![e_var("value")]),
                vec![
                    s_return(e_str("int")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_bool", vec![e_var("value")]),
                vec![
                    s_return(e_str("bool")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_float", vec![e_var("value")]),
                vec![
                    s_return(e_str("float")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![
                    s_return(e_str("string")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![
                    s_return(e_str("array")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_null", vec![e_var("value")]),
                vec![
                    s_return(e_str("null")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("object")),
        ])
}

/// `attrIntValue` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_attrintvalue() -> MethodBuilder {
    method("attrIntValue")
        .private()
        .param("value", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_binop(e_call("is_int", vec![e_var("value")]), BinOp::Or, e_call("is_bool", vec![e_var("value")])),
                vec![
                    s_return(e_cast(CastType::Int, e_var("value"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![
                    s_assign("_sval", e_cast(CastType::String, e_var("value"))),
                    s_if(
                        e_binop(e_binop(e_binop(e_call("is_numeric", vec![e_var("_sval")]), BinOp::And, e_binop(e_call("strpos", vec![e_var("_sval"), e_str(".")]), BinOp::StrictEq, e_bool(false))), BinOp::And, e_binop(e_call("strpos", vec![e_var("_sval"), e_str("e")]), BinOp::StrictEq, e_bool(false))), BinOp::And, e_binop(e_call("strpos", vec![e_var("_sval"), e_str("E")]), BinOp::StrictEq, e_bool(false))),
                        vec![
                            s_return(e_cast(CastType::Int, e_var("_sval"))),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("Attribute value must be of type int for selected attribute, "), BinOp::Concat, e_method_call(e_this(), "attrValueTypeName", vec![e_var("value")])), BinOp::Concat, e_str(" given"))])),
        ])
}

/// `attrBoolValue` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_attrboolvalue() -> MethodBuilder {
    method("attrBoolValue")
        .private()
        .param("value", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("is_bool", vec![e_var("value")]), BinOp::Or, e_call("is_int", vec![e_var("value")])),
                vec![
                    s_return(e_cast(CastType::Bool, e_var("value"))),
                ],
                vec![],
                None,
            ),
            s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("Attribute value must be of type bool for selected attribute, "), BinOp::Concat, e_method_call(e_this(), "attrValueTypeName", vec![e_var("value")])), BinOp::Concat, e_str(" given"))])),
        ])
}

/// `validateStatementClassConfig` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_validatestatementclassconfig() -> MethodBuilder {
    method("validateStatementClassConfig")
        .private()
        .param("value", t_mixed())
        .param("fromSetAttribute", TypeExpr::Bool)
        .returns(t_array())
        .body(vec![
            s_if(
                e_not(e_call("is_array", vec![e_var("value")])),
                vec![
                    s_if(
                        e_var("fromSetAttribute"),
                        vec![
                            s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS value must be of type array, "), BinOp::Concat, e_method_call(e_this(), "attrValueTypeName", vec![e_var("value")])), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("PDO::ATTR_STATEMENT_CLASS value must be of type array, "), BinOp::Concat, e_method_call(e_this(), "attrValueTypeName", vec![e_var("value")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("array_key_exists", vec![e_int(0), e_var("value")])),
                vec![
                    s_if(
                        e_var("fromSetAttribute"),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS value must be an array with the format array(classname, constructor_args)")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("ValueError", vec![e_str("PDO::ATTR_STATEMENT_CLASS value must be an array with the format array(classname, constructor_args)")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("is_string", vec![e_index(e_var("value"), e_int(0))])),
                vec![
                    s_if(
                        e_var("fromSetAttribute"),
                        vec![
                            s_throw(e_new("TypeError", vec![e_str("PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS class must be a valid class")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("TypeError", vec![e_str("PDO::ATTR_STATEMENT_CLASS class must be a valid class")])),
                ],
                vec![],
                None,
            ),
            s_assign("_class", e_cast(CastType::String, e_index(e_var("value"), e_int(0)))),
            s_assign("_status", e_call("__elephc_pdo_statement_class_status", vec![e_var("_class")])),
            s_if(
                e_binop(e_var("_status"), BinOp::Eq, e_int(0)),
                vec![
                    s_if(
                        e_var("fromSetAttribute"),
                        vec![
                            s_throw(e_new("TypeError", vec![e_str("PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS class must be a valid class")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("TypeError", vec![e_str("PDO::ATTR_STATEMENT_CLASS class must be a valid class")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_status"), BinOp::Eq, e_int(1)),
                vec![
                    s_if(
                        e_var("fromSetAttribute"),
                        vec![
                            s_throw(e_new("TypeError", vec![e_str("PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS class must be derived from PDOStatement")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("TypeError", vec![e_str("PDO::ATTR_STATEMENT_CLASS class must be derived from PDOStatement")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_status"), BinOp::Eq, e_int(2)),
                vec![
                    s_if(
                        e_var("fromSetAttribute"),
                        vec![
                            s_throw(e_new("TypeError", vec![e_str("PDO::setAttribute(): Argument #2 ($value) User-supplied statement class cannot have a public constructor")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("TypeError", vec![e_str("User-supplied statement class cannot have a public constructor")])),
                ],
                vec![],
                None,
            ),
            s_assign("_config", e_array(vec![e_var("_class")])),
            s_if(
                e_call("array_key_exists", vec![e_int(1), e_var("value")]),
                vec![
                    s_if(
                        e_not(e_call("is_array", vec![e_index(e_var("value"), e_int(1))])),
                        vec![
                            s_if(
                                e_var("fromSetAttribute"),
                                vec![
                                    s_throw(e_new("TypeError", vec![e_str("PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS constructor_args must be of type ?array, array given")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_throw(e_new("TypeError", vec![e_str("PDO::ATTR_STATEMENT_CLASS constructor_args must be of type ?array, array given")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_array_assign("_config", e_int(1), e_index(e_var("value"), e_int(1))),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_config")),
        ])
}

/// `setAttribute` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_setattribute() -> MethodBuilder {
    method("setAttribute")
        .param("attribute", TypeExpr::Int)
        .param_untyped("value")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_driver", e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")])),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(0)), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("mysql")), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("odbc"))), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("informix"))), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("ibm"))), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("oci"))), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("cubrid")))),
                vec![
                    s_assign("_autocommit", e_method_call(e_this(), "attrBoolValue", vec![e_var("value")])),
                    s_if(
                        e_binop(e_call("elephc_pdo_set_autocommit", vec![e_this_prop("conn"), e_ternary(e_var("_autocommit"), e_int(1), e_int(0))]), BinOp::StrictNotEq, e_int(1)),
                        vec![
                            s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "autoCommit", e_var("_autocommit")),
                ],
                vec![
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(0)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("firebird"))), vec![
                    s_assign("_autocommit", e_method_call(e_this(), "attrBoolValue", vec![e_var("value")])),
                    s_if(
                        e_binop(e_call("elephc_pdo_firebird_set_attribute_int", vec![e_this_prop("conn"), e_int(0), e_ternary(e_var("_autocommit"), e_int(1), e_int(0))]), BinOp::StrictNotEq, e_int(1)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "autoCommit", e_var("_autocommit")),
                ]),
                (e_binop(e_var("attribute"), BinOp::Eq, e_int(3)), vec![
                    s_assign("_attrErrMode", e_method_call(e_this(), "attrIntValue", vec![e_var("value")])),
                    s_expr(e_method_call(e_this(), "checkErrMode", vec![e_var("_attrErrMode")])),
                    s_prop_assign(e_this(), "errMode", e_var("_attrErrMode")),
                ]),
                (e_binop(e_var("attribute"), BinOp::Eq, e_int(13)), vec![
                    s_if(
                        e_this_prop("persistent"),
                        vec![
                            s_expr(e_method_call(e_this(), "failCode", vec![e_str("HY000"), e_str("PDO::ATTR_STATEMENT_CLASS cannot be used with persistent PDO instances")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "statementClassConfig", e_method_call(e_this(), "validateStatementClassConfig", vec![e_var("value"), e_bool(true)])),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(2)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlite"))), vec![
                    s_expr(e_call("elephc_pdo_set_busy_timeout", vec![e_this_prop("conn"), e_binop(e_method_call(e_this(), "attrIntValue", vec![e_var("value")]), BinOp::Mul, e_int(1000))])),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(2)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("dblib"))), vec![
                    s_return(e_binop(e_call("elephc_pdo_dblib_set_attribute", vec![e_this_prop("conn"), e_int(2), e_method_call(e_this(), "attrIntValue", vec![e_var("value")])]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(2)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("cubrid"))), vec![
                    s_return(e_binop(e_call("elephc_pdo_cubrid_set_attribute", vec![e_this_prop("conn"), e_int(2), e_method_call(e_this(), "attrIntValue", vec![e_var("value")])]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_var("attribute"), BinOp::Eq, e_int(19)), vec![
                    s_assign("_attrFetchMode", e_method_call(e_this(), "attrIntValue", vec![e_var("value")])),
                    s_expr(e_method_call(e_this(), "checkDefaultFetchMode", vec![e_var("_attrFetchMode")])),
                    s_prop_assign(e_this(), "defaultFetchMode", e_var("_attrFetchMode")),
                ]),
                (e_binop(e_var("attribute"), BinOp::Eq, e_int(17)), vec![
                    s_prop_assign(e_this(), "stringifyFetches", e_method_call(e_this(), "attrBoolValue", vec![e_var("value")])),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(21)), BinOp::And, e_binop(e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("mysql")), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("dblib"))), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")))), vec![
                    s_assign("_defaultStringType", e_method_call(e_this(), "attrIntValue", vec![e_var("value")])),
                    s_prop_assign(e_this(), "defaultStrParam", e_ternary(e_binop(e_var("_defaultStringType"), BinOp::Eq, e_int(1073741824)), e_int(1073741824), e_int(536870912))),
                    s_if(
                        e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")),
                        vec![
                            s_return(e_binop(e_call("elephc_pdo_odbc_set_attribute", vec![e_this_prop("conn"), e_int(21), e_this_prop("defaultStrParam")]), BinOp::StrictEq, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(14)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("mysql"))), vec![
                    s_return(e_binop(e_call("elephc_pdo_set_fetch_table_names", vec![e_this_prop("conn"), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("value")]), e_int(1), e_int(0))]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1000)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("mysql"))), vec![
                    s_return(e_binop(e_call("elephc_pdo_set_buffered_query", vec![e_this_prop("conn"), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("value")]), e_int(1), e_int(0))]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("pgsql"))), vec![
                    s_return(e_binop(e_call("elephc_pdo_set_prefetch", vec![e_this_prop("conn"), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("value")]), e_int(1), e_int(0))]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("oci"))), vec![
                    s_return(e_binop(e_call("elephc_pdo_oci_set_attribute_int", vec![e_this_prop("conn"), e_int(1), e_method_call(e_this(), "attrIntValue", vec![e_var("value")])]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_var("attribute"), BinOp::Eq, e_int(20)), vec![
                    s_if(
                        e_binop(e_var("_driver"), BinOp::StrictEq, e_str("dblib")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")),
                        vec![
                            s_prop_assign(e_this(), "emulatePrepares", e_method_call(e_this(), "attrBoolValue", vec![e_var("value")])),
                            s_return(e_binop(e_call("elephc_pdo_odbc_set_attribute", vec![e_this_prop("conn"), e_int(20), e_ternary(e_this_prop("emulatePrepares"), e_int(1), e_int(0))]), BinOp::StrictEq, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("_driver"), BinOp::StrictNotEq, e_str("mysql")), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictNotEq, e_str("pgsql"))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "emulatePrepares", e_method_call(e_this(), "attrBoolValue", vec![e_var("value")])),
                ]),
                (e_binop(e_var("attribute"), BinOp::Eq, e_int(8)), vec![
                    s_assign("_attrCase", e_method_call(e_this(), "attrIntValue", vec![e_var("value")])),
                    s_expr(e_method_call(e_this(), "checkAttrCase", vec![e_var("_attrCase")])),
                    s_prop_assign(e_this(), "attrCase", e_var("_attrCase")),
                ]),
                (e_binop(e_var("attribute"), BinOp::Eq, e_int(11)), vec![
                    s_prop_assign(e_this(), "oracleNulls", e_method_call(e_this(), "attrIntValue", vec![e_var("value")])),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1002)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlite"))), vec![
                    s_expr(e_call("elephc_pdo_set_extended_result_codes", vec![e_this_prop("conn"), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("value")]), e_int(1), e_int(0))])),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1005)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlite"))), vec![
                    s_assign("_transactionMode", e_method_call(e_this(), "attrIntValue", vec![e_var("value")])),
                    s_if(
                        e_binop(e_binop(e_var("_transactionMode"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("_transactionMode"), BinOp::Gt, e_int(2))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_call("elephc_pdo_set_transaction_mode", vec![e_this_prop("conn"), e_var("_transactionMode")]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1000)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("pgsql"))), vec![
                    s_prop_assign(e_this(), "disablePrepares", e_method_call(e_this(), "attrBoolValue", vec![e_var("value")])),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1004)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("mysql"))), vec![
                    s_prop_assign(e_this(), "emulatePrepares", e_method_call(e_this(), "attrBoolValue", vec![e_var("value")])),
                ]),
                (e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1000)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1001))), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("cubrid"))), vec![
                    s_return(e_binop(e_call("elephc_pdo_cubrid_set_attribute", vec![e_this_prop("conn"), e_var("attribute"), e_method_call(e_this(), "attrIntValue", vec![e_var("value")])]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1001)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1002))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1005))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1006))), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("dblib"))), vec![
                    s_if(
                        e_binop(e_var("attribute"), BinOp::Eq, e_int(1001)),
                        vec![
                            s_return(e_binop(e_call("elephc_pdo_dblib_set_attribute", vec![e_this_prop("conn"), e_var("attribute"), e_method_call(e_this(), "attrIntValue", vec![e_var("value")])]), BinOp::StrictEq, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_call("elephc_pdo_dblib_set_attribute", vec![e_this_prop("conn"), e_var("attribute"), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("value")]), e_int(1), e_int(0))]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1000)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1001))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1002))), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("firebird"))), vec![
                    s_return(e_binop(e_call("elephc_pdo_firebird_set_attribute_text", vec![e_this_prop("conn"), e_var("attribute"), e_cast(CastType::String, e_var("value"))]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(14)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1003))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1007))), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("firebird"))), BinOp::Or, e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1001)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("odbc")))), vec![
                    s_if(
                        e_binop(e_var("_driver"), BinOp::StrictEq, e_str("odbc")),
                        vec![
                            s_return(e_binop(e_call("elephc_pdo_odbc_set_attribute", vec![e_this_prop("conn"), e_int(1001), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("value")]), e_int(1), e_int(0))]), BinOp::StrictEq, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_firebirdValue", e_ternary(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(14)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1007))), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("value")]), e_int(1), e_int(0)), e_method_call(e_this(), "attrIntValue", vec![e_var("value")]))),
                    s_return(e_binop(e_call("elephc_pdo_firebird_set_attribute_int", vec![e_this_prop("conn"), e_var("attribute"), e_var("_firebirdValue")]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1000)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1001))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1002))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1003))), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("oci"))), vec![
                    s_return(e_binop(e_call("elephc_pdo_oci_set_attribute_text", vec![e_this_prop("conn"), e_var("attribute"), e_cast(CastType::String, e_var("value"))]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1004)), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("oci"))), vec![
                    s_return(e_binop(e_call("elephc_pdo_oci_set_attribute_int", vec![e_this_prop("conn"), e_int(1004), e_method_call(e_this(), "attrIntValue", vec![e_var("value")])]), BinOp::StrictEq, e_int(1))),
                ]),
                (e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1281)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1282))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1283))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1284))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(2562))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(2563))), BinOp::And, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("ibm"))), vec![
                    s_if(
                        e_binop(e_call("elephc_pdo_ibm_set_attribute_text", vec![e_this_prop("conn"), e_var("attribute"), e_cast(CastType::String, e_var("value"))]), BinOp::StrictNotEq, e_int(1)),
                        vec![
                            s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
                (e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1000)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1001))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1002))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1004))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1005))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1006))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1007))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1008)))), vec![
                    s_assign("_sqlsrvValue", e_ternary(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1002)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1005))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1006))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1007))), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("value")]), e_int(1), e_int(0)), e_method_call(e_this(), "attrIntValue", vec![e_var("value")]))),
                    s_if(
                        e_binop(e_call("elephc_pdo_odbc_set_attribute", vec![e_this_prop("conn"), e_var("attribute"), e_var("_sqlsrvValue")]), BinOp::StrictNotEq, e_int(1)),
                        vec![
                            s_expr(e_method_call(e_this(), "failCode", vec![e_str("IMSSP"), e_str("An invalid attribute was designated on the PDO object.")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
                (e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")), BinOp::And, e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(10)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1003))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1009)))), vec![
                    s_expr(e_method_call(e_this(), "failCode", vec![e_str("IMSSP"), e_str("The given attribute is only supported on the PDOStatement object.")])),
                    s_return(e_bool(false)),
                ]),
                (e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(0)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(2))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(9))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(12)))), vec![
                    s_expr(e_method_call(e_this(), "failCode", vec![e_str("IMSSP"), e_str("An unsupported attribute was designated on the PDO object.")])),
                    s_return(e_bool(false)),
                ]),
                (e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(4)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(5))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(6))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(7))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(16)))), vec![
                    s_expr(e_method_call(e_this(), "failCode", vec![e_str("IMSSP"), e_str("A read-only attribute was designated on the PDO object.")])),
                    s_return(e_bool(false)),
                ]),
            ],
                Some(vec![
                s_prop_assign(e_this(), "hasOperation", e_bool(true)),
                s_return(e_bool(false)),
            ]),
            ),
            s_return(e_bool(true)),
        ])
}

/// `__elephcDrainPgsqlNotices` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_elephcdrainpgsqlnotices() -> MethodBuilder {
    method("__elephcDrainPgsqlNotices")
        .protected()
        .returns(TypeExpr::Void)
}

/// `getAttribute` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_getattribute() -> MethodBuilder {
    method("getAttribute")
        .param("attribute", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("sqlsrv")),
                vec![
                    s_if(
                        e_binop(e_var("attribute"), BinOp::Eq, e_int(5)),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("DriverName"), e_call("elephc_pdo_sqlsrv_info", vec![e_this_prop("conn"), e_int(3)])), (e_str("DriverODBCVer"), e_call("elephc_pdo_sqlsrv_info", vec![e_this_prop("conn"), e_int(4)])), (e_str("DriverVer"), e_call("elephc_pdo_sqlsrv_info", vec![e_this_prop("conn"), e_int(5)])), (e_str("ExtensionVer"), e_str("5.13.1"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("attribute"), BinOp::Eq, e_int(6)),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("CurrentDatabase"), e_call("elephc_pdo_sqlsrv_info", vec![e_this_prop("conn"), e_int(0)])), (e_str("SQLServerVersion"), e_call("elephc_pdo_sqlsrv_info", vec![e_this_prop("conn"), e_int(1)])), (e_str("SQLServerName"), e_call("elephc_pdo_sqlsrv_info", vec![e_this_prop("conn"), e_int(2)]))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1000)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1001))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1002))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1004))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1005))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1006))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1007))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1008))),
                        vec![
                            s_assign("_sqlsrvValue", e_call("elephc_pdo_odbc_attribute", vec![e_this_prop("conn"), e_var("attribute")])),
                            s_if(
                                e_binop(e_var("_sqlsrvValue"), BinOp::GtEq, e_int(0)),
                                vec![
                                    s_return(e_ternary(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1002)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1005))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1006))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1007))), e_binop(e_var("_sqlsrvValue"), BinOp::StrictEq, e_int(1)), e_var("_sqlsrvValue"))),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1003)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1009))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(10))),
                        vec![
                            s_expr(e_method_call(e_this(), "failCode", vec![e_str("IMSSP"), e_str("The given attribute is only supported on the PDOStatement object.")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("attribute"), BinOp::Eq, e_int(7)),
                        vec![
                            s_expr(e_method_call(e_this(), "failCode", vec![e_str("IMSSP"), e_str("An invalid attribute was designated on the PDO object.")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1281)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1282))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1283))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1284))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(2562))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("ibm"))),
                vec![
                    s_assign("_ibmText", e_call("elephc_pdo_ibm_attribute_text", vec![e_this_prop("conn"), e_var("attribute")])),
                    s_if(
                        e_binop(e_call("elephc_pdo_sqlstate", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("00000")),
                        vec![
                            s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_ibmText")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(2561)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("ibm"))),
                vec![
                    s_assign("_trusted", e_call("elephc_pdo_ibm_attribute_int", vec![e_this_prop("conn"), e_var("attribute")])),
                    s_if(
                        e_binop(e_var("_trusted"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_trusted"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_trustedUser", e_call("elephc_pdo_ibm_attribute_text", vec![e_this_prop("conn"), e_int(2562)])),
                    s_if(
                        e_binop(e_call("elephc_pdo_sqlstate", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("00000")),
                        vec![
                            s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_trustedUser")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(0)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1004))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("oci"))),
                vec![
                    s_assign("_ociValue", e_call("elephc_pdo_oci_attribute_int", vec![e_this_prop("conn"), e_var("attribute")])),
                    s_if(
                        e_binop(e_var("_ociValue"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_return(e_ternary(e_binop(e_var("attribute"), BinOp::Eq, e_int(0)), e_binop(e_var("_ociValue"), BinOp::StrictEq, e_int(1)), e_var("_ociValue"))),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(0)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1001))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("odbc"))), BinOp::Or, e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(0)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(14))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1003))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1007))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("firebird")))),
                vec![
                    s_if(
                        e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("odbc")),
                        vec![
                            s_assign("_odbcValue", e_call("elephc_pdo_odbc_attribute", vec![e_this_prop("conn"), e_var("attribute")])),
                            s_if(
                                e_binop(e_var("_odbcValue"), BinOp::GtEq, e_int(0)),
                                vec![
                                    s_return(e_binop(e_var("_odbcValue"), BinOp::StrictEq, e_int(1))),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_firebirdValue", e_call("elephc_pdo_firebird_attribute_int", vec![e_this_prop("conn"), e_var("attribute")])),
                    s_if(
                        e_binop(e_var("_firebirdValue"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_return(e_ternary(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(0)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(14))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1007))), e_binop(e_var("_firebirdValue"), BinOp::StrictEq, e_int(1)), e_var("_firebirdValue"))),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1000)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1001))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1002))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("firebird"))),
                vec![
                    s_return(e_call("elephc_pdo_firebird_attribute_text", vec![e_this_prop("conn"), e_var("attribute")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(0)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(2))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1000))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1001))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1002))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("cubrid"))),
                vec![
                    s_assign("_cubridValue", e_call("elephc_pdo_cubrid_attribute", vec![e_this_prop("conn"), e_var("attribute")])),
                    s_if(
                        e_binop(e_binop(e_var("_cubridValue"), BinOp::GtEq, e_neg(e_int(1))), BinOp::And, e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(2)), BinOp::Or, e_binop(e_var("_cubridValue"), BinOp::GtEq, e_int(0)))),
                        vec![
                            s_return(e_ternary(e_binop(e_var("attribute"), BinOp::Eq, e_int(0)), e_binop(e_var("_cubridValue"), BinOp::StrictEq, e_int(1)), e_var("_cubridValue"))),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(0)), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("mysql")), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("odbc"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("informix"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("ibm"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("oci")))),
                vec![
                    s_return(e_binop(e_call("elephc_pdo_autocommit", vec![e_this_prop("conn")]), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(14)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("mysql"))),
                vec![
                    s_return(e_binop(e_call("elephc_pdo_fetch_table_names", vec![e_this_prop("conn")]), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1000)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("mysql"))),
                vec![
                    s_return(e_binop(e_call("elephc_pdo_buffered_query", vec![e_this_prop("conn")]), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("attribute"), BinOp::Eq, e_int(3)),
                vec![
                    s_return(e_this_prop("errMode")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("attribute"), BinOp::Eq, e_int(12)),
                vec![
                    s_return(e_this_prop("persistent")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("attribute"), BinOp::Eq, e_int(13)),
                vec![
                    s_return(e_this_prop("statementClassConfig")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("attribute"), BinOp::Eq, e_int(16)),
                vec![
                    s_return(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(7)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("firebird"))),
                vec![
                    s_return(e_binop(e_call("elephc_pdo_connection_status", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("1"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("attribute"), BinOp::Eq, e_int(19)),
                vec![
                    s_return(e_this_prop("defaultFetchMode")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("attribute"), BinOp::Eq, e_int(17)),
                vec![
                    s_return(e_this_prop("stringifyFetches")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(21)), BinOp::And, e_binop(e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("mysql")), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("dblib"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("sqlsrv")))),
                vec![
                    s_return(e_this_prop("defaultStrParam")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(20)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("dblib"))),
                vec![
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(20)), BinOp::And, e_binop(e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("mysql")), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("pgsql"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("sqlsrv")))),
                vec![
                    s_return(e_this_prop("emulatePrepares")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1000)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("pgsql"))),
                vec![
                    s_return(e_this_prop("disablePrepares")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1004)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("mysql"))),
                vec![
                    s_return(e_this_prop("emulatePrepares")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("attribute"), BinOp::Eq, e_int(8)),
                vec![
                    s_return(e_this_prop("attrCase")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("attribute"), BinOp::Eq, e_int(11)),
                vec![
                    s_return(e_this_prop("oracleNulls")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(4)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("dblib"))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("informix"))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("ibm"))),
                vec![
                    s_return(e_call("elephc_pdo_server_version", vec![e_this_prop("conn")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1005)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("sqlite"))),
                vec![
                    s_return(e_call("elephc_pdo_transaction_mode", vec![e_this_prop("conn")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(5)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("dblib"))),
                vec![
                    s_return(e_call("elephc_pdo_client_version", vec![e_this_prop("conn")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1002)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("dblib"))),
                vec![
                    s_return(e_binop(e_call("elephc_pdo_dblib_attribute_bool", vec![e_this_prop("conn"), e_var("attribute")]), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1003)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("dblib"))),
                vec![
                    s_return(e_call("elephc_pdo_client_version", vec![e_this_prop("conn")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1004)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("dblib"))),
                vec![
                    s_return(e_call("elephc_pdo_server_version", vec![e_this_prop("conn")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1005)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1006))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("dblib"))),
                vec![
                    s_return(e_binop(e_call("elephc_pdo_dblib_attribute_bool", vec![e_this_prop("conn"), e_var("attribute")]), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(6)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("sqlite"))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("dblib"))),
                vec![
                    s_assign("serverInfo", e_call("elephc_pdo_server_info", vec![e_this_prop("conn")])),
                    s_if(
                        e_binop(e_var("serverInfo"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_expr(e_method_call(e_this(), "failCode", vec![e_str("HY000"), e_str("failed to read server information")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("serverInfo")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(7)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("sqlite"))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("dblib"))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("odbc"))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("informix"))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("ibm"))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("oci"))), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("cubrid"))),
                vec![
                    s_return(e_call("elephc_pdo_connection_status", vec![e_this_prop("conn")])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "failCode", vec![e_str("IM001"), e_str("driver does not support that attribute")])),
            s_return(e_bool(false)),
        ])
}

/// `exec` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_exec() -> MethodBuilder {
    method("exec")
        .param("statement", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::Bool]))
        .body(vec![
            s_if(
                e_binop(e_var("statement"), BinOp::StrictEq, e_str("")),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("PDO::exec(): Argument #1 ($statement) must not be empty")])),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "hasOperation", e_bool(true)),
            s_assign("_affected", e_call("elephc_pdo_exec", vec![e_this_prop("conn"), e_var("statement")])),
            s_if(
                e_binop(e_var("_affected"), BinOp::Lt, e_int(0)),
                vec![
                    s_expr(e_method_call(e_this(), "throwAuthorizerError", vec![e_str("PDO::exec")])),
                    s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_affected")),
        ])
}

/// `query` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_query() -> MethodBuilder {
    method("query")
        .param("query", TypeExpr::Str)
        .param_default("fetchMode", t_nullable(TypeExpr::Int), e_null())
        .variadic("fetchModeArgs", Some(t_mixed()))
        .returns(t_union(vec![t_class("PDOStatement"), TypeExpr::Bool]))
        .body(vec![
            s_if(
                e_binop(e_var("query"), BinOp::StrictEq, e_str("")),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("PDO::query(): Argument #1 ($statement) must not be empty")])),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "prepareOperation", e_str("PDO::query")),
            s_assign("_statement", e_method_call(e_this(), "prepare", vec![e_var("query")])),
            s_if(
                e_binop(e_var("_statement"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_method_call(e_var("_statement"), "execute", vec![]), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("fetchMode"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_method_call(e_var("_statement"), "setFetchMode", vec![e_cast(CastType::Int, e_var("fetchMode")), e_spread(e_var("fetchModeArgs"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_statement")),
        ])
}

/// `lastInsertId` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_lastinsertid() -> MethodBuilder {
    method("lastInsertId")
        .param_default("name", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::Bool]))
        .body(vec![
            s_prop_assign(e_this(), "hasOperation", e_bool(true)),
            s_assign("_id", e_call("elephc_pdo_last_insert_id_text", vec![e_this_prop("conn"), e_null_coalesce(e_var("name"), e_str(""))])),
            s_if(
                e_binop(e_var("_id"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_return(e_var("_id")),
                ],
                vec![],
                None,
            ),
            s_assign("_sqlstate", e_call("elephc_pdo_sqlstate", vec![e_this_prop("conn")])),
            s_if(
                e_binop(e_var("_sqlstate"), BinOp::StrictNotEq, e_str("00000")),
                vec![
                    s_expr(e_method_call(e_this(), "failCode", vec![e_var("_sqlstate"), e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                ],
                vec![],
                Some(vec![
                s_expr(e_method_call(e_this(), "failCode", vec![e_str("IM001"), e_str("driver does not support lastInsertId()")])),
            ]),
            ),
            s_return(e_bool(false)),
        ])
}

/// `beginTransaction` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_begintransaction() -> MethodBuilder {
    method("beginTransaction")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_live", e_call("elephc_pdo_in_transaction", vec![e_this_prop("conn")])),
            s_assign("_alreadyActive", e_binop(e_binop(e_var("_live"), BinOp::StrictEq, e_int(1)), BinOp::Or, e_binop(e_binop(e_var("_live"), BinOp::StrictEq, e_neg(e_int(1))), BinOp::And, e_this_prop("inTxn")))),
            s_if(
                e_var("_alreadyActive"),
                vec![
                    s_throw(e_new("PDOException", vec![e_str("There is already an active transaction")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("elephc_pdo_begin", vec![e_this_prop("conn")]), BinOp::NotEq, e_int(1)),
                vec![
                    s_prop_assign(e_this(), "hasOperation", e_bool(true)),
                    s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "hasOperation", e_bool(true)),
            s_prop_assign(e_this(), "inTxn", e_bool(true)),
            s_return(e_bool(true)),
        ])
}

/// `commit` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_commit() -> MethodBuilder {
    method("commit")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_method_call(e_this(), "inTransaction", vec![])),
                vec![
                    s_throw(e_new("PDOException", vec![e_str("There is no active transaction")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("elephc_pdo_commit", vec![e_this_prop("conn")]), BinOp::NotEq, e_int(1)),
                vec![
                    s_prop_assign(e_this(), "hasOperation", e_bool(true)),
                    s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "hasOperation", e_bool(true)),
            s_prop_assign(e_this(), "inTxn", e_bool(false)),
            s_return(e_bool(true)),
        ])
}

/// `rollBack` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_rollback() -> MethodBuilder {
    method("rollBack")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_method_call(e_this(), "inTransaction", vec![])),
                vec![
                    s_throw(e_new("PDOException", vec![e_str("There is no active transaction")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("elephc_pdo_rollback", vec![e_this_prop("conn")]), BinOp::NotEq, e_int(1)),
                vec![
                    s_prop_assign(e_this(), "hasOperation", e_bool(true)),
                    s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "hasOperation", e_bool(true)),
            s_prop_assign(e_this(), "inTxn", e_bool(false)),
            s_return(e_bool(true)),
        ])
}

/// `inTransaction` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_intransaction() -> MethodBuilder {
    method("inTransaction")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_live", e_call("elephc_pdo_in_transaction", vec![e_this_prop("conn")])),
            s_if(
                e_binop(e_binop(e_var("_live"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("_live"), BinOp::StrictEq, e_int(1))),
                vec![
                    s_return(e_binop(e_var("_live"), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_return(e_this_prop("inTxn")),
        ])
}

/// `connectionId` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_connectionid() -> MethodBuilder {
    method("connectionId")
        .protected()
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_this_prop("conn")),
        ])
}

/// `sqliteCreateCollation` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_sqlitecreatecollation() -> MethodBuilder {
    method("sqliteCreateCollation")
        .param("name", TypeExpr::Str)
        .param("callback", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_call("is_callable", vec![e_var("callback")])),
                vec![
                    s_throw(e_new("TypeError", vec![e_str("PDO::sqliteCreateCollation(): Argument #2 ($callback) must be a valid callback")])),
                ],
                vec![],
                None,
            ),
            s_assign("_normalized", e_call("__elephc_normalize_callable", vec![e_var("callback")])),
            s_assign("_descriptor", e_call("__elephc_callable_ptr", vec![e_var("_normalized")])),
            s_assign("_adapter", e_call("__elephc_pdo_adapter_addr", vec![e_int(0)])),
            s_if(
                e_binop(e_call("elephc_pdo_create_collation", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("name"), e_var("_descriptor"), e_var("_adapter")]), BinOp::StrictNotEq, e_int(1)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_prop_array_assign(e_this(), "pdoUdfCallbacks", e_binop(e_str("collation:"), BinOp::Concat, e_call("strtolower", vec![e_var("name")])), e_var("_normalized")),
            s_return(e_bool(true)),
        ])
}

/// `sqliteCreateFunction` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_sqlitecreatefunction() -> MethodBuilder {
    method("sqliteCreateFunction")
        .param("name", TypeExpr::Str)
        .param("callback", t_mixed())
        .param_default("numArgs", TypeExpr::Int, e_neg(e_int(1)))
        .param_default("flags", TypeExpr::Int, e_int(0))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_call("is_callable", vec![e_var("callback")])),
                vec![
                    s_throw(e_new("TypeError", vec![e_str("PDO::sqliteCreateFunction(): Argument #2 ($callback) must be a valid callback")])),
                ],
                vec![],
                None,
            ),
            s_assign("_normalized", e_call("__elephc_normalize_callable", vec![e_var("callback")])),
            s_assign("_descriptor", e_call("__elephc_callable_ptr", vec![e_var("_normalized")])),
            s_assign("_adapter", e_call("__elephc_pdo_adapter_addr", vec![e_int(1)])),
            s_if(
                e_binop(e_call("elephc_pdo_create_function", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("name"), e_var("numArgs"), e_var("flags"), e_var("_descriptor"), e_var("_adapter")]), BinOp::StrictNotEq, e_int(1)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_prop_array_assign(e_this(), "pdoUdfCallbacks", e_binop(e_binop(e_binop(e_binop(e_str("function:"), BinOp::Concat, e_call("strtolower", vec![e_var("name")])), BinOp::Concat, e_str(":")), BinOp::Concat, e_var("numArgs")), BinOp::Concat, e_str(":scalar")), e_var("_normalized")),
            s_return(e_bool(true)),
        ])
}

/// `sqliteCreateAggregate` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_sqlitecreateaggregate() -> MethodBuilder {
    method("sqliteCreateAggregate")
        .param("name", TypeExpr::Str)
        .param("step", t_mixed())
        .param("finalize", t_mixed())
        .param_default("numArgs", TypeExpr::Int, e_neg(e_int(1)))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_not(e_call("is_callable", vec![e_var("step")])), BinOp::Or, e_not(e_call("is_callable", vec![e_var("finalize")]))),
                vec![
                    s_throw(e_new("TypeError", vec![e_str("PDO::sqliteCreateAggregate(): step and finalize must be valid callbacks")])),
                ],
                vec![],
                None,
            ),
            s_assign("_normalizedStep", e_call("__elephc_normalize_callable", vec![e_var("step")])),
            s_assign("_normalizedFinal", e_call("__elephc_normalize_callable", vec![e_var("finalize")])),
            s_assign("_stepDesc", e_call("__elephc_callable_ptr", vec![e_var("_normalizedStep")])),
            s_assign("_stepAdapter", e_call("__elephc_pdo_adapter_addr", vec![e_int(2)])),
            s_assign("_finalDesc", e_call("__elephc_callable_ptr", vec![e_var("_normalizedFinal")])),
            s_assign("_finalAdapter", e_call("__elephc_pdo_adapter_addr", vec![e_int(3)])),
            s_if(
                e_binop(e_call("elephc_pdo_create_aggregate", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("name"), e_var("numArgs"), e_var("_stepDesc"), e_var("_stepAdapter"), e_var("_finalDesc"), e_var("_finalAdapter")]), BinOp::StrictNotEq, e_int(1)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_rootKey", e_binop(e_binop(e_binop(e_str("function:"), BinOp::Concat, e_call("strtolower", vec![e_var("name")])), BinOp::Concat, e_str(":")), BinOp::Concat, e_var("numArgs"))),
            s_prop_array_assign(e_this(), "pdoUdfCallbacks", e_binop(e_var("_rootKey"), BinOp::Concat, e_str(":step")), e_var("_normalizedStep")),
            s_prop_array_assign(e_this(), "pdoUdfCallbacks", e_binop(e_var("_rootKey"), BinOp::Concat, e_str(":final")), e_var("_normalizedFinal")),
            s_return(e_bool(true)),
        ])
}

/// `pdoPgsqlCopyOptions` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_pdopgsqlcopyoptions() -> MethodBuilder {
    method("pdoPgsqlCopyOptions")
        .private()
        .param("separator", TypeExpr::Str)
        .param("nullAs", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("_sep", e_ternary(e_binop(e_var("separator"), BinOp::StrictEq, e_str("")), e_str("\t"), e_call("substr", vec![e_var("separator"), e_int(0), e_int(1)]))),
            s_if(
                e_binop(e_binop(e_var("_sep"), BinOp::StrictEq, e_str("\t")), BinOp::And, e_binop(e_var("nullAs"), BinOp::StrictEq, e_str("\\N"))),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_assign("_delim", e_ternary(e_binop(e_var("_sep"), BinOp::StrictEq, e_str("\t")), e_str("E'\\t'"), e_binop(e_binop(e_str("'"), BinOp::Concat, e_var("_sep")), BinOp::Concat, e_str("'")))),
            s_assign("_null", e_binop(e_binop(e_str("'"), BinOp::Concat, e_call("str_replace", vec![e_str("'"), e_str("''"), e_var("nullAs")])), BinOp::Concat, e_str("'"))),
            s_return(e_binop(e_binop(e_binop(e_binop(e_str(" WITH (DELIMITER "), BinOp::Concat, e_var("_delim")), BinOp::Concat, e_str(", NULL ")), BinOp::Concat, e_var("_null")), BinOp::Concat, e_str(")"))),
        ])
}

/// `pdoPgsqlCopyTarget` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_pdopgsqlcopytarget() -> MethodBuilder {
    method("pdoPgsqlCopyTarget")
        .private()
        .param("tableName", TypeExpr::Str)
        .param("fields", t_nullable(TypeExpr::Str))
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("fields"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_return(e_binop(e_binop(e_binop(e_var("tableName"), BinOp::Concat, e_str(" (")), BinOp::Concat, e_var("fields")), BinOp::Concat, e_str(")"))),
                ],
                vec![],
                None,
            ),
            s_return(e_var("tableName")),
        ])
}

/// `pgsqlCopyFromArray` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_pgsqlcopyfromarray() -> MethodBuilder {
    method("pgsqlCopyFromArray")
        .param("tableName", TypeExpr::Str)
        .param("rows", t_array())
        .param_default("separator", TypeExpr::Str, e_str("\t"))
        .param_default("nullAs", TypeExpr::Str, e_str("\\N"))
        .param_default("fields", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_data", e_binop(e_call("implode", vec![e_str("\n"), e_var("rows")]), BinOp::Concat, e_str("\n"))),
            s_assign("_sql", e_binop(e_binop(e_binop(e_str("COPY "), BinOp::Concat, e_method_call(e_this(), "pdoPgsqlCopyTarget", vec![e_var("tableName"), e_var("fields")])), BinOp::Concat, e_str(" FROM STDIN")), BinOp::Concat, e_method_call(e_this(), "pdoPgsqlCopyOptions", vec![e_var("separator"), e_var("nullAs")]))),
            s_return(e_binop(e_call("elephc_pdo_copy_in", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("_sql"), e_var("_data")]), BinOp::GtEq, e_int(0))),
        ])
}

/// `pgsqlCopyFromFile` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_pgsqlcopyfromfile() -> MethodBuilder {
    method("pgsqlCopyFromFile")
        .param("tableName", TypeExpr::Str)
        .param("filename", TypeExpr::Str)
        .param_default("separator", TypeExpr::Str, e_str("\t"))
        .param_default("nullAs", TypeExpr::Str, e_str("\\N"))
        .param_default("fields", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_data", e_call("file_get_contents", vec![e_var("filename")])),
            s_if(
                e_binop(e_var("_data"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_sql", e_binop(e_binop(e_binop(e_str("COPY "), BinOp::Concat, e_method_call(e_this(), "pdoPgsqlCopyTarget", vec![e_var("tableName"), e_var("fields")])), BinOp::Concat, e_str(" FROM STDIN")), BinOp::Concat, e_method_call(e_this(), "pdoPgsqlCopyOptions", vec![e_var("separator"), e_var("nullAs")]))),
            s_return(e_binop(e_call("elephc_pdo_copy_in", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("_sql"), e_cast(CastType::String, e_var("_data"))]), BinOp::GtEq, e_int(0))),
        ])
}

/// `pgsqlCopyToArray` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_pgsqlcopytoarray() -> MethodBuilder {
    method("pgsqlCopyToArray")
        .param("tableName", TypeExpr::Str)
        .param_default("separator", TypeExpr::Str, e_str("\t"))
        .param_default("nullAs", TypeExpr::Str, e_str("\\N"))
        .param_default("fields", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![t_array(), TypeExpr::False]))
        .body(vec![
            s_assign("_sql", e_binop(e_binop(e_binop(e_str("COPY "), BinOp::Concat, e_method_call(e_this(), "pdoPgsqlCopyTarget", vec![e_var("tableName"), e_var("fields")])), BinOp::Concat, e_str(" TO STDOUT")), BinOp::Concat, e_method_call(e_this(), "pdoPgsqlCopyOptions", vec![e_var("separator"), e_var("nullAs")]))),
            s_assign("_raw", e_call("elephc_pdo_copy_out", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("_sql")])),
            s_if(
                e_binop(e_var("_raw"), BinOp::StrictEq, e_str("")),
                vec![
                    s_if(
                        e_binop(e_call("elephc_pdo_errcode", vec![e_method_call(e_this(), "connectionId", vec![])]), BinOp::NotEq, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array(vec![])),
                ],
                vec![],
                None,
            ),
            s_assign("_lines", e_call("explode", vec![e_str("\n"), e_call("rtrim", vec![e_var("_raw"), e_str("\n")])])),
            s_assign("_out", e_array(vec![])),
            s_foreach(e_var("_lines"), None, "_line", vec![
                s_array_push("_out", e_binop(e_var("_line"), BinOp::Concat, e_str("\n"))),
            ]),
            s_return(e_var("_out")),
        ])
}

/// `pgsqlCopyToFile` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_pgsqlcopytofile() -> MethodBuilder {
    method("pgsqlCopyToFile")
        .param("tableName", TypeExpr::Str)
        .param("filename", TypeExpr::Str)
        .param_default("separator", TypeExpr::Str, e_str("\t"))
        .param_default("nullAs", TypeExpr::Str, e_str("\\N"))
        .param_default("fields", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_sql", e_binop(e_binop(e_binop(e_str("COPY "), BinOp::Concat, e_method_call(e_this(), "pdoPgsqlCopyTarget", vec![e_var("tableName"), e_var("fields")])), BinOp::Concat, e_str(" TO STDOUT")), BinOp::Concat, e_method_call(e_this(), "pdoPgsqlCopyOptions", vec![e_var("separator"), e_var("nullAs")]))),
            s_assign("_raw", e_call("elephc_pdo_copy_out", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("_sql")])),
            s_if(
                e_binop(e_binop(e_var("_raw"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_call("elephc_pdo_errcode", vec![e_method_call(e_this(), "connectionId", vec![])]), BinOp::NotEq, e_int(0))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_call("file_put_contents", vec![e_var("filename"), e_var("_raw")]), BinOp::StrictNotEq, e_bool(false))),
        ])
}

/// `pgsqlLOBCreate` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_pgsqllobcreate() -> MethodBuilder {
    method("pgsqlLOBCreate")
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::Bool]))
        .body(vec![
            s_if(
                e_not(e_method_call(e_this(), "inTransaction", vec![])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_oid", e_call("elephc_pdo_lob_create", vec![e_method_call(e_this(), "connectionId", vec![])])),
            s_return(e_ternary(e_binop(e_var("_oid"), BinOp::StrictEq, e_str("")), e_bool(false), e_var("_oid"))),
        ])
}

/// `pgsqlLOBOpen` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_pgsqllobopen() -> MethodBuilder {
    method("pgsqlLOBOpen")
        .param("oid", TypeExpr::Str)
        .param_default("mode", TypeExpr::Str, e_str("rb"))
        .returns(t_mixed())
        .body(vec![
            s_return(e_static_call("__ElephcPDOPgsqlLobStream", "create", vec![e_this(), e_method_call(e_this(), "connectionId", vec![]), e_var("oid"), e_var("mode")])),
        ])
}

/// `pgsqlLOBUnlink` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_pgsqllobunlink() -> MethodBuilder {
    method("pgsqlLOBUnlink")
        .param("oid", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_method_call(e_this(), "inTransaction", vec![])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_call("elephc_pdo_lob_unlink", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("oid")]), BinOp::StrictEq, e_int(1))),
        ])
}

/// `pgsqlGetNotify` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_pgsqlgetnotify() -> MethodBuilder {
    method("pgsqlGetNotify")
        .param_default("fetchMode", TypeExpr::Int, e_int(0))
        .param_default("timeoutMilliseconds", TypeExpr::Int, e_int(0))
        .returns(t_mixed())
        .body(vec![
            s_assign("_raw", e_call("elephc_pdo_get_notify", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("timeoutMilliseconds")])),
            s_if(
                e_binop(e_var("_raw"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_parts", e_call("explode", vec![e_str("\t"), e_var("_raw")])),
            s_assign("_pid", e_ternary(e_call("isset", vec![e_index(e_var("_parts"), e_int(1))]), e_cast(CastType::Int, e_index(e_var("_parts"), e_int(1))), e_int(0))),
            s_assign("_payload", e_ternary(e_call("isset", vec![e_index(e_var("_parts"), e_int(2))]), e_index(e_var("_parts"), e_int(2)), e_str(""))),
            s_if(
                e_binop(e_var("fetchMode"), BinOp::Eq, e_int(2)),
                vec![
                    s_return(e_array_assoc(vec![(e_str("message"), e_index(e_var("_parts"), e_int(0))), (e_str("pid"), e_var("_pid")), (e_str("payload"), e_var("_payload"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_array(vec![e_index(e_var("_parts"), e_int(0)), e_var("_pid"), e_var("_payload")])),
        ])
}

/// `pgsqlGetPid` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_pgsqlgetpid() -> MethodBuilder {
    method("pgsqlGetPid")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_pdo_backend_pid", vec![e_method_call(e_this(), "connectionId", vec![])])),
        ])
}

/// `errorCode` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_errorcode() -> MethodBuilder {
    method("errorCode")
        .returns(t_nullable(TypeExpr::Str))
        .body(vec![
            s_if(
                e_not(e_this_prop("hasOperation")),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_return(e_call("elephc_pdo_sqlstate", vec![e_this_prop("conn")])),
        ])
}

/// `errorInfo` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_errorinfo() -> MethodBuilder {
    method("errorInfo")
        .returns(t_array())
        .body(vec![
            s_if(
                e_not(e_this_prop("hasOperation")),
                vec![
                    s_return(e_array(vec![e_str(""), e_null(), e_null()])),
                ],
                vec![],
                None,
            ),
            s_assign("_sqlstate", e_call("elephc_pdo_sqlstate", vec![e_this_prop("conn")])),
            s_if(
                e_binop(e_var("_sqlstate"), BinOp::StrictEq, e_str("00000")),
                vec![
                    s_return(e_array(vec![e_str("00000"), e_null(), e_null()])),
                ],
                vec![],
                None,
            ),
            s_assign("_message", e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])),
            s_if(
                e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("dblib")),
                vec![
                    s_return(e_method_call(e_this(), "dblibErrorInfo", vec![e_var("_message")])),
                ],
                vec![],
                None,
            ),
            s_return(e_array(vec![e_var("_sqlstate"), e_call("elephc_pdo_errcode", vec![e_this_prop("conn")]), e_var("_message")])),
        ])
}

/// `quote` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_quote() -> MethodBuilder {
    method("quote")
        .param("string", TypeExpr::Str)
        .param_default("type", TypeExpr::Int, e_int(2))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::Bool]))
        .body(vec![
            s_prop_assign(e_this(), "hasOperation", e_bool(true)),
            s_assign("_driver", e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")])),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("odbc")),
                vec![
                    s_expr(e_method_call(e_this(), "failCode", vec![e_str("IM001"), e_str("driver does not support quoting")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("mysql")),
                vec![
                    s_if(
                        e_binop(e_call("elephc_pdo_no_backslash_escapes", vec![e_this_prop("conn")]), BinOp::NotEq, e_int(0)),
                        vec![
                            s_assign("_s", e_call("str_replace", vec![e_str("'"), e_str("''"), e_var("string")])),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("_s", e_call("str_replace", vec![e_str("\\"), e_str("\\\\"), e_var("string")])),
                        s_assign("_s", e_call("str_replace", vec![e_str("'"), e_str("\\'"), e_var("_s")])),
                        s_assign("_s", e_call("str_replace", vec![e_str("\""), e_str("\\\""), e_var("_s")])),
                        s_assign("_s", e_call("str_replace", vec![e_call("chr", vec![e_int(0)]), e_str("\\0"), e_var("_s")])),
                        s_assign("_s", e_call("str_replace", vec![e_call("chr", vec![e_int(10)]), e_str("\\n"), e_var("_s")])),
                        s_assign("_s", e_call("str_replace", vec![e_call("chr", vec![e_int(13)]), e_str("\\r"), e_var("_s")])),
                        s_assign("_s", e_call("str_replace", vec![e_call("chr", vec![e_int(26)]), e_str("\\Z"), e_var("_s")])),
                    ]),
                    ),
                    s_assign("_quoted", e_binop(e_binop(e_str("'"), BinOp::Concat, e_var("_s")), BinOp::Concat, e_str("'"))),
                    s_if(
                        e_binop(e_var("type"), BinOp::Eq, e_int(3)),
                        vec![
                            s_return(e_binop(e_str("_binary"), BinOp::Concat, e_var("_quoted"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_quoted")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("pgsql")),
                vec![
                    s_if(
                        e_binop(e_var("type"), BinOp::Eq, e_int(3)),
                        vec![
                            s_return(e_binop(e_binop(e_str("'\\x"), BinOp::Concat, e_call("bin2hex", vec![e_var("string")])), BinOp::Concat, e_str("'"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_doubled", e_call("str_replace", vec![e_str("'"), e_str("''"), e_var("string")])),
                    s_if(
                        e_binop(e_call("strpos", vec![e_var("string"), e_str("\\")]), BinOp::StrictNotEq, e_bool(false)),
                        vec![
                            s_return(e_binop(e_binop(e_str("E'"), BinOp::Concat, e_call("str_replace", vec![e_str("\\"), e_str("\\\\"), e_var("_doubled")])), BinOp::Concat, e_str("'"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_binop(e_str("'"), BinOp::Concat, e_var("_doubled")), BinOp::Concat, e_str("'"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("dblib")),
                vec![
                    s_assign("_stringFlags", e_binop(e_var("type"), BinOp::BitAnd, e_int(1610612736))),
                    s_assign("_national", e_binop(e_binop(e_var("_stringFlags"), BinOp::Eq, e_int(1073741824)), BinOp::Or, e_binop(e_binop(e_var("_stringFlags"), BinOp::Eq, e_int(0)), BinOp::And, e_binop(e_this_prop("defaultStrParam"), BinOp::Eq, e_int(1073741824))))),
                    s_if(
                        e_binop(e_var("_stringFlags"), BinOp::Eq, e_int(536870912)),
                        vec![
                            s_assign("_national", e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_binop(e_binop(e_ternary(e_var("_national"), e_str("N"), e_str("")), BinOp::Concat, e_str("'")), BinOp::Concat, e_call("str_replace", vec![e_str("'"), e_str("''"), e_var("string")])), BinOp::Concat, e_str("'"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")),
                vec![
                    s_assign("_encoding", e_call("elephc_pdo_odbc_attribute", vec![e_this_prop("conn"), e_int(1000)])),
                    s_if(
                        e_binop(e_binop(e_var("_encoding"), BinOp::Eq, e_int(2)), BinOp::Or, e_binop(e_var("type"), BinOp::Eq, e_int(3))),
                        vec![
                            s_return(e_binop(e_str("0x"), BinOp::Concat, e_call("strtoupper", vec![e_call("bin2hex", vec![e_var("string")])]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_stringFlags", e_binop(e_var("type"), BinOp::BitAnd, e_int(1610612736))),
                    s_assign("_national", e_binop(e_binop(e_binop(e_var("_encoding"), BinOp::Eq, e_int(65001)), BinOp::Or, e_binop(e_var("_stringFlags"), BinOp::Eq, e_int(1073741824))), BinOp::Or, e_binop(e_binop(e_var("_stringFlags"), BinOp::Eq, e_int(0)), BinOp::And, e_binop(e_this_prop("defaultStrParam"), BinOp::Eq, e_int(1073741824))))),
                    s_if(
                        e_binop(e_var("_stringFlags"), BinOp::Eq, e_int(536870912)),
                        vec![
                            s_assign("_national", e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_binop(e_binop(e_ternary(e_var("_national"), e_str("N"), e_str("")), BinOp::Concat, e_str("'")), BinOp::Concat, e_call("str_replace", vec![e_str("'"), e_str("''"), e_var("string")])), BinOp::Concat, e_str("'"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("cubrid")),
                vec![
                    s_assign("_length", e_call("elephc_pdo_cubrid_quote", vec![e_this_prop("conn"), e_var("string"), e_call("strlen", vec![e_var("string")])])),
                    s_if(
                        e_binop(e_var("_length"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "fail", vec![e_str("CUBRID failed to quote the string")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_ptr_read_string", vec![e_call("elephc_pdo_blob_data_ptr", vec![]), e_var("_length")])),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_binop(e_str("'"), BinOp::Concat, e_call("str_replace", vec![e_str("'"), e_str("''"), e_var("string")])), BinOp::Concat, e_str("'"))),
        ])
}

/// `__clone` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_clone() -> MethodBuilder {
    method("__clone")
        .returns(TypeExpr::Void)
        .body(vec![
            s_throw(e_new("Error", vec![e_binop(e_str("Trying to clone an uncloneable object of class "), BinOp::Concat, e_call("get_class", vec![e_this()]))])),
        ])
}

/// `__serialize` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_serialize() -> MethodBuilder {
    method("__serialize")
        .returns(t_array())
        .body(vec![
            s_throw(e_new("Exception", vec![e_binop(e_binop(e_str("Serialization of '"), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str("' is not allowed"))])),
        ])
}

/// `__sleep` — lifted out of `decl_class_pdo` so it builds in its own stack frame.
fn pdo_sleep() -> MethodBuilder {
    method("__sleep")
        .returns(t_array())
        .body(vec![
            s_throw(e_new("Exception", vec![e_binop(e_binop(e_str("Serialization of '"), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str("' is not allowed"))])),
        ])
}

/// `__construct` — lifted out of `decl_class_pdorow` so it builds in its own stack frame.
fn pdorow_construct() -> MethodBuilder {
    method("__construct")
        .private()
        .param_default("internal", TypeExpr::Bool, e_bool(false))
        .param_default("queryString", TypeExpr::Str, e_str(""))
        .body(vec![
            s_if(
                e_not(e_var("internal")),
                vec![
                    s_throw(e_new("PDOException", vec![e_str("You may not create a PDORow manually")])),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "queryString", e_var("queryString")),
            s_prop_assign(e_this(), "columns", e_array(vec![])),
            s_prop_assign(e_this(), "names", e_array(vec![])),
        ])
}

/// `__elephcRefresh` — lifted out of `decl_class_pdorow` so it builds in its own stack frame.
fn pdorow_elephcrefresh() -> MethodBuilder {
    method("__elephcRefresh")
        .private()
        .param("columns", t_array())
        .param("names", t_array())
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "columns", e_var("columns")),
            s_prop_assign(e_this(), "names", e_var("names")),
        ])
}

/// `__get` — lifted out of `decl_class_pdorow` so it builds in its own stack frame.
fn pdorow_get() -> MethodBuilder {
    method("__get")
        .param("name", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_call("is_numeric", vec![e_var("name")]),
                vec![
                    s_return(e_method_call(e_this(), "offsetGet", vec![e_cast(CastType::Int, e_var("name"))])),
                ],
                vec![],
                None,
            ),
            s_assign("_count", e_call("count", vec![e_this_prop("names")])),
            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                s_if(
                    e_binop(e_index(e_this_prop("names"), e_var("_i")), BinOp::StrictEq, e_var("name")),
                    vec![
                        s_return(e_index(e_this_prop("columns"), e_var("_i"))),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_null()),
        ])
}

/// `__isset` — lifted out of `decl_class_pdorow` so it builds in its own stack frame.
fn pdorow_isset() -> MethodBuilder {
    method("__isset")
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_method_call(e_this(), "__get", vec![e_var("name")]), BinOp::StrictNotEq, e_null())),
        ])
}

/// `__set` — lifted out of `decl_class_pdorow` so it builds in its own stack frame.
fn pdorow_set() -> MethodBuilder {
    method("__set")
        .param("name", TypeExpr::Str)
        .param("value", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("_unusedName", e_var("name")),
            s_assign("_unusedValue", e_var("value")),
            s_throw(e_new("Error", vec![e_str("Cannot write to PDORow property")])),
        ])
}

/// `__unset` — lifted out of `decl_class_pdorow` so it builds in its own stack frame.
fn pdorow_unset() -> MethodBuilder {
    method("__unset")
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("_unusedName", e_var("name")),
            s_throw(e_new("Error", vec![e_str("Cannot unset PDORow property")])),
        ])
}

/// `offsetExists` — lifted out of `decl_class_pdorow` so it builds in its own stack frame.
fn pdorow_offsetexists() -> MethodBuilder {
    method("offsetExists")
        .param("offset", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_method_call(e_this(), "offsetGet", vec![e_var("offset")]), BinOp::StrictNotEq, e_null())),
        ])
}

/// `offsetGet` — lifted out of `decl_class_pdorow` so it builds in its own stack frame.
fn pdorow_offsetget() -> MethodBuilder {
    method("offsetGet")
        .param("offset", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_call("is_int", vec![e_var("offset")]),
                vec![
                    s_assign("_index", e_cast(CastType::Int, e_var("offset"))),
                    s_if(
                        e_binop(e_binop(e_var("_index"), BinOp::GtEq, e_int(0)), BinOp::And, e_binop(e_var("_index"), BinOp::Lt, e_call("count", vec![e_this_prop("columns")]))),
                        vec![
                            s_return(e_index(e_this_prop("columns"), e_var("_index"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_this(), "__get", vec![e_cast(CastType::String, e_var("offset"))])),
        ])
}

/// `offsetSet` — lifted out of `decl_class_pdorow` so it builds in its own stack frame.
fn pdorow_offsetset() -> MethodBuilder {
    method("offsetSet")
        .param("offset", t_mixed())
        .param("value", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("_unusedValue", e_var("value")),
            s_if(
                e_binop(e_var("offset"), BinOp::StrictEq, e_null()),
                vec![
                    s_throw(e_new("Error", vec![e_str("Cannot append to PDORow offset")])),
                ],
                vec![],
                None,
            ),
            s_throw(e_new("Error", vec![e_str("Cannot write to PDORow offset")])),
        ])
}

/// `offsetUnset` — lifted out of `decl_class_pdorow` so it builds in its own stack frame.
fn pdorow_offsetunset() -> MethodBuilder {
    method("offsetUnset")
        .param("offset", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("_unusedOffset", e_var("offset")),
            s_throw(e_new("Error", vec![e_str("Cannot unset PDORow offset")])),
        ])
}

/// `__serialize` — lifted out of `decl_class_pdorow` so it builds in its own stack frame.
fn pdorow_serialize() -> MethodBuilder {
    method("__serialize")
        .returns(t_array())
        .body(vec![
            s_throw(e_new("Exception", vec![e_str("Serialization of 'PDORow' is not allowed")])),
        ])
}

/// `__sleep` — lifted out of `decl_class_pdorow` so it builds in its own stack frame.
fn pdorow_sleep() -> MethodBuilder {
    method("__sleep")
        .returns(t_array())
        .body(vec![
            s_throw(e_new("Exception", vec![e_str("Serialization of 'PDORow' is not allowed")])),
        ])
}

/// `__construct` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_construct() -> MethodBuilder {
    method("__construct")
        .param("handle", TypeExpr::Int)
        .param("connection", TypeExpr::Int)
        .param_default("errMode", TypeExpr::Int, e_int(2))
        .param_default("query", TypeExpr::Str, e_str(""))
        .body(vec![
            s_if(
                e_binop(e_call("elephc_pdo_driver_name", vec![e_var("connection")]), BinOp::StrictEq, e_str("")),
                vec![
                    s_throw(e_new("PDOException", vec![e_str("You should not create a PDOStatement manually")])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephcInitialize", vec![e_var("handle"), e_var("connection"), e_var("errMode"), e_var("query")])),
        ])
}

/// `__elephcInitialize` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_elephcinitialize() -> MethodBuilder {
    method("__elephcInitialize")
        .private()
        .param("handle", TypeExpr::Int)
        .param("connection", TypeExpr::Int)
        .param_default("errMode", TypeExpr::Int, e_int(2))
        .param_default("query", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "stmt", e_var("handle")),
            s_prop_assign(e_this(), "conn", e_var("connection")),
            s_prop_assign(e_this(), "errMode", e_var("errMode")),
            s_prop_assign(e_this(), "queryString", e_var("query")),
            s_prop_assign(e_this(), "fetchMode", e_int(4)),
            s_prop_assign(e_this(), "fetchTarget", e_null()),
            s_prop_assign(e_this(), "fetchCtorArgs", e_array(vec![])),
            s_prop_assign(e_this(), "fetchPropsLate", e_bool(false)),
            s_prop_assign(e_this(), "boundParams", e_array(vec![])),
            s_prop_assign(e_this(), "boundNames", e_array(vec![])),
            s_prop_assign(e_this(), "boundValues", e_array(vec![])),
            s_prop_assign(e_this(), "boundTypes", e_array(vec![])),
            s_prop_assign(e_this(), "boundDriverOptions", e_array(vec![])),
            s_prop_assign(e_this(), "boundPhpTypes", e_array(vec![])),
            s_prop_assign(e_this(), "boundNormalizedIndexes", e_array(vec![])),
            s_prop_assign(e_this(), "boundParamRefIndexes", e_array(vec![])),
            s_prop_assign(e_this(), "boundParamRefGetters", e_array(vec![])),
            s_prop_assign(e_this(), "boundParamRefStreamReaders", e_array(vec![])),
            s_prop_assign(e_this(), "boundParamRefSetters", e_array(vec![])),
            s_prop_assign(e_this(), "boundParamMaxLengths", e_array(vec![])),
            s_prop_assign(e_this(), "boundColumnKinds", e_array(vec![])),
            s_prop_assign(e_this(), "boundColumnIndexes", e_array(vec![])),
            s_prop_assign(e_this(), "boundColumnNames", e_array(vec![])),
            s_prop_assign(e_this(), "boundColumnSetters", e_array(vec![])),
            s_prop_assign(e_this(), "boundColumnTypes", e_array(vec![])),
            s_prop_assign(e_this(), "fetchColumn", e_int(0)),
            s_prop_assign(e_this(), "rowCount", e_int(0)),
            s_prop_assign(e_this(), "executed", e_binop(e_var("query"), BinOp::StrictEq, e_str("__elephc_cubrid_schema__"))),
            s_prop_assign(e_this(), "hasOperation", e_bool(false)),
            s_prop_assign(e_this(), "lazyRow", e_null()),
            s_prop_assign(e_this(), "hasPendingStep", e_bool(false)),
            s_prop_assign(e_this(), "pendingStep", e_int(0)),
            s_prop_assign(e_this(), "scrollable", e_bool(false)),
            s_prop_assign(e_this(), "stringifyFetches", e_bool(false)),
            s_prop_assign(e_this(), "defaultStrParam", e_int(536870912)),
            s_prop_assign(e_this(), "emulatePrepares", e_bool(false)),
            s_prop_assign(e_this(), "attrCase", e_int(0)),
            s_prop_assign(e_this(), "oracleNulls", e_int(0)),
            s_prop_assign(e_this(), "owner", e_null()),
        ])
}

/// `setOwner` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_setowner() -> MethodBuilder {
    method("setOwner")
        .param("owner", t_class("PDO"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "owner", e_var("owner")),
        ])
}

/// `setStringifyFetches` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_setstringifyfetches() -> MethodBuilder {
    method("setStringifyFetches")
        .param("on", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "stringifyFetches", e_var("on")),
        ])
}

/// `setDefaultStrParam` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_setdefaultstrparam() -> MethodBuilder {
    method("setDefaultStrParam")
        .param("type", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "defaultStrParam", e_var("type")),
        ])
}

/// `setEmulatePrepares` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_setemulateprepares() -> MethodBuilder {
    method("setEmulatePrepares")
        .param("on", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "emulatePrepares", e_var("on")),
        ])
}

/// `setAttrCase` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_setattrcase() -> MethodBuilder {
    method("setAttrCase")
        .param("mode", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "attrCase", e_var("mode")),
        ])
}

/// `setOracleNulls` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_setoraclenulls() -> MethodBuilder {
    method("setOracleNulls")
        .param("mode", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "oracleNulls", e_var("mode")),
        ])
}

/// `currentStringifyFetches` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_currentstringifyfetches() -> MethodBuilder {
    method("currentStringifyFetches")
        .private()
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_this_prop("owner"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_return(e_cast(CastType::Bool, e_method_call(e_this_prop("owner"), "getAttribute", vec![e_class_const("PDO", "ATTR_STRINGIFY_FETCHES")]))),
                ],
                vec![],
                None,
            ),
            s_return(e_this_prop("stringifyFetches")),
        ])
}

/// `currentAttrCase` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_currentattrcase() -> MethodBuilder {
    method("currentAttrCase")
        .private()
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_binop(e_this_prop("owner"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_return(e_cast(CastType::Int, e_method_call(e_this_prop("owner"), "getAttribute", vec![e_class_const("PDO", "ATTR_CASE")]))),
                ],
                vec![],
                None,
            ),
            s_return(e_this_prop("attrCase")),
        ])
}

/// `currentOracleNulls` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_currentoraclenulls() -> MethodBuilder {
    method("currentOracleNulls")
        .private()
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_binop(e_this_prop("owner"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_return(e_cast(CastType::Int, e_method_call(e_this_prop("owner"), "getAttribute", vec![e_class_const("PDO", "ATTR_ORACLE_NULLS")]))),
                ],
                vec![],
                None,
            ),
            s_return(e_this_prop("oracleNulls")),
        ])
}

/// `setScrollable` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_setscrollable() -> MethodBuilder {
    method("setScrollable")
        .param("scrollable", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "scrollable", e_var("scrollable")),
        ])
}

/// `dblibStatementErrorInfo` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_dblibstatementerrorinfo() -> MethodBuilder {
    method("dblibStatementErrorInfo")
        .private()
        .param("message", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            s_assign("_sqlstate", e_call("elephc_pdo_stmt_sqlstate", vec![e_this_prop("stmt")])),
            s_assign("_native", e_call("elephc_pdo_stmt_errcode", vec![e_this_prop("stmt")])),
            s_assign("_osCode", e_call("elephc_pdo_dblib_stmt_os_errcode", vec![e_this_prop("stmt")])),
            s_assign("_severity", e_call("elephc_pdo_dblib_stmt_severity", vec![e_this_prop("stmt")])),
            s_assign("_query", e_call("elephc_pdo_stmt_sent_sql", vec![e_this_prop("stmt")])),
            s_if(
                e_binop(e_var("_query"), BinOp::StrictEq, e_str("")),
                vec![
                    s_assign("_query", e_this_prop("queryString")),
                ],
                vec![],
                None,
            ),
            s_assign("_formatted", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("message"), BinOp::Concat, e_str(" [")), BinOp::Concat, e_var("_native")), BinOp::Concat, e_str("] (severity ")), BinOp::Concat, e_var("_severity")), BinOp::Concat, e_str(") [")), BinOp::Concat, e_var("_query")), BinOp::Concat, e_str("]"))),
            s_assign("_info", e_array(vec![e_var("_sqlstate"), e_var("_native"), e_var("_formatted"), e_var("_osCode"), e_var("_severity")])),
            s_assign("_osMessage", e_call("elephc_pdo_dblib_stmt_os_errmsg", vec![e_this_prop("stmt")])),
            s_if(
                e_binop(e_var("_osMessage"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_array_push("_info", e_var("_osMessage")),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_info")),
        ])
}

/// `fail` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_fail() -> MethodBuilder {
    method("fail")
        .private()
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_this_prop("errMode"), BinOp::Eq, e_int(0)),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("_sqlstate", e_call("elephc_pdo_stmt_sqlstate", vec![e_this_prop("stmt")])),
            s_assign("_native", e_call("elephc_pdo_stmt_errcode", vec![e_this_prop("stmt")])),
            s_assign("_errorInfo", e_array(vec![e_var("_sqlstate"), e_var("_native"), e_var("message")])),
            s_if(
                e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("dblib")),
                vec![
                    s_assign("_errorInfo", e_method_call(e_this(), "dblibStatementErrorInfo", vec![e_var("message")])),
                    s_assign("message", e_cast(CastType::String, e_index(e_var("_errorInfo"), e_int(2)))),
                ],
                vec![],
                None,
            ),
            s_assign("_full", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("SQLSTATE["), BinOp::Concat, e_var("_sqlstate")), BinOp::Concat, e_str("]: ")), BinOp::Concat, e_call("__elephc_pdo_sqlstate_description", vec![e_var("_sqlstate")])), BinOp::Concat, e_str(": ")), BinOp::Concat, e_var("_native")), BinOp::Concat, e_str(" ")), BinOp::Concat, e_var("message"))),
            s_if(
                e_binop(e_this_prop("errMode"), BinOp::Eq, e_int(2)),
                vec![
                    s_throw(e_static_call("PDOException", "__elephcFromErrorInfo", vec![e_var("_full"), e_var("_errorInfo")])),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("fwrite", vec![e_const("STDERR"), e_binop(e_binop(e_str("PDO error: "), BinOp::Concat, e_var("_full")), BinOp::Concat, e_str("\n"))])),
        ])
}

/// `failCode` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_failcode() -> MethodBuilder {
    method("failCode")
        .private()
        .param("sqlstate", TypeExpr::Str)
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_this_prop("errMode"), BinOp::Eq, e_int(0)),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("_full", e_call("__elephc_pdo_impl_error_message", vec![e_var("sqlstate"), e_var("message")])),
            s_if(
                e_binop(e_this_prop("errMode"), BinOp::Eq, e_int(2)),
                vec![
                    s_throw(e_static_call("PDOException", "__elephcFromErrorInfo", vec![e_var("_full"), e_array(vec![e_var("sqlstate"), e_int(0)])])),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("fwrite", vec![e_const("STDERR"), e_binop(e_binop(e_str("PDO error: "), BinOp::Concat, e_var("_full")), BinOp::Concat, e_str("\n"))])),
        ])
}

/// `errorCode` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_errorcode() -> MethodBuilder {
    method("errorCode")
        .returns(t_nullable(TypeExpr::Str))
        .body(vec![
            s_if(
                e_not(e_this_prop("hasOperation")),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_return(e_call("elephc_pdo_stmt_sqlstate", vec![e_this_prop("stmt")])),
        ])
}

/// `errorInfo` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_errorinfo() -> MethodBuilder {
    method("errorInfo")
        .returns(t_array())
        .body(vec![
            s_if(
                e_not(e_this_prop("hasOperation")),
                vec![
                    s_return(e_array(vec![e_str(""), e_null(), e_null()])),
                ],
                vec![],
                None,
            ),
            s_assign("_sqlstate", e_call("elephc_pdo_stmt_sqlstate", vec![e_this_prop("stmt")])),
            s_if(
                e_binop(e_var("_sqlstate"), BinOp::StrictEq, e_str("00000")),
                vec![
                    s_return(e_array(vec![e_str("00000"), e_null(), e_null()])),
                ],
                vec![],
                None,
            ),
            s_assign("_message", e_call("elephc_pdo_stmt_errmsg", vec![e_this_prop("stmt")])),
            s_if(
                e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("dblib")),
                vec![
                    s_return(e_method_call(e_this(), "dblibStatementErrorInfo", vec![e_var("_message")])),
                ],
                vec![],
                None,
            ),
            s_return(e_array(vec![e_var("_sqlstate"), e_call("elephc_pdo_stmt_errcode", vec![e_this_prop("stmt")]), e_var("_message")])),
        ])
}

/// `setDefaultFetchMode` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_setdefaultfetchmode() -> MethodBuilder {
    method("setDefaultFetchMode")
        .param("mode", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "fetchMode", e_var("mode")),
        ])
}

/// `argValueTypeName` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_argvaluetypename() -> MethodBuilder {
    method("argValueTypeName")
        .private()
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_call("is_int", vec![e_var("value")]),
                vec![
                    s_return(e_str("int")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_bool", vec![e_var("value")]),
                vec![
                    s_if(
                        e_cast(CastType::Bool, e_var("value")),
                        vec![
                            s_return(e_str("true")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_str("false")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_float", vec![e_var("value")]),
                vec![
                    s_return(e_str("float")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![
                    s_return(e_str("string")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![
                    s_return(e_str("array")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_null", vec![e_var("value")]),
                vec![
                    s_return(e_str("null")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("object")),
        ])
}

/// `copyConstructorArgs` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_copyconstructorargs() -> MethodBuilder {
    method("copyConstructorArgs")
        .private()
        .param("source", t_mixed())
        .returns(t_array())
        .body(vec![
            s_assign("_copy", e_array(vec![])),
            s_foreach(e_var("source"), Some("_key"), "_value", vec![
                s_array_assign("_copy", e_var("_key"), e_var("_value")),
            ]),
            s_return(e_var("_copy")),
        ])
}

/// `bindValue` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_bindvalue() -> MethodBuilder {
    method("bindValue")
        .param_untyped("parameter")
        .param_untyped("value")
        .param_default("type", TypeExpr::Int, e_int(2))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_this(), "bindValueWithDriverOption", vec![e_var("parameter"), e_var("value"), e_var("type"), e_null()])),
        ])
}

/// `bindValueWithDriverOption` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_bindvaluewithdriveroption() -> MethodBuilder {
    method("bindValueWithDriverOption")
        .private()
        .param_untyped("parameter")
        .param_untyped("value")
        .param("type", TypeExpr::Int)
        .param("driverOption", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_call("is_int", vec![e_var("parameter")]),
                vec![
                    s_if(
                        e_binop(e_cast(CastType::Int, e_var("parameter")), BinOp::Lt, e_int(1)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("PDOStatement::bindValue(): Argument #1 ($param) must be greater than or equal to 1")])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![
                (e_binop(e_cast(CastType::String, e_var("parameter")), BinOp::StrictEq, e_str("")), vec![
                    s_throw(e_new("ValueError", vec![e_str("PDOStatement::bindValue(): Argument #1 ($param) must not be empty")])),
                ]),
            ],
                None,
            ),
            s_if(
                e_call("is_int", vec![e_var("parameter")]),
                vec![
                    s_assign("_slot", e_cast(CastType::Int, e_var("parameter"))),
                    s_assign("_pname", e_str("")),
                ],
                vec![],
                Some(vec![
                s_assign("_slot", e_cast(CastType::Int, e_call("elephc_pdo_bind_parameter_index", vec![e_this_prop("stmt"), e_cast(CastType::String, e_var("parameter"))]))),
                s_assign("_pname", e_cast(CastType::String, e_var("parameter"))),
            ]),
            ),
            s_prop_array_push(e_this(), "boundParams", e_var("_slot")),
            s_prop_array_push(e_this(), "boundNames", e_var("_pname")),
            s_prop_array_push(e_this(), "boundValues", e_var("value")),
            s_prop_array_push(e_this(), "boundTypes", e_var("type")),
            s_prop_array_push(e_this(), "boundDriverOptions", e_var("driverOption")),
            s_prop_array_push(e_this(), "boundPhpTypes", e_var("type")),
            s_return(e_bool(true)),
        ])
}

/// `bindParam` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_bindparam() -> MethodBuilder {
    method("bindParam")
        .param_untyped("parameter")
        .param_by_ref("variable", Some(t_mixed()))
        .param_default("type", TypeExpr::Int, e_int(2))
        .param_default("maxLength", TypeExpr::Int, e_int(0))
        .param_default("driverOptions", t_mixed(), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_call("is_int", vec![e_var("parameter")]),
                vec![
                    s_if(
                        e_binop(e_cast(CastType::Int, e_var("parameter")), BinOp::Lt, e_int(1)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("PDOStatement::bindParam(): Argument #1 ($param) must be greater than or equal to 1")])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![
                (e_binop(e_cast(CastType::String, e_var("parameter")), BinOp::StrictEq, e_str("")), vec![
                    s_throw(e_new("ValueError", vec![e_str("PDOStatement::bindParam(): Argument #1 ($param) must not be empty")])),
                ]),
            ],
                None,
            ),
            s_assign("_ok", e_method_call(e_this(), "bindValueWithDriverOption", vec![e_var("parameter"), e_var("variable"), e_var("type"), e_var("driverOptions")])),
            s_assign("_boundIndex", e_binop(e_call("count", vec![e_this_prop("boundValues")]), BinOp::Sub, e_int(1))),
            s_assign("_getter", closure()
                .captures_ref("variable")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_var("variable")),
                ])
                .build()),
            s_assign("_streamReader", closure()
                .captures_ref("variable")
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_not(e_call("is_resource", vec![e_var("variable")])),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_contents", e_call("stream_get_contents", vec![e_var("variable")])),
                    s_if(
                        e_binop(e_var("_contents"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_cast(CastType::String, e_var("_contents"))),
                ])
                .build()),
            s_assign("_setter", closure()
                .param("_value", t_mixed())
                .captures_ref("variable")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("variable", e_var("_value")),
                ])
                .build()),
            s_prop_array_push(e_this(), "boundParamRefIndexes", e_var("_boundIndex")),
            s_prop_array_push(e_this(), "boundParamRefGetters", e_var("_getter")),
            s_prop_array_push(e_this(), "boundParamRefStreamReaders", e_var("_streamReader")),
            s_prop_array_push(e_this(), "boundParamRefSetters", e_var("_setter")),
            s_prop_array_push(e_this(), "boundParamMaxLengths", e_var("maxLength")),
            s_return(e_var("_ok")),
        ])
}

/// `bindColumn` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_bindcolumn() -> MethodBuilder {
    method("bindColumn")
        .param("column", t_union(vec![TypeExpr::Str, TypeExpr::Int]))
        .param_by_ref("var", Some(t_union(vec![TypeExpr::Str, TypeExpr::Int, TypeExpr::Float, TypeExpr::Bool, TypeExpr::Void])))
        .param_default("type", TypeExpr::Int, e_int(2))
        .param_default("maxLength", TypeExpr::Int, e_int(0))
        .param_default("driverOptions", t_mixed(), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("is_int", vec![e_var("column")]), BinOp::And, e_binop(e_cast(CastType::Int, e_var("column")), BinOp::Lt, e_int(1))),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("PDOStatement::bindColumn(): Argument #1 ($column) must be greater than or equal to 1")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("is_string", vec![e_var("column")]), BinOp::And, e_binop(e_cast(CastType::String, e_var("column")), BinOp::StrictEq, e_str(""))),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("PDOStatement::bindColumn(): Argument #1 ($column) must not be empty")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_null", vec![e_var("var")]),
                vec![
                    s_assign("var", e_null()),
                ],
                vec![],
                None,
            ),
            s_assign("_setter", closure()
                .param("_value", t_union(vec![TypeExpr::Str, TypeExpr::Int, TypeExpr::Float, TypeExpr::Bool, TypeExpr::Void]))
                .captures_ref("var")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("var", e_var("_value")),
                ])
                .build()),
            s_if(
                e_call("is_int", vec![e_var("column")]),
                vec![
                    s_prop_array_push(e_this(), "boundColumnKinds", e_int(0)),
                    s_prop_array_push(e_this(), "boundColumnIndexes", e_cast(CastType::Int, e_var("column"))),
                    s_prop_array_push(e_this(), "boundColumnNames", e_str("")),
                ],
                vec![],
                Some(vec![
                s_prop_array_push(e_this(), "boundColumnKinds", e_int(1)),
                s_prop_array_push(e_this(), "boundColumnIndexes", e_int(0)),
                s_prop_array_push(e_this(), "boundColumnNames", e_cast(CastType::String, e_var("column"))),
            ]),
            ),
            s_prop_array_push(e_this(), "boundColumnSetters", e_var("_setter")),
            s_prop_array_push(e_this(), "boundColumnTypes", e_var("type")),
            s_assign("_unusedMaxLength", e_var("maxLength")),
            s_assign("_unusedDriverOptions", e_var("driverOptions")),
            s_return(e_bool(true)),
        ])
}

/// `syncOutputParameters` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_syncoutputparameters() -> MethodBuilder {
    method("syncOutputParameters")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("_refCount", e_call("count", vec![e_this_prop("boundParamRefIndexes")])),
            s_for(Some(s_assign("_ri", e_int(0))), Some(e_binop(e_var("_ri"), BinOp::Lt, e_var("_refCount"))), Some(s_expr(e_post_inc("_ri"))), vec![
                s_assign("_boundIndex", e_cast(CastType::Int, e_index(e_this_prop("boundParamRefIndexes"), e_var("_ri")))),
                s_assign("_slot", e_cast(CastType::Int, e_index(e_this_prop("boundParams"), e_var("_boundIndex")))),
                s_assign("_length", e_call("elephc_pdo_output_data", vec![e_this_prop("stmt"), e_var("_slot")])),
                s_if(
                    e_binop(e_var("_length"), BinOp::Eq, e_neg(e_int(3))),
                    vec![
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_setter", e_index(e_this_prop("boundParamRefSetters"), e_var("_ri"))),
                s_if(
                    e_call("is_callable", vec![e_var("_setter")]),
                    vec![
                        s_typed_assign(t_class("callable"), "_typedSetter", e_var("_setter")),
                        s_if(
                            e_binop(e_var("_length"), BinOp::Eq, e_neg(e_int(2))),
                            vec![
                                s_expr(e_call("call_user_func_array", vec![e_var("_typedSetter"), e_array(vec![e_null()])])),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("_bytes", e_str("")),
                            s_if(
                                e_binop(e_var("_length"), BinOp::Gt, e_int(0)),
                                vec![
                                    s_assign("_bytes", e_call("__elephc_ptr_read_string", vec![e_call("elephc_pdo_blob_data_ptr", vec![]), e_var("_length")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_call("elephc_pdo_output_is_lob", vec![e_this_prop("stmt"), e_var("_slot")]), BinOp::NotEq, e_int(0)),
                                vec![
                                    s_assign("_stream", e_call("fopen", vec![e_str("php://memory"), e_str("r+")])),
                                    s_expr(e_call("fwrite", vec![e_var("_stream"), e_var("_bytes")])),
                                    s_expr(e_call("rewind", vec![e_var("_stream")])),
                                    s_expr(e_call("call_user_func_array", vec![e_var("_typedSetter"), e_array(vec![e_var("_stream")])])),
                                ],
                                vec![
                                (e_binop(e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("ibm")), BinOp::And, e_binop(e_call("elephc_pdo_output_is_numeric", vec![e_this_prop("stmt"), e_var("_slot")]), BinOp::NotEq, e_int(0))), BinOp::And, e_binop(e_binop(e_cast(CastType::Int, e_index(e_this_prop("boundTypes"), e_var("_boundIndex"))), BinOp::BitAnd, e_int(65535)), BinOp::Eq, e_class_const("PDO", "PARAM_INT"))), vec![
                                    s_expr(e_call("call_user_func_array", vec![e_var("_typedSetter"), e_array(vec![e_cast(CastType::Int, e_var("_bytes"))])])),
                                ]),
                                (e_binop(e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("ibm")), BinOp::And, e_binop(e_call("elephc_pdo_output_is_numeric", vec![e_this_prop("stmt"), e_var("_slot")]), BinOp::NotEq, e_int(0))), BinOp::And, e_binop(e_binop(e_cast(CastType::Int, e_index(e_this_prop("boundTypes"), e_var("_boundIndex"))), BinOp::BitAnd, e_int(65535)), BinOp::Eq, e_class_const("PDO", "PARAM_BOOL"))), vec![
                                    s_expr(e_call("call_user_func_array", vec![e_var("_typedSetter"), e_array(vec![e_cast(CastType::Bool, e_cast(CastType::Int, e_var("_bytes")))])])),
                                ]),
                                (e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("informix")), BinOp::And, e_binop(e_binop(e_cast(CastType::Int, e_index(e_this_prop("boundTypes"), e_var("_boundIndex"))), BinOp::BitAnd, e_int(65535)), BinOp::Eq, e_class_const("PDO", "PARAM_INT"))), vec![
                                    s_expr(e_call("call_user_func_array", vec![e_var("_typedSetter"), e_array(vec![e_cast(CastType::Int, e_var("_bytes"))])])),
                                ]),
                                (e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("informix")), BinOp::And, e_binop(e_binop(e_cast(CastType::Int, e_index(e_this_prop("boundTypes"), e_var("_boundIndex"))), BinOp::BitAnd, e_int(65535)), BinOp::Eq, e_class_const("PDO", "PARAM_BOOL"))), vec![
                                    s_expr(e_call("call_user_func_array", vec![e_var("_typedSetter"), e_array(vec![e_cast(CastType::Bool, e_cast(CastType::Int, e_var("_bytes")))])])),
                                ]),
                            ],
                                Some(vec![
                                s_expr(e_call("call_user_func_array", vec![e_var("_typedSetter"), e_array(vec![e_var("_bytes")])])),
                            ]),
                            ),
                        ]),
                        ),
                    ],
                    vec![],
                    None,
                ),
            ]),
        ])
}

/// `execute` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_execute() -> MethodBuilder {
    method("execute")
        .param_default("params", t_nullable(t_array()), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_prop_assign(e_this(), "executed", e_bool(true)),
            s_prop_assign(e_this(), "hasOperation", e_bool(true)),
            s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
            s_expr(e_call("elephc_pdo_clear_bindings", vec![e_this_prop("stmt")])),
            s_prop_assign(e_this(), "hasPendingStep", e_bool(false)),
            s_prop_assign(e_this(), "pendingStep", e_int(0)),
            s_assign("_bindError", e_str("")),
            s_if(
                e_binop(e_var("params"), BinOp::StrictEq, e_null()),
                vec![
                    s_assign("_count", e_call("count", vec![e_this_prop("boundParams")])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_slot", e_cast(CastType::Int, e_index(e_this_prop("boundParams"), e_var("_i")))),
                        s_assign("_value", e_index(e_this_prop("boundValues"), e_var("_i"))),
                        s_assign("_isRefBind", e_bool(false)),
                        s_assign("_refWasStream", e_bool(false)),
                        s_assign("_refMaxLength", e_int(0)),
                        s_assign("_refCount", e_call("count", vec![e_this_prop("boundParamRefIndexes")])),
                        s_for(Some(s_assign("_ri", e_int(0))), Some(e_binop(e_var("_ri"), BinOp::Lt, e_var("_refCount"))), Some(s_expr(e_post_inc("_ri"))), vec![
                            s_if(
                                e_binop(e_index(e_this_prop("boundParamRefIndexes"), e_var("_ri")), BinOp::Eq, e_var("_i")),
                                vec![
                                    s_assign("_isRefBind", e_bool(true)),
                                    s_assign("_refMaxLength", e_cast(CastType::Int, e_index(e_this_prop("boundParamMaxLengths"), e_var("_ri")))),
                                    s_assign("_streamReader", e_index(e_this_prop("boundParamRefStreamReaders"), e_var("_ri"))),
                                    s_if(
                                        e_call("is_callable", vec![e_var("_streamReader")]),
                                        vec![
                                            s_typed_assign(t_class("callable"), "_typedStreamReader", e_var("_streamReader")),
                                            s_assign("_streamContents", e_call("call_user_func_array", vec![e_var("_typedStreamReader"), e_array(vec![])])),
                                            s_if(
                                                e_binop(e_var("_streamContents"), BinOp::StrictEq, e_bool(false)),
                                                vec![
                                                    s_assign("_bindError", e_str("__elephc_pdo_driver_error")),
                                                    s_break(1),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                            s_if(
                                                e_binop(e_var("_streamContents"), BinOp::StrictNotEq, e_null()),
                                                vec![
                                                    s_assign("_value", e_cast(CastType::String, e_var("_streamContents"))),
                                                    s_assign("_refWasStream", e_bool(true)),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_not(e_var("_refWasStream")),
                                        vec![
                                            s_assign("_getter", e_index(e_this_prop("boundParamRefGetters"), e_var("_ri"))),
                                            s_if(
                                                e_call("is_callable", vec![e_var("_getter")]),
                                                vec![
                                                    s_typed_assign(t_class("callable"), "_typedGetter", e_var("_getter")),
                                                    s_assign("_value", e_call("call_user_func_array", vec![e_var("_typedGetter"), e_array(vec![])])),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_break(1),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                        s_if(
                            e_binop(e_var("_bindError"), BinOp::StrictNotEq, e_str("")),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_rawBindType", e_cast(CastType::Int, e_index(e_this_prop("boundTypes"), e_var("_i")))),
                        s_assign("_btype", e_binop(e_var("_rawBindType"), BinOp::BitAnd, e_int(65535))),
                        s_assign("_driverOption", e_index(e_this_prop("boundDriverOptions"), e_var("_i"))),
                        s_if(
                            e_binop(e_var("_slot"), BinOp::Lt, e_int(1)),
                            vec![
                                s_assign("_bindError", e_str("parameter was not defined")),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_brc", e_int(0)),
                        s_assign("_driverName", e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")])),
                        s_if(
                            e_binop(e_binop(e_var("_driverName"), BinOp::StrictEq, e_str("cubrid")), BinOp::And, e_call("is_array", vec![e_var("_value")])),
                            vec![
                                s_assign("_setFrame", e_binop(e_cast(CastType::String, e_call("count", vec![e_var("_value")])), BinOp::Concat, e_str(":"))),
                                s_foreach(e_var("_value"), None, "_setValue", vec![
                                    s_assign("_setString", e_cast(CastType::String, e_var("_setValue"))),
                                    s_assign("_setFrame", e_binop(e_var("_setFrame"), BinOp::Concat, e_binop(e_binop(e_cast(CastType::String, e_call("strlen", vec![e_var("_setString")])), BinOp::Concat, e_str(":")), BinOp::Concat, e_var("_setString")))),
                                ]),
                                s_assign("_cubridType", e_ternary(e_binop(e_var("_driverOption"), BinOp::StrictEq, e_null()), e_str(""), e_cast(CastType::String, e_var("_driverOption")))),
                                s_assign("_brc", e_call("elephc_pdo_cubrid_bind_typed", vec![e_this_prop("stmt"), e_var("_slot"), e_var("_setFrame"), e_call("strlen", vec![e_var("_setFrame")]), e_var("_cubridType"), e_int(1), e_var("_btype")])),
                            ],
                            vec![
                            (e_binop(e_binop(e_var("_driverName"), BinOp::StrictEq, e_str("cubrid")), BinOp::And, e_binop(e_var("_driverOption"), BinOp::StrictNotEq, e_null())), vec![
                                s_assign("_cubridType", e_cast(CastType::String, e_var("_driverOption"))),
                                s_assign("_typedValue", e_str("")),
                                s_if(
                                    e_binop(e_binop(e_call("strtoupper", vec![e_var("_cubridType")]), BinOp::StrictEq, e_str("BLOB")), BinOp::Or, e_binop(e_call("strtoupper", vec![e_var("_cubridType")]), BinOp::StrictEq, e_str("CLOB"))),
                                    vec![
                                        s_if(
                                            e_var("_refWasStream"),
                                            vec![
                                                s_assign("_typedValue", e_cast(CastType::String, e_var("_value"))),
                                            ],
                                            vec![
                                            (e_call("is_resource", vec![e_var("_value")]), vec![
                                                s_assign("_streamValue", e_call("stream_get_contents", vec![e_var("_value")])),
                                                s_if(
                                                    e_binop(e_var("_streamValue"), BinOp::StrictEq, e_bool(false)),
                                                    vec![
                                                        s_assign("_bindError", e_str("__elephc_pdo_driver_error")),
                                                        s_break(1),
                                                    ],
                                                    vec![],
                                                    None,
                                                ),
                                                s_assign("_typedValue", e_cast(CastType::String, e_var("_streamValue"))),
                                            ]),
                                        ],
                                            Some(vec![
                                            s_assign("_fileValue", e_call("file_get_contents", vec![e_cast(CastType::String, e_var("_value"))])),
                                            s_if(
                                                e_binop(e_var("_fileValue"), BinOp::StrictEq, e_bool(false)),
                                                vec![
                                                    s_assign("_bindError", e_str("__elephc_pdo_driver_error")),
                                                    s_break(1),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                            s_assign("_typedValue", e_cast(CastType::String, e_var("_fileValue"))),
                                        ]),
                                        ),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_assign("_typedValue", e_cast(CastType::String, e_var("_value"))),
                                ]),
                                ),
                                s_assign("_brc", e_call("elephc_pdo_cubrid_bind_typed", vec![e_this_prop("stmt"), e_var("_slot"), e_var("_typedValue"), e_call("strlen", vec![e_var("_typedValue")]), e_var("_cubridType"), e_int(0), e_var("_btype")])),
                            ]),
                            (e_binop(e_binop(e_var("_btype"), BinOp::Eq, e_int(0)), BinOp::Or, e_call("is_null", vec![e_var("_value")])), vec![
                                s_assign("_brc", e_call("elephc_pdo_bind_null", vec![e_this_prop("stmt"), e_var("_slot")])),
                            ]),
                            (e_binop(e_var("_btype"), BinOp::Eq, e_int(1)), vec![
                                s_assign("_brc", e_call("elephc_pdo_bind_int", vec![e_this_prop("stmt"), e_var("_slot"), e_cast(CastType::Int, e_var("_value"))])),
                            ]),
                            (e_binop(e_var("_btype"), BinOp::Eq, e_int(5)), vec![
                                s_assign("_bval", e_ternary(e_cast(CastType::Bool, e_var("_value")), e_int(1), e_int(0))),
                                s_assign("_brc", e_call("elephc_pdo_bind_bool", vec![e_this_prop("stmt"), e_var("_slot"), e_var("_bval")])),
                            ]),
                            (e_binop(e_var("_btype"), BinOp::Eq, e_int(3)), vec![
                                s_if(
                                    e_var("_refWasStream"),
                                    vec![
                                        s_assign("_s", e_cast(CastType::String, e_var("_value"))),
                                    ],
                                    vec![
                                    (e_binop(e_binop(e_var("_driverName"), BinOp::StrictEq, e_str("cubrid")), BinOp::And, e_not(e_call("is_resource", vec![e_var("_value")]))), vec![
                                        s_assign("_lobFileValue", e_call("file_get_contents", vec![e_cast(CastType::String, e_var("_value"))])),
                                        s_if(
                                            e_binop(e_var("_lobFileValue"), BinOp::StrictEq, e_bool(false)),
                                            vec![
                                                s_assign("_bindError", e_str("__elephc_pdo_driver_error")),
                                                s_break(1),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_assign("_s", e_cast(CastType::String, e_var("_lobFileValue"))),
                                    ]),
                                    (e_call("is_resource", vec![e_var("_value")]), vec![
                                        s_assign("_lobStreamValue", e_call("stream_get_contents", vec![e_var("_value")])),
                                        s_if(
                                            e_binop(e_var("_lobStreamValue"), BinOp::StrictEq, e_bool(false)),
                                            vec![
                                                s_assign("_bindError", e_str("__elephc_pdo_driver_error")),
                                                s_break(1),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_assign("_s", e_cast(CastType::String, e_var("_lobStreamValue"))),
                                    ]),
                                ],
                                    Some(vec![
                                    s_assign("_s", e_cast(CastType::String, e_var("_value"))),
                                ]),
                                ),
                                s_assign("_brc", e_call("elephc_pdo_bind_blob", vec![e_this_prop("stmt"), e_var("_slot"), e_var("_s"), e_call("strlen", vec![e_var("_s")])])),
                            ]),
                            (e_binop(e_var("_btype"), BinOp::Eq, e_int(100)), vec![
                                s_assign("_brc", e_call("elephc_pdo_bind_double", vec![e_this_prop("stmt"), e_var("_slot"), e_cast(CastType::Float, e_var("_value"))])),
                            ]),
                        ],
                            Some(vec![
                            s_assign("_s", e_cast(CastType::String, e_var("_value"))),
                            s_assign("_stringFlags", e_binop(e_var("_rawBindType"), BinOp::BitAnd, e_int(1610612736))),
                            s_assign("_national", e_binop(e_binop(e_var("_stringFlags"), BinOp::Eq, e_int(1073741824)), BinOp::Or, e_binop(e_binop(e_var("_stringFlags"), BinOp::Eq, e_int(0)), BinOp::And, e_binop(e_this_prop("defaultStrParam"), BinOp::Eq, e_int(1073741824))))),
                            s_if(
                                e_var("_national"),
                                vec![
                                    s_assign("_brc", e_call("elephc_pdo_bind_text_national", vec![e_this_prop("stmt"), e_var("_slot"), e_var("_s"), e_call("strlen", vec![e_var("_s")])])),
                                ],
                                vec![],
                                Some(vec![
                                s_assign("_brc", e_call("elephc_pdo_bind_text", vec![e_this_prop("stmt"), e_var("_slot"), e_var("_s"), e_call("strlen", vec![e_var("_s")])])),
                            ]),
                            ),
                        ]),
                        ),
                        s_if(
                            e_binop(e_var("_brc"), BinOp::Eq, e_int(0)),
                            vec![
                                s_assign("_bindError", e_str("__elephc_pdo_no_detail")),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_isInputOutput", e_binop(e_binop(e_var("_rawBindType"), BinOp::BitAnd, e_class_const("PDO", "PARAM_INPUT_OUTPUT")), BinOp::NotEq, e_int(0))),
                        s_assign("_driverName", e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")])),
                        s_assign("_isCliOutput", e_binop(e_binop(e_var("_isRefBind"), BinOp::And, e_binop(e_var("_refMaxLength"), BinOp::Gt, e_int(0))), BinOp::And, e_binop(e_binop(e_binop(e_var("_driverName"), BinOp::StrictEq, e_str("odbc")), BinOp::Or, e_binop(e_var("_driverName"), BinOp::StrictEq, e_str("informix"))), BinOp::Or, e_binop(e_var("_driverName"), BinOp::StrictEq, e_str("ibm"))))),
                        s_assign("_isNullLobOutput", e_binop(e_binop(e_binop(e_binop(e_var("_driverName"), BinOp::StrictEq, e_str("oci")), BinOp::And, e_var("_isRefBind")), BinOp::And, e_binop(e_var("_btype"), BinOp::Eq, e_class_const("PDO", "PARAM_LOB"))), BinOp::And, e_call("is_null", vec![e_var("_value")]))),
                        s_if(
                            e_binop(e_binop(e_var("_isInputOutput"), BinOp::Or, e_var("_isCliOutput")), BinOp::Or, e_var("_isNullLobOutput")),
                            vec![
                                s_assign("_brc", e_call("elephc_pdo_bind_output", vec![e_this_prop("stmt"), e_var("_slot"), e_var("_rawBindType"), e_var("_refMaxLength")])),
                                s_if(
                                    e_binop(e_var("_brc"), BinOp::Lt, e_int(0)),
                                    vec![
                                        s_assign("_bindError", e_str("__elephc_pdo_driver_error")),
                                        s_break(1),
                                    ],
                                    vec![
                                    (e_binop(e_var("_brc"), BinOp::Eq, e_int(0)), vec![
                                        s_assign("_bindError", e_str("__elephc_pdo_no_detail")),
                                        s_break(1),
                                    ]),
                                ],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_array_push(e_this(), "boundNormalizedIndexes", e_var("_i")),
                    ]),
                ],
                vec![],
                Some(vec![
                s_prop_assign(e_this(), "boundParams", e_array(vec![])),
                s_prop_assign(e_this(), "boundNames", e_array(vec![])),
                s_prop_assign(e_this(), "boundValues", e_array(vec![])),
                s_prop_assign(e_this(), "boundTypes", e_array(vec![])),
                s_prop_assign(e_this(), "boundDriverOptions", e_array(vec![])),
                s_prop_assign(e_this(), "boundPhpTypes", e_array(vec![])),
                s_prop_assign(e_this(), "boundNormalizedIndexes", e_array(vec![])),
                s_prop_assign(e_this(), "boundParamRefIndexes", e_array(vec![])),
                s_prop_assign(e_this(), "boundParamRefGetters", e_array(vec![])),
                s_prop_assign(e_this(), "boundParamRefStreamReaders", e_array(vec![])),
                s_prop_assign(e_this(), "boundParamRefSetters", e_array(vec![])),
                s_prop_assign(e_this(), "boundParamMaxLengths", e_array(vec![])),
                s_foreach(e_var("params"), Some("_key"), "_pv", vec![
                    s_if(
                        e_call("is_int", vec![e_var("_key")]),
                        vec![
                            s_assign("_idx", e_binop(e_var("_key"), BinOp::Add, e_int(1))),
                            s_assign("_pname", e_str("")),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("_idx", e_call("elephc_pdo_bind_parameter_index", vec![e_this_prop("stmt"), e_cast(CastType::String, e_var("_key"))])),
                        s_assign("_pname", e_cast(CastType::String, e_var("_key"))),
                    ]),
                    ),
                    s_assign("_pslot", e_cast(CastType::Int, e_var("_idx"))),
                    s_if(
                        e_binop(e_var("_pslot"), BinOp::Lt, e_int(1)),
                        vec![
                            s_assign("_bindError", e_str("parameter was not defined")),
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_prc", e_int(0)),
                    s_if(
                        e_call("is_int", vec![e_var("_pv")]),
                        vec![
                            s_assign("_prc", e_call("elephc_pdo_bind_int", vec![e_this_prop("stmt"), e_var("_pslot"), e_cast(CastType::Int, e_var("_pv"))])),
                            s_prop_array_push(e_this(), "boundTypes", e_int(1)),
                        ],
                        vec![
                        (e_call("is_bool", vec![e_var("_pv")]), vec![
                            s_assign("_prc", e_call("elephc_pdo_bind_int", vec![e_this_prop("stmt"), e_var("_pslot"), e_cast(CastType::Int, e_var("_pv"))])),
                            s_prop_array_push(e_this(), "boundTypes", e_int(1)),
                        ]),
                        (e_call("is_float", vec![e_var("_pv")]), vec![
                            s_assign("_prc", e_call("elephc_pdo_bind_double", vec![e_this_prop("stmt"), e_var("_pslot"), e_cast(CastType::Float, e_var("_pv"))])),
                            s_prop_array_push(e_this(), "boundTypes", e_int(100)),
                        ]),
                        (e_call("is_null", vec![e_var("_pv")]), vec![
                            s_assign("_prc", e_call("elephc_pdo_bind_null", vec![e_this_prop("stmt"), e_var("_pslot")])),
                            s_prop_array_push(e_this(), "boundTypes", e_int(0)),
                        ]),
                    ],
                        Some(vec![
                        s_assign("_ps", e_cast(CastType::String, e_var("_pv"))),
                        s_if(
                            e_binop(e_this_prop("defaultStrParam"), BinOp::Eq, e_int(1073741824)),
                            vec![
                                s_assign("_prc", e_call("elephc_pdo_bind_text_national", vec![e_this_prop("stmt"), e_var("_pslot"), e_var("_ps"), e_call("strlen", vec![e_var("_ps")])])),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("_prc", e_call("elephc_pdo_bind_text", vec![e_this_prop("stmt"), e_var("_pslot"), e_var("_ps"), e_call("strlen", vec![e_var("_ps")])])),
                        ]),
                        ),
                        s_prop_array_push(e_this(), "boundTypes", e_int(2)),
                    ]),
                    ),
                    s_prop_array_push(e_this(), "boundDriverOptions", e_null()),
                    s_prop_array_push(e_this(), "boundParams", e_var("_pslot")),
                    s_prop_array_push(e_this(), "boundNames", e_var("_pname")),
                    s_prop_array_push(e_this(), "boundValues", e_var("_pv")),
                    s_prop_array_push(e_this(), "boundPhpTypes", e_int(2)),
                    s_if(
                        e_binop(e_var("_prc"), BinOp::Eq, e_int(0)),
                        vec![
                            s_assign("_bindError", e_str("__elephc_pdo_no_detail")),
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_array_push(e_this(), "boundNormalizedIndexes", e_binop(e_call("count", vec![e_this_prop("boundValues")]), BinOp::Sub, e_int(1))),
                ]),
            ]),
            ),
            s_if(
                e_binop(e_var("_bindError"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_prop_assign(e_this(), "executed", e_bool(false)),
                    s_if(
                        e_binop(e_var("_bindError"), BinOp::StrictEq, e_str("__elephc_pdo_driver_error")),
                        vec![
                            s_expr(e_method_call(e_this(), "failCode", vec![e_call("elephc_pdo_stmt_sqlstate", vec![e_this_prop("stmt")]), e_call("elephc_pdo_stmt_errmsg", vec![e_this_prop("stmt")])])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_bindDetail", e_ternary(e_binop(e_var("_bindError"), BinOp::StrictEq, e_str("__elephc_pdo_no_detail")), e_str(""), e_var("_bindError"))),
                    s_expr(e_method_call(e_this(), "failCode", vec![e_str("HY093"), e_var("_bindDetail")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("dblib")), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("firebird"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("cubrid"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("odbc"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("informix"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("ibm"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("sqlsrv"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("oci"))),
                vec![
                    s_assign("_step", e_call("elephc_pdo_step", vec![e_this_prop("stmt")])),
                    s_if(
                        e_binop(e_var("_step"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                            s_prop_assign(e_this(), "rowCount", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")]), BinOp::Gt, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "pendingStep", e_var("_step")),
                            s_prop_assign(e_this(), "hasPendingStep", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![
                (e_binop(e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")]), BinOp::Eq, e_int(0)), vec![
                    s_assign("_step", e_call("elephc_pdo_step", vec![e_this_prop("stmt")])),
                    s_if(
                        e_binop(e_binop(e_this_prop("owner"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("pgsql"))),
                        vec![
                            s_expr(e_method_call(e_this_prop("owner"), "__elephcDrainPgsqlNotices", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_step"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                            s_prop_assign(e_this(), "rowCount", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ]),
            ],
                Some(vec![
                s_prop_assign(e_this(), "pendingStep", e_call("elephc_pdo_step", vec![e_this_prop("stmt")])),
                s_prop_assign(e_this(), "hasPendingStep", e_bool(true)),
                s_if(
                    e_binop(e_binop(e_this_prop("owner"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("pgsql"))),
                    vec![
                        s_expr(e_method_call(e_this_prop("owner"), "__elephcDrainPgsqlNotices", vec![])),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_this_prop("pendingStep"), BinOp::Lt, e_int(0)),
                    vec![
                        s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                        s_prop_assign(e_this(), "rowCount", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            ),
            s_expr(e_method_call(e_this(), "syncOutputParameters", vec![])),
            s_prop_assign(e_this(), "rowCount", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
            s_if(
                e_binop(e_binop(e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")]), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("sqlite"))),
                vec![
                    s_prop_assign(e_this(), "rowCount", e_int(0)),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
}

/// `columnValue` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_columnvalue() -> MethodBuilder {
    method("columnValue")
        .private()
        .param("index", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_assign("_type", e_call("elephc_pdo_column_type", vec![e_this_prop("stmt"), e_var("index")])),
            s_assign("_stringifyFetches", e_method_call(e_this(), "currentStringifyFetches", vec![])),
            s_assign("_oracleNulls", e_method_call(e_this(), "currentOracleNulls", vec![])),
            s_if(
                e_binop(e_var("_type"), BinOp::Eq, e_int(1)),
                vec![
                    s_assign("_intVal", e_call("elephc_pdo_column_int", vec![e_this_prop("stmt"), e_var("index")])),
                    s_if(
                        e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("pgsql")), BinOp::And, e_binop(e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("index")]), BinOp::StrictEq, e_str("bool"))),
                        vec![
                            s_return(e_binop(e_var("_intVal"), BinOp::NotEq, e_int(0))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_var("_stringifyFetches"),
                        vec![
                            s_return(e_cast(CastType::String, e_var("_intVal"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_intVal")),
                ],
                vec![
                (e_binop(e_var("_type"), BinOp::Eq, e_int(2)), vec![
                    s_assign("_dblVal", e_call("elephc_pdo_column_double", vec![e_this_prop("stmt"), e_var("index")])),
                    s_if(
                        e_var("_stringifyFetches"),
                        vec![
                            s_return(e_cast(CastType::String, e_var("_dblVal"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_dblVal")),
                ]),
                (e_binop(e_var("_type"), BinOp::Eq, e_int(5)), vec![
                    s_if(
                        e_binop(e_var("_oracleNulls"), BinOp::Eq, e_int(2)),
                        vec![
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_null()),
                ]),
            ],
                None,
            ),
            s_assign("_len", e_call("elephc_pdo_column_data_len", vec![e_this_prop("stmt"), e_var("index")])),
            s_assign("_out", e_str("")),
            s_if(
                e_binop(e_var("_len"), BinOp::Gt, e_int(0)),
                vec![
                    s_assign("_out", e_call("__elephc_ptr_read_string", vec![e_call("elephc_pdo_column_data_ptr", vec![e_this_prop("stmt"), e_var("index")]), e_var("_len")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("_oracleNulls"), BinOp::Eq, e_int(1)), BinOp::And, e_binop(e_var("_out"), BinOp::StrictEq, e_str(""))),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("sqlsrv")), BinOp::And, e_binop(e_call("elephc_pdo_sqlsrv_column_is_datetime", vec![e_this_prop("stmt"), e_var("index")]), BinOp::StrictEq, e_int(1))),
                vec![
                    s_return(e_new("DateTime", vec![e_var("_out")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("_type"), BinOp::Eq, e_int(4)), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("pgsql")), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("informix"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("ibm"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("oci"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("cubrid")))),
                vec![
                    s_assign("_stream", e_call("fopen", vec![e_str("php://memory"), e_str("r+")])),
                    s_expr(e_call("fwrite", vec![e_var("_stream"), e_var("_out")])),
                    s_expr(e_call("rewind", vec![e_var("_stream")])),
                    s_return(e_var("_stream")),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_out")),
        ])
}

/// `columnName` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_columnname() -> MethodBuilder {
    method("columnName")
        .private()
        .param("index", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("_raw", e_call("elephc_pdo_column_name", vec![e_this_prop("stmt"), e_var("index")])),
            s_assign("_attrCase", e_method_call(e_this(), "currentAttrCase", vec![])),
            s_if(
                e_binop(e_var("_attrCase"), BinOp::Eq, e_int(1)),
                vec![
                    s_return(e_call("strtoupper", vec![e_var("_raw")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_attrCase"), BinOp::Eq, e_int(2)),
                vec![
                    s_return(e_call("strtolower", vec![e_var("_raw")])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_raw")),
        ])
}

/// `assignColumns` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_assigncolumns() -> MethodBuilder {
    method("assignColumns")
        .private()
        .param("object", t_mixed())
        .param("count", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_return(e_method_call(e_this(), "assignColumnsFrom", vec![e_var("object"), e_int(0), e_var("count")])),
        ])
}

/// `assignColumnsFrom` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_assigncolumnsfrom() -> MethodBuilder {
    method("assignColumnsFrom")
        .private()
        .param("object", t_mixed())
        .param("start", TypeExpr::Int)
        .param("count", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_for(Some(s_assign("_i", e_var("start"))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("_i"))), vec![
                s_assign("_value", e_method_call(e_this(), "columnValue", vec![e_var("_i")])),
                s_assign("_name", e_method_call(e_this(), "columnName", vec![e_var("_i")])),
                s_expr(e_assign(e_dyn_prop(e_var("object"), e_var("_name")), e_var("_value"))),
            ]),
            s_return(e_var("object")),
        ])
}

/// `hydrateClass` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_hydrateclass() -> MethodBuilder {
    method("hydrateClass")
        .private()
        .param("class", TypeExpr::Str)
        .param("start", TypeExpr::Int)
        .param("count", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_this_prop("fetchPropsLate"),
                vec![
                    s_return(e_method_call(e_this(), "assignColumnsFrom", vec![e_new_dynamic(e_var("class"), vec![e_spread(e_this_prop("fetchCtorArgs"))]), e_var("start"), e_var("count")])),
                ],
                vec![],
                None,
            ),
            s_assign("_object", e_method_call(e_this(), "assignColumnsFrom", vec![e_call("__elephc_new_without_constructor", vec![e_var("class")]), e_var("start"), e_var("count")])),
            s_if(
                e_call("__elephc_class_has_constructor", vec![e_var("class")]),
                vec![
                    s_expr(e_call("call_user_func_array", vec![e_array(vec![e_var("_object"), e_str("__construct")]), e_this_prop("fetchCtorArgs")])),
                ],
                vec![
                (e_binop(e_call("count", vec![e_this_prop("fetchCtorArgs")]), BinOp::NotEq, e_int(0)), vec![
                    s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Class "), BinOp::Concat, e_var("class")), BinOp::Concat, e_str(" does not have a constructor, so you cannot pass any constructor arguments"))])),
                ]),
            ],
                None,
            ),
            s_return(e_var("_object")),
        ])
}

/// `updateBoundColumns` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_updateboundcolumns() -> MethodBuilder {
    method("updateBoundColumns")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("_columnCount", e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")])),
            s_assign("_bindingCount", e_call("count", vec![e_this_prop("boundColumnSetters")])),
            s_for(Some(s_assign("_bi", e_int(0))), Some(e_binop(e_var("_bi"), BinOp::Lt, e_var("_bindingCount"))), Some(s_expr(e_post_inc("_bi"))), vec![
                s_assign("_shadowed", e_bool(false)),
                s_for(Some(s_assign("_bj", e_binop(e_var("_bi"), BinOp::Add, e_int(1)))), Some(e_binop(e_var("_bj"), BinOp::Lt, e_var("_bindingCount"))), Some(s_expr(e_post_inc("_bj"))), vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_index(e_this_prop("boundColumnKinds"), e_var("_bj")), BinOp::Eq, e_index(e_this_prop("boundColumnKinds"), e_var("_bi"))), BinOp::And, e_binop(e_index(e_this_prop("boundColumnIndexes"), e_var("_bj")), BinOp::Eq, e_index(e_this_prop("boundColumnIndexes"), e_var("_bi")))), BinOp::And, e_binop(e_index(e_this_prop("boundColumnNames"), e_var("_bj")), BinOp::StrictEq, e_index(e_this_prop("boundColumnNames"), e_var("_bi")))),
                        vec![
                            s_assign("_shadowed", e_bool(true)),
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                s_if(
                    e_var("_shadowed"),
                    vec![
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_columnIndex", e_neg(e_int(1))),
                s_if(
                    e_binop(e_index(e_this_prop("boundColumnKinds"), e_var("_bi")), BinOp::Eq, e_int(0)),
                    vec![
                        s_assign("_columnIndex", e_binop(e_cast(CastType::Int, e_index(e_this_prop("boundColumnIndexes"), e_var("_bi"))), BinOp::Sub, e_int(1))),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("_key", e_index(e_this_prop("boundColumnNames"), e_var("_bi"))),
                    s_for(Some(s_assign("_ci", e_int(0))), Some(e_binop(e_var("_ci"), BinOp::Lt, e_var("_columnCount"))), Some(s_expr(e_post_inc("_ci"))), vec![
                        s_if(
                            e_binop(e_method_call(e_this(), "columnName", vec![e_var("_ci")]), BinOp::StrictEq, e_var("_key")),
                            vec![
                                s_assign("_columnIndex", e_var("_ci")),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                ]),
                ),
                s_if(
                    e_binop(e_binop(e_var("_columnIndex"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("_columnIndex"), BinOp::GtEq, e_var("_columnCount"))),
                    vec![
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_value", e_method_call(e_this(), "columnValue", vec![e_var("_columnIndex")])),
                s_assign("_type", e_binop(e_cast(CastType::Int, e_index(e_this_prop("boundColumnTypes"), e_var("_bi"))), BinOp::BitAnd, e_int(65535))),
                s_if(
                    e_binop(e_var("_value"), BinOp::StrictNotEq, e_null()),
                    vec![
                        s_if(
                            e_binop(e_var("_type"), BinOp::Eq, e_int(0)),
                            vec![
                                s_assign("_value", e_null()),
                            ],
                            vec![
                            (e_binop(e_var("_type"), BinOp::Eq, e_int(1)), vec![
                                s_assign("_value", e_cast(CastType::Int, e_var("_value"))),
                            ]),
                            (e_binop(e_var("_type"), BinOp::Eq, e_int(2)), vec![
                                s_assign("_value", e_cast(CastType::String, e_var("_value"))),
                            ]),
                            (e_binop(e_var("_type"), BinOp::Eq, e_int(5)), vec![
                                s_assign("_value", e_cast(CastType::Bool, e_var("_value"))),
                            ]),
                        ],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_setter", e_index(e_this_prop("boundColumnSetters"), e_var("_bi"))),
                s_if(
                    e_call("is_callable", vec![e_var("_setter")]),
                    vec![
                        s_typed_assign(t_class("callable"), "_typedSetter", e_var("_setter")),
                        s_expr(e_call("call_user_func_array", vec![e_var("_typedSetter"), e_array(vec![e_var("_value")])])),
                    ],
                    vec![],
                    None,
                ),
            ]),
        ])
}

/// `stepCursor` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_stepcursor() -> MethodBuilder {
    method("stepCursor")
        .private()
        .param_default("orientation", TypeExpr::Int, e_int(0))
        .param_default("offset", TypeExpr::Int, e_int(0))
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_this_prop("scrollable"),
                vec![
                    s_if(
                        e_this_prop("hasPendingStep"),
                        vec![
                            s_prop_assign(e_this(), "hasPendingStep", e_bool(false)),
                            s_if(
                                e_binop(e_var("orientation"), BinOp::Eq, e_int(0)),
                                vec![
                                    s_assign("_rc", e_this_prop("pendingStep")),
                                    s_if(
                                        e_binop(e_var("_rc"), BinOp::Gt, e_int(0)),
                                        vec![
                                            s_expr(e_method_call(e_this(), "updateBoundColumns", vec![])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return(e_var("_rc")),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_rc", e_call("elephc_pdo_step_oriented", vec![e_this_prop("stmt"), e_var("orientation"), e_var("offset")])),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Gt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "updateBoundColumns", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_rc")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_this_prop("hasPendingStep"),
                vec![
                    s_prop_assign(e_this(), "hasPendingStep", e_bool(false)),
                    s_assign("_rc", e_this_prop("pendingStep")),
                ],
                vec![],
                Some(vec![
                s_assign("_rc", e_call("elephc_pdo_step", vec![e_this_prop("stmt")])),
            ]),
            ),
            s_if(
                e_binop(e_var("_rc"), BinOp::Gt, e_int(0)),
                vec![
                    s_expr(e_method_call(e_this(), "updateBoundColumns", vec![])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_rc")),
        ])
}

/// `assignColumnsFromOrStd` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_assigncolumnsfromorstd() -> MethodBuilder {
    method("assignColumnsFromOrStd")
        .private()
        .param("object", t_mixed())
        .param("start", TypeExpr::Int)
        .param("count", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_var("object"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_method_call(e_this(), "assignColumnsFrom", vec![e_new("stdClass", vec![]), e_var("start"), e_var("count")])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_this(), "assignColumnsFrom", vec![e_var("object"), e_var("start"), e_var("count")])),
        ])
}

/// `hydrateClassOrStdWithoutConstructor` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_hydrateclassorstdwithoutconstructor() -> MethodBuilder {
    method("hydrateClassOrStdWithoutConstructor")
        .private()
        .param("object", t_mixed())
        .param("class", TypeExpr::Str)
        .param("start", TypeExpr::Int)
        .param("count", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_var("object"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_method_call(e_this(), "assignColumnsFrom", vec![e_new("stdClass", vec![]), e_var("start"), e_var("count")])),
                ],
                vec![],
                None,
            ),
            s_assign("_object", e_method_call(e_this(), "assignColumnsFrom", vec![e_var("object"), e_var("start"), e_var("count")])),
            s_if(
                e_call("__elephc_class_has_constructor", vec![e_var("class")]),
                vec![
                    s_expr(e_call("call_user_func_array", vec![e_array(vec![e_var("_object"), e_str("__construct")]), e_this_prop("fetchCtorArgs")])),
                ],
                vec![
                (e_binop(e_call("count", vec![e_this_prop("fetchCtorArgs")]), BinOp::NotEq, e_int(0)), vec![
                    s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Class "), BinOp::Concat, e_var("class")), BinOp::Concat, e_str(" does not have a constructor, so you cannot pass any constructor arguments"))])),
                ]),
            ],
                None,
            ),
            s_return(e_var("_object")),
        ])
}

/// `hydrateClassOrStd` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_hydrateclassorstd() -> MethodBuilder {
    method("hydrateClassOrStd")
        .private()
        .param("class", TypeExpr::Str)
        .param("start", TypeExpr::Int)
        .param("count", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_call("__elephc_pdo_statement_class_status", vec![e_var("class")]), BinOp::Eq, e_int(0)),
                vec![
                    s_return(e_method_call(e_this(), "assignColumnsFrom", vec![e_new("stdClass", vec![]), e_var("start"), e_var("count")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_this_prop("fetchPropsLate"),
                vec![
                    s_return(e_method_call(e_this(), "assignColumnsFromOrStd", vec![e_new_dynamic(e_var("class"), vec![e_spread(e_this_prop("fetchCtorArgs"))]), e_var("start"), e_var("count")])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_this(), "hydrateClassOrStdWithoutConstructor", vec![e_call("__elephc_new_without_constructor", vec![e_var("class")]), e_var("class"), e_var("start"), e_var("count")])),
        ])
}

/// `groupKey` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_groupkey() -> MethodBuilder {
    method("groupKey")
        .private()
        .param("index", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_assign("_key", e_cast(CastType::String, e_method_call(e_this(), "columnValue", vec![e_var("index")]))),
            s_assign("_integerKey", e_cast(CastType::Int, e_var("_key"))),
            s_if(
                e_binop(e_cast(CastType::String, e_var("_integerKey")), BinOp::StrictEq, e_var("_key")),
                vec![
                    s_return(e_var("_integerKey")),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_key")),
        ])
}

/// `groupRow` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_grouprow() -> MethodBuilder {
    method("groupRow")
        .private()
        .param("base", TypeExpr::Int)
        .param("count", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_var("base"), BinOp::Eq, e_int(7)),
                vec![
                    s_return(e_method_call(e_this(), "columnValue", vec![e_this_prop("fetchColumn")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("base"), BinOp::Eq, e_int(5)),
                vec![
                    s_return(e_method_call(e_this(), "assignColumnsFrom", vec![e_new("stdClass", vec![]), e_int(1), e_var("count")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("base"), BinOp::Eq, e_int(8)),
                vec![
                    s_if(
                        e_binop(e_this_prop("fetchTarget"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("_gClass", e_this_prop("fetchTarget")),
                            s_return(e_method_call(e_this(), "hydrateClass", vec![e_var("_gClass"), e_int(1), e_var("count")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "assignColumnsFrom", vec![e_new("stdClass", vec![]), e_int(1), e_var("count")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("base"), BinOp::Eq, e_int(3)),
                vec![
                    s_assign("_gNum", e_array(vec![])),
                    s_assign("_gIdx", e_int(0)),
                    s_for(Some(s_assign("_i", e_int(1))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_assign("_gNum", e_var("_gIdx"), e_method_call(e_this(), "columnValue", vec![e_var("_i")])),
                        s_assign("_gIdx", e_binop(e_var("_gIdx"), BinOp::Add, e_int(1))),
                    ]),
                    s_return(e_var("_gNum")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("base"), BinOp::Eq, e_int(2)),
                vec![
                    s_assign("_gAssoc", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(1))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_gName", e_method_call(e_this(), "columnName", vec![e_var("_i")])),
                        s_array_assign("_gAssoc", e_var("_gName"), e_method_call(e_this(), "columnValue", vec![e_var("_i")])),
                    ]),
                    s_return(e_var("_gAssoc")),
                ],
                vec![],
                None,
            ),
            s_assign("_gBoth", e_array(vec![])),
            s_assign("_gPos", e_int(0)),
            s_for(Some(s_assign("_i", e_int(1))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("_i"))), vec![
                s_assign("_gBothName", e_method_call(e_this(), "columnName", vec![e_var("_i")])),
                s_assign("_gBothVal", e_method_call(e_this(), "columnValue", vec![e_var("_i")])),
                s_array_assign("_gBoth", e_var("_gBothName"), e_var("_gBothVal")),
                s_array_assign("_gBoth", e_var("_gPos"), e_var("_gBothVal")),
                s_assign("_gPos", e_binop(e_var("_gPos"), BinOp::Add, e_int(1))),
            ]),
            s_return(e_var("_gBoth")),
        ])
}

/// `fetchColumn` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_fetchcolumn() -> MethodBuilder {
    method("fetchColumn")
        .param_default("column", TypeExpr::Int, e_int(0))
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_not(e_this_prop("executed")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_rc", e_method_call(e_this(), "stepCursor", vec![])),
            s_if(
                e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                vec![
                    s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_rc"), BinOp::Eq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("column"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("Column index must be greater than or equal to 0")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("column"), BinOp::GtEq, e_method_call(e_this(), "columnCount", vec![])),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("Invalid column index")])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_this(), "columnValue", vec![e_var("column")])),
        ])
}

/// `closeCursor` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_closecursor() -> MethodBuilder {
    method("closeCursor")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
            s_prop_assign(e_this(), "executed", e_bool(false)),
            s_prop_assign(e_this(), "hasPendingStep", e_bool(false)),
            s_return(e_bool(true)),
        ])
}

/// `fetchObject` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_fetchobject() -> MethodBuilder {
    method("fetchObject")
        .param_default("class", t_nullable(TypeExpr::Str), e_str("stdClass"))
        .param_default("constructorArgs", t_array(), e_array(vec![]))
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_not(e_this_prop("executed")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_rc", e_method_call(e_this(), "stepCursor", vec![])),
            s_if(
                e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                vec![
                    s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_rc"), BinOp::Eq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_count", e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")])),
            s_if(
                e_binop(e_binop(e_var("class"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("class"), BinOp::StrictEq, e_str("stdClass"))),
                vec![
                    s_return(e_method_call(e_this(), "assignColumns", vec![e_new("stdClass", vec![]), e_var("_count")])),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "fetchCtorArgs", e_method_call(e_this(), "copyConstructorArgs", vec![e_var("constructorArgs")])),
            s_prop_assign(e_this(), "fetchPropsLate", e_bool(false)),
            s_return(e_method_call(e_this(), "hydrateClass", vec![e_cast(CastType::String, e_var("class")), e_int(0), e_var("_count")])),
        ])
}

/// `rowCount` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_rowcount() -> MethodBuilder {
    method("rowCount")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_this_prop("rowCount")),
        ])
}

/// `columnCount` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_columncount() -> MethodBuilder {
    method("columnCount")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")])),
        ])
}

/// `getAttribute` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_getattribute() -> MethodBuilder {
    method("getAttribute")
        .param("name", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("sqlsrv")), BinOp::And, e_binop(e_binop(e_var("name"), BinOp::Eq, e_int(10)), BinOp::Or, e_binop(e_binop(e_var("name"), BinOp::GtEq, e_int(1000)), BinOp::And, e_binop(e_var("name"), BinOp::LtEq, e_int(1009))))),
                vec![
                    s_assign("_sqlsrvValue", e_call("elephc_pdo_sqlsrv_stmt_attribute", vec![e_this_prop("stmt"), e_var("name")])),
                    s_if(
                        e_binop(e_var("_sqlsrvValue"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_return(e_ternary(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("name"), BinOp::Eq, e_int(1002)), BinOp::Or, e_binop(e_var("name"), BinOp::Eq, e_int(1005))), BinOp::Or, e_binop(e_var("name"), BinOp::Eq, e_int(1006))), BinOp::Or, e_binop(e_var("name"), BinOp::Eq, e_int(1007))), BinOp::Or, e_binop(e_var("name"), BinOp::Eq, e_int(1009))), e_binop(e_var("_sqlsrvValue"), BinOp::StrictEq, e_int(1)), e_var("_sqlsrvValue"))),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("name"), BinOp::Eq, e_int(9)), BinOp::And, e_binop(e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("odbc")), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("informix"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("ibm")))),
                vec![
                    s_assign("_cursorName", e_call("elephc_pdo_odbc_stmt_cursor_name", vec![e_this_prop("stmt")])),
                    s_return(e_ternary(e_binop(e_var("_cursorName"), BinOp::StrictEq, e_str("")), e_null(), e_var("_cursorName"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("name"), BinOp::Eq, e_int(1001)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("odbc"))),
                vec![
                    s_return(e_binop(e_call("elephc_pdo_odbc_stmt_assume_utf8", vec![e_this_prop("stmt")]), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("name"), BinOp::Eq, e_int(9)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("firebird"))),
                vec![
                    s_assign("_cursorName", e_call("elephc_pdo_firebird_stmt_cursor_name", vec![e_this_prop("stmt")])),
                    s_return(e_ternary(e_binop(e_var("_cursorName"), BinOp::StrictEq, e_str("")), e_null(), e_var("_cursorName"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("name"), BinOp::Eq, e_int(1001)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("pgsql"))),
                vec![
                    s_prop_assign(e_this(), "hasOperation", e_bool(true)),
                    s_assign("_memory", e_call("elephc_pdo_result_memory_size", vec![e_this_prop("stmt")])),
                    s_return(e_ternary(e_binop(e_var("_memory"), BinOp::Lt, e_int(0)), e_null(), e_var("_memory"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::Eq, e_int(1001)),
                vec![
                    s_return(e_binop(e_call("elephc_pdo_stmt_readonly", vec![e_this_prop("stmt")]), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("name"), BinOp::Eq, e_int(1003)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("sqlite"))),
                vec![
                    s_return(e_binop(e_call("elephc_pdo_stmt_busy", vec![e_this_prop("stmt")]), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("name"), BinOp::Eq, e_int(1004)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("sqlite"))),
                vec![
                    s_return(e_call("elephc_pdo_stmt_explain_mode", vec![e_this_prop("stmt")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::Eq, e_int(20)),
                vec![
                    s_return(e_this_prop("emulatePrepares")),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "failCode", vec![e_str("IM001"), e_str("This driver doesn't support getting attributes")])),
            s_return(e_bool(false)),
        ])
}

/// `setAttribute` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_setattribute() -> MethodBuilder {
    method("setAttribute")
        .param("attribute", TypeExpr::Int)
        .param("value", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("sqlsrv")),
                vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1002)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(10))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1003))),
                        vec![
                            s_expr(e_method_call(e_this(), "failCode", vec![e_str("IMSSP"), e_str("The attribute may only be set when preparing a statement.")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1000)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1001))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1004))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1005))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1006))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1007))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1008))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1009))),
                        vec![
                            s_assign("_sqlsrvValue", e_ternary(e_binop(e_binop(e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1005)), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1006))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1007))), BinOp::Or, e_binop(e_var("attribute"), BinOp::Eq, e_int(1009))), e_ternary(e_var("value"), e_int(1), e_int(0)), e_cast(CastType::Int, e_var("value")))),
                            s_if(
                                e_binop(e_call("elephc_pdo_sqlsrv_stmt_set_attribute", vec![e_this_prop("stmt"), e_var("attribute"), e_var("_sqlsrvValue")]), BinOp::StrictNotEq, e_int(1)),
                                vec![
                                    s_expr(e_method_call(e_this(), "failCode", vec![e_str("IMSSP"), e_str("An invalid statement attribute was designated.")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(9)), BinOp::And, e_binop(e_binop(e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("odbc")), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("informix"))), BinOp::Or, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("ibm")))),
                vec![
                    s_return(e_binop(e_call("elephc_pdo_odbc_stmt_set_cursor_name", vec![e_this_prop("stmt"), e_cast(CastType::String, e_var("value"))]), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1001)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("odbc"))),
                vec![
                    s_return(e_binop(e_call("elephc_pdo_odbc_stmt_set_assume_utf8", vec![e_this_prop("stmt"), e_ternary(e_var("value"), e_int(1), e_int(0))]), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(9)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("firebird"))),
                vec![
                    s_assign("_cursorName", e_cast(CastType::String, e_var("value"))),
                    s_if(
                        e_binop(e_call("strlen", vec![e_var("_cursorName")]), BinOp::Gt, e_int(31)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("Cursor name must not be longer than 31 bytes")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_call("elephc_pdo_firebird_stmt_set_cursor_name", vec![e_this_prop("stmt"), e_var("_cursorName")]), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(1004)), BinOp::And, e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictEq, e_str("sqlite"))),
                vec![
                    s_if(
                        e_not(e_call("is_int", vec![e_var("value")])),
                        vec![
                            s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("explain mode must be of type int, "), BinOp::Concat, e_method_call(e_this(), "argValueTypeName", vec![e_var("value")])), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_explainMode", e_cast(CastType::Int, e_var("value"))),
                    s_if(
                        e_binop(e_binop(e_var("_explainMode"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("_explainMode"), BinOp::Gt, e_int(2))),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("explain mode must be one of the Pdo\\Sqlite::EXPLAIN_MODE_* constants")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_call("elephc_pdo_stmt_set_explain_mode", vec![e_this_prop("stmt"), e_var("_explainMode")]), BinOp::StrictEq, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_assign("_unusedAttribute", e_var("attribute")),
            s_assign("_unusedValue", e_var("value")),
            s_expr(e_method_call(e_this(), "failCode", vec![e_str("IM001"), e_str("This driver doesn't support setting attributes")])),
            s_return(e_bool(false)),
        ])
}

/// `nextRowset` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_nextrowset() -> MethodBuilder {
    method("nextRowset")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_rowsetDriver", e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")])),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_rowsetDriver"), BinOp::StrictEq, e_str("mysql")), BinOp::Or, e_binop(e_var("_rowsetDriver"), BinOp::StrictEq, e_str("dblib"))), BinOp::Or, e_binop(e_var("_rowsetDriver"), BinOp::StrictEq, e_str("odbc"))), BinOp::Or, e_binop(e_var("_rowsetDriver"), BinOp::StrictEq, e_str("informix"))), BinOp::Or, e_binop(e_var("_rowsetDriver"), BinOp::StrictEq, e_str("ibm"))), BinOp::Or, e_binop(e_var("_rowsetDriver"), BinOp::StrictEq, e_str("sqlsrv"))), BinOp::Or, e_binop(e_var("_rowsetDriver"), BinOp::StrictEq, e_str("cubrid"))),
                vec![
                    s_if(
                        e_binop(e_call("elephc_pdo_next_rowset", vec![e_this_prop("stmt")]), BinOp::StrictNotEq, e_int(1)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "hasPendingStep", e_bool(false)),
                    s_prop_assign(e_this(), "pendingStep", e_int(0)),
                    s_prop_assign(e_this(), "executed", e_bool(true)),
                    s_prop_assign(e_this(), "hasOperation", e_bool(true)),
                    s_prop_assign(e_this(), "rowCount", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "failCode", vec![e_str("IM001"), e_str("driver does not support multiple rowsets")])),
            s_return(e_bool(false)),
        ])
}

/// `getColumnMeta` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_getcolumnmeta() -> MethodBuilder {
    method("getColumnMeta")
        .param("column", TypeExpr::Int)
        .returns(t_union(vec![t_array(), TypeExpr::Bool]))
        .body(vec![
            s_if(
                e_binop(e_var("column"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("PDOStatement::getColumnMeta(): Argument #1 ($column) must be greater than or equal to 0")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_this_prop("executed")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("column"), BinOp::GtEq, e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_oid", e_call("elephc_pdo_column_type_oid", vec![e_this_prop("stmt"), e_var("column")])),
            s_if(
                e_binop(e_var("_oid"), BinOp::Gt, e_int(0)),
                vec![
                    s_assign("_pgType", e_int(2)),
                    s_if(
                        e_binop(e_var("_oid"), BinOp::Eq, e_int(16)),
                        vec![
                            s_assign("_pgType", e_int(5)),
                        ],
                        vec![
                        (e_binop(e_binop(e_var("_oid"), BinOp::Eq, e_int(17)), BinOp::Or, e_binop(e_var("_oid"), BinOp::Eq, e_int(26))), vec![
                            s_assign("_pgType", e_int(3)),
                        ]),
                        (e_binop(e_binop(e_binop(e_var("_oid"), BinOp::Eq, e_int(20)), BinOp::Or, e_binop(e_var("_oid"), BinOp::Eq, e_int(21))), BinOp::Or, e_binop(e_var("_oid"), BinOp::Eq, e_int(23))), vec![
                            s_assign("_pgType", e_int(1)),
                        ]),
                    ],
                        None,
                    ),
                    s_assign("_pgMeta", e_array_assoc(vec![(e_str("name"), e_method_call(e_this(), "columnName", vec![e_var("column")])), (e_str("native_type"), e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("pdo_type"), e_var("_pgType")), (e_str("len"), e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("column")])), (e_str("precision"), e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("column")])), (e_str("flags"), e_array(vec![])), (e_str("pgsql:oid"), e_var("_oid")), (e_str("pgsql:table_oid"), e_call("elephc_pdo_column_table_oid", vec![e_this_prop("stmt"), e_var("column")]))])),
                    s_assign("_pgTable", e_call("elephc_pdo_column_table_name", vec![e_this_prop("stmt"), e_var("column")])),
                    s_if(
                        e_binop(e_var("_pgTable"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_array_assign("_pgMeta", e_str("table"), e_var("_pgTable")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_pgMeta")),
                ],
                vec![],
                None,
            ),
            s_assign("_driver", e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")])),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("cubrid")),
                vec![
                    s_assign("_cubridFlags", e_call("elephc_pdo_column_flags", vec![e_this_prop("stmt"), e_var("column")])),
                    s_assign("_cubridUnique", e_binop(e_binop(e_var("_cubridFlags"), BinOp::BitAnd, e_int(4)), BinOp::StrictNotEq, e_int(0))),
                    s_return(e_array_assoc(vec![(e_str("type"), e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("name"), e_method_call(e_this(), "columnName", vec![e_var("column")])), (e_str("table"), e_call("elephc_pdo_column_table_name", vec![e_this_prop("stmt"), e_var("column")])), (e_str("def"), e_call("elephc_pdo_cubrid_column_default", vec![e_this_prop("stmt"), e_var("column")])), (e_str("precision"), e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("column")])), (e_str("scale"), e_call("elephc_pdo_cubrid_column_scale", vec![e_this_prop("stmt"), e_var("column")])), (e_str("not_null"), e_ternary(e_binop(e_binop(e_var("_cubridFlags"), BinOp::BitAnd, e_int(1)), BinOp::StrictNotEq, e_int(0)), e_int(1), e_int(0))), (e_str("auto_increment"), e_ternary(e_binop(e_binop(e_var("_cubridFlags"), BinOp::BitAnd, e_int(2)), BinOp::StrictNotEq, e_int(0)), e_int(1), e_int(0))), (e_str("unique_key"), e_ternary(e_var("_cubridUnique"), e_int(1), e_int(0))), (e_str("multiple_key"), e_ternary(e_var("_cubridUnique"), e_int(0), e_int(1))), (e_str("primary_key"), e_ternary(e_binop(e_binop(e_var("_cubridFlags"), BinOp::BitAnd, e_int(8)), BinOp::StrictNotEq, e_int(0)), e_int(1), e_int(0))), (e_str("foreign_key"), e_ternary(e_binop(e_binop(e_var("_cubridFlags"), BinOp::BitAnd, e_int(16)), BinOp::StrictNotEq, e_int(0)), e_int(1), e_int(0))), (e_str("reverse_index"), e_ternary(e_binop(e_binop(e_var("_cubridFlags"), BinOp::BitAnd, e_int(32)), BinOp::StrictNotEq, e_int(0)), e_int(1), e_int(0))), (e_str("reverse_unique"), e_ternary(e_binop(e_binop(e_var("_cubridFlags"), BinOp::BitAnd, e_int(64)), BinOp::StrictNotEq, e_int(0)), e_int(1), e_int(0)))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("firebird")),
                vec![
                    s_return(e_array_assoc(vec![(e_str("pdo_type"), e_call("elephc_pdo_firebird_column_pdo_type", vec![e_this_prop("stmt"), e_var("column")]))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("odbc")),
                vec![
                    s_return(e_array_assoc(vec![(e_str("pdo_type"), e_int(2))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")),
                vec![
                    s_if(
                        e_binop(e_call("elephc_pdo_sqlsrv_stmt_attribute", vec![e_this_prop("stmt"), e_int(1009)]), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_assign("_sqlsrvPairCount", e_call("elephc_pdo_sqlsrv_classification_pair_count", vec![e_this_prop("stmt"), e_var("column")])),
                            s_if(
                                e_binop(e_var("_sqlsrvPairCount"), BinOp::Lt, e_int(0)),
                                vec![
                                    s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_stmt_errmsg", vec![e_this_prop("stmt")])])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_sqlsrvDataClassification", e_array(vec![])),
                            s_assign("_sqlsrvPairIndex", e_int(0)),
                            s_while(e_binop(e_var("_sqlsrvPairIndex"), BinOp::Lt, e_var("_sqlsrvPairCount")), vec![
                                s_assign("_sqlsrvProperty", e_array_assoc(vec![(e_str("Label"), e_array_assoc(vec![(e_str("name"), e_call("elephc_pdo_sqlsrv_classification_text", vec![e_this_prop("stmt"), e_var("column"), e_var("_sqlsrvPairIndex"), e_int(0)])), (e_str("id"), e_call("elephc_pdo_sqlsrv_classification_text", vec![e_this_prop("stmt"), e_var("column"), e_var("_sqlsrvPairIndex"), e_int(1)]))])), (e_str("Information Type"), e_array_assoc(vec![(e_str("name"), e_call("elephc_pdo_sqlsrv_classification_text", vec![e_this_prop("stmt"), e_var("column"), e_var("_sqlsrvPairIndex"), e_int(2)])), (e_str("id"), e_call("elephc_pdo_sqlsrv_classification_text", vec![e_this_prop("stmt"), e_var("column"), e_var("_sqlsrvPairIndex"), e_int(3)]))]))])),
                                s_assign("_sqlsrvPairRank", e_call("elephc_pdo_sqlsrv_classification_pair_rank", vec![e_this_prop("stmt"), e_var("column"), e_var("_sqlsrvPairIndex")])),
                                s_if(
                                    e_binop(e_var("_sqlsrvPairRank"), BinOp::GtEq, e_int(0)),
                                    vec![
                                        s_array_assign("_sqlsrvProperty", e_str("rank"), e_var("_sqlsrvPairRank")),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_array_push("_sqlsrvDataClassification", e_var("_sqlsrvProperty")),
                                s_assign("_sqlsrvPairIndex", e_binop(e_var("_sqlsrvPairIndex"), BinOp::Add, e_int(1))),
                            ]),
                            s_assign("_sqlsrvQueryRank", e_call("elephc_pdo_sqlsrv_classification_query_rank", vec![e_this_prop("stmt")])),
                            s_if(
                                e_binop(e_var("_sqlsrvQueryRank"), BinOp::GtEq, e_int(0)),
                                vec![
                                    s_array_assign("_sqlsrvDataClassification", e_str("rank"), e_var("_sqlsrvQueryRank")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_array_assoc(vec![(e_str("flags"), e_array_assoc(vec![(e_str("Data Classification"), e_var("_sqlsrvDataClassification"))])), (e_str("sqlsrv:decl_type"), e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("native_type"), e_str("string")), (e_str("table"), e_call("elephc_pdo_column_table_name", vec![e_this_prop("stmt"), e_var("column")])), (e_str("pdo_type"), e_int(2)), (e_str("name"), e_method_call(e_this(), "columnName", vec![e_var("column")])), (e_str("len"), e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("column")])), (e_str("precision"), e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("column")]))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array_assoc(vec![(e_str("flags"), e_int(0)), (e_str("sqlsrv:decl_type"), e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("native_type"), e_str("string")), (e_str("table"), e_call("elephc_pdo_column_table_name", vec![e_this_prop("stmt"), e_var("column")])), (e_str("pdo_type"), e_int(2)), (e_str("name"), e_method_call(e_this(), "columnName", vec![e_var("column")])), (e_str("len"), e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("column")])), (e_str("precision"), e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("column")]))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("informix")),
                vec![
                    s_assign("_informixFlags", e_array(vec![])),
                    s_assign("_informixFlagBits", e_call("elephc_pdo_column_flags", vec![e_this_prop("stmt"), e_var("column")])),
                    s_array_assign("_informixFlags", e_str("not_null"), e_binop(e_binop(e_var("_informixFlagBits"), BinOp::BitAnd, e_int(1)), BinOp::StrictNotEq, e_int(0))),
                    s_array_assign("_informixFlags", e_str("unsigned"), e_binop(e_binop(e_var("_informixFlagBits"), BinOp::BitAnd, e_int(2)), BinOp::StrictNotEq, e_int(0))),
                    s_array_assign("_informixFlags", e_str("auto_increment"), e_binop(e_binop(e_var("_informixFlagBits"), BinOp::BitAnd, e_int(4)), BinOp::StrictNotEq, e_int(0))),
                    s_assign("_informixTable", e_call("elephc_pdo_column_table_name", vec![e_this_prop("stmt"), e_var("column")])),
                    s_if(
                        e_binop(e_var("_informixTable"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("scale"), e_call("elephc_pdo_informix_column_scale", vec![e_this_prop("stmt"), e_var("column")])), (e_str("table"), e_var("_informixTable")), (e_str("native_type"), e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("flags"), e_var("_informixFlags")), (e_str("pdo_type"), e_call("elephc_pdo_informix_column_pdo_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("name"), e_method_call(e_this(), "columnName", vec![e_var("column")])), (e_str("len"), e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("column")])), (e_str("precision"), e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("column")]))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array_assoc(vec![(e_str("scale"), e_call("elephc_pdo_informix_column_scale", vec![e_this_prop("stmt"), e_var("column")])), (e_str("native_type"), e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("flags"), e_var("_informixFlags")), (e_str("pdo_type"), e_call("elephc_pdo_informix_column_pdo_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("name"), e_method_call(e_this(), "columnName", vec![e_var("column")])), (e_str("len"), e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("column")])), (e_str("precision"), e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("column")]))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("ibm")),
                vec![
                    s_assign("_ibmFlags", e_array(vec![])),
                    s_assign("_ibmFlagBits", e_call("elephc_pdo_column_flags", vec![e_this_prop("stmt"), e_var("column")])),
                    s_array_assign("_ibmFlags", e_str("not_null"), e_binop(e_binop(e_var("_ibmFlagBits"), BinOp::BitAnd, e_int(1)), BinOp::StrictNotEq, e_int(0))),
                    s_array_assign("_ibmFlags", e_str("unsigned"), e_binop(e_binop(e_var("_ibmFlagBits"), BinOp::BitAnd, e_int(2)), BinOp::StrictNotEq, e_int(0))),
                    s_array_assign("_ibmFlags", e_str("auto_increment"), e_binop(e_binop(e_var("_ibmFlagBits"), BinOp::BitAnd, e_int(4)), BinOp::StrictNotEq, e_int(0))),
                    s_assign("_ibmTable", e_call("elephc_pdo_column_table_name", vec![e_this_prop("stmt"), e_var("column")])),
                    s_if(
                        e_binop(e_var("_ibmTable"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("scale"), e_call("elephc_pdo_ibm_column_scale", vec![e_this_prop("stmt"), e_var("column")])), (e_str("table"), e_var("_ibmTable")), (e_str("native_type"), e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("flags"), e_var("_ibmFlags")), (e_str("pdo_type"), e_call("elephc_pdo_ibm_column_pdo_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("name"), e_method_call(e_this(), "columnName", vec![e_var("column")])), (e_str("len"), e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("column")])), (e_str("precision"), e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("column")]))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array_assoc(vec![(e_str("scale"), e_call("elephc_pdo_ibm_column_scale", vec![e_this_prop("stmt"), e_var("column")])), (e_str("native_type"), e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("flags"), e_var("_ibmFlags")), (e_str("pdo_type"), e_call("elephc_pdo_ibm_column_pdo_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("name"), e_method_call(e_this(), "columnName", vec![e_var("column")])), (e_str("len"), e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("column")])), (e_str("precision"), e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("column")]))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("oci")),
                vec![
                    s_assign("_ociFlags", e_array(vec![])),
                    s_assign("_ociFlagBits", e_call("elephc_pdo_oci_column_flags", vec![e_this_prop("stmt"), e_var("column")])),
                    s_if(
                        e_binop(e_binop(e_var("_ociFlagBits"), BinOp::BitAnd, e_int(1)), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_array_push("_ociFlags", e_str("nullable")),
                        ],
                        vec![],
                        Some(vec![
                        s_array_push("_ociFlags", e_str("not_null")),
                    ]),
                    ),
                    s_if(
                        e_binop(e_binop(e_var("_ociFlagBits"), BinOp::BitAnd, e_int(4)), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_array_push("_ociFlags", e_str("blob")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_ociType", e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("column")])),
                    s_return(e_array_assoc(vec![(e_str("oci:decl_type"), e_var("_ociType")), (e_str("native_type"), e_var("_ociType")), (e_str("pdo_type"), e_call("elephc_pdo_oci_column_pdo_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("scale"), e_call("elephc_pdo_oci_column_scale", vec![e_this_prop("stmt"), e_var("column")])), (e_str("flags"), e_var("_ociFlags"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("dblib")),
                vec![
                    s_assign("_dblibNativeId", e_call("elephc_pdo_dblib_column_native_type_id", vec![e_this_prop("stmt"), e_var("column")])),
                    s_assign("_dblibPdoType", e_int(2)),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_var("_dblibNativeId"), BinOp::Eq, e_int(48)), BinOp::Or, e_binop(e_var("_dblibNativeId"), BinOp::Eq, e_int(50))), BinOp::Or, e_binop(e_var("_dblibNativeId"), BinOp::Eq, e_int(52))), BinOp::Or, e_binop(e_var("_dblibNativeId"), BinOp::Eq, e_int(56))),
                        vec![
                            s_assign("_dblibPdoType", e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array_assoc(vec![(e_str("max_length"), e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("column")])), (e_str("precision"), e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("column")])), (e_str("scale"), e_call("elephc_pdo_dblib_column_scale", vec![e_this_prop("stmt"), e_var("column")])), (e_str("column_source"), e_call("elephc_pdo_dblib_column_source", vec![e_this_prop("stmt"), e_var("column")])), (e_str("native_type"), e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("column")])), (e_str("native_type_id"), e_var("_dblibNativeId")), (e_str("native_usertype_id"), e_call("elephc_pdo_dblib_column_user_type_id", vec![e_this_prop("stmt"), e_var("column")])), (e_str("pdo_type"), e_var("_dblibPdoType"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("mysql")),
                vec![
                    s_assign("_myNative", e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("column")])),
                    s_assign("_myType", e_int(2)),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_myNative"), BinOp::StrictEq, e_str("BIT")), BinOp::Or, e_binop(e_var("_myNative"), BinOp::StrictEq, e_str("YEAR"))), BinOp::Or, e_binop(e_var("_myNative"), BinOp::StrictEq, e_str("TINY"))), BinOp::Or, e_binop(e_var("_myNative"), BinOp::StrictEq, e_str("SHORT"))), BinOp::Or, e_binop(e_var("_myNative"), BinOp::StrictEq, e_str("INT24"))), BinOp::Or, e_binop(e_var("_myNative"), BinOp::StrictEq, e_str("LONG"))), BinOp::Or, e_binop(e_var("_myNative"), BinOp::StrictEq, e_str("LONGLONG"))),
                        vec![
                            s_assign("_myType", e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_myFlags", e_array(vec![])),
                    s_assign("_myFlagBits", e_call("elephc_pdo_column_flags", vec![e_this_prop("stmt"), e_var("column")])),
                    s_if(
                        e_binop(e_binop(e_var("_myFlagBits"), BinOp::BitAnd, e_int(1)), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_array_push("_myFlags", e_str("not_null")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("_myFlagBits"), BinOp::BitAnd, e_int(2)), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_array_push("_myFlags", e_str("primary_key")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("_myFlagBits"), BinOp::BitAnd, e_int(8)), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_array_push("_myFlags", e_str("multiple_key")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("_myFlagBits"), BinOp::BitAnd, e_int(4)), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_array_push("_myFlags", e_str("unique_key")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("_myFlagBits"), BinOp::BitAnd, e_int(16)), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_array_push("_myFlags", e_str("blob")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_myNative"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("pdo_type"), e_var("_myType")), (e_str("flags"), e_var("_myFlags")), (e_str("table"), e_call("elephc_pdo_column_table_name", vec![e_this_prop("stmt"), e_var("column")])), (e_str("name"), e_method_call(e_this(), "columnName", vec![e_var("column")])), (e_str("len"), e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("column")])), (e_str("precision"), e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("column")]))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array_assoc(vec![(e_str("native_type"), e_var("_myNative")), (e_str("pdo_type"), e_var("_myType")), (e_str("flags"), e_var("_myFlags")), (e_str("table"), e_call("elephc_pdo_column_table_name", vec![e_this_prop("stmt"), e_var("column")])), (e_str("name"), e_method_call(e_this(), "columnName", vec![e_var("column")])), (e_str("len"), e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("column")])), (e_str("precision"), e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("column")]))])),
                ],
                vec![],
                None,
            ),
            s_assign("_type", e_call("elephc_pdo_column_type", vec![e_this_prop("stmt"), e_var("column")])),
            s_assign("_native", e_str("null")),
            s_assign("_pdoType", e_int(0)),
            s_assign("_flags", e_array(vec![])),
            s_if(
                e_binop(e_var("_type"), BinOp::Eq, e_int(1)),
                vec![
                    s_assign("_native", e_str("integer")),
                    s_assign("_pdoType", e_int(1)),
                ],
                vec![
                (e_binop(e_var("_type"), BinOp::Eq, e_int(2)), vec![
                    s_assign("_native", e_str("double")),
                    s_assign("_pdoType", e_int(2)),
                ]),
                (e_binop(e_var("_type"), BinOp::Eq, e_int(3)), vec![
                    s_assign("_native", e_str("string")),
                    s_assign("_pdoType", e_int(2)),
                ]),
                (e_binop(e_var("_type"), BinOp::Eq, e_int(4)), vec![
                    s_assign("_native", e_str("string")),
                    s_assign("_pdoType", e_int(2)),
                    s_array_push("_flags", e_str("blob")),
                ]),
            ],
                None,
            ),
            s_assign("_meta", e_array_assoc(vec![(e_str("name"), e_method_call(e_this(), "columnName", vec![e_var("column")])), (e_str("native_type"), e_var("_native")), (e_str("pdo_type"), e_var("_pdoType")), (e_str("len"), e_neg(e_int(1))), (e_str("precision"), e_int(0)), (e_str("flags"), e_var("_flags"))])),
            s_assign("_decltype", e_call("elephc_pdo_column_decltype", vec![e_this_prop("stmt"), e_var("column")])),
            s_if(
                e_binop(e_var("_decltype"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_array_assign("_meta", e_str("sqlite:decl_type"), e_var("_decltype")),
                ],
                vec![],
                None,
            ),
            s_assign("_table", e_call("elephc_pdo_column_table_name", vec![e_this_prop("stmt"), e_var("column")])),
            s_if(
                e_binop(e_var("_table"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_array_assign("_meta", e_str("table"), e_var("_table")),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_meta")),
        ])
}

/// `debugDumpParams` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_debugdumpparams() -> MethodBuilder {
    method("debugDumpParams")
        .returns(t_nullable(TypeExpr::Bool))
        .body(vec![
            s_echo(e_binop(e_binop(e_binop(e_binop(e_str("SQL: ["), BinOp::Concat, e_call("strlen", vec![e_this_prop("queryString")])), BinOp::Concat, e_str("] ")), BinOp::Concat, e_this_prop("queryString")), BinOp::Concat, e_str("\n"))),
            s_assign("_sentSql", e_call("elephc_pdo_stmt_sent_sql", vec![e_this_prop("stmt")])),
            s_if(
                e_binop(e_var("_sentSql"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_echo(e_binop(e_binop(e_binop(e_binop(e_str("Sent SQL: ["), BinOp::Concat, e_call("strlen", vec![e_var("_sentSql")])), BinOp::Concat, e_str("] ")), BinOp::Concat, e_var("_sentSql")), BinOp::Concat, e_str("\n"))),
                ],
                vec![],
                None,
            ),
            s_assign("_recordCount", e_call("count", vec![e_this_prop("boundValues")])),
            s_assign("_pcount", e_int(0)),
            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_recordCount"))), Some(s_expr(e_post_inc("_i"))), vec![
                s_assign("_shadowed", e_bool(false)),
                s_for(Some(s_assign("_j", e_binop(e_var("_i"), BinOp::Add, e_int(1)))), Some(e_binop(e_var("_j"), BinOp::Lt, e_var("_recordCount"))), Some(s_expr(e_post_inc("_j"))), vec![
                    s_assign("_bothPositional", e_binop(e_binop(e_index(e_this_prop("boundNames"), e_var("_i")), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_index(e_this_prop("boundNames"), e_var("_j")), BinOp::StrictEq, e_str("")))),
                    s_assign("_bothNamed", e_binop(e_binop(e_index(e_this_prop("boundNames"), e_var("_i")), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_index(e_this_prop("boundNames"), e_var("_j")), BinOp::StrictNotEq, e_str("")))),
                    s_if(
                        e_binop(e_binop(e_var("_bothPositional"), BinOp::Or, e_var("_bothNamed")), BinOp::And, e_binop(e_index(e_this_prop("boundParams"), e_var("_i")), BinOp::Eq, e_index(e_this_prop("boundParams"), e_var("_j")))),
                        vec![
                            s_assign("_shadowed", e_bool(true)),
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                s_if(
                    e_not(e_var("_shadowed")),
                    vec![
                        s_assign("_pcount", e_binop(e_var("_pcount"), BinOp::Add, e_int(1))),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_echo(e_binop(e_binop(e_str("Params:  "), BinOp::Concat, e_var("_pcount")), BinOp::Concat, e_str("\n"))),
            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_recordCount"))), Some(s_expr(e_post_inc("_i"))), vec![
                s_assign("_shadowed", e_bool(false)),
                s_for(Some(s_assign("_j", e_binop(e_var("_i"), BinOp::Add, e_int(1)))), Some(e_binop(e_var("_j"), BinOp::Lt, e_var("_recordCount"))), Some(s_expr(e_post_inc("_j"))), vec![
                    s_assign("_bothPositional", e_binop(e_binop(e_index(e_this_prop("boundNames"), e_var("_i")), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_index(e_this_prop("boundNames"), e_var("_j")), BinOp::StrictEq, e_str("")))),
                    s_assign("_bothNamed", e_binop(e_binop(e_index(e_this_prop("boundNames"), e_var("_i")), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_index(e_this_prop("boundNames"), e_var("_j")), BinOp::StrictNotEq, e_str("")))),
                    s_if(
                        e_binop(e_binop(e_var("_bothPositional"), BinOp::Or, e_var("_bothNamed")), BinOp::And, e_binop(e_index(e_this_prop("boundParams"), e_var("_i")), BinOp::Eq, e_index(e_this_prop("boundParams"), e_var("_j")))),
                        vec![
                            s_assign("_shadowed", e_bool(true)),
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                s_if(
                    e_var("_shadowed"),
                    vec![
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_dname", e_cast(CastType::String, e_index(e_this_prop("boundNames"), e_var("_i")))),
                s_assign("_dno", e_binop(e_cast(CastType::Int, e_index(e_this_prop("boundParams"), e_var("_i"))), BinOp::Sub, e_int(1))),
                s_if(
                    e_binop(e_var("_dname"), BinOp::StrictNotEq, e_str("")),
                    vec![
                        s_assign("_normalized", e_bool(false)),
                        s_foreach(e_this_prop("boundNormalizedIndexes"), None, "_normalizedIndex", vec![
                            s_if(
                                e_binop(e_var("_normalizedIndex"), BinOp::Eq, e_var("_i")),
                                vec![
                                    s_assign("_normalized", e_bool(true)),
                                    s_break(1),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                        s_if(
                            e_not(e_var("_normalized")),
                            vec![
                                s_assign("_dno", e_neg(e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_dtype", e_cast(CastType::Int, e_index(e_this_prop("boundPhpTypes"), e_var("_i")))),
                s_assign("_dlen", e_call("strlen", vec![e_var("_dname")])),
                s_if(
                    e_binop(e_var("_dname"), BinOp::StrictEq, e_str("")),
                    vec![
                        s_echo(e_binop(e_binop(e_str("Key: Position #"), BinOp::Concat, e_var("_dno")), BinOp::Concat, e_str(":\n"))),
                    ],
                    vec![],
                    Some(vec![
                    s_echo(e_binop(e_binop(e_binop(e_binop(e_str("Key: Name: ["), BinOp::Concat, e_var("_dlen")), BinOp::Concat, e_str("] ")), BinOp::Concat, e_var("_dname")), BinOp::Concat, e_str("\n"))),
                ]),
                ),
                s_echo(e_binop(e_binop(e_str("paramno="), BinOp::Concat, e_var("_dno")), BinOp::Concat, e_str("\n"))),
                s_echo(e_binop(e_binop(e_binop(e_binop(e_str("name=["), BinOp::Concat, e_var("_dlen")), BinOp::Concat, e_str("] \"")), BinOp::Concat, e_var("_dname")), BinOp::Concat, e_str("\"\n"))),
                s_echo(e_str("is_param=1\n")),
                s_echo(e_binop(e_binop(e_str("param_type="), BinOp::Concat, e_var("_dtype")), BinOp::Concat, e_str("\n"))),
            ]),
            s_return(e_null()),
        ])
}

/// `getIterator` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_getiterator() -> MethodBuilder {
    method("getIterator")
        .returns(t_class("\\Iterator"))
        .body(vec![
            s_return(e_new("__ElephcPDOStatementIterator", vec![e_this()])),
        ])
}

/// `__destruct` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_destruct() -> MethodBuilder {
    method("__destruct")
        .body(vec![
            s_expr(e_call("elephc_pdo_finalize", vec![e_this_prop("stmt")])),
            s_prop_assign(e_this(), "owner", e_null()),
        ])
}

/// `__clone` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_clone() -> MethodBuilder {
    method("__clone")
        .returns(TypeExpr::Void)
        .body(vec![
            s_throw(e_new("Error", vec![e_binop(e_str("Trying to clone an uncloneable object of class "), BinOp::Concat, e_call("get_class", vec![e_this()]))])),
        ])
}

/// `__serialize` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_serialize() -> MethodBuilder {
    method("__serialize")
        .returns(t_array())
        .body(vec![
            s_throw(e_new("Exception", vec![e_binop(e_binop(e_str("Serialization of '"), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str("' is not allowed"))])),
        ])
}

/// `__sleep` — lifted out of `decl_class_pdostatement` so it builds in its own stack frame.
fn pdostatement_sleep() -> MethodBuilder {
    method("__sleep")
        .returns(t_array())
        .body(vec![
            s_throw(e_new("Exception", vec![e_binop(e_binop(e_str("Serialization of '"), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str("' is not allowed"))])),
        ])
}

/// `__construct` — lifted out of `decl_class_elephcpdostatementiterator` so it builds in its own stack frame.
fn elephcpdostatementiterator_construct() -> MethodBuilder {
    method("__construct")
        .param("statement", t_class("PDOStatement"))
        .body(vec![
            s_prop_assign(e_this(), "statement", e_var("statement")),
            s_prop_assign(e_this(), "row", e_null()),
            s_prop_assign(e_this(), "position", e_int(0)),
        ])
}

/// `rewind` — lifted out of `decl_class_elephcpdostatementiterator` so it builds in its own stack frame.
fn elephcpdostatementiterator_rewind() -> MethodBuilder {
    method("rewind")
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "row", e_method_call(e_this_prop("statement"), "fetch", vec![])),
            s_prop_assign(e_this(), "position", e_int(0)),
        ])
}

/// `valid` — lifted out of `decl_class_elephcpdostatementiterator` so it builds in its own stack frame.
fn elephcpdostatementiterator_valid() -> MethodBuilder {
    method("valid")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_this_prop("row"), BinOp::StrictNotEq, e_bool(false))),
        ])
}

/// `current` — lifted out of `decl_class_elephcpdostatementiterator` so it builds in its own stack frame.
fn elephcpdostatementiterator_current() -> MethodBuilder {
    method("current")
        .returns(t_mixed())
        .body(vec![
            s_return(e_this_prop("row")),
        ])
}

/// `key` — lifted out of `decl_class_elephcpdostatementiterator` so it builds in its own stack frame.
fn elephcpdostatementiterator_key() -> MethodBuilder {
    method("key")
        .returns(t_mixed())
        .body(vec![
            s_return(e_this_prop("position")),
        ])
}

/// `next` — lifted out of `decl_class_elephcpdostatementiterator` so it builds in its own stack frame.
fn elephcpdostatementiterator_next() -> MethodBuilder {
    method("next")
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "row", e_method_call(e_this_prop("statement"), "fetch", vec![])),
            s_prop_assign(e_this(), "position", e_binop(e_this_prop("position"), BinOp::Add, e_int(1))),
        ])
}

/// `__construct` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_construct_4() -> MethodBuilder {
    method("__construct")
        .param("dsn", TypeExpr::Str)
        .param_default("username", t_nullable(TypeExpr::Str), e_null())
        .param_default("password", t_nullable(TypeExpr::Str), e_null())
        .param_attr("\\SensitiveParameter")
        .param_default("options", t_nullable(t_array()), e_null())
        .body(vec![
            s_assign("_operation", e_binop(e_call("get_class", vec![e_this()]), BinOp::Concat, e_str("::__construct"))),
            s_assign("_dblibDsn", e_self_call("resolveDsnAlias", vec![e_var("dsn"), e_var("_operation")])),
            s_assign("_dblibDsn", e_self_call("resolveDsnUri", vec![e_var("_dblibDsn"), e_var("_operation")])),
            s_expr(e_method_call(e_this(), "checkDriverSubclassDsn", vec![e_var("_dblibDsn"), e_str("Pdo\\Dblib"), e_str("dblib")])),
            s_expr(e_parent_call("__construct", vec![e_var("_dblibDsn"), e_var("username"), e_var("password"), e_var("options")])),
        ])
}

/// `__construct` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_construct_3() -> MethodBuilder {
    method("__construct")
        .param("dsn", TypeExpr::Str)
        .param_default("username", t_nullable(TypeExpr::Str), e_null())
        .param_default("password", t_nullable(TypeExpr::Str), e_null())
        .param_attr("\\SensitiveParameter")
        .param_default("options", t_nullable(t_array()), e_null())
        .body(vec![
            s_assign("_operation", e_binop(e_call("get_class", vec![e_this()]), BinOp::Concat, e_str("::__construct"))),
            s_assign("_firebirdDsn", e_self_call("resolveDsnAlias", vec![e_var("dsn"), e_var("_operation")])),
            s_assign("_firebirdDsn", e_self_call("resolveDsnUri", vec![e_var("_firebirdDsn"), e_var("_operation")])),
            s_expr(e_method_call(e_this(), "checkDriverSubclassDsn", vec![e_var("_firebirdDsn"), e_str("Pdo\\Firebird"), e_str("firebird")])),
            s_expr(e_parent_call("__construct", vec![e_var("_firebirdDsn"), e_var("username"), e_var("password"), e_var("options")])),
        ])
}

/// `getApiVersion` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_getapiversion() -> MethodBuilder {
    method("getApiVersion")
        .static_()
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_int(40)),
        ])
}

/// `__construct` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_construct_2() -> MethodBuilder {
    method("__construct")
        .param("dsn", TypeExpr::Str)
        .param_default("username", t_nullable(TypeExpr::Str), e_null())
        .param_default("password", t_nullable(TypeExpr::Str), e_null())
        .param_attr("\\SensitiveParameter")
        .param_default("options", t_nullable(t_array()), e_null())
        .body(vec![
            s_assign("_operation", e_binop(e_call("get_class", vec![e_this()]), BinOp::Concat, e_str("::__construct"))),
            s_assign("_odbcDsn", e_self_call("resolveDsnAlias", vec![e_var("dsn"), e_var("_operation")])),
            s_assign("_odbcDsn", e_self_call("resolveDsnUri", vec![e_var("_odbcDsn"), e_var("_operation")])),
            s_expr(e_method_call(e_this(), "checkDriverSubclassDsn", vec![e_var("_odbcDsn"), e_str("Pdo\\Odbc"), e_str("odbc")])),
            s_expr(e_parent_call("__construct", vec![e_var("_odbcDsn"), e_var("username"), e_var("password"), e_var("options")])),
        ])
}

/// `__construct` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_construct() -> MethodBuilder {
    method("__construct")
        .param("dsn", TypeExpr::Str)
        .param_default("username", t_nullable(TypeExpr::Str), e_null())
        .param_default("password", t_nullable(TypeExpr::Str), e_null())
        .param_attr("\\SensitiveParameter")
        .param_default("options", t_nullable(t_array()), e_null())
        .body(vec![
            s_assign("_operation", e_binop(e_call("get_class", vec![e_this()]), BinOp::Concat, e_str("::__construct"))),
            s_assign("_ibmDsn", e_self_call("resolveDsnAlias", vec![e_var("dsn"), e_var("_operation")])),
            s_assign("_ibmDsn", e_self_call("resolveDsnUri", vec![e_var("_ibmDsn"), e_var("_operation")])),
            s_expr(e_method_call(e_this(), "checkDriverSubclassDsn", vec![e_var("_ibmDsn"), e_str("Pdo\\Ibm"), e_str("ibm")])),
            s_expr(e_parent_call("__construct", vec![e_var("_ibmDsn"), e_var("username"), e_var("password"), e_var("options")])),
        ])
}

/// `loadExtension` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_loadextension() -> MethodBuilder {
    method("loadExtension")
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("")),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("Pdo\\Sqlite::loadExtension(): Argument #1 ($name) must not be empty")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("\\elephc_pdo_load_extension", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("name")]), BinOp::StrictNotEq, e_int(1)),
                vec![
                    s_throw(e_new_fq("PDOException", vec![e_binop(e_str("Failed to load SQLite extension: "), BinOp::Concat, e_var("name"))])),
                ],
                vec![],
                None,
            ),
        ])
}

/// `openBlob` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_openblob() -> MethodBuilder {
    method("openBlob")
        .param("table", TypeExpr::Str)
        .param("column", TypeExpr::Str)
        .param("rowid", TypeExpr::Int)
        .param_default("dbname", t_nullable(TypeExpr::Str), e_str("main"))
        .param_default("flags", TypeExpr::Int, e_int(1))
        .returns(t_mixed())
        .body(vec![
            s_assign("_db", e_ternary(e_binop(e_var("dbname"), BinOp::StrictEq, e_null()), e_str("main"), e_var("dbname"))),
            s_return(e_static_call("\\__ElephcPDOSqliteBlobStream", "create", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("table"), e_var("column"), e_var("rowid"), e_var("_db"), e_var("flags")])),
        ])
}

/// `createCollation` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_createcollation() -> MethodBuilder {
    method("createCollation")
        .param("name", TypeExpr::Str)
        .param("callback", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_call("\\is_callable", vec![e_var("callback")])),
                vec![
                    s_throw(e_new_fq("TypeError", vec![e_str("Pdo\\Sqlite::createCollation(): Argument #2 ($callback) must be a valid callback")])),
                ],
                vec![],
                None,
            ),
            s_assign("_normalized", e_call("\\__elephc_normalize_callable", vec![e_var("callback")])),
            s_assign("_descriptor", e_call("\\__elephc_callable_ptr", vec![e_var("_normalized")])),
            s_assign("_adapter", e_call("\\__elephc_pdo_adapter_addr", vec![e_int(0)])),
            s_if(
                e_binop(e_call("\\elephc_pdo_create_collation", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("name"), e_var("_descriptor"), e_var("_adapter")]), BinOp::StrictNotEq, e_int(1)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_prop_array_assign(e_this(), "pdoUdfCallbacks", e_binop(e_str("collation:"), BinOp::Concat, e_call("\\strtolower", vec![e_var("name")])), e_var("_normalized")),
            s_return(e_bool(true)),
        ])
}

/// `createFunction` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_createfunction() -> MethodBuilder {
    method("createFunction")
        .param("function_name", TypeExpr::Str)
        .param("callback", t_mixed())
        .param_default("num_args", TypeExpr::Int, e_neg(e_int(1)))
        .param_default("flags", TypeExpr::Int, e_int(0))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_call("\\is_callable", vec![e_var("callback")])),
                vec![
                    s_throw(e_new_fq("TypeError", vec![e_str("Pdo\\Sqlite::createFunction(): Argument #2 ($callback) must be a valid callback")])),
                ],
                vec![],
                None,
            ),
            s_assign("_normalized", e_call("\\__elephc_normalize_callable", vec![e_var("callback")])),
            s_assign("_descriptor", e_call("\\__elephc_callable_ptr", vec![e_var("_normalized")])),
            s_assign("_adapter", e_call("\\__elephc_pdo_adapter_addr", vec![e_int(1)])),
            s_if(
                e_binop(e_call("\\elephc_pdo_create_function", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("function_name"), e_var("num_args"), e_var("flags"), e_var("_descriptor"), e_var("_adapter")]), BinOp::StrictNotEq, e_int(1)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_prop_array_assign(e_this(), "pdoUdfCallbacks", e_binop(e_binop(e_binop(e_binop(e_str("function:"), BinOp::Concat, e_call("\\strtolower", vec![e_var("function_name")])), BinOp::Concat, e_str(":")), BinOp::Concat, e_var("num_args")), BinOp::Concat, e_str(":scalar")), e_var("_normalized")),
            s_return(e_bool(true)),
        ])
}

/// `createAggregate` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_createaggregate() -> MethodBuilder {
    method("createAggregate")
        .param("name", TypeExpr::Str)
        .param("step", t_mixed())
        .param("finalize", t_mixed())
        .param_default("numArgs", TypeExpr::Int, e_neg(e_int(1)))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_not(e_call("\\is_callable", vec![e_var("step")])), BinOp::Or, e_not(e_call("\\is_callable", vec![e_var("finalize")]))),
                vec![
                    s_throw(e_new_fq("TypeError", vec![e_str("Pdo\\Sqlite::createAggregate(): step and finalize must be valid callbacks")])),
                ],
                vec![],
                None,
            ),
            s_assign("_normalizedStep", e_call("\\__elephc_normalize_callable", vec![e_var("step")])),
            s_assign("_normalizedFinal", e_call("\\__elephc_normalize_callable", vec![e_var("finalize")])),
            s_assign("_stepDesc", e_call("\\__elephc_callable_ptr", vec![e_var("_normalizedStep")])),
            s_assign("_stepAdapter", e_call("\\__elephc_pdo_adapter_addr", vec![e_int(2)])),
            s_assign("_finalDesc", e_call("\\__elephc_callable_ptr", vec![e_var("_normalizedFinal")])),
            s_assign("_finalAdapter", e_call("\\__elephc_pdo_adapter_addr", vec![e_int(3)])),
            s_if(
                e_binop(e_call("\\elephc_pdo_create_aggregate", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("name"), e_var("numArgs"), e_var("_stepDesc"), e_var("_stepAdapter"), e_var("_finalDesc"), e_var("_finalAdapter")]), BinOp::StrictNotEq, e_int(1)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_rootKey", e_binop(e_binop(e_binop(e_str("function:"), BinOp::Concat, e_call("\\strtolower", vec![e_var("name")])), BinOp::Concat, e_str(":")), BinOp::Concat, e_var("numArgs"))),
            s_prop_array_assign(e_this(), "pdoUdfCallbacks", e_binop(e_var("_rootKey"), BinOp::Concat, e_str(":step")), e_var("_normalizedStep")),
            s_prop_array_assign(e_this(), "pdoUdfCallbacks", e_binop(e_var("_rootKey"), BinOp::Concat, e_str(":final")), e_var("_normalizedFinal")),
            s_return(e_bool(true)),
        ])
}

/// `getWarningCount` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_getwarningcount() -> MethodBuilder {
    method("getWarningCount")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("\\elephc_pdo_warning_count", vec![e_method_call(e_this(), "connectionId", vec![])])),
        ])
}

/// `setNoticeCallback` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_setnoticecallback() -> MethodBuilder {
    method("setNoticeCallback")
        .param("callback", t_nullable(t_class("callable")))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("callback"), BinOp::StrictEq, e_null()),
                vec![
                    s_prop_assign(e_this(), "noticeCallback", closure()
                        .param_untyped("_message")
                        .body(vec![])
                        .build()),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_typed_assign(t_class("callable"), "_typedNoticeCallback", e_var("callback")),
            s_prop_assign(e_this(), "noticeCallback", e_var("_typedNoticeCallback")),
        ])
}

/// `__elephcDrainPgsqlNotices` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_elephcdrainpgsqlnotices() -> MethodBuilder {
    method("__elephcDrainPgsqlNotices")
        .protected()
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("_cb", e_this_prop("noticeCallback")),
            s_while(e_bool(true), vec![
                s_assign("_msg", e_call("\\elephc_pdo_get_notice", vec![e_method_call(e_this(), "connectionId", vec![])])),
                s_if(
                    e_binop(e_var("_msg"), BinOp::StrictEq, e_str("")),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_expr(e_closure_call("_cb", vec![e_var("_msg")])),
            ]),
        ])
}

/// `exec` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_exec() -> MethodBuilder {
    method("exec")
        .param("statement", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::Bool]))
        .body(vec![
            s_assign("_result", e_parent_call("exec", vec![e_var("statement")])),
            s_expr(e_method_call(e_this(), "__elephcDrainPgsqlNotices", vec![])),
            s_return(e_var("_result")),
        ])
}

/// `query` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_query() -> MethodBuilder {
    method("query")
        .param("query", TypeExpr::Str)
        .param_default("fetchMode", t_nullable(TypeExpr::Int), e_null())
        .variadic("fetchModeArgs", Some(t_mixed()))
        .returns(t_union(vec![t_class("\\PDOStatement"), TypeExpr::Bool]))
        .body(vec![
            s_assign("_result", e_parent_call("query", vec![e_var("query"), e_var("fetchMode"), e_spread(e_var("fetchModeArgs"))])),
            s_expr(e_method_call(e_this(), "__elephcDrainPgsqlNotices", vec![])),
            s_return(e_var("_result")),
        ])
}

/// `escapeIdentifier` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_escapeidentifier() -> MethodBuilder {
    method("escapeIdentifier")
        .param("input", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("_doubled", e_call("\\str_replace", vec![e_str("\""), e_str("\"\""), e_var("input")])),
            s_return(e_binop(e_binop(e_str("\""), BinOp::Concat, e_var("_doubled")), BinOp::Concat, e_str("\""))),
        ])
}

/// `getPid` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_getpid() -> MethodBuilder {
    method("getPid")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("\\elephc_pdo_backend_pid", vec![e_method_call(e_this(), "connectionId", vec![])])),
        ])
}

/// `lobCreate` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_lobcreate() -> MethodBuilder {
    method("lobCreate")
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::Bool]))
        .body(vec![
            s_if(
                e_not(e_method_call(e_this(), "inTransaction", vec![])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_oid", e_call("\\elephc_pdo_lob_create", vec![e_method_call(e_this(), "connectionId", vec![])])),
            s_return(e_ternary(e_binop(e_var("_oid"), BinOp::StrictEq, e_str("")), e_bool(false), e_var("_oid"))),
        ])
}

/// `lobUnlink` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_lobunlink() -> MethodBuilder {
    method("lobUnlink")
        .param("oid", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_method_call(e_this(), "inTransaction", vec![])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_call("\\elephc_pdo_lob_unlink", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("oid")]), BinOp::StrictEq, e_int(1))),
        ])
}

/// `lobOpen` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_lobopen() -> MethodBuilder {
    method("lobOpen")
        .param("oid", TypeExpr::Str)
        .param_default("mode", TypeExpr::Str, e_str("rb"))
        .returns(t_mixed())
        .body(vec![
            s_return(e_static_call("\\__ElephcPDOPgsqlLobStream", "create", vec![e_this(), e_method_call(e_this(), "connectionId", vec![]), e_var("oid"), e_var("mode")])),
        ])
}

/// `copyOptions` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_copyoptions() -> MethodBuilder {
    method("copyOptions")
        .private()
        .param("separator", TypeExpr::Str)
        .param("nullAs", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("_sep", e_ternary(e_binop(e_var("separator"), BinOp::StrictEq, e_str("")), e_str("\t"), e_call("\\substr", vec![e_var("separator"), e_int(0), e_int(1)]))),
            s_if(
                e_binop(e_binop(e_var("_sep"), BinOp::StrictEq, e_str("\t")), BinOp::And, e_binop(e_var("nullAs"), BinOp::StrictEq, e_str("\\N"))),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_assign("_delim", e_ternary(e_binop(e_var("_sep"), BinOp::StrictEq, e_str("\t")), e_str("E'\\t'"), e_binop(e_binop(e_str("'"), BinOp::Concat, e_var("_sep")), BinOp::Concat, e_str("'")))),
            s_assign("_null", e_binop(e_binop(e_str("'"), BinOp::Concat, e_call("\\str_replace", vec![e_str("'"), e_str("''"), e_var("nullAs")])), BinOp::Concat, e_str("'"))),
            s_return(e_binop(e_binop(e_binop(e_binop(e_str(" WITH (DELIMITER "), BinOp::Concat, e_var("_delim")), BinOp::Concat, e_str(", NULL ")), BinOp::Concat, e_var("_null")), BinOp::Concat, e_str(")"))),
        ])
}

/// `copyTarget` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_copytarget() -> MethodBuilder {
    method("copyTarget")
        .private()
        .param("tableName", TypeExpr::Str)
        .param("fields", t_nullable(TypeExpr::Str))
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("fields"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_return(e_binop(e_binop(e_binop(e_var("tableName"), BinOp::Concat, e_str(" (")), BinOp::Concat, e_var("fields")), BinOp::Concat, e_str(")"))),
                ],
                vec![],
                None,
            ),
            s_return(e_var("tableName")),
        ])
}

/// `copyFromArray` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_copyfromarray() -> MethodBuilder {
    method("copyFromArray")
        .param("tableName", TypeExpr::Str)
        .param("rows", t_array())
        .param_default("separator", TypeExpr::Str, e_str("\t"))
        .param_default("nullAs", TypeExpr::Str, e_str("\\N"))
        .param_default("fields", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_data", e_binop(e_call("\\implode", vec![e_str("\n"), e_var("rows")]), BinOp::Concat, e_str("\n"))),
            s_assign("_sql", e_binop(e_binop(e_binop(e_str("COPY "), BinOp::Concat, e_method_call(e_this(), "copyTarget", vec![e_var("tableName"), e_var("fields")])), BinOp::Concat, e_str(" FROM STDIN")), BinOp::Concat, e_method_call(e_this(), "copyOptions", vec![e_var("separator"), e_var("nullAs")]))),
            s_return(e_binop(e_call("\\elephc_pdo_copy_in", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("_sql"), e_var("_data")]), BinOp::GtEq, e_int(0))),
        ])
}

/// `copyFromFile` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_copyfromfile() -> MethodBuilder {
    method("copyFromFile")
        .param("tableName", TypeExpr::Str)
        .param("filename", TypeExpr::Str)
        .param_default("separator", TypeExpr::Str, e_str("\t"))
        .param_default("nullAs", TypeExpr::Str, e_str("\\N"))
        .param_default("fields", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_data", e_call("\\file_get_contents", vec![e_var("filename")])),
            s_if(
                e_binop(e_var("_data"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_sql", e_binop(e_binop(e_binop(e_str("COPY "), BinOp::Concat, e_method_call(e_this(), "copyTarget", vec![e_var("tableName"), e_var("fields")])), BinOp::Concat, e_str(" FROM STDIN")), BinOp::Concat, e_method_call(e_this(), "copyOptions", vec![e_var("separator"), e_var("nullAs")]))),
            s_return(e_binop(e_call("\\elephc_pdo_copy_in", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("_sql"), e_cast(CastType::String, e_var("_data"))]), BinOp::GtEq, e_int(0))),
        ])
}

/// `copyToArray` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_copytoarray() -> MethodBuilder {
    method("copyToArray")
        .param("tableName", TypeExpr::Str)
        .param_default("separator", TypeExpr::Str, e_str("\t"))
        .param_default("nullAs", TypeExpr::Str, e_str("\\N"))
        .param_default("fields", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![t_array(), TypeExpr::False]))
        .body(vec![
            s_assign("_sql", e_binop(e_binop(e_binop(e_str("COPY "), BinOp::Concat, e_method_call(e_this(), "copyTarget", vec![e_var("tableName"), e_var("fields")])), BinOp::Concat, e_str(" TO STDOUT")), BinOp::Concat, e_method_call(e_this(), "copyOptions", vec![e_var("separator"), e_var("nullAs")]))),
            s_assign("_raw", e_call("\\elephc_pdo_copy_out", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("_sql")])),
            s_if(
                e_binop(e_var("_raw"), BinOp::StrictEq, e_str("")),
                vec![
                    s_if(
                        e_binop(e_call("\\elephc_pdo_errcode", vec![e_method_call(e_this(), "connectionId", vec![])]), BinOp::NotEq, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array(vec![])),
                ],
                vec![],
                None,
            ),
            s_assign("_lines", e_call("\\explode", vec![e_str("\n"), e_call("\\rtrim", vec![e_var("_raw"), e_str("\n")])])),
            s_assign("_out", e_array(vec![])),
            s_foreach(e_var("_lines"), None, "_line", vec![
                s_array_push("_out", e_binop(e_var("_line"), BinOp::Concat, e_str("\n"))),
            ]),
            s_return(e_var("_out")),
        ])
}

/// `copyToFile` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_copytofile() -> MethodBuilder {
    method("copyToFile")
        .param("tableName", TypeExpr::Str)
        .param("filename", TypeExpr::Str)
        .param_default("separator", TypeExpr::Str, e_str("\t"))
        .param_default("nullAs", TypeExpr::Str, e_str("\\N"))
        .param_default("fields", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_sql", e_binop(e_binop(e_binop(e_str("COPY "), BinOp::Concat, e_method_call(e_this(), "copyTarget", vec![e_var("tableName"), e_var("fields")])), BinOp::Concat, e_str(" TO STDOUT")), BinOp::Concat, e_method_call(e_this(), "copyOptions", vec![e_var("separator"), e_var("nullAs")]))),
            s_assign("_raw", e_call("\\elephc_pdo_copy_out", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("_sql")])),
            s_if(
                e_binop(e_binop(e_var("_raw"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_call("\\elephc_pdo_errcode", vec![e_method_call(e_this(), "connectionId", vec![])]), BinOp::NotEq, e_int(0))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_call("\\file_put_contents", vec![e_var("filename"), e_var("_raw")]), BinOp::StrictNotEq, e_bool(false))),
        ])
}

/// `getNotify` — lifted out of `decl_stmt_bootstrap_1` so it builds in its own stack frame.
fn stmt_bootstrap_1_getnotify() -> MethodBuilder {
    method("getNotify")
        .param_default("fetchMode", TypeExpr::Int, e_int(0))
        .param_default("timeoutMilliseconds", TypeExpr::Int, e_int(0))
        .returns(t_mixed())
        .body(vec![
            s_assign("_raw", e_call("\\elephc_pdo_get_notify", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("timeoutMilliseconds")])),
            s_if(
                e_binop(e_var("_raw"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_sep1", e_cast(CastType::Int, e_call("\\strpos", vec![e_var("_raw"), e_str("\t")]))),
            s_assign("_channel", e_call("\\substr", vec![e_var("_raw"), e_int(0), e_var("_sep1")])),
            s_assign("_rest", e_call("\\substr", vec![e_var("_raw"), e_binop(e_var("_sep1"), BinOp::Add, e_int(1))])),
            s_assign("_sep2", e_cast(CastType::Int, e_call("\\strpos", vec![e_var("_rest"), e_str("\t")]))),
            s_assign("_pid", e_cast(CastType::Int, e_call("\\substr", vec![e_var("_rest"), e_int(0), e_var("_sep2")]))),
            s_assign("_payload", e_call("\\substr", vec![e_var("_rest"), e_binop(e_var("_sep2"), BinOp::Add, e_int(1))])),
            s_if(
                e_binop(e_var("fetchMode"), BinOp::Eq, e_int(2)),
                vec![
                    s_return(e_array_assoc(vec![(e_str("message"), e_var("_channel")), (e_str("pid"), e_var("_pid")), (e_str("payload"), e_var("_payload"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_array(vec![e_var("_channel"), e_var("_pid"), e_var("_payload")])),
        ])
}

/// `elephc_pdo_available_driver_count` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_available_driver_count() -> Stmt {
    extern_fn("elephc_pdo_available_driver_count", "elephc_pdo")
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_available_driver_name` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_available_driver_name() -> Stmt {
    extern_fn("elephc_pdo_available_driver_name", "elephc_pdo")
        .param("index", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_ini_dsn_defined` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_ini_dsn_defined() -> Stmt {
    extern_fn("elephc_pdo_ini_dsn_defined", "elephc_pdo")
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_ini_dsn_value` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_ini_dsn_value() -> Stmt {
    extern_fn("elephc_pdo_ini_dsn_value", "elephc_pdo")
        .param("name", CType::Str)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_open` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_open() -> Stmt {
    extern_fn("elephc_pdo_open", "elephc_pdo")
        .param("dsn", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_open_persistent` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_open_persistent() -> Stmt {
    extern_fn("elephc_pdo_open_persistent", "elephc_pdo")
        .param("dsn", CType::Str)
        .param("persistent", CType::Int)
        .param("sqlite_flags", CType::Int)
        .param("my_init_command", CType::Str)
        .param("my_ssl_config", CType::Str)
        .param("my_found_rows", CType::Int)
        .param("persistent_key", CType::Str)
        .param("my_driver_config", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_last_open_error` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_last_open_error() -> Stmt {
    extern_fn("elephc_pdo_last_open_error", "elephc_pdo")
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_last_open_sqlstate` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_last_open_sqlstate() -> Stmt {
    extern_fn("elephc_pdo_last_open_sqlstate", "elephc_pdo")
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_last_open_native_code` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_last_open_native_code() -> Stmt {
    extern_fn("elephc_pdo_last_open_native_code", "elephc_pdo")
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_close` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_close() -> Stmt {
    extern_fn("elephc_pdo_close", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_pdo_release` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_release() -> Stmt {
    extern_fn("elephc_pdo_release", "elephc_pdo")
        .param("conn", CType::Int)
        .param("resetPgsqlSession", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_pdo_clear_callbacks` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_clear_callbacks() -> Stmt {
    extern_fn("elephc_pdo_clear_callbacks", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_exec` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_exec() -> Stmt {
    extern_fn("elephc_pdo_exec", "elephc_pdo")
        .param("conn", CType::Int)
        .param("sql", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_last_insert_id` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_last_insert_id() -> Stmt {
    extern_fn("elephc_pdo_last_insert_id", "elephc_pdo")
        .param("conn", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_changes` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_changes() -> Stmt {
    extern_fn("elephc_pdo_changes", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_begin` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_begin() -> Stmt {
    extern_fn("elephc_pdo_begin", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_commit` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_commit() -> Stmt {
    extern_fn("elephc_pdo_commit", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_rollback` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_rollback() -> Stmt {
    extern_fn("elephc_pdo_rollback", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_errcode` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_errcode() -> Stmt {
    extern_fn("elephc_pdo_errcode", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_errmsg` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_errmsg() -> Stmt {
    extern_fn("elephc_pdo_errmsg", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_prepare` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_prepare() -> Stmt {
    extern_fn("elephc_pdo_prepare", "elephc_pdo")
        .param("conn", CType::Int)
        .param("sql", CType::Str)
        .param("emulated", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_bind_parameter_index` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_bind_parameter_index() -> Stmt {
    extern_fn("elephc_pdo_bind_parameter_index", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_bind_int` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_bind_int() -> Stmt {
    extern_fn("elephc_pdo_bind_int", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("idx", CType::Int)
        .param("val", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_bind_double` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_bind_double() -> Stmt {
    extern_fn("elephc_pdo_bind_double", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("idx", CType::Int)
        .param("val", CType::Float)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_bind_text` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_bind_text() -> Stmt {
    extern_fn("elephc_pdo_bind_text", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("idx", CType::Int)
        .param("val", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_bind_text_national` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_bind_text_national() -> Stmt {
    extern_fn("elephc_pdo_bind_text_national", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("idx", CType::Int)
        .param("val", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_bind_blob` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_bind_blob() -> Stmt {
    extern_fn("elephc_pdo_bind_blob", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("idx", CType::Int)
        .param("data", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_bind_null` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_bind_null() -> Stmt {
    extern_fn("elephc_pdo_bind_null", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("idx", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_bind_output` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_bind_output() -> Stmt {
    extern_fn("elephc_pdo_bind_output", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("idx", CType::Int)
        .param("type", CType::Int)
        .param("maxLength", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_output_data` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_output_data() -> Stmt {
    extern_fn("elephc_pdo_output_data", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("idx", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_output_is_lob` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_output_is_lob() -> Stmt {
    extern_fn("elephc_pdo_output_is_lob", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("idx", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_output_is_numeric` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_output_is_numeric() -> Stmt {
    extern_fn("elephc_pdo_output_is_numeric", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("idx", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_reset` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_reset() -> Stmt {
    extern_fn("elephc_pdo_reset", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_clear_bindings` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_clear_bindings() -> Stmt {
    extern_fn("elephc_pdo_clear_bindings", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_step` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_step() -> Stmt {
    extern_fn("elephc_pdo_step", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_step_oriented` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_step_oriented() -> Stmt {
    extern_fn("elephc_pdo_step_oriented", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("orientation", CType::Int)
        .param("offset", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_result_memory_size` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_result_memory_size() -> Stmt {
    extern_fn("elephc_pdo_result_memory_size", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_next_rowset` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_next_rowset() -> Stmt {
    extern_fn("elephc_pdo_next_rowset", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_column_count` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_count() -> Stmt {
    extern_fn("elephc_pdo_column_count", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_column_name` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_name() -> Stmt {
    extern_fn("elephc_pdo_column_name", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_column_type` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_type() -> Stmt {
    extern_fn("elephc_pdo_column_type", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_column_int` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_int() -> Stmt {
    extern_fn("elephc_pdo_column_int", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_column_double` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_double() -> Stmt {
    extern_fn("elephc_pdo_column_double", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Float)
        .build()
}

/// `elephc_pdo_column_data_len` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_data_len() -> Stmt {
    extern_fn("elephc_pdo_column_data_len", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_column_data_ptr` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_data_ptr() -> Stmt {
    extern_fn("elephc_pdo_column_data_ptr", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_pdo_column_data_byte` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_data_byte() -> Stmt {
    extern_fn("elephc_pdo_column_data_byte", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .param("offset", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_finalize` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_finalize() -> Stmt {
    extern_fn("elephc_pdo_finalize", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_driver_name` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_driver_name() -> Stmt {
    extern_fn("elephc_pdo_driver_name", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_sqlstate` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_sqlstate() -> Stmt {
    extern_fn("elephc_pdo_sqlstate", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_stmt_errcode` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_stmt_errcode() -> Stmt {
    extern_fn("elephc_pdo_stmt_errcode", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_stmt_errmsg` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_stmt_errmsg() -> Stmt {
    extern_fn("elephc_pdo_stmt_errmsg", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_stmt_sqlstate` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_stmt_sqlstate() -> Stmt {
    extern_fn("elephc_pdo_stmt_sqlstate", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_stmt_sent_sql` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_stmt_sent_sql() -> Stmt {
    extern_fn("elephc_pdo_stmt_sent_sql", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_bind_bool` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_bind_bool() -> Stmt {
    extern_fn("elephc_pdo_bind_bool", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("idx", CType::Int)
        .param("val", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_set_busy_timeout` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_set_busy_timeout() -> Stmt {
    extern_fn("elephc_pdo_set_busy_timeout", "elephc_pdo")
        .param("conn", CType::Int)
        .param("ms", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_dblib_set_attribute` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_dblib_set_attribute() -> Stmt {
    extern_fn("elephc_pdo_dblib_set_attribute", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_dblib_attribute_bool` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_dblib_attribute_bool() -> Stmt {
    extern_fn("elephc_pdo_dblib_attribute_bool", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_dblib_os_errcode` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_dblib_os_errcode() -> Stmt {
    extern_fn("elephc_pdo_dblib_os_errcode", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_dblib_severity` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_dblib_severity() -> Stmt {
    extern_fn("elephc_pdo_dblib_severity", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_dblib_os_errmsg` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_dblib_os_errmsg() -> Stmt {
    extern_fn("elephc_pdo_dblib_os_errmsg", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_dblib_stmt_os_errcode` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_dblib_stmt_os_errcode() -> Stmt {
    extern_fn("elephc_pdo_dblib_stmt_os_errcode", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_dblib_stmt_severity` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_dblib_stmt_severity() -> Stmt {
    extern_fn("elephc_pdo_dblib_stmt_severity", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_dblib_stmt_os_errmsg` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_dblib_stmt_os_errmsg() -> Stmt {
    extern_fn("elephc_pdo_dblib_stmt_os_errmsg", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_firebird_set_attribute_int` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_firebird_set_attribute_int() -> Stmt {
    extern_fn("elephc_pdo_firebird_set_attribute_int", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_firebird_set_attribute_text` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_firebird_set_attribute_text() -> Stmt {
    extern_fn("elephc_pdo_firebird_set_attribute_text", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .param("value", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_firebird_attribute_int` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_firebird_attribute_int() -> Stmt {
    extern_fn("elephc_pdo_firebird_attribute_int", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_firebird_attribute_text` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_firebird_attribute_text() -> Stmt {
    extern_fn("elephc_pdo_firebird_attribute_text", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_firebird_column_pdo_type` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_firebird_column_pdo_type() -> Stmt {
    extern_fn("elephc_pdo_firebird_column_pdo_type", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_firebird_stmt_set_cursor_name` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_firebird_stmt_set_cursor_name() -> Stmt {
    extern_fn("elephc_pdo_firebird_stmt_set_cursor_name", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_firebird_stmt_cursor_name` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_firebird_stmt_cursor_name() -> Stmt {
    extern_fn("elephc_pdo_firebird_stmt_cursor_name", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_odbc_set_attribute` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_odbc_set_attribute() -> Stmt {
    extern_fn("elephc_pdo_odbc_set_attribute", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_odbc_attribute` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_odbc_attribute() -> Stmt {
    extern_fn("elephc_pdo_odbc_attribute", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_odbc_stmt_set_cursor_name` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_odbc_stmt_set_cursor_name() -> Stmt {
    extern_fn("elephc_pdo_odbc_stmt_set_cursor_name", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_odbc_stmt_cursor_name` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_odbc_stmt_cursor_name() -> Stmt {
    extern_fn("elephc_pdo_odbc_stmt_cursor_name", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_odbc_stmt_set_assume_utf8` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_odbc_stmt_set_assume_utf8() -> Stmt {
    extern_fn("elephc_pdo_odbc_stmt_set_assume_utf8", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("enabled", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_odbc_stmt_assume_utf8` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_odbc_stmt_assume_utf8() -> Stmt {
    extern_fn("elephc_pdo_odbc_stmt_assume_utf8", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_oci_set_attribute_int` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_oci_set_attribute_int() -> Stmt {
    extern_fn("elephc_pdo_oci_set_attribute_int", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_oci_set_attribute_text` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_oci_set_attribute_text() -> Stmt {
    extern_fn("elephc_pdo_oci_set_attribute_text", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .param("value", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_oci_attribute_int` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_oci_attribute_int() -> Stmt {
    extern_fn("elephc_pdo_oci_attribute_int", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_oci_column_pdo_type` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_oci_column_pdo_type() -> Stmt {
    extern_fn("elephc_pdo_oci_column_pdo_type", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_oci_column_scale` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_oci_column_scale() -> Stmt {
    extern_fn("elephc_pdo_oci_column_scale", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_oci_column_flags` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_oci_column_flags() -> Stmt {
    extern_fn("elephc_pdo_oci_column_flags", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_informix_column_scale` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_informix_column_scale() -> Stmt {
    extern_fn("elephc_pdo_informix_column_scale", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_informix_column_pdo_type` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_informix_column_pdo_type() -> Stmt {
    extern_fn("elephc_pdo_informix_column_pdo_type", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_ibm_set_attribute_text` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_ibm_set_attribute_text() -> Stmt {
    extern_fn("elephc_pdo_ibm_set_attribute_text", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .param("value", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_ibm_attribute_text` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_ibm_attribute_text() -> Stmt {
    extern_fn("elephc_pdo_ibm_attribute_text", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_ibm_attribute_int` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_ibm_attribute_int() -> Stmt {
    extern_fn("elephc_pdo_ibm_attribute_int", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_ibm_column_scale` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_ibm_column_scale() -> Stmt {
    extern_fn("elephc_pdo_ibm_column_scale", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_ibm_column_pdo_type` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_ibm_column_pdo_type() -> Stmt {
    extern_fn("elephc_pdo_ibm_column_pdo_type", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_sqlsrv_stmt_set_attribute` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_sqlsrv_stmt_set_attribute() -> Stmt {
    extern_fn("elephc_pdo_sqlsrv_stmt_set_attribute", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("attribute", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_sqlsrv_stmt_configure` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_sqlsrv_stmt_configure() -> Stmt {
    extern_fn("elephc_pdo_sqlsrv_stmt_configure", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("attribute", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_sqlsrv_stmt_attribute` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_sqlsrv_stmt_attribute() -> Stmt {
    extern_fn("elephc_pdo_sqlsrv_stmt_attribute", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("attribute", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_sqlsrv_column_is_datetime` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_sqlsrv_column_is_datetime() -> Stmt {
    extern_fn("elephc_pdo_sqlsrv_column_is_datetime", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_sqlsrv_info` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_sqlsrv_info() -> Stmt {
    extern_fn("elephc_pdo_sqlsrv_info", "elephc_pdo")
        .param("conn", CType::Int)
        .param("field", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_sqlsrv_classification_pair_count` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_sqlsrv_classification_pair_count() -> Stmt {
    extern_fn("elephc_pdo_sqlsrv_classification_pair_count", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_sqlsrv_classification_text` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_sqlsrv_classification_text() -> Stmt {
    extern_fn("elephc_pdo_sqlsrv_classification_text", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .param("pair", CType::Int)
        .param("field", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_sqlsrv_classification_pair_rank` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_sqlsrv_classification_pair_rank() -> Stmt {
    extern_fn("elephc_pdo_sqlsrv_classification_pair_rank", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .param("pair", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_sqlsrv_classification_query_rank` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_sqlsrv_classification_query_rank() -> Stmt {
    extern_fn("elephc_pdo_sqlsrv_classification_query_rank", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_cubrid_set_attribute` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_cubrid_set_attribute() -> Stmt {
    extern_fn("elephc_pdo_cubrid_set_attribute", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_cubrid_attribute` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_cubrid_attribute() -> Stmt {
    extern_fn("elephc_pdo_cubrid_attribute", "elephc_pdo")
        .param("conn", CType::Int)
        .param("attribute", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_cubrid_quote` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_cubrid_quote() -> Stmt {
    extern_fn("elephc_pdo_cubrid_quote", "elephc_pdo")
        .param("conn", CType::Int)
        .param("data", CType::Str)
        .param("length", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_cubrid_bind_typed` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_cubrid_bind_typed() -> Stmt {
    extern_fn("elephc_pdo_cubrid_bind_typed", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("index", CType::Int)
        .param("data", CType::Str)
        .param("length", CType::Int)
        .param("typeName", CType::Str)
        .param("isSet", CType::Int)
        .param("pdoType", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_cubrid_schema` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_cubrid_schema() -> Stmt {
    extern_fn("elephc_pdo_cubrid_schema", "elephc_pdo")
        .param("conn", CType::Int)
        .param("schemaType", CType::Int)
        .param("className", CType::Str)
        .param("attributeName", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_cubrid_column_scale` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_cubrid_column_scale() -> Stmt {
    extern_fn("elephc_pdo_cubrid_column_scale", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_cubrid_column_default` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_cubrid_column_default() -> Stmt {
    extern_fn("elephc_pdo_cubrid_column_default", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("column", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_server_version` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_server_version() -> Stmt {
    extern_fn("elephc_pdo_server_version", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_client_version` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_client_version() -> Stmt {
    extern_fn("elephc_pdo_client_version", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_server_info` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_server_info() -> Stmt {
    extern_fn("elephc_pdo_server_info", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_connection_status` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_connection_status() -> Stmt {
    extern_fn("elephc_pdo_connection_status", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_last_insert_id_text` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_last_insert_id_text() -> Stmt {
    extern_fn("elephc_pdo_last_insert_id_text", "elephc_pdo")
        .param("conn", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_backend_pid` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_backend_pid() -> Stmt {
    extern_fn("elephc_pdo_backend_pid", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_warning_count` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_warning_count() -> Stmt {
    extern_fn("elephc_pdo_warning_count", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_lob_create` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_lob_create() -> Stmt {
    extern_fn("elephc_pdo_lob_create", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_lob_unlink` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_lob_unlink() -> Stmt {
    extern_fn("elephc_pdo_lob_unlink", "elephc_pdo")
        .param("conn", CType::Int)
        .param("oid", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_copy_in` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_copy_in() -> Stmt {
    extern_fn("elephc_pdo_copy_in", "elephc_pdo")
        .param("conn", CType::Int)
        .param("copy_sql", CType::Str)
        .param("data", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_copy_out` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_copy_out() -> Stmt {
    extern_fn("elephc_pdo_copy_out", "elephc_pdo")
        .param("conn", CType::Int)
        .param("copy_sql", CType::Str)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_column_decltype` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_decltype() -> Stmt {
    extern_fn("elephc_pdo_column_decltype", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_load_extension` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_load_extension() -> Stmt {
    extern_fn("elephc_pdo_load_extension", "elephc_pdo")
        .param("conn", CType::Int)
        .param("path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_get_notify` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_get_notify() -> Stmt {
    extern_fn("elephc_pdo_get_notify", "elephc_pdo")
        .param("conn", CType::Int)
        .param("timeout_ms", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_blob_read` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_blob_read() -> Stmt {
    extern_fn("elephc_pdo_blob_read", "elephc_pdo")
        .param("conn", CType::Int)
        .param("table", CType::Str)
        .param("column", CType::Str)
        .param("rowid", CType::Int)
        .param("dbname", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_lob_get` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_lob_get() -> Stmt {
    extern_fn("elephc_pdo_lob_get", "elephc_pdo")
        .param("conn", CType::Int)
        .param("oid", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_blob_byte` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_blob_byte() -> Stmt {
    extern_fn("elephc_pdo_blob_byte", "elephc_pdo")
        .param("offset", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_blob_write` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_blob_write() -> Stmt {
    extern_fn("elephc_pdo_blob_write", "elephc_pdo")
        .param("conn", CType::Int)
        .param("table", CType::Str)
        .param("column", CType::Str)
        .param("rowid", CType::Int)
        .param("dbname", CType::Str)
        .param("data", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_lob_put` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_lob_put() -> Stmt {
    extern_fn("elephc_pdo_lob_put", "elephc_pdo")
        .param("conn", CType::Int)
        .param("oid", CType::Str)
        .param("data", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_lob_size` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_lob_size() -> Stmt {
    extern_fn("elephc_pdo_lob_size", "elephc_pdo")
        .param("conn", CType::Int)
        .param("oid", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_lob_read_at` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_lob_read_at() -> Stmt {
    extern_fn("elephc_pdo_lob_read_at", "elephc_pdo")
        .param("conn", CType::Int)
        .param("oid", CType::Str)
        .param("offset", CType::Int)
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_lob_write_at` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_lob_write_at() -> Stmt {
    extern_fn("elephc_pdo_lob_write_at", "elephc_pdo")
        .param("conn", CType::Int)
        .param("oid", CType::Str)
        .param("offset", CType::Int)
        .param("data", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_blob_size` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_blob_size() -> Stmt {
    extern_fn("elephc_pdo_blob_size", "elephc_pdo")
        .param("conn", CType::Int)
        .param("table", CType::Str)
        .param("column", CType::Str)
        .param("rowid", CType::Int)
        .param("dbname", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_blob_read_at` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_blob_read_at() -> Stmt {
    extern_fn("elephc_pdo_blob_read_at", "elephc_pdo")
        .param("conn", CType::Int)
        .param("table", CType::Str)
        .param("column", CType::Str)
        .param("rowid", CType::Int)
        .param("dbname", CType::Str)
        .param("offset", CType::Int)
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_blob_write_at` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_blob_write_at() -> Stmt {
    extern_fn("elephc_pdo_blob_write_at", "elephc_pdo")
        .param("conn", CType::Int)
        .param("table", CType::Str)
        .param("column", CType::Str)
        .param("rowid", CType::Int)
        .param("dbname", CType::Str)
        .param("offset", CType::Int)
        .param("data", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_create_collation` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_create_collation() -> Stmt {
    extern_fn("elephc_pdo_create_collation", "elephc_pdo")
        .param("conn", CType::Int)
        .param("name", CType::Str)
        .param("descriptor", CType::Ptr)
        .param("adapter", CType::Ptr)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_create_function` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_create_function() -> Stmt {
    extern_fn("elephc_pdo_create_function", "elephc_pdo")
        .param("conn", CType::Int)
        .param("name", CType::Str)
        .param("num_args", CType::Int)
        .param("flags", CType::Int)
        .param("descriptor", CType::Ptr)
        .param("adapter", CType::Ptr)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_create_aggregate` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_create_aggregate() -> Stmt {
    extern_fn("elephc_pdo_create_aggregate", "elephc_pdo")
        .param("conn", CType::Int)
        .param("name", CType::Str)
        .param("num_args", CType::Int)
        .param("step_descriptor", CType::Ptr)
        .param("step_adapter", CType::Ptr)
        .param("final_descriptor", CType::Ptr)
        .param("final_adapter", CType::Ptr)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_get_notice` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_get_notice() -> Stmt {
    extern_fn("elephc_pdo_get_notice", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_stmt_readonly` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_stmt_readonly() -> Stmt {
    extern_fn("elephc_pdo_stmt_readonly", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_no_backslash_escapes` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_no_backslash_escapes() -> Stmt {
    extern_fn("elephc_pdo_no_backslash_escapes", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_in_transaction` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_in_transaction() -> Stmt {
    extern_fn("elephc_pdo_in_transaction", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_set_autocommit` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_set_autocommit() -> Stmt {
    extern_fn("elephc_pdo_set_autocommit", "elephc_pdo")
        .param("conn", CType::Int)
        .param("enabled", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_autocommit` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_autocommit() -> Stmt {
    extern_fn("elephc_pdo_autocommit", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_set_fetch_table_names` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_set_fetch_table_names() -> Stmt {
    extern_fn("elephc_pdo_set_fetch_table_names", "elephc_pdo")
        .param("conn", CType::Int)
        .param("enabled", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_fetch_table_names` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_fetch_table_names() -> Stmt {
    extern_fn("elephc_pdo_fetch_table_names", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_set_buffered_query` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_set_buffered_query() -> Stmt {
    extern_fn("elephc_pdo_set_buffered_query", "elephc_pdo")
        .param("conn", CType::Int)
        .param("enabled", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_buffered_query` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_buffered_query() -> Stmt {
    extern_fn("elephc_pdo_buffered_query", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_set_prefetch` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_set_prefetch() -> Stmt {
    extern_fn("elephc_pdo_set_prefetch", "elephc_pdo")
        .param("conn", CType::Int)
        .param("enabled", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_stmt_set_prefetch` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_stmt_set_prefetch() -> Stmt {
    extern_fn("elephc_pdo_stmt_set_prefetch", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("enabled", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_stmt_enable_simple_streaming` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_stmt_enable_simple_streaming() -> Stmt {
    extern_fn("elephc_pdo_stmt_enable_simple_streaming", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_column_native_type` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_native_type() -> Stmt {
    extern_fn("elephc_pdo_column_native_type", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_column_type_oid` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_type_oid() -> Stmt {
    extern_fn("elephc_pdo_column_type_oid", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_column_table_name` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_table_name() -> Stmt {
    extern_fn("elephc_pdo_column_table_name", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_pdo_column_flags` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_flags() -> Stmt {
    extern_fn("elephc_pdo_column_flags", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_blob_data_ptr` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_blob_data_ptr() -> Stmt {
    extern_fn("elephc_pdo_blob_data_ptr", "elephc_pdo")
        .returns(CType::Ptr)
        .build()
}

/// `elephc_pdo_set_extended_result_codes` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_set_extended_result_codes() -> Stmt {
    extern_fn("elephc_pdo_set_extended_result_codes", "elephc_pdo")
        .param("conn", CType::Int)
        .param("on", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_set_transaction_mode` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_set_transaction_mode() -> Stmt {
    extern_fn("elephc_pdo_set_transaction_mode", "elephc_pdo")
        .param("conn", CType::Int)
        .param("mode", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_transaction_mode` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_transaction_mode() -> Stmt {
    extern_fn("elephc_pdo_transaction_mode", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_stmt_busy` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_stmt_busy() -> Stmt {
    extern_fn("elephc_pdo_stmt_busy", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_stmt_explain_mode` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_stmt_explain_mode() -> Stmt {
    extern_fn("elephc_pdo_stmt_explain_mode", "elephc_pdo")
        .param("stmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_stmt_set_explain_mode` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_stmt_set_explain_mode() -> Stmt {
    extern_fn("elephc_pdo_stmt_set_explain_mode", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("mode", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_set_authorizer` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_set_authorizer() -> Stmt {
    extern_fn("elephc_pdo_set_authorizer", "elephc_pdo")
        .param("conn", CType::Int)
        .param("descriptor", CType::Ptr)
        .param("adapter", CType::Ptr)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_clear_authorizer` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_clear_authorizer() -> Stmt {
    extern_fn("elephc_pdo_clear_authorizer", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_take_authorizer_error` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_take_authorizer_error() -> Stmt {
    extern_fn("elephc_pdo_take_authorizer_error", "elephc_pdo")
        .param("conn", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_column_table_oid` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_table_oid() -> Stmt {
    extern_fn("elephc_pdo_column_table_oid", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_column_len` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_len() -> Stmt {
    extern_fn("elephc_pdo_column_len", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_column_precision` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_column_precision() -> Stmt {
    extern_fn("elephc_pdo_column_precision", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_dblib_column_native_type_id` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_dblib_column_native_type_id() -> Stmt {
    extern_fn("elephc_pdo_dblib_column_native_type_id", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_dblib_column_user_type_id` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_dblib_column_user_type_id() -> Stmt {
    extern_fn("elephc_pdo_dblib_column_user_type_id", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_dblib_column_scale` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_dblib_column_scale() -> Stmt {
    extern_fn("elephc_pdo_dblib_column_scale", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_pdo_dblib_column_source` — transcribed from the PHP form.
fn decl_extern_elephc_pdo_dblib_column_source() -> Stmt {
    extern_fn("elephc_pdo_dblib_column_source", "elephc_pdo")
        .param("stmt", CType::Int)
        .param("i", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `pdo_drivers` — transcribed from the PHP form.
fn decl_fn_pdo_drivers(php_version: PhpVersion, drivers: OptionalDrivers) -> Stmt {
    // SQLSRV exists only from PHP 8.3 and was dropped after 8.5, so outside that window
    // it is off whatever the build asks for — which is also why it shows no delta at 8.6
    // and had to be read at 8.5.
    let sqlsrv_usable =
        drivers.sqlsrv && php_version >= PhpVersion::Php83 && php_version <= PhpVersion::Php85;
    function("pdo_drivers")
        .returns(t_array())
        .body(vec![
            s_assign("_drivers", e_array(vec![])),
            s_assign("_count", e_call("elephc_pdo_available_driver_count", vec![])),
            // The bridge reports every driver it was linked with, so this filters `sqlsrv`
            // back OUT when the build cannot use it — otherwise a program is told the
            // driver is available and then fails to connect with it.
            s_for(Some(s_assign("_index", e_int(0))), Some(e_binop(e_var("_index"), BinOp::Lt, e_var("_count"))), Some(s_expr(e_post_inc("_index"))), if sqlsrv_usable {
                vec![
                    s_array_push("_drivers", e_call("elephc_pdo_available_driver_name", vec![e_var("_index")])),
                ]
            } else {
                vec![
                    s_assign("_availableDriver", e_call("elephc_pdo_available_driver_name", vec![e_var("_index")])),
                    s_if(
                        e_binop(e_var("_availableDriver"), BinOp::StrictEq, e_str("sqlsrv")),
                        vec![
                            s_continue(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_array_push("_drivers", e_var("_availableDriver")),
                ]
            }),
            s_return(e_var("_drivers")),
        ])
        .build()
}

/// `__elephc_pdo_sqlstate_description_0` — transcribed from the PHP form.
fn decl_fn_elephc_pdo_sqlstate_description_0() -> Stmt {
    function("__elephc_pdo_sqlstate_description_0")
        .param("state", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("00000")),
                vec![
                    s_return(e_str("No error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01000")),
                vec![
                    s_return(e_str("Warning")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01001")),
                vec![
                    s_return(e_str("Cursor operation conflict")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01002")),
                vec![
                    s_return(e_str("Disconnect error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01003")),
                vec![
                    s_return(e_str("NULL value eliminated in set function")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01004")),
                vec![
                    s_return(e_str("String data, right truncated")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01006")),
                vec![
                    s_return(e_str("Privilege not revoked")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01007")),
                vec![
                    s_return(e_str("Privilege not granted")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01008")),
                vec![
                    s_return(e_str("Implicit zero bit padding")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("0100C")),
                vec![
                    s_return(e_str("Dynamic result sets returned")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01P01")),
                vec![
                    s_return(e_str("Deprecated feature")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01S00")),
                vec![
                    s_return(e_str("Invalid connection string attribute")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01S01")),
                vec![
                    s_return(e_str("Error in row")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01S02")),
                vec![
                    s_return(e_str("Option value changed")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01S06")),
                vec![
                    s_return(e_str("Attempt to fetch before the result set returned the first rowset")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01S07")),
                vec![
                    s_return(e_str("Fractional truncation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01S08")),
                vec![
                    s_return(e_str("Error saving File DSN")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("01S09")),
                vec![
                    s_return(e_str("Invalid keyword")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("02000")),
                vec![
                    s_return(e_str("No data")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("02001")),
                vec![
                    s_return(e_str("No additional dynamic result sets returned")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("03000")),
                vec![
                    s_return(e_str("Sql statement not yet complete")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("07002")),
                vec![
                    s_return(e_str("COUNT field incorrect")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("07005")),
                vec![
                    s_return(e_str("Prepared statement not a cursor-specification")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("07006")),
                vec![
                    s_return(e_str("Restricted data type attribute violation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("07009")),
                vec![
                    s_return(e_str("Invalid descriptor index")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("07S01")),
                vec![
                    s_return(e_str("Invalid use of default parameter")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("08000")),
                vec![
                    s_return(e_str("Connection exception")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("08001")),
                vec![
                    s_return(e_str("Client unable to establish connection")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("08002")),
                vec![
                    s_return(e_str("Connection name in use")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("08003")),
                vec![
                    s_return(e_str("Connection does not exist")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("08004")),
                vec![
                    s_return(e_str("Server rejected the connection")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("08006")),
                vec![
                    s_return(e_str("Connection failure")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("08007")),
                vec![
                    s_return(e_str("Connection failure during transaction")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("08S01")),
                vec![
                    s_return(e_str("Communication link failure")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("09000")),
                vec![
                    s_return(e_str("Triggered action exception")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("0A000")),
                vec![
                    s_return(e_str("Feature not supported")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("0B000")),
                vec![
                    s_return(e_str("Invalid transaction initiation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("0F000")),
                vec![
                    s_return(e_str("Locator exception")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("0F001")),
                vec![
                    s_return(e_str("Invalid locator specification")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("0L000")),
                vec![
                    s_return(e_str("Invalid grantor")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("0LP01")),
                vec![
                    s_return(e_str("Invalid grant operation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("0P000")),
                vec![
                    s_return(e_str("Invalid role specification")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_pdo_sqlstate_description_2` — transcribed from the PHP form.
fn decl_fn_elephc_pdo_sqlstate_description_2() -> Stmt {
    function("__elephc_pdo_sqlstate_description_2")
        .param("state", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("21000")),
                vec![
                    s_return(e_str("Cardinality violation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("21S01")),
                vec![
                    s_return(e_str("Insert value list does not match column list")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("21S02")),
                vec![
                    s_return(e_str("Degree of derived table does not match column list")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22000")),
                vec![
                    s_return(e_str("Data exception")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22001")),
                vec![
                    s_return(e_str("String data, right truncated")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22002")),
                vec![
                    s_return(e_str("Indicator variable required but not supplied")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22003")),
                vec![
                    s_return(e_str("Numeric value out of range")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22004")),
                vec![
                    s_return(e_str("Null value not allowed")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22005")),
                vec![
                    s_return(e_str("Error in assignment")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22007")),
                vec![
                    s_return(e_str("Invalid datetime format")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22008")),
                vec![
                    s_return(e_str("Datetime field overflow")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22009")),
                vec![
                    s_return(e_str("Invalid time zone displacement value")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2200B")),
                vec![
                    s_return(e_str("Escape character conflict")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2200C")),
                vec![
                    s_return(e_str("Invalid use of escape character")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2200D")),
                vec![
                    s_return(e_str("Invalid escape octet")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2200F")),
                vec![
                    s_return(e_str("Zero length character string")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2200G")),
                vec![
                    s_return(e_str("Most specific type mismatch")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22010")),
                vec![
                    s_return(e_str("Invalid indicator parameter value")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22011")),
                vec![
                    s_return(e_str("Substring error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22012")),
                vec![
                    s_return(e_str("Division by zero")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22015")),
                vec![
                    s_return(e_str("Interval field overflow")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22018")),
                vec![
                    s_return(e_str("Invalid character value for cast specification")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22019")),
                vec![
                    s_return(e_str("Invalid escape character")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2201B")),
                vec![
                    s_return(e_str("Invalid regular expression")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2201E")),
                vec![
                    s_return(e_str("Invalid argument for logarithm")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2201F")),
                vec![
                    s_return(e_str("Invalid argument for power function")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2201G")),
                vec![
                    s_return(e_str("Invalid argument for width bucket function")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22020")),
                vec![
                    s_return(e_str("Invalid limit value")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22021")),
                vec![
                    s_return(e_str("Character not in repertoire")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22022")),
                vec![
                    s_return(e_str("Indicator overflow")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22023")),
                vec![
                    s_return(e_str("Invalid parameter value")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22024")),
                vec![
                    s_return(e_str("Unterminated c string")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22025")),
                vec![
                    s_return(e_str("Invalid escape sequence")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22026")),
                vec![
                    s_return(e_str("String data, length mismatch")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22027")),
                vec![
                    s_return(e_str("Trim error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2202E")),
                vec![
                    s_return(e_str("Array subscript error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22P01")),
                vec![
                    s_return(e_str("Floating point exception")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22P02")),
                vec![
                    s_return(e_str("Invalid text representation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22P03")),
                vec![
                    s_return(e_str("Invalid binary representation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22P04")),
                vec![
                    s_return(e_str("Bad copy file format")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("22P05")),
                vec![
                    s_return(e_str("Untranslatable character")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("23000")),
                vec![
                    s_return(e_str("Integrity constraint violation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("23001")),
                vec![
                    s_return(e_str("Restrict violation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("23502")),
                vec![
                    s_return(e_str("Not null violation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("23503")),
                vec![
                    s_return(e_str("Foreign key violation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("23505")),
                vec![
                    s_return(e_str("Unique violation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("23514")),
                vec![
                    s_return(e_str("Check violation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("24000")),
                vec![
                    s_return(e_str("Invalid cursor state")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25000")),
                vec![
                    s_return(e_str("Invalid transaction state")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25001")),
                vec![
                    s_return(e_str("Active sql transaction")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25002")),
                vec![
                    s_return(e_str("Branch transaction already active")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25003")),
                vec![
                    s_return(e_str("Inappropriate access mode for branch transaction")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25004")),
                vec![
                    s_return(e_str("Inappropriate isolation level for branch transaction")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25005")),
                vec![
                    s_return(e_str("No active sql transaction for branch transaction")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25006")),
                vec![
                    s_return(e_str("Read only sql transaction")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25007")),
                vec![
                    s_return(e_str("Schema and data statement mixing not supported")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25008")),
                vec![
                    s_return(e_str("Held cursor requires same isolation level")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25P01")),
                vec![
                    s_return(e_str("No active sql transaction")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25P02")),
                vec![
                    s_return(e_str("In failed sql transaction")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25S01")),
                vec![
                    s_return(e_str("Transaction state")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25S02")),
                vec![
                    s_return(e_str("Transaction is still active")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("25S03")),
                vec![
                    s_return(e_str("Transaction is rolled back")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("26000")),
                vec![
                    s_return(e_str("Invalid sql statement name")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("27000")),
                vec![
                    s_return(e_str("Triggered data change violation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("28000")),
                vec![
                    s_return(e_str("Invalid authorization specification")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2B000")),
                vec![
                    s_return(e_str("Dependent privilege descriptors still exist")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2BP01")),
                vec![
                    s_return(e_str("Dependent objects still exist")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2D000")),
                vec![
                    s_return(e_str("Invalid transaction termination")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2F000")),
                vec![
                    s_return(e_str("Sql routine exception")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2F002")),
                vec![
                    s_return(e_str("Modifying sql data not permitted")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2F003")),
                vec![
                    s_return(e_str("Prohibited sql statement attempted")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2F004")),
                vec![
                    s_return(e_str("Reading sql data not permitted")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("2F005")),
                vec![
                    s_return(e_str("Function executed no return statement")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_pdo_sqlstate_description_3` — transcribed from the PHP form.
fn decl_fn_elephc_pdo_sqlstate_description_3() -> Stmt {
    function("__elephc_pdo_sqlstate_description_3")
        .param("state", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("34000")),
                vec![
                    s_return(e_str("Invalid cursor name")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("38000")),
                vec![
                    s_return(e_str("External routine exception")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("38001")),
                vec![
                    s_return(e_str("Containing sql not permitted")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("38002")),
                vec![
                    s_return(e_str("Modifying sql data not permitted")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("38003")),
                vec![
                    s_return(e_str("Prohibited sql statement attempted")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("38004")),
                vec![
                    s_return(e_str("Reading sql data not permitted")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("39000")),
                vec![
                    s_return(e_str("External routine invocation exception")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("39001")),
                vec![
                    s_return(e_str("Invalid sqlstate returned")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("39004")),
                vec![
                    s_return(e_str("Null value not allowed")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("39P01")),
                vec![
                    s_return(e_str("Trigger protocol violated")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("39P02")),
                vec![
                    s_return(e_str("Srf protocol violated")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("3B000")),
                vec![
                    s_return(e_str("Savepoint exception")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("3B001")),
                vec![
                    s_return(e_str("Invalid savepoint specification")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("3C000")),
                vec![
                    s_return(e_str("Duplicate cursor name")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("3D000")),
                vec![
                    s_return(e_str("Invalid catalog name")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("3F000")),
                vec![
                    s_return(e_str("Invalid schema name")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_pdo_sqlstate_description_4` — transcribed from the PHP form.
fn decl_fn_elephc_pdo_sqlstate_description_4() -> Stmt {
    function("__elephc_pdo_sqlstate_description_4")
        .param("state", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("40000")),
                vec![
                    s_return(e_str("Transaction rollback")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("40001")),
                vec![
                    s_return(e_str("Serialization failure")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("40002")),
                vec![
                    s_return(e_str("Transaction integrity constraint violation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("40003")),
                vec![
                    s_return(e_str("Statement completion unknown")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("40P01")),
                vec![
                    s_return(e_str("Deadlock detected")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42000")),
                vec![
                    s_return(e_str("Syntax error or access violation")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42501")),
                vec![
                    s_return(e_str("Insufficient privilege")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42601")),
                vec![
                    s_return(e_str("Syntax error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42602")),
                vec![
                    s_return(e_str("Invalid name")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42611")),
                vec![
                    s_return(e_str("Invalid column definition")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42622")),
                vec![
                    s_return(e_str("Name too long")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42701")),
                vec![
                    s_return(e_str("Duplicate column")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42702")),
                vec![
                    s_return(e_str("Ambiguous column")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42703")),
                vec![
                    s_return(e_str("Undefined column")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42704")),
                vec![
                    s_return(e_str("Undefined object")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42710")),
                vec![
                    s_return(e_str("Duplicate object")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42712")),
                vec![
                    s_return(e_str("Duplicate alias")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42723")),
                vec![
                    s_return(e_str("Duplicate function")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42725")),
                vec![
                    s_return(e_str("Ambiguous function")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42803")),
                vec![
                    s_return(e_str("Grouping error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42804")),
                vec![
                    s_return(e_str("Datatype mismatch")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42809")),
                vec![
                    s_return(e_str("Wrong object type")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42830")),
                vec![
                    s_return(e_str("Invalid foreign key")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42846")),
                vec![
                    s_return(e_str("Cannot coerce")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42883")),
                vec![
                    s_return(e_str("Undefined function")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42939")),
                vec![
                    s_return(e_str("Reserved name")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P01")),
                vec![
                    s_return(e_str("Undefined table")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P02")),
                vec![
                    s_return(e_str("Undefined parameter")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P03")),
                vec![
                    s_return(e_str("Duplicate cursor")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P04")),
                vec![
                    s_return(e_str("Duplicate database")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P05")),
                vec![
                    s_return(e_str("Duplicate prepared statement")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P06")),
                vec![
                    s_return(e_str("Duplicate schema")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P07")),
                vec![
                    s_return(e_str("Duplicate table")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P08")),
                vec![
                    s_return(e_str("Ambiguous parameter")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P09")),
                vec![
                    s_return(e_str("Ambiguous alias")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P10")),
                vec![
                    s_return(e_str("Invalid column reference")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P11")),
                vec![
                    s_return(e_str("Invalid cursor definition")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P12")),
                vec![
                    s_return(e_str("Invalid database definition")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P13")),
                vec![
                    s_return(e_str("Invalid function definition")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P14")),
                vec![
                    s_return(e_str("Invalid prepared statement definition")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P15")),
                vec![
                    s_return(e_str("Invalid schema definition")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P16")),
                vec![
                    s_return(e_str("Invalid table definition")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P17")),
                vec![
                    s_return(e_str("Invalid object definition")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42P18")),
                vec![
                    s_return(e_str("Indeterminate datatype")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42S01")),
                vec![
                    s_return(e_str("Base table or view already exists")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42S02")),
                vec![
                    s_return(e_str("Base table or view not found")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42S11")),
                vec![
                    s_return(e_str("Index already exists")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42S12")),
                vec![
                    s_return(e_str("Index not found")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42S21")),
                vec![
                    s_return(e_str("Column already exists")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("42S22")),
                vec![
                    s_return(e_str("Column not found")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("44000")),
                vec![
                    s_return(e_str("WITH CHECK OPTION violation")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_pdo_sqlstate_description_5` — transcribed from the PHP form.
fn decl_fn_elephc_pdo_sqlstate_description_5() -> Stmt {
    function("__elephc_pdo_sqlstate_description_5")
        .param("state", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("53000")),
                vec![
                    s_return(e_str("Insufficient resources")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("53100")),
                vec![
                    s_return(e_str("Disk full")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("53200")),
                vec![
                    s_return(e_str("Out of memory")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("53300")),
                vec![
                    s_return(e_str("Too many connections")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("54000")),
                vec![
                    s_return(e_str("Program limit exceeded")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("54001")),
                vec![
                    s_return(e_str("Statement too complex")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("54011")),
                vec![
                    s_return(e_str("Too many columns")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("54023")),
                vec![
                    s_return(e_str("Too many arguments")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("55000")),
                vec![
                    s_return(e_str("Object not in prerequisite state")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("55006")),
                vec![
                    s_return(e_str("Object in use")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("55P02")),
                vec![
                    s_return(e_str("Cant change runtime param")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("55P03")),
                vec![
                    s_return(e_str("Lock not available")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("57000")),
                vec![
                    s_return(e_str("Operator intervention")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("57014")),
                vec![
                    s_return(e_str("Query canceled")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("57P01")),
                vec![
                    s_return(e_str("Admin shutdown")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("57P02")),
                vec![
                    s_return(e_str("Crash shutdown")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("57P03")),
                vec![
                    s_return(e_str("Cannot connect now")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("58030")),
                vec![
                    s_return(e_str("Io error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("58P01")),
                vec![
                    s_return(e_str("Undefined file")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("58P02")),
                vec![
                    s_return(e_str("Duplicate file")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_pdo_sqlstate_description_f` — transcribed from the PHP form.
fn decl_fn_elephc_pdo_sqlstate_description_f() -> Stmt {
    function("__elephc_pdo_sqlstate_description_f")
        .param("state", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("F0000")),
                vec![
                    s_return(e_str("Config file error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("F0001")),
                vec![
                    s_return(e_str("Lock file exists")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_pdo_sqlstate_description_h` — transcribed from the PHP form.
fn decl_fn_elephc_pdo_sqlstate_description_h() -> Stmt {
    function("__elephc_pdo_sqlstate_description_h")
        .param("state", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY000")),
                vec![
                    s_return(e_str("General error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY001")),
                vec![
                    s_return(e_str("Memory allocation error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY003")),
                vec![
                    s_return(e_str("Invalid application buffer type")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY004")),
                vec![
                    s_return(e_str("Invalid SQL data type")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY007")),
                vec![
                    s_return(e_str("Associated statement is not prepared")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY008")),
                vec![
                    s_return(e_str("Operation canceled")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY009")),
                vec![
                    s_return(e_str("Invalid use of null pointer")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY010")),
                vec![
                    s_return(e_str("Function sequence error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY011")),
                vec![
                    s_return(e_str("Attribute cannot be set now")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY012")),
                vec![
                    s_return(e_str("Invalid transaction operation code")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY013")),
                vec![
                    s_return(e_str("Memory management error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY014")),
                vec![
                    s_return(e_str("Limit on the number of handles exceeded")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY015")),
                vec![
                    s_return(e_str("No cursor name available")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY016")),
                vec![
                    s_return(e_str("Cannot modify an implementation row descriptor")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY017")),
                vec![
                    s_return(e_str("Invalid use of an automatically allocated descriptor handle")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY018")),
                vec![
                    s_return(e_str("Server declined cancel request")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY019")),
                vec![
                    s_return(e_str("Non-character and non-binary data sent in pieces")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY020")),
                vec![
                    s_return(e_str("Attempt to concatenate a null value")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY021")),
                vec![
                    s_return(e_str("Inconsistent descriptor information")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY024")),
                vec![
                    s_return(e_str("Invalid attribute value")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY090")),
                vec![
                    s_return(e_str("Invalid string or buffer length")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY091")),
                vec![
                    s_return(e_str("Invalid descriptor field identifier")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY092")),
                vec![
                    s_return(e_str("Invalid attribute/option identifier")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY093")),
                vec![
                    s_return(e_str("Invalid parameter number")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY095")),
                vec![
                    s_return(e_str("Function type out of range")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY096")),
                vec![
                    s_return(e_str("Invalid information type")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY097")),
                vec![
                    s_return(e_str("Column type out of range")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY098")),
                vec![
                    s_return(e_str("Scope type out of range")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY099")),
                vec![
                    s_return(e_str("Nullable type out of range")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY100")),
                vec![
                    s_return(e_str("Uniqueness option type out of range")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY101")),
                vec![
                    s_return(e_str("Accuracy option type out of range")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY103")),
                vec![
                    s_return(e_str("Invalid retrieval code")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY104")),
                vec![
                    s_return(e_str("Invalid precision or scale value")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY105")),
                vec![
                    s_return(e_str("Invalid parameter type")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY106")),
                vec![
                    s_return(e_str("Fetch type out of range")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY107")),
                vec![
                    s_return(e_str("Row value out of range")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY109")),
                vec![
                    s_return(e_str("Invalid cursor position")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY110")),
                vec![
                    s_return(e_str("Invalid driver completion")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HY111")),
                vec![
                    s_return(e_str("Invalid bookmark value")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HYC00")),
                vec![
                    s_return(e_str("Optional feature not implemented")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HYT00")),
                vec![
                    s_return(e_str("Timeout expired")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("HYT01")),
                vec![
                    s_return(e_str("Connection timeout expired")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_pdo_sqlstate_description_i` — transcribed from the PHP form.
fn decl_fn_elephc_pdo_sqlstate_description_i() -> Stmt {
    function("__elephc_pdo_sqlstate_description_i")
        .param("state", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM001")),
                vec![
                    s_return(e_str("Driver does not support this function")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM002")),
                vec![
                    s_return(e_str("Data source name not found and no default driver specified")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM003")),
                vec![
                    s_return(e_str("Specified driver could not be loaded")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM004")),
                vec![
                    s_return(e_str("Driver's SQLAllocHandle on SQL_HANDLE_ENV failed")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM005")),
                vec![
                    s_return(e_str("Driver's SQLAllocHandle on SQL_HANDLE_DBC failed")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM006")),
                vec![
                    s_return(e_str("Driver's SQLSetConnectAttr failed")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM007")),
                vec![
                    s_return(e_str("No data source or driver specified; dialog prohibited")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM008")),
                vec![
                    s_return(e_str("Dialog failed")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM009")),
                vec![
                    s_return(e_str("Unable to load translation DLL")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM010")),
                vec![
                    s_return(e_str("Data source name too long")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM011")),
                vec![
                    s_return(e_str("Driver name too long")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM012")),
                vec![
                    s_return(e_str("DRIVER keyword syntax error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM013")),
                vec![
                    s_return(e_str("Trace file error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM014")),
                vec![
                    s_return(e_str("Invalid name of File DSN")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("IM015")),
                vec![
                    s_return(e_str("Corrupt file data source")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_pdo_sqlstate_description_p` — transcribed from the PHP form.
fn decl_fn_elephc_pdo_sqlstate_description_p() -> Stmt {
    function("__elephc_pdo_sqlstate_description_p")
        .param("state", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("P0000")),
                vec![
                    s_return(e_str("Plpgsql error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("P0001")),
                vec![
                    s_return(e_str("Raise exception")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_pdo_sqlstate_description_x` — transcribed from the PHP form.
fn decl_fn_elephc_pdo_sqlstate_description_x() -> Stmt {
    function("__elephc_pdo_sqlstate_description_x")
        .param("state", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("XX000")),
                vec![
                    s_return(e_str("Internal error")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("state"), BinOp::StrictEq, e_str("XX001")),
                vec![
                    s_return(e_str("Data corrupted")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_pdo_sqlstate_description` — transcribed from the PHP form.
fn decl_fn_elephc_pdo_sqlstate_description() -> Stmt {
    function("__elephc_pdo_sqlstate_description")
        .param("state", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("_prefix", e_call("substr", vec![e_var("state"), e_int(0), e_int(1)])),
            s_if(
                e_binop(e_var("_prefix"), BinOp::StrictEq, e_str("0")),
                vec![
                    s_assign("_description", e_call("__elephc_pdo_sqlstate_description_0", vec![e_var("state")])),
                    s_if(
                        e_binop(e_var("_description"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_return(e_var("_description")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_prefix"), BinOp::StrictEq, e_str("2")),
                vec![
                    s_assign("_description", e_call("__elephc_pdo_sqlstate_description_2", vec![e_var("state")])),
                    s_if(
                        e_binop(e_var("_description"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_return(e_var("_description")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_prefix"), BinOp::StrictEq, e_str("3")),
                vec![
                    s_assign("_description", e_call("__elephc_pdo_sqlstate_description_3", vec![e_var("state")])),
                    s_if(
                        e_binop(e_var("_description"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_return(e_var("_description")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_prefix"), BinOp::StrictEq, e_str("4")),
                vec![
                    s_assign("_description", e_call("__elephc_pdo_sqlstate_description_4", vec![e_var("state")])),
                    s_if(
                        e_binop(e_var("_description"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_return(e_var("_description")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_prefix"), BinOp::StrictEq, e_str("5")),
                vec![
                    s_assign("_description", e_call("__elephc_pdo_sqlstate_description_5", vec![e_var("state")])),
                    s_if(
                        e_binop(e_var("_description"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_return(e_var("_description")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_prefix"), BinOp::StrictEq, e_str("F")),
                vec![
                    s_assign("_description", e_call("__elephc_pdo_sqlstate_description_f", vec![e_var("state")])),
                    s_if(
                        e_binop(e_var("_description"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_return(e_var("_description")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_prefix"), BinOp::StrictEq, e_str("H")),
                vec![
                    s_assign("_description", e_call("__elephc_pdo_sqlstate_description_h", vec![e_var("state")])),
                    s_if(
                        e_binop(e_var("_description"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_return(e_var("_description")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_prefix"), BinOp::StrictEq, e_str("I")),
                vec![
                    s_assign("_description", e_call("__elephc_pdo_sqlstate_description_i", vec![e_var("state")])),
                    s_if(
                        e_binop(e_var("_description"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_return(e_var("_description")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_prefix"), BinOp::StrictEq, e_str("P")),
                vec![
                    s_assign("_description", e_call("__elephc_pdo_sqlstate_description_p", vec![e_var("state")])),
                    s_if(
                        e_binop(e_var("_description"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_return(e_var("_description")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_prefix"), BinOp::StrictEq, e_str("X")),
                vec![
                    s_assign("_description", e_call("__elephc_pdo_sqlstate_description_x", vec![e_var("state")])),
                    s_if(
                        e_binop(e_var("_description"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_return(e_var("_description")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_return(e_str("<<Unknown error>>")),
        ])
        .build()
}

/// `__elephc_pdo_impl_error_message` — transcribed from the PHP form.
fn decl_fn_elephc_pdo_impl_error_message() -> Stmt {
    function("__elephc_pdo_impl_error_message")
        .param("state", TypeExpr::Str)
        .param("detail", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("_message", e_binop(e_binop(e_binop(e_str("SQLSTATE["), BinOp::Concat, e_var("state")), BinOp::Concat, e_str("]: ")), BinOp::Concat, e_call("__elephc_pdo_sqlstate_description", vec![e_var("state")]))),
            s_if(
                e_binop(e_var("detail"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_return(e_binop(e_binop(e_var("_message"), BinOp::Concat, e_str(": ")), BinOp::Concat, e_var("detail"))),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_message")),
        ])
        .build()
}

/// `PDOException` — transcribed from the PHP form.
fn decl_class_pdoexception() -> Stmt {
    class("PDOException")
        .extends("RuntimeException")
        .prop("errorInfo", t_nullable(t_array()), Some(e_null()))
        .private_prop("sqlStateCode", TypeExpr::Str, Some(e_str("")))
        .prop("previous", t_nullable(t_class("Throwable")), Some(e_null()))
        .method(pdoexception_construct())
        .method(pdoexception_elephcfromerrorinfo())
        .method(pdoexception_getcode())
        .method(pdoexception_getprevious())
        .build()
}

/// `__ElephcPDOSqliteBlobStream` — transcribed from the PHP form.
fn decl_class_elephcpdosqliteblobstream() -> Stmt {
    class("__ElephcPDOSqliteBlobStream")
        .final_()
        .private_static_prop("registered", TypeExpr::Bool, Some(e_bool(false)))
        .private_static_prop("pendingConn", TypeExpr::Int, Some(e_int(0)))
        .private_static_prop("pendingTable", TypeExpr::Str, Some(e_str("")))
        .private_static_prop("pendingColumn", TypeExpr::Str, Some(e_str("")))
        .private_static_prop("pendingRowid", TypeExpr::Int, Some(e_int(0)))
        .private_static_prop("pendingDbname", TypeExpr::Str, Some(e_str("main")))
        .private_static_prop("pendingSize", TypeExpr::Int, Some(e_int(0)))
        .private_static_prop("pendingWritable", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("conn", TypeExpr::Int, Some(e_int(0)))
        .private_prop("table", TypeExpr::Str, Some(e_str("")))
        .private_prop("column", TypeExpr::Str, Some(e_str("")))
        .private_prop("rowid", TypeExpr::Int, Some(e_int(0)))
        .private_prop("dbname", TypeExpr::Str, Some(e_str("main")))
        .private_prop("size", TypeExpr::Int, Some(e_int(0)))
        .private_prop("position", TypeExpr::Int, Some(e_int(0)))
        .private_prop("writable", TypeExpr::Bool, Some(e_bool(false)))
        .method(elephcpdosqliteblobstream_create())
        .method(elephcpdosqliteblobstream_stream_open())
        .method(elephcpdosqliteblobstream_stream_read())
        .method(elephcpdosqliteblobstream_stream_write())
        .method(elephcpdosqliteblobstream_stream_tell())
        .method(elephcpdosqliteblobstream_stream_eof())
        .method(elephcpdosqliteblobstream_stream_seek())
        .method(elephcpdosqliteblobstream_stream_stat())
        .method(elephcpdosqliteblobstream_stream_flush())
        .method(elephcpdosqliteblobstream_stream_close())
        .build()
}

/// `__ElephcPDOPgsqlLobStream` — transcribed from the PHP form.
fn decl_class_elephcpdopgsqllobstream() -> Stmt {
    class("__ElephcPDOPgsqlLobStream")
        .final_()
        .private_static_prop("registered", TypeExpr::Bool, Some(e_bool(false)))
        .private_static_prop("pendingConn", TypeExpr::Int, Some(e_int(0)))
        .private_static_prop("pendingOid", TypeExpr::Str, Some(e_str("")))
        .private_static_prop("pendingSize", TypeExpr::Int, Some(e_int(0)))
        .private_static_prop("pendingWritable", TypeExpr::Bool, Some(e_bool(false)))
        .private_static_prop("pendingOwner", t_nullable(t_class("PDO")), Some(e_null()))
        .private_prop("conn", TypeExpr::Int, Some(e_int(0)))
        .private_prop("oid", TypeExpr::Str, Some(e_str("")))
        .private_prop("size", TypeExpr::Int, Some(e_int(0)))
        .private_prop("position", TypeExpr::Int, Some(e_int(0)))
        .private_prop("writable", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("owner", t_nullable(t_class("PDO")), Some(e_null()))
        .method(elephcpdopgsqllobstream_create())
        .method(elephcpdopgsqllobstream_stream_open())
        .method(elephcpdopgsqllobstream_stream_read())
        .method(elephcpdopgsqllobstream_stream_write())
        .method(elephcpdopgsqllobstream_stream_tell())
        .method(elephcpdopgsqllobstream_stream_eof())
        .method(elephcpdopgsqllobstream_stream_seek())
        .method(elephcpdopgsqllobstream_stream_stat())
        .method(elephcpdopgsqllobstream_stream_flush())
        .method(elephcpdopgsqllobstream_stream_close())
        .build()
}

/// `PDO` — transcribed from the PHP form.
fn decl_class_pdo(php_version: PhpVersion, drivers: OptionalDrivers) -> Stmt {
    // SQLSRV exists only from PHP 8.3 and was dropped after 8.5, so outside that window
    // it is off whatever the build asks for — which is also why it shows no delta at 8.6
    // and had to be read at 8.5.
    let sqlsrv_usable =
        drivers.sqlsrv && php_version >= PhpVersion::Php83 && php_version <= PhpVersion::Php85;
    let renumbered_flags = php_version >= PhpVersion::Php85;
    class("PDO")
        .constant("FETCH_ASSOC", e_int(2))
        .constant("FETCH_NUM", e_int(3))
        .constant("FETCH_BOTH", e_int(4))
        .constant("FETCH_OBJ", e_int(5))
        .constant("FETCH_COLUMN", e_int(7))
        .constant("FETCH_CLASS", e_int(8))
        .constant("FETCH_INTO", e_int(9))
        .constant("PARAM_NULL", e_int(0))
        .constant("PARAM_INT", e_int(1))
        .constant("PARAM_STR", e_int(2))
        .constant("PARAM_BOOL", e_int(5))
        .constant("ATTR_TIMEOUT", e_int(2))
        .constant("ATTR_ERRMODE", e_int(3))
        .constant("ATTR_PERSISTENT", e_int(12))
        .constant("ATTR_DRIVER_NAME", e_int(16))
        .constant("ERRMODE_SILENT", e_int(0))
        .constant("ERRMODE_WARNING", e_int(1))
        .constant("ERRMODE_EXCEPTION", e_int(2))
        .constant("ERR_NONE", e_str("00000"))
        .constant("FETCH_DEFAULT", e_int(0))
        .constant("FETCH_LAZY", e_int(1))
        .constant("FETCH_BOUND", e_int(6))
        .constant("FETCH_FUNC", e_int(10))
        .constant("FETCH_NAMED", e_int(11))
        .constant("FETCH_KEY_PAIR", e_int(12))
        // PHP 8.5 renumbered every high fetch FLAG into the low byte. The decoders below move
        // with them, which is the reason the two must be selected by the same profile: a
        // program compiled for 8.4 read with 8.5 masks decodes a different fetch mode.
        .constant("FETCH_GROUP", e_int(if renumbered_flags { 32 } else { 0x10000 }))
        .constant("FETCH_UNIQUE", e_int(if renumbered_flags { 64 } else { 0x30000 }))
        .constant("FETCH_CLASSTYPE", e_int(if renumbered_flags { 128 } else { 0x40000 }))
        .constant("FETCH_SERIALIZE", e_int(if renumbered_flags { 512 } else { 0x80000 }))
        .constant("FETCH_PROPS_LATE", e_int(if renumbered_flags { 256 } else { 0x100000 }))
        .constant("FETCH_ORI_NEXT", e_int(0))
        .constant("FETCH_ORI_PRIOR", e_int(1))
        .constant("FETCH_ORI_FIRST", e_int(2))
        .constant("FETCH_ORI_LAST", e_int(3))
        .constant("FETCH_ORI_ABS", e_int(4))
        .constant("FETCH_ORI_REL", e_int(5))
        .constant("PARAM_LOB", e_int(3))
        .constant("PARAM_STMT", e_int(4))
        .constant("PARAM_INPUT_OUTPUT", e_int(2147483648))
        .constant("PARAM_STR_NATL", e_int(1073741824))
        .constant("PARAM_STR_CHAR", e_int(536870912))
        .constant("PARAM_EVT_ALLOC", e_int(0))
        .constant("PARAM_EVT_FREE", e_int(1))
        .constant("PARAM_EVT_EXEC_PRE", e_int(2))
        .constant("PARAM_EVT_EXEC_POST", e_int(3))
        .constant("PARAM_EVT_FETCH_PRE", e_int(4))
        .constant("PARAM_EVT_FETCH_POST", e_int(5))
        .constant("PARAM_EVT_NORMALIZE", e_int(6))
        .constant("ATTR_AUTOCOMMIT", e_int(0))
        .constant("ATTR_PREFETCH", e_int(1))
        .constant("ATTR_SERVER_VERSION", e_int(4))
        .constant("ATTR_CLIENT_VERSION", e_int(5))
        .constant("ATTR_SERVER_INFO", e_int(6))
        .constant("ATTR_CONNECTION_STATUS", e_int(7))
        .constant("ATTR_CASE", e_int(8))
        .constant("ATTR_CURSOR_NAME", e_int(9))
        .constant("ATTR_CURSOR", e_int(10))
        .constant("ATTR_ORACLE_NULLS", e_int(11))
        .constant("ATTR_STATEMENT_CLASS", e_int(13))
        .constant("ATTR_FETCH_TABLE_NAMES", e_int(14))
        .constant("ATTR_FETCH_CATALOG_NAMES", e_int(15))
        .constant("ATTR_STRINGIFY_FETCHES", e_int(17))
        .constant("ATTR_MAX_COLUMN_LEN", e_int(18))
        .constant("ATTR_DEFAULT_FETCH_MODE", e_int(19))
        .constant("ATTR_EMULATE_PREPARES", e_int(20))
        .constant("ATTR_DEFAULT_STR_PARAM", e_int(21))
        .constant("ATTR_DRIVER_SPECIFIC", e_int(1000))
        .constant("CASE_NATURAL", e_int(0))
        .constant("CASE_UPPER", e_int(1))
        .constant("CASE_LOWER", e_int(2))
        .constant("NULL_NATURAL", e_int(0))
        .constant("NULL_EMPTY_STRING", e_int(1))
        .constant("NULL_TO_STRING", e_int(2))
        .constant("CURSOR_FWDONLY", e_int(0))
        .constant("CURSOR_SCROLL", e_int(1))
        .constant("SQLITE_DETERMINISTIC", e_int(2048))
        .constant("SQLITE_ATTR_OPEN_FLAGS", e_int(1000))
        .constant("SQLITE_OPEN_READONLY", e_int(1))
        .constant("SQLITE_OPEN_READWRITE", e_int(2))
        .constant("SQLITE_OPEN_CREATE", e_int(4))
            .when(drivers.dblib, |class| {
                class.constant_attributed("DBLIB_ATTR_CONNECTION_TIMEOUT", e_int(1000), deprecated_alias(php_version, "Dblib", "ATTR_CONNECTION_TIMEOUT"))
            .constant_attributed("DBLIB_ATTR_QUERY_TIMEOUT", e_int(1001), deprecated_alias(php_version, "Dblib", "ATTR_QUERY_TIMEOUT"))
            .constant_attributed("DBLIB_ATTR_STRINGIFY_UNIQUEIDENTIFIER", e_int(1002), deprecated_alias(php_version, "Dblib", "ATTR_STRINGIFY_UNIQUEIDENTIFIER"))
            .constant_attributed("DBLIB_ATTR_VERSION", e_int(1003), deprecated_alias(php_version, "Dblib", "ATTR_VERSION"))
            .constant_attributed("DBLIB_ATTR_TDS_VERSION", e_int(1004), deprecated_alias(php_version, "Dblib", "ATTR_TDS_VERSION"))
            .constant_attributed("DBLIB_ATTR_SKIP_EMPTY_ROWSETS", e_int(1005), deprecated_alias(php_version, "Dblib", "ATTR_SKIP_EMPTY_ROWSETS"))
            .constant_attributed("DBLIB_ATTR_DATETIME_CONVERT", e_int(1006), deprecated_alias(php_version, "Dblib", "ATTR_DATETIME_CONVERT"))
            })
            .when(drivers.firebird, |class| {
                class.constant_attributed("FB_ATTR_DATE_FORMAT", e_int(1000), deprecated_alias(php_version, "Firebird", "ATTR_DATE_FORMAT"))
            .constant_attributed("FB_ATTR_TIME_FORMAT", e_int(1001), deprecated_alias(php_version, "Firebird", "ATTR_TIME_FORMAT"))
            .constant_attributed("FB_ATTR_TIMESTAMP_FORMAT", e_int(1002), deprecated_alias(php_version, "Firebird", "ATTR_TIMESTAMP_FORMAT"))
            })
            .when(drivers.odbc, |class| {
                class.constant_attributed("ODBC_ATTR_USE_CURSOR_LIBRARY", e_int(1000), deprecated_alias(php_version, "Odbc", "ATTR_USE_CURSOR_LIBRARY"))
            .constant_attributed("ODBC_ATTR_ASSUME_UTF8", e_int(1001), deprecated_alias(php_version, "Odbc", "ATTR_ASSUME_UTF8"))
            .constant_attributed("ODBC_SQL_USE_IF_NEEDED", e_int(0), deprecated_alias(php_version, "Odbc", "SQL_USE_IF_NEEDED"))
            .constant_attributed("ODBC_SQL_USE_ODBC", e_int(1), deprecated_alias(php_version, "Odbc", "SQL_USE_ODBC"))
            .constant_attributed("ODBC_SQL_USE_DRIVER", e_int(2), deprecated_alias(php_version, "Odbc", "SQL_USE_DRIVER"))
            })
            .when(drivers.ibm, |class| {
                class.constant_attributed("SQL_ATTR_INFO_USERID", e_int(1281), deprecated_alias(php_version, "Ibm", "ATTR_INFO_USERID"))
            .constant_attributed("SQL_ATTR_INFO_ACCTSTR", e_int(1282), deprecated_alias(php_version, "Ibm", "ATTR_INFO_ACCTSTR"))
            .constant_attributed("SQL_ATTR_INFO_APPLNAME", e_int(1283), deprecated_alias(php_version, "Ibm", "ATTR_INFO_APPLNAME"))
            .constant_attributed("SQL_ATTR_INFO_WRKSTNNAME", e_int(1284), deprecated_alias(php_version, "Ibm", "ATTR_INFO_WRKSTNNAME"))
            .constant_attributed("SQL_ATTR_USE_TRUSTED_CONTEXT", e_int(2561), deprecated_alias(php_version, "Ibm", "ATTR_USE_TRUSTED_CONTEXT"))
            .constant_attributed("SQL_ATTR_TRUSTED_CONTEXT_USERID", e_int(2562), deprecated_alias(php_version, "Ibm", "ATTR_TRUSTED_CONTEXT_USERID"))
            .constant_attributed("SQL_ATTR_TRUSTED_CONTEXT_PASSWORD", e_int(2563), deprecated_alias(php_version, "Ibm", "ATTR_TRUSTED_CONTEXT_PASSWORD"))
            })
            .when(sqlsrv_usable, |class| {
                class.constant("SQLSRV_ATTR_ENCODING", e_int(1000))
            .constant("SQLSRV_ATTR_QUERY_TIMEOUT", e_int(1001))
            .constant("SQLSRV_ATTR_DIRECT_QUERY", e_int(1002))
            .constant("SQLSRV_ATTR_CURSOR_SCROLL_TYPE", e_int(1003))
            .constant("SQLSRV_ATTR_CLIENT_BUFFER_MAX_KB_SIZE", e_int(1004))
            .constant("SQLSRV_ATTR_FETCHES_NUMERIC_TYPE", e_int(1005))
            .constant("SQLSRV_ATTR_FETCHES_DATETIME_TYPE", e_int(1006))
            .constant("SQLSRV_ATTR_FORMAT_DECIMALS", e_int(1007))
            .constant("SQLSRV_ATTR_DECIMAL_PLACES", e_int(1008))
            .constant("SQLSRV_ATTR_DATA_CLASSIFICATION", e_int(1009))
            .constant("SQLSRV_PARAM_OUT_DEFAULT_SIZE", e_neg(e_int(1)))
            .constant("SQLSRV_ENCODING_DEFAULT", e_int(1))
            .constant("SQLSRV_ENCODING_BINARY", e_int(2))
            .constant("SQLSRV_ENCODING_SYSTEM", e_int(3))
            .constant("SQLSRV_ENCODING_UTF8", e_int(65001))
            .constant("SQLSRV_CURSOR_STATIC", e_int(3))
            .constant("SQLSRV_CURSOR_DYNAMIC", e_int(2))
            .constant("SQLSRV_CURSOR_KEYSET", e_int(1))
            .constant("SQLSRV_CURSOR_BUFFERED", e_int(42))
            .constant("SQLSRV_TXN_READ_UNCOMMITTED", e_str("READ_UNCOMMITTED"))
            .constant("SQLSRV_TXN_READ_COMMITTED", e_str("READ_COMMITTED"))
            .constant("SQLSRV_TXN_REPEATABLE_READ", e_str("REPEATABLE_READ"))
            .constant("SQLSRV_TXN_SERIALIZABLE", e_str("SERIALIZABLE"))
            .constant("SQLSRV_TXN_SNAPSHOT", e_str("SNAPSHOT"))
            })
            .when(drivers.oci, |class| {
                class.constant("OCI_ATTR_ACTION", e_int(1000))
            .constant("OCI_ATTR_CLIENT_INFO", e_int(1001))
            .constant("OCI_ATTR_CLIENT_IDENTIFIER", e_int(1002))
            .constant("OCI_ATTR_MODULE", e_int(1003))
            .constant("OCI_ATTR_CALL_TIMEOUT", e_int(1004))
            })
            .when(drivers.cubrid, |class| {
                class.constant("CUBRID_ATTR_ISOLATION_LEVEL", e_int(1000))
            .constant("CUBRID_ATTR_LOCK_TIMEOUT", e_int(1001))
            .constant("CUBRID_ATTR_MAX_STRING_LENGTH", e_int(1002))
            .constant("TRAN_REP_CLASS_COMMIT_INSTANCE", e_int(4))
            .constant("TRAN_REP_CLASS_REP_INSTANCE", e_int(5))
            .constant("TRAN_SERIALIZABLE", e_int(6))
            .constant("CUBRID_SCH_TABLE", e_int(1))
            .constant("CUBRID_SCH_VIEW", e_int(2))
            .constant("CUBRID_SCH_QUERY_SPEC", e_int(3))
            .constant("CUBRID_SCH_ATTRIBUTE", e_int(4))
            .constant("CUBRID_SCH_TABLE_ATTRIBUTE", e_int(5))
            .constant("CUBRID_SCH_METHOD", e_int(6))
            .constant("CUBRID_SCH_TABLE_METHOD", e_int(7))
            .constant("CUBRID_SCH_METHOD_FILE", e_int(8))
            .constant("CUBRID_SCH_SUPER_TABLE", e_int(9))
            .constant("CUBRID_SCH_SUB_TABLE", e_int(10))
            .constant("CUBRID_SCH_CONSTRAINT", e_int(11))
            .constant("CUBRID_SCH_TRIGGER", e_int(12))
            .constant("CUBRID_SCH_TABLE_PRIVILEGE", e_int(13))
            .constant("CUBRID_SCH_COL_PRIVILEGE", e_int(14))
            .constant("CUBRID_SCH_DIRECT_SUPER_TABLE", e_int(15))
            .constant("CUBRID_SCH_PRIMARY_KEY", e_int(16))
            .constant("CUBRID_SCH_IMPORTED_KEYS", e_int(17))
            .constant("CUBRID_SCH_EXPORTED_KEYS", e_int(18))
            .constant("CUBRID_SCH_CROSS_REFERENCE", e_int(19))
            .constant("CUBRID_SCH_ATTR_WITH_SYNONYM", e_int(20))
            })
        .constant("SQLITE_ATTR_READONLY_STATEMENT", e_int(1001))
        .constant("SQLITE_ATTR_EXTENDED_RESULT_CODES", e_int(1002))
        .private_prop("conn", TypeExpr::Int, None)
        .private_prop("errMode", TypeExpr::Int, None)
        .private_prop("persistent", TypeExpr::Bool, None)
        .private_prop("attributes", t_array(), None)
        .private_prop("hasOperation", TypeExpr::Bool, None)
        .private_prop("inTxn", TypeExpr::Bool, None)
        .private_prop("autoCommit", TypeExpr::Bool, None)
        .private_prop("defaultStrParam", TypeExpr::Int, None)
        .private_prop("defaultFetchMode", TypeExpr::Int, None)
        .private_prop("statementClassConfig", t_array(), None)
        .private_prop("stringifyFetches", TypeExpr::Bool, None)
        .private_prop("attrCase", TypeExpr::Int, None)
        .private_prop("oracleNulls", TypeExpr::Int, None)
        .private_prop("emulatePrepares", TypeExpr::Bool, None)
        .private_prop("disablePrepares", TypeExpr::Bool, None)
        .private_prop("prepareOperation", TypeExpr::Str, None)
        .protected_prop("pdoUdfCallbacks", t_array(), None)
        .method(pdo_resolvedsnuri())
        .method(pdo_resolvedsnalias())
        .method(pdo_checkdsnissupported())
        .method(
            method("checkDriverSubclassDsn")
                .protected()
                .param("dsn", TypeExpr::Str)
                .param("calledClass", TypeExpr::Str)
                .param("expectedDriver", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_call("str_starts_with", vec![e_var("dsn"), e_binop(e_var("expectedDriver"), BinOp::Concat, e_str(":"))]),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_dsnDriver", e_str("")),
                    s_assign("_dsnClass", e_str("")),
                    s_if(
                        e_call("str_starts_with", vec![e_var("dsn"), e_str("sqlite:")]),
                        vec![
                            s_assign("_dsnDriver", e_str("sqlite")),
                            s_assign("_dsnClass", e_str("Pdo\\Sqlite")),
                        ],
                        [
                            vec![
                            (e_call("str_starts_with", vec![e_var("dsn"), e_str("mysql:")]), vec![
                                s_assign("_dsnDriver", e_str("mysql")),
                                s_assign("_dsnClass", e_str("Pdo\\Mysql")),
                            ]),
                            (e_call("str_starts_with", vec![e_var("dsn"), e_str("pgsql:")]), vec![
                                s_assign("_dsnDriver", e_str("pgsql")),
                                s_assign("_dsnClass", e_str("Pdo\\Pgsql")),
                            ]),
                            (e_call("str_starts_with", vec![e_var("dsn"), e_str("dblib:")]), vec![
                                s_assign("_dsnDriver", e_str("dblib")),
                                s_assign("_dsnClass", e_str("Pdo\\Dblib")),
                            ]),
                            (e_call("str_starts_with", vec![e_var("dsn"), e_str("firebird:")]), vec![
                                s_assign("_dsnDriver", e_str("firebird")),
                                s_assign("_dsnClass", e_str("Pdo\\Firebird")),
                            ]),
                            (e_call("str_starts_with", vec![e_var("dsn"), e_str("odbc:")]), vec![
                                s_assign("_dsnDriver", e_str("odbc")),
                                s_assign("_dsnClass", e_str("Pdo\\Odbc")),
                            ]),
                            ],
                            // Each optional driver contributes its own piece here, in the order the DSN prefixes are tested.
                            if drivers.ibm {
                                vec![
                                (e_call("str_starts_with", vec![e_var("dsn"), e_str("ibm:")]), vec![
                                    s_assign("_dsnDriver", e_str("ibm")),
                                    s_assign("_dsnClass", e_str("Pdo\\Ibm")),
                                ]),
                                ]
                            } else {
                                vec![]
                            },
                            if drivers.oci {
                                vec![
                                (e_call("str_starts_with", vec![e_var("dsn"), e_str("oci:")]), vec![
                                    s_assign("_dsnDriver", e_str("oci")),
                                    s_assign("_dsnClass", e_str("PDO")),
                                ]),
                                ]
                            } else {
                                vec![]
                            },
                            if sqlsrv_usable {
                                vec![
                                (e_call("str_starts_with", vec![e_var("dsn"), e_str("sqlsrv:")]), vec![
                                    s_assign("_dsnDriver", e_str("sqlsrv")),
                                    s_assign("_dsnClass", e_str("PDO")),
                                ]),
                                ]
                            } else {
                                vec![]
                            },
                            vec![
                            ],
                        ]
                        .concat(),
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_dsnDriver"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("PDOException", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_var("calledClass"), BinOp::Concat, e_str("::__construct() cannot be used for connecting to the \"")), BinOp::Concat, e_var("_dsnDriver")), BinOp::Concat, e_str("\" driver, either call ")), BinOp::Concat, e_var("_dsnClass")), BinOp::Concat, e_str("::__construct() or PDO::__construct() instead"))])),
                ]),
        )
        .method(
            method("__construct")
                .param("dsn", TypeExpr::Str)
                .param_default("username", t_nullable(TypeExpr::Str), e_null())
                .param_default("password", t_nullable(TypeExpr::Str), e_null())
                .when(php_version >= PhpVersion::Php82, |method| {
                    method.param_attr("\\SensitiveParameter")
                })
                .param_default("options", t_nullable(t_array()), e_null())
                .body(vec![
                    s_assign("_operation", e_binop(e_call("get_class", vec![e_this()]), BinOp::Concat, e_str("::__construct"))),
                    s_assign("_dsn", e_self_call("resolveDsnAlias", vec![e_var("dsn"), e_var("_operation")])),
                    s_assign("_dsn", e_self_call("resolveDsnUri", vec![e_var("_dsn"), e_var("_operation")])),
                    s_expr(e_method_call(e_this(), "checkDsnIsSupported", vec![e_var("_dsn")])),
                    s_prop_assign(e_this(), "errMode", e_int(2)),
                    s_prop_assign(e_this(), "persistent", e_bool(false)),
                    s_prop_assign(e_this(), "attributes", e_array(vec![])),
                    s_prop_assign(e_this(), "hasOperation", e_bool(false)),
                    s_prop_assign(e_this(), "inTxn", e_bool(false)),
                    s_prop_assign(e_this(), "autoCommit", e_bool(true)),
                    s_prop_assign(e_this(), "defaultStrParam", e_int(536870912)),
                    s_prop_assign(e_this(), "defaultFetchMode", e_int(4)),
                    s_prop_assign(e_this(), "statementClassConfig", e_array(vec![e_str("PDOStatement")])),
                    s_prop_assign(e_this(), "stringifyFetches", e_bool(false)),
                    s_prop_assign(e_this(), "attrCase", e_int(0)),
                    s_if(
                        e_binop(e_call("str_starts_with", vec![e_var("_dsn"), e_str("informix:")]), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("ibm:")])),
                        vec![
                            s_prop_assign(e_this(), "attrCase", e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "oracleNulls", e_int(0)),
                    s_prop_assign(e_this(), "emulatePrepares", e_binop(e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:")), BinOp::Or, e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("dblib:")))),
                    s_prop_assign(e_this(), "disablePrepares", e_bool(false)),
                    s_prop_assign(e_this(), "prepareOperation", e_str("PDO::prepare")),
                    s_prop_assign(e_this(), "pdoUdfCallbacks", e_array(vec![])),
                    s_assign("_openFlags", e_int(0)),
                    s_assign("_myInitCommand", e_str("")),
                    s_assign("_mySslCa", e_str("")),
                    s_assign("_mySslCert", e_str("")),
                    s_assign("_mySslKey", e_str("")),
                    s_assign("_mySslVerify", e_neg(e_int(1))),
                    s_assign("_myFoundRows", e_int(0)),
                    s_assign("_myBufferedQuery", e_int(1)),
                    s_assign("_myLocalInfile", e_int(0)),
                    s_assign("_myLocalInfileDirectory", e_str("")),
                    s_assign("_myCompress", e_int(0)),
                    s_assign("_myIgnoreSpace", e_int(0)),
                    s_assign("_myMultiStatements", e_int(1)),
                    s_assign("_mySslCapath", e_str("")),
                    s_assign("_mySslCipher", e_str("")),
                    s_assign("_myServerPublicKey", e_str("")),
                    s_assign("_persistentKey", e_str("")),
                    s_assign("_statementClassConfigured", e_bool(false)),
                    s_if(
                        e_binop(e_var("options"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_foreach(e_var("options"), Some("_attr"), "_val", vec![
                                s_if(
                                    e_binop(e_call("str_starts_with", vec![e_var("_dsn"), e_str("cubrid:")]), BinOp::And, e_call("is_string", vec![e_var("_attr")])),
                                    vec![
                                        s_if(
                                            e_not(e_call("is_string", vec![e_var("_val")])),
                                            vec![
                                                s_throw(e_new("PDOException", vec![e_str("Invalid CUBRID connection option")])),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_assign("_dsn", e_binop(e_binop(e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";")), BinOp::Concat, e_cast(CastType::String, e_var("_attr"))), BinOp::Concat, e_str("=")), BinOp::Concat, e_cast(CastType::String, e_var("_val")))),
                                        s_continue(1),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("_iattr", e_cast(CastType::Int, e_var("_attr"))),
                                s_if(
                                    e_binop(e_var("_iattr"), BinOp::Eq, e_int(0)),
                                    vec![
                                        s_prop_assign(e_this(), "autoCommit", e_method_call(e_this(), "attrBoolValue", vec![e_var("_val")])),
                                    ],
                                    vec![
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(3)), vec![
                                        s_assign("_ctorErrMode", e_method_call(e_this(), "attrIntValue", vec![e_var("_val")])),
                                        s_expr(e_method_call(e_this(), "checkErrMode", vec![e_var("_ctorErrMode")])),
                                        s_prop_assign(e_this(), "errMode", e_var("_ctorErrMode")),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(12)), vec![
                                        s_if(
                                            e_binop(e_binop(e_call("is_string", vec![e_var("_val")]), BinOp::And, e_binop(e_cast(CastType::String, e_var("_val")), BinOp::StrictNotEq, e_str(""))), BinOp::And, e_not(e_call("is_numeric", vec![e_cast(CastType::String, e_var("_val"))]))),
                                            vec![
                                                s_prop_assign(e_this(), "persistent", e_bool(true)),
                                                s_assign("_persistentKey", e_cast(CastType::String, e_var("_val"))),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_prop_assign(e_this(), "persistent", e_binop(e_cast(CastType::Int, e_var("_val")), BinOp::NotEq, e_int(0))),
                                        ]),
                                        ),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(13)), vec![
                                        s_prop_assign(e_this(), "statementClassConfig", e_method_call(e_this(), "validateStatementClassConfig", vec![e_var("_val"), e_bool(false)])),
                                        s_assign("_statementClassConfigured", e_bool(true)),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(19)), vec![
                                        s_assign("_ctorFetchMode", e_method_call(e_this(), "attrIntValue", vec![e_var("_val")])),
                                        s_expr(e_method_call(e_this(), "checkDefaultFetchMode", vec![e_var("_ctorFetchMode")])),
                                        s_prop_assign(e_this(), "defaultFetchMode", e_var("_ctorFetchMode")),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(17)), vec![
                                        s_prop_assign(e_this(), "stringifyFetches", e_method_call(e_this(), "attrBoolValue", vec![e_var("_val")])),
                                    ]),
                                    (e_binop(e_binop(e_var("_iattr"), BinOp::Eq, e_int(21)), BinOp::And, e_binop(e_binop(e_call("str_starts_with", vec![e_var("_dsn"), e_str("mysql:")]), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("dblib:")])), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("sqlsrv:")]))), vec![
                                        s_assign("_defaultStringType", e_method_call(e_this(), "attrIntValue", vec![e_var("_val")])),
                                        s_prop_assign(e_this(), "defaultStrParam", e_ternary(e_binop(e_var("_defaultStringType"), BinOp::Eq, e_int(1073741824)), e_int(1073741824), e_int(536870912))),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(20)), vec![
                                        s_if(
                                            e_call("str_starts_with", vec![e_var("_dsn"), e_str("dblib:")]),
                                            vec![
                                                s_prop_assign(e_this(), "emulatePrepares", e_bool(true)),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_prop_assign(e_this(), "emulatePrepares", e_method_call(e_this(), "attrBoolValue", vec![e_var("_val")])),
                                        ]),
                                        ),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(8)), vec![
                                        s_assign("_ctorCase", e_method_call(e_this(), "attrIntValue", vec![e_var("_val")])),
                                        s_expr(e_method_call(e_this(), "checkAttrCase", vec![e_var("_ctorCase")])),
                                        s_prop_assign(e_this(), "attrCase", e_var("_ctorCase")),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(11)), vec![
                                        s_prop_assign(e_this(), "oracleNulls", e_method_call(e_this(), "attrIntValue", vec![e_var("_val")])),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(2)), vec![
                                        s_assign("_unusedTimeout", e_method_call(e_this(), "attrIntValue", vec![e_var("_val")])),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(1000)), vec![
                                        s_if(
                                            e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(7)]), BinOp::StrictEq, e_str("sqlite:")),
                                            vec![
                                                s_assign("_openFlags", e_cast(CastType::Int, e_var("_val"))),
                                            ],
                                            vec![
                                            (e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("pgsql:")), vec![
                                                s_prop_assign(e_this(), "disablePrepares", e_method_call(e_this(), "attrBoolValue", vec![e_var("_val")])),
                                            ]),
                                            (e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:")), vec![
                                                s_assign("_myBufferedQuery", e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("_val")]), e_int(1), e_int(0))),
                                            ]),
                                        ],
                                            None,
                                        ),
                                    ]),
                                    (e_binop(e_binop(e_var("_iattr"), BinOp::Eq, e_int(1004)), BinOp::And, e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:"))), vec![
                                        s_prop_assign(e_this(), "emulatePrepares", e_method_call(e_this(), "attrBoolValue", vec![e_var("_val")])),
                                    ]),
                                    (e_binop(e_binop(e_var("_iattr"), BinOp::Eq, e_int(1001)), BinOp::And, e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:"))), vec![
                                        s_assign("_myLocalInfile", e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("_val")]), e_int(1), e_int(0))),
                                    ]),
                                    (e_binop(e_binop(e_var("_iattr"), BinOp::Eq, e_int(1002)), BinOp::And, e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:"))), vec![
                                        s_assign("_myInitCommand", e_cast(CastType::String, e_var("_val"))),
                                    ]),
                                    (e_binop(e_binop(e_var("_iattr"), BinOp::Eq, e_int(1003)), BinOp::And, e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:"))), vec![
                                        s_assign("_myCompress", e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("_val")]), e_int(1), e_int(0))),
                                    ]),
                                    (e_binop(e_binop(e_var("_iattr"), BinOp::Eq, e_int(1005)), BinOp::And, e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:"))), vec![
                                        s_assign("_myFoundRows", e_ternary(e_cast(CastType::Bool, e_var("_val")), e_int(1), e_int(0))),
                                    ]),
                                    (e_binop(e_binop(e_var("_iattr"), BinOp::Eq, e_int(1006)), BinOp::And, e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:"))), vec![
                                        s_assign("_myIgnoreSpace", e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("_val")]), e_int(1), e_int(0))),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(1009)), vec![
                                        s_assign("_mySslCa", e_cast(CastType::String, e_var("_val"))),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(1008)), vec![
                                        s_assign("_mySslCert", e_cast(CastType::String, e_var("_val"))),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(1007)), vec![
                                        s_assign("_mySslKey", e_cast(CastType::String, e_var("_val"))),
                                    ]),
                                    (e_binop(e_var("_iattr"), BinOp::Eq, e_int(1014)), vec![
                                        s_assign("_mySslVerify", e_ternary(e_cast(CastType::Bool, e_var("_val")), e_int(1), e_int(0))),
                                    ]),
                                    (e_binop(e_binop(e_var("_iattr"), BinOp::Eq, e_int(1010)), BinOp::And, e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:"))), vec![
                                        s_assign("_mySslCapath", e_cast(CastType::String, e_var("_val"))),
                                    ]),
                                    (e_binop(e_binop(e_var("_iattr"), BinOp::Eq, e_int(1011)), BinOp::And, e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:"))), vec![
                                        s_assign("_mySslCipher", e_cast(CastType::String, e_var("_val"))),
                                    ]),
                                    (e_binop(e_binop(e_var("_iattr"), BinOp::Eq, e_int(1012)), BinOp::And, e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:"))), vec![
                                        s_assign("_myServerPublicKey", e_cast(CastType::String, e_var("_val"))),
                                    ]),
                                    (e_binop(e_binop(e_var("_iattr"), BinOp::Eq, e_int(1013)), BinOp::And, e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:"))), vec![
                                        s_assign("_myMultiStatements", e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("_val")]), e_int(1), e_int(0))),
                                    ]),
                                    (e_binop(e_binop(e_var("_iattr"), BinOp::Eq, e_int(1015)), BinOp::And, e_binop(e_call("substr", vec![e_var("_dsn"), e_int(0), e_int(6)]), BinOp::StrictEq, e_str("mysql:"))), vec![
                                        s_assign("_myLocalInfileDirectory", e_cast(CastType::String, e_var("_val"))),
                                    ]),
                                ],
                                    None,
                                ),
                                s_prop_array_assign(e_this(), "attributes", e_var("_iattr"), e_var("_val")),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_statementClassConfigured"), BinOp::And, e_this_prop("persistent")),
                        vec![
                            s_throw(e_new("PDOException", vec![e_str("SQLSTATE[HY000]: General error: PDO::ATTR_STATEMENT_CLASS cannot be used with persistent PDO instances")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("str_starts_with", vec![e_var("_dsn"), e_str("sqlsrv:")]), BinOp::And, e_this_prop("persistent")),
                        vec![
                            s_throw(e_static_call("PDOException", "__elephcFromErrorInfo", vec![e_str("SQLSTATE[IMSSP]: An unsupported attribute was designated on the PDO object."), e_array(vec![e_str("IMSSP"), e_neg(e_int(38)), e_str("An unsupported attribute was designated on the PDO object.")])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("str_starts_with", vec![e_var("_dsn"), e_str("sqlsrv:")]), BinOp::And, e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(0))])),
                        vec![
                            s_throw(e_static_call("PDOException", "__elephcFromErrorInfo", vec![e_str("SQLSTATE[IMSSP]: An unsupported attribute was designated on the PDO object."), e_array(vec![e_str("IMSSP"), e_neg(e_int(38)), e_str("An unsupported attribute was designated on the PDO object.")])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_call("str_starts_with", vec![e_var("_dsn"), e_str("pgsql:")]), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("mysql:")])), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("dblib:")])), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("firebird:")])), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("odbc:")])), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("informix:")])), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("ibm:")])), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("oci:")])), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("sqlsrv:")])), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("cubrid:")])),
                        vec![
                            s_assign("_dsnIsMysql", e_call("str_starts_with", vec![e_var("_dsn"), e_str("mysql:")])),
                            s_assign("_dsnIsDblib", e_call("str_starts_with", vec![e_var("_dsn"), e_str("dblib:")])),
                            s_assign("_dsnIsFirebird", e_call("str_starts_with", vec![e_var("_dsn"), e_str("firebird:")])),
                            s_assign("_dsnIsOdbc", e_call("str_starts_with", vec![e_var("_dsn"), e_str("odbc:")])),
                            s_assign("_dsnIsInformix", e_call("str_starts_with", vec![e_var("_dsn"), e_str("informix:")])),
                            s_assign("_dsnIsIbm", e_call("str_starts_with", vec![e_var("_dsn"), e_str("ibm:")])),
                            s_assign("_dsnIsOci", e_call("str_starts_with", vec![e_var("_dsn"), e_str("oci:")])),
                            s_assign("_dsnIsSqlsrv", e_call("str_starts_with", vec![e_var("_dsn"), e_str("sqlsrv:")])),
                            s_assign("_dsnIsCubrid", e_call("str_starts_with", vec![e_var("_dsn"), e_str("cubrid:")])),
                            s_if(
                                e_binop(e_binop(e_var("username"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_dsnIsMysql"), BinOp::Or, e_var("_dsnIsDblib")), BinOp::Or, e_var("_dsnIsFirebird")), BinOp::Or, e_var("_dsnIsOci")), BinOp::Or, e_var("_dsnIsCubrid")), BinOp::Or, e_not(e_call("str_contains", vec![e_var("_dsn"), e_str("user=")])))),
                                vec![
                                    s_assign("_encUser", e_call("str_replace", vec![e_str(";"), e_str("%3B"), e_call("str_replace", vec![e_str("%"), e_str("%25"), e_var("username")])])),
                                    s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";user=")), BinOp::Concat, e_var("_encUser"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_var("password"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_dsnIsMysql"), BinOp::Or, e_var("_dsnIsDblib")), BinOp::Or, e_var("_dsnIsFirebird")), BinOp::Or, e_var("_dsnIsOci")), BinOp::Or, e_var("_dsnIsCubrid")), BinOp::Or, e_not(e_call("str_contains", vec![e_var("_dsn"), e_str("password=")])))),
                                vec![
                                    s_assign("_encPass", e_call("str_replace", vec![e_str(";"), e_str("%3B"), e_call("str_replace", vec![e_str("%"), e_str("%25"), e_var("password")])])),
                                    s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";password=")), BinOp::Concat, e_var("_encPass"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(2))]),
                                vec![
                                    s_if(
                                        e_var("_dsnIsDblib"),
                                        vec![
                                            s_if(
                                                e_binop(e_not(e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(1000))])), BinOp::And, e_not(e_call("str_contains", vec![e_var("_dsn"), e_str("connection_timeout=")]))),
                                                vec![
                                                    s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";connection_timeout=")), BinOp::Concat, e_cast(CastType::Int, e_index(e_this_prop("attributes"), e_int(2))))),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                            s_if(
                                                e_binop(e_not(e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(1001))])), BinOp::And, e_not(e_call("str_contains", vec![e_var("_dsn"), e_str("query_timeout=")]))),
                                                vec![
                                                    s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";query_timeout=")), BinOp::Concat, e_cast(CastType::Int, e_index(e_this_prop("attributes"), e_int(2))))),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                        ],
                                        vec![
                                        (e_not(e_call("str_contains", vec![e_var("_dsn"), e_str("connect_timeout=")])), vec![
                                            s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";connect_timeout=")), BinOp::Concat, e_cast(CastType::Int, e_index(e_this_prop("attributes"), e_int(2))))),
                                        ]),
                                    ],
                                        None,
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_var("_dsnIsDblib"),
                                vec![
                                    s_if(
                                        e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(1000))]),
                                        vec![
                                            s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";connection_timeout=")), BinOp::Concat, e_cast(CastType::Int, e_index(e_this_prop("attributes"), e_int(1000))))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(1001))]),
                                        vec![
                                            s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";query_timeout=")), BinOp::Concat, e_cast(CastType::Int, e_index(e_this_prop("attributes"), e_int(1001))))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(1002))]),
                                        vec![
                                            s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";stringify_uniqueidentifier=")), BinOp::Concat, e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_index(e_this_prop("attributes"), e_int(1002))]), e_str("1"), e_str("0")))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(1005))]),
                                        vec![
                                            s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";skip_empty_rowsets=")), BinOp::Concat, e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_index(e_this_prop("attributes"), e_int(1005))]), e_str("1"), e_str("0")))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(1006))]),
                                        vec![
                                            s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";datetime_convert=")), BinOp::Concat, e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_index(e_this_prop("attributes"), e_int(1006))]), e_str("1"), e_str("0")))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_var("_dsnIsOdbc"),
                                vec![
                                    s_assign("_odbcCursorLibrary", e_ternary(e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(1000))]), e_method_call(e_this(), "attrIntValue", vec![e_index(e_this_prop("attributes"), e_int(1000))]), e_int(0))),
                                    s_assign("_odbcAssumeUtf8", e_binop(e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(1001))]), BinOp::And, e_method_call(e_this(), "attrBoolValue", vec![e_index(e_this_prop("attributes"), e_int(1001))]))),
                                    s_assign("_dsn", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";elephc_odbc_cursor_library=")), BinOp::Concat, e_var("_odbcCursorLibrary")), BinOp::Concat, e_str(";elephc_odbc_assume_utf8=")), BinOp::Concat, e_ternary(e_var("_odbcAssumeUtf8"), e_str("1"), e_str("0"))), BinOp::Concat, e_str(";elephc_odbc_autocommit=")), BinOp::Concat, e_ternary(e_this_prop("autoCommit"), e_str("1"), e_str("0")))),
                                ],
                                vec![
                                (e_binop(e_var("_dsnIsInformix"), BinOp::Or, e_var("_dsnIsIbm")), vec![
                                    s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";elephc_odbc_autocommit=")), BinOp::Concat, e_ternary(e_this_prop("autoCommit"), e_str("1"), e_str("0")))),
                                    s_if(
                                        e_binop(e_var("_dsnIsIbm"), BinOp::And, e_binop(e_var("options"), BinOp::StrictNotEq, e_null())),
                                        vec![
                                            s_foreach(e_var("options"), Some("_ibmKey"), "_ibmRawValue", vec![
                                                s_if(
                                                    e_not(e_call("is_int", vec![e_var("_ibmKey")])),
                                                    vec![
                                                        s_continue(1),
                                                    ],
                                                    vec![],
                                                    None,
                                                ),
                                                s_assign("_ibmAttribute", e_cast(CastType::Int, e_var("_ibmKey"))),
                                                s_if(
                                                    e_binop(e_var("_ibmAttribute"), BinOp::Eq, e_int(2561)),
                                                    vec![
                                                        s_if(
                                                            e_method_call(e_this(), "attrBoolValue", vec![e_var("_ibmRawValue")]),
                                                            vec![
                                                                s_assign("_dsn", e_binop(e_var("_dsn"), BinOp::Concat, e_str(";elephc_ibm_attr_2561=1"))),
                                                                s_break(1),
                                                            ],
                                                            vec![],
                                                            None,
                                                        ),
                                                    ],
                                                    vec![
                                                    (e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_ibmAttribute"), BinOp::Eq, e_int(1281)), BinOp::Or, e_binop(e_var("_ibmAttribute"), BinOp::Eq, e_int(1282))), BinOp::Or, e_binop(e_var("_ibmAttribute"), BinOp::Eq, e_int(1283))), BinOp::Or, e_binop(e_var("_ibmAttribute"), BinOp::Eq, e_int(1284))), BinOp::Or, e_binop(e_var("_ibmAttribute"), BinOp::Eq, e_int(2562))), BinOp::Or, e_binop(e_var("_ibmAttribute"), BinOp::Eq, e_int(2563))), vec![
                                                        s_assign("_ibmValue", e_cast(CastType::String, e_var("_ibmRawValue"))),
                                                        s_assign("_ibmValue", e_call("str_replace", vec![e_str(";"), e_str("%3B"), e_call("str_replace", vec![e_str("%"), e_str("%25"), e_var("_ibmValue")])])),
                                                        s_assign("_dsn", e_binop(e_binop(e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";elephc_ibm_attr_")), BinOp::Concat, e_var("_ibmAttribute")), BinOp::Concat, e_str("=")), BinOp::Concat, e_var("_ibmValue"))),
                                                    ]),
                                                ],
                                                    None,
                                                ),
                                            ]),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ]),
                                (e_var("_dsnIsOci"), vec![
                                    s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";elephc_oci_autocommit=")), BinOp::Concat, e_ternary(e_this_prop("autoCommit"), e_str("1"), e_str("0")))),
                                ]),
                            ],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_mySslConfig", e_str("")),
                    s_if(
                        e_binop(e_var("_mySslCa"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("_mySslConfig", e_binop(e_binop(e_binop(e_var("_mySslConfig"), BinOp::Concat, e_str("ca=")), BinOp::Concat, e_var("_mySslCa")), BinOp::Concat, e_str(";"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_mySslCert"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("_mySslConfig", e_binop(e_binop(e_binop(e_var("_mySslConfig"), BinOp::Concat, e_str("cert=")), BinOp::Concat, e_var("_mySslCert")), BinOp::Concat, e_str(";"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_mySslKey"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("_mySslConfig", e_binop(e_binop(e_binop(e_var("_mySslConfig"), BinOp::Concat, e_str("key=")), BinOp::Concat, e_var("_mySslKey")), BinOp::Concat, e_str(";"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_mySslVerify"), BinOp::NotEq, e_neg(e_int(1))),
                        vec![
                            s_assign("_mySslConfig", e_binop(e_binop(e_binop(e_var("_mySslConfig"), BinOp::Concat, e_str("verify=")), BinOp::Concat, e_var("_mySslVerify")), BinOp::Concat, e_str(";"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_myEncode", closure()
                        .param("_option", TypeExpr::Str)
                        .returns(TypeExpr::Str)
                        .body(vec![
                            s_return(e_call("str_replace", vec![e_str("="), e_str("%3D"), e_call("str_replace", vec![e_str(";"), e_str("%3B"), e_call("str_replace", vec![e_str("%"), e_str("%25"), e_var("_option")])])])),
                        ])
                        .build()),
                    s_assign("_myDriverConfig", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("local="), BinOp::Concat, e_var("_myLocalInfile")), BinOp::Concat, e_str(";dir=")), BinOp::Concat, e_closure_call("_myEncode", vec![e_var("_myLocalInfileDirectory")])), BinOp::Concat, e_str(";compress=")), BinOp::Concat, e_var("_myCompress")), BinOp::Concat, e_str(";ignore=")), BinOp::Concat, e_var("_myIgnoreSpace")), BinOp::Concat, e_str(";multi=")), BinOp::Concat, e_var("_myMultiStatements")), BinOp::Concat, e_str(";buffered=")), BinOp::Concat, e_var("_myBufferedQuery")), BinOp::Concat, e_str(";capath=")), BinOp::Concat, e_closure_call("_myEncode", vec![e_var("_mySslCapath")])), BinOp::Concat, e_str(";cipher=")), BinOp::Concat, e_closure_call("_myEncode", vec![e_var("_mySslCipher")])), BinOp::Concat, e_str(";serverkey=")), BinOp::Concat, e_closure_call("_myEncode", vec![e_var("_myServerPublicKey")])), BinOp::Concat, e_str(";"))),
                    s_prop_assign(e_this(), "conn", e_call("elephc_pdo_open_persistent", vec![e_var("_dsn"), e_ternary(e_this_prop("persistent"), e_int(1), e_int(0)), e_var("_openFlags"), e_var("_myInitCommand"), e_var("_mySslConfig"), e_var("_myFoundRows"), e_var("_persistentKey"), e_var("_myDriverConfig")])),
                    s_if(
                        e_binop(e_this_prop("conn"), BinOp::Lt, e_int(0)),
                        vec![
                            s_assign("_openMsg", e_call("elephc_pdo_last_open_error", vec![])),
                            s_assign("_sqlstate", e_call("elephc_pdo_last_open_sqlstate", vec![])),
                            s_assign("_nativeCode", e_call("elephc_pdo_last_open_native_code", vec![])),
                            s_if(
                                e_binop(e_var("_sqlstate"), BinOp::StrictEq, e_str("")),
                                vec![
                                    s_assign("_sqlstate", e_ternary(e_binop(e_call("str_starts_with", vec![e_var("_dsn"), e_str("sqlite:")]), BinOp::Or, e_call("str_starts_with", vec![e_var("_dsn"), e_str("oci:")])), e_str("HY000"), e_str("08006"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_nativeInfo", e_ternary(e_binop(e_var("_nativeCode"), BinOp::Eq, e_int(0)), e_null(), e_var("_nativeCode"))),
                            s_throw(e_static_call("PDOException", "__elephcFromErrorInfo", vec![e_binop(e_binop(e_binop(e_str("SQLSTATE["), BinOp::Concat, e_var("_sqlstate")), BinOp::Concat, e_str("]: ")), BinOp::Concat, e_var("_openMsg")), e_array(vec![e_var("_sqlstate"), e_var("_nativeInfo"), e_var("_openMsg")])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("str_starts_with", vec![e_var("_dsn"), e_str("mysql:")]),
                        vec![
                            s_if(
                                e_binop(e_call("elephc_pdo_set_autocommit", vec![e_this_prop("conn"), e_ternary(e_this_prop("autoCommit"), e_int(1), e_int(0))]), BinOp::StrictNotEq, e_int(1)),
                                vec![
                                    s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(14))]),
                                vec![
                                    s_expr(e_call("elephc_pdo_set_fetch_table_names", vec![e_this_prop("conn"), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_index(e_this_prop("attributes"), e_int(14))]), e_int(1), e_int(0))])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![
                        (e_call("str_starts_with", vec![e_var("_dsn"), e_str("sqlite:")]), vec![
                            s_if(
                                e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(1002))]),
                                vec![
                                    s_expr(e_call("elephc_pdo_set_extended_result_codes", vec![e_this_prop("conn"), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_index(e_this_prop("attributes"), e_int(1002))]), e_int(1), e_int(0))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(1005))]),
                                vec![
                                    s_assign("_constructorTransactionMode", e_method_call(e_this(), "attrIntValue", vec![e_index(e_this_prop("attributes"), e_int(1005))])),
                                    s_if(
                                        e_binop(e_binop(e_var("_constructorTransactionMode"), BinOp::GtEq, e_int(0)), BinOp::And, e_binop(e_var("_constructorTransactionMode"), BinOp::LtEq, e_int(2))),
                                        vec![
                                            s_expr(e_call("elephc_pdo_set_transaction_mode", vec![e_this_prop("conn"), e_var("_constructorTransactionMode")])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                        (e_binop(e_call("str_starts_with", vec![e_var("_dsn"), e_str("pgsql:")]), BinOp::And, e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(1))])), vec![
                            s_expr(e_call("elephc_pdo_set_prefetch", vec![e_this_prop("conn"), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_index(e_this_prop("attributes"), e_int(1))]), e_int(1), e_int(0))])),
                        ]),
                        (e_call("str_starts_with", vec![e_var("_dsn"), e_str("firebird:")]), vec![
                            s_foreach(e_array(vec![e_int(0), e_int(14), e_int(1003), e_int(1007)]), None, "_firebirdIntAttribute", vec![
                                s_if(
                                    e_call("isset", vec![e_index(e_this_prop("attributes"), e_var("_firebirdIntAttribute"))]),
                                    vec![
                                        s_assign("_firebirdValue", e_ternary(e_binop(e_binop(e_binop(e_var("_firebirdIntAttribute"), BinOp::Eq, e_int(0)), BinOp::Or, e_binop(e_var("_firebirdIntAttribute"), BinOp::Eq, e_int(14))), BinOp::Or, e_binop(e_var("_firebirdIntAttribute"), BinOp::Eq, e_int(1007))), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_index(e_this_prop("attributes"), e_var("_firebirdIntAttribute"))]), e_int(1), e_int(0)), e_method_call(e_this(), "attrIntValue", vec![e_index(e_this_prop("attributes"), e_var("_firebirdIntAttribute"))]))),
                                        s_expr(e_call("elephc_pdo_firebird_set_attribute_int", vec![e_this_prop("conn"), e_var("_firebirdIntAttribute"), e_var("_firebirdValue")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_foreach(e_array(vec![e_int(1000), e_int(1001), e_int(1002)]), None, "_firebirdTextAttribute", vec![
                                s_if(
                                    e_call("isset", vec![e_index(e_this_prop("attributes"), e_var("_firebirdTextAttribute"))]),
                                    vec![
                                        s_expr(e_call("elephc_pdo_firebird_set_attribute_text", vec![e_this_prop("conn"), e_var("_firebirdTextAttribute"), e_cast(CastType::String, e_index(e_this_prop("attributes"), e_var("_firebirdTextAttribute")))])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                        ]),
                        (e_call("str_starts_with", vec![e_var("_dsn"), e_str("sqlsrv:")]), vec![
                            s_foreach(e_array(vec![e_int(10), e_int(1003), e_int(1009)]), None, "_sqlsrvStatementOnlyAttribute", vec![
                                s_if(
                                    e_call("isset", vec![e_index(e_this_prop("attributes"), e_var("_sqlsrvStatementOnlyAttribute"))]),
                                    vec![
                                        s_throw(e_static_call("PDOException", "__elephcFromErrorInfo", vec![e_str("SQLSTATE[IMSSP]: The given attribute is only supported on the PDOStatement object."), e_array(vec![e_str("IMSSP"), e_neg(e_int(39)), e_str("The given attribute is only supported on the PDOStatement object.")])])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_foreach(e_array(vec![e_int(17), e_int(20), e_int(21), e_int(1000), e_int(1001), e_int(1002), e_int(1004), e_int(1005), e_int(1006), e_int(1007), e_int(1008)]), None, "_sqlsrvAttribute", vec![
                                s_if(
                                    e_not(e_call("isset", vec![e_index(e_this_prop("attributes"), e_var("_sqlsrvAttribute"))])),
                                    vec![
                                        s_continue(1),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("_sqlsrvRaw", e_index(e_this_prop("attributes"), e_var("_sqlsrvAttribute"))),
                                s_assign("_sqlsrvValue", e_ternary(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_sqlsrvAttribute"), BinOp::Eq, e_int(17)), BinOp::Or, e_binop(e_var("_sqlsrvAttribute"), BinOp::Eq, e_int(20))), BinOp::Or, e_binop(e_var("_sqlsrvAttribute"), BinOp::Eq, e_int(1002))), BinOp::Or, e_binop(e_var("_sqlsrvAttribute"), BinOp::Eq, e_int(1005))), BinOp::Or, e_binop(e_var("_sqlsrvAttribute"), BinOp::Eq, e_int(1006))), BinOp::Or, e_binop(e_var("_sqlsrvAttribute"), BinOp::Eq, e_int(1007))), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("_sqlsrvRaw")]), e_int(1), e_int(0)), e_method_call(e_this(), "attrIntValue", vec![e_var("_sqlsrvRaw")]))),
                                s_if(
                                    e_binop(e_call("elephc_pdo_odbc_set_attribute", vec![e_this_prop("conn"), e_var("_sqlsrvAttribute"), e_var("_sqlsrvValue")]), BinOp::StrictNotEq, e_int(1)),
                                    vec![
                                        s_throw(e_static_call("PDOException", "__elephcFromErrorInfo", vec![e_str("SQLSTATE[IMSSP]: An invalid attribute was designated on the PDO object."), e_array(vec![e_str("IMSSP"), e_int(0), e_str("An invalid attribute was designated on the PDO object.")])])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                        ]),
                        (e_call("str_starts_with", vec![e_var("_dsn"), e_str("oci:")]), vec![
                            s_foreach(e_array(vec![e_int(0), e_int(1), e_int(1004)]), None, "_ociIntAttribute", vec![
                                s_if(
                                    e_call("isset", vec![e_index(e_this_prop("attributes"), e_var("_ociIntAttribute"))]),
                                    vec![
                                        s_assign("_ociValue", e_ternary(e_binop(e_var("_ociIntAttribute"), BinOp::Eq, e_int(0)), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_index(e_this_prop("attributes"), e_var("_ociIntAttribute"))]), e_int(1), e_int(0)), e_method_call(e_this(), "attrIntValue", vec![e_index(e_this_prop("attributes"), e_var("_ociIntAttribute"))]))),
                                        s_expr(e_call("elephc_pdo_oci_set_attribute_int", vec![e_this_prop("conn"), e_var("_ociIntAttribute"), e_var("_ociValue")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_foreach(e_array(vec![e_int(1000), e_int(1001), e_int(1002), e_int(1003)]), None, "_ociTextAttribute", vec![
                                s_if(
                                    e_call("isset", vec![e_index(e_this_prop("attributes"), e_var("_ociTextAttribute"))]),
                                    vec![
                                        s_expr(e_call("elephc_pdo_oci_set_attribute_text", vec![e_this_prop("conn"), e_var("_ociTextAttribute"), e_cast(CastType::String, e_index(e_this_prop("attributes"), e_var("_ociTextAttribute")))])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                        ]),
                        (e_call("str_starts_with", vec![e_var("_dsn"), e_str("cubrid:")]), vec![
                            s_if(
                                e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(0))]),
                                vec![
                                    s_expr(e_call("elephc_pdo_set_autocommit", vec![e_this_prop("conn"), e_ternary(e_this_prop("autoCommit"), e_int(1), e_int(0))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_foreach(e_array(vec![e_int(1000), e_int(1001)]), None, "_cubridAttribute", vec![
                                s_if(
                                    e_call("isset", vec![e_index(e_this_prop("attributes"), e_var("_cubridAttribute"))]),
                                    vec![
                                        s_expr(e_call("elephc_pdo_cubrid_set_attribute", vec![e_this_prop("conn"), e_var("_cubridAttribute"), e_method_call(e_this(), "attrIntValue", vec![e_index(e_this_prop("attributes"), e_var("_cubridAttribute"))])])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                        ]),
                    ],
                        None,
                    ),
                    s_if(
                        e_call("isset", vec![e_index(e_this_prop("attributes"), e_int(2))]),
                        vec![
                            s_if(
                                e_call("str_starts_with", vec![e_var("_dsn"), e_str("cubrid:")]),
                                vec![
                                    s_expr(e_call("elephc_pdo_cubrid_set_attribute", vec![e_this_prop("conn"), e_int(2), e_cast(CastType::Int, e_index(e_this_prop("attributes"), e_int(2)))])),
                                ],
                                vec![
                                (e_call("str_starts_with", vec![e_var("_dsn"), e_str("dblib:")]), vec![
                                    s_expr(e_call("elephc_pdo_dblib_set_attribute", vec![e_this_prop("conn"), e_int(2), e_cast(CastType::Int, e_index(e_this_prop("attributes"), e_int(2)))])),
                                ]),
                            ],
                                Some(vec![
                                s_expr(e_call("elephc_pdo_set_busy_timeout", vec![e_this_prop("conn"), e_binop(e_cast(CastType::Int, e_index(e_this_prop("attributes"), e_int(2))), BinOp::Mul, e_int(1000))])),
                            ]),
                            ),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(pdo_dbliberrorinfo())
        .method(pdo_fail())
        .method(pdo_throwauthorizererror())
        .method(pdo_failcode())
        .method(pdo_checkerrmode())
        .method(pdo_checkdefaultfetchmode())
        .method(pdo_checkattrcase())
        .method(pdo_attrvaluetypename())
        .method(pdo_attrintvalue())
        .method(pdo_attrboolvalue())
        .method(pdo_validatestatementclassconfig())
        .method(pdo_setattribute())
        .method(pdo_elephcdrainpgsqlnotices())
        .method(pdo_getattribute())
        .method(pdo_exec())
        .method(
            method("prepare")
                .param("query", TypeExpr::Str)
                .param_default("options", t_array(), e_array(vec![]))
                .returns(t_union(vec![t_class("PDOStatement"), TypeExpr::Bool]))
            .body(
                [
                    vec![
                        s_assign("_operation", e_this_prop("prepareOperation")),
                        s_prop_assign(e_this(), "prepareOperation", e_str("PDO::prepare")),
                        s_if(
                            e_binop(e_var("query"), BinOp::StrictEq, e_str("")),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str("PDO::prepare(): Argument #1 ($query) must not be empty")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_driver", e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")])),
                        s_assign("_statementConfig", e_this_prop("statementClassConfig")),
                        s_if(
                            e_call("array_key_exists", vec![e_int(13), e_var("options")]),
                            vec![
                                s_assign("_statementConfig", e_method_call(e_this(), "validateStatementClassConfig", vec![e_index(e_var("options"), e_int(13)), e_bool(false)])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_statementClass", e_cast(CastType::String, e_index(e_var("_statementConfig"), e_int(0)))),
                        s_assign("_statementStatus", e_call("__elephc_pdo_statement_class_status", vec![e_var("_statementClass")])),
                        s_if(
                            e_binop(e_binop(e_var("_statementStatus"), BinOp::Eq, e_int(4)), BinOp::Or, e_binop(e_var("_statementStatus"), BinOp::Eq, e_int(6))),
                            vec![
                                s_throw(e_new("Error", vec![e_binop(e_str("Cannot instantiate abstract class "), BinOp::Concat, e_var("_statementClass"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_hasStatementConstructor", e_binop(e_var("_statementStatus"), BinOp::Eq, e_int(5))),
                        s_if(
                            e_binop(e_call("array_key_exists", vec![e_int(1), e_var("_statementConfig")]), BinOp::And, e_not(e_var("_hasStatementConstructor"))),
                            vec![
                                s_throw(e_new("Error", vec![e_str("User-supplied statement does not accept constructor arguments")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_emulated", e_this_prop("emulatePrepares")),
                        s_assign("_disable", e_this_prop("disablePrepares")),
                        s_assign("_scrollable", e_bool(false)),
                        s_assign("_prefetchOverride", e_neg(e_int(2))),
                        s_if(
                            e_call("array_key_exists", vec![e_int(10), e_var("options")]),
                            vec![
                                s_assign("_cursorMode", e_cast(CastType::Int, e_index(e_var("options"), e_int(10)))),
                                s_if(
                                    e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlite")), BinOp::And, e_binop(e_var("_cursorMode"), BinOp::StrictNotEq, e_int(0))),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("pgsql")), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("odbc"))), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("informix"))), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("ibm"))), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("oci"))), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv"))), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("cubrid"))), BinOp::And, e_binop(e_var("_cursorMode"), BinOp::StrictEq, e_int(1))),
                                    vec![
                                        s_assign("_scrollable", e_bool(true)),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_call("isset", vec![e_index(e_var("options"), e_int(20))]),
                            vec![
                                s_assign("_emulated", e_method_call(e_this(), "attrBoolValue", vec![e_index(e_var("options"), e_int(20))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("pgsql")), BinOp::And, e_call("array_key_exists", vec![e_int(1), e_var("options")])),
                            vec![
                                s_assign("_prefetchOverride", e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_index(e_var("options"), e_int(1))]), e_int(1), e_int(0))),
                            ],
                            vec![
                            (e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("oci")), BinOp::And, e_call("array_key_exists", vec![e_int(1), e_var("options")])), vec![
                                s_assign("_prefetchOverride", e_method_call(e_this(), "attrIntValue", vec![e_index(e_var("options"), e_int(1))])),
                            ]),
                        ],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("pgsql")), BinOp::And, e_call("isset", vec![e_index(e_var("options"), e_int(1000))])),
                            vec![
                                s_assign("_disable", e_method_call(e_this(), "attrBoolValue", vec![e_index(e_var("options"), e_int(1000))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("mysql")), BinOp::And, e_call("isset", vec![e_index(e_var("options"), e_int(1004))])),
                            vec![
                                s_assign("_emulated", e_method_call(e_this(), "attrBoolValue", vec![e_index(e_var("options"), e_int(1004))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_simple", e_ternary(e_binop(e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("mysql")), BinOp::And, e_var("_emulated")), BinOp::Or, e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("pgsql")), BinOp::And, e_binop(e_binop(e_var("_emulated"), BinOp::Or, e_var("_disable")), BinOp::Or, e_var("_scrollable")))), e_int(1), e_int(0))),
                        s_if(
                            e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")), BinOp::And, e_var("_emulated")),
                            vec![
                                s_assign("_simple", e_binop(e_var("_simple"), BinOp::BitOr, e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("odbc")), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("informix"))), BinOp::Or, e_binop(e_var("_driver"), BinOp::StrictEq, e_str("ibm"))), BinOp::And, e_var("_scrollable")),
                            vec![
                                s_assign("_simple", e_int(2)),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")), BinOp::And, e_var("_scrollable")),
                            vec![
                                s_assign("_simple", e_binop(e_var("_simple"), BinOp::BitOr, e_int(2))),
                                s_if(
                                    e_call("array_key_exists", vec![e_int(1003), e_var("options")]),
                                    vec![
                                        s_assign("_sqlsrvCursorType", e_method_call(e_this(), "attrIntValue", vec![e_index(e_var("options"), e_int(1003))])),
                                        s_if(
                                            e_binop(e_binop(e_binop(e_binop(e_var("_sqlsrvCursorType"), BinOp::StrictNotEq, e_int(1)), BinOp::And, e_binop(e_var("_sqlsrvCursorType"), BinOp::StrictNotEq, e_int(2))), BinOp::And, e_binop(e_var("_sqlsrvCursorType"), BinOp::StrictNotEq, e_int(3))), BinOp::And, e_binop(e_var("_sqlsrvCursorType"), BinOp::StrictNotEq, e_int(42))),
                                            vec![
                                                s_expr(e_method_call(e_this(), "failCode", vec![e_str("IMSSP"), e_str("An invalid statement option was designated.")])),
                                                s_return(e_bool(false)),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_assign("_simple", e_binop(e_var("_simple"), BinOp::BitOr, e_binop(e_var("_sqlsrvCursorType"), BinOp::ShiftLeft, e_int(8)))),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![
                            (e_binop(e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")), BinOp::And, e_call("array_key_exists", vec![e_int(1003), e_var("options")])), vec![
                                s_expr(e_method_call(e_this(), "failCode", vec![e_str("IMSSP"), e_str("The cursor type must be scrollable to use a scroll type.")])),
                                s_return(e_bool(false)),
                            ]),
                        ],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")),
                            vec![
                                s_assign("_directQuery", e_binop(e_call("elephc_pdo_odbc_attribute", vec![e_this_prop("conn"), e_int(1002)]), BinOp::StrictEq, e_int(1))),
                                s_if(
                                    e_call("array_key_exists", vec![e_int(1002), e_var("options")]),
                                    vec![
                                        s_assign("_directQuery", e_method_call(e_this(), "attrBoolValue", vec![e_index(e_var("options"), e_int(1002))])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_var("_directQuery"),
                                    vec![
                                        s_assign("_simple", e_binop(e_var("_simple"), BinOp::BitOr, e_int(4))),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "hasOperation", e_bool(true)),
                        s_assign("_handle", e_call("elephc_pdo_prepare", vec![e_this_prop("conn"), e_var("query"), e_var("_simple")])),
                        s_if(
                            e_binop(e_var("_handle"), BinOp::Lt, e_int(0)),
                            vec![
                                s_expr(e_method_call(e_this(), "throwAuthorizerError", vec![e_var("_operation")])),
                                s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("_driver"), BinOp::StrictEq, e_str("sqlsrv")),
                            vec![
                                s_foreach(e_array(vec![e_int(1000), e_int(1001), e_int(1002), e_int(1003), e_int(1004), e_int(1005), e_int(1006), e_int(1007), e_int(1008), e_int(1009)]), None, "_sqlsrvOption", vec![
                                    s_if(
                                        e_not(e_call("isset", vec![e_index(e_var("options"), e_var("_sqlsrvOption"))])),
                                        vec![
                                            s_continue(1),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("_sqlsrvRaw", e_index(e_var("options"), e_var("_sqlsrvOption"))),
                                    s_assign("_sqlsrvOptionValue", e_ternary(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_sqlsrvOption"), BinOp::Eq, e_int(1002)), BinOp::Or, e_binop(e_var("_sqlsrvOption"), BinOp::Eq, e_int(1005))), BinOp::Or, e_binop(e_var("_sqlsrvOption"), BinOp::Eq, e_int(1006))), BinOp::Or, e_binop(e_var("_sqlsrvOption"), BinOp::Eq, e_int(1007))), BinOp::Or, e_binop(e_var("_sqlsrvOption"), BinOp::Eq, e_int(1009))), e_ternary(e_method_call(e_this(), "attrBoolValue", vec![e_var("_sqlsrvRaw")]), e_int(1), e_int(0)), e_method_call(e_this(), "attrIntValue", vec![e_var("_sqlsrvRaw")]))),
                                    s_if(
                                        e_binop(e_call("elephc_pdo_sqlsrv_stmt_configure", vec![e_var("_handle"), e_var("_sqlsrvOption"), e_var("_sqlsrvOptionValue")]), BinOp::StrictNotEq, e_int(1)),
                                        vec![
                                            s_expr(e_method_call(e_this(), "failCode", vec![e_str("IMSSP"), e_str("An invalid statement option was designated.")])),
                                            s_return(e_bool(false)),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ]),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("_prefetchOverride"), BinOp::NotEq, e_neg(e_int(2))),
                            vec![
                                s_expr(e_call("elephc_pdo_stmt_set_prefetch", vec![e_var("_handle"), e_var("_prefetchOverride")])),
                            ],
                            vec![],
                            None,
                        ),
                        // PostgreSQL simple streaming is PHP 8.5 and up. The statement is ABSENT
                        // below that, not present with a false guard, so it is spliced rather than
                        // conditioned — an `if` with an empty body is a different AST.
                    ],
                    // PostgreSQL simple streaming is PHP 8.5 and up. The statement is
                    // ABSENT below that, not present with a false guard — an `if` with an
                    // empty body is a different node — so it is SPLICED, not conditioned.
                    if php_version >= PhpVersion::Php85 {
                            vec![s_if(
                                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("pgsql")),
                                vec![
                                    s_expr(e_call("elephc_pdo_stmt_enable_simple_streaming", vec![e_var("_handle")])),
                                ],
                                vec![],
                                None,
                            )]
                    } else {
                        vec![]
                    },
                    vec![
                        s_assign("_stmt", e_call("__elephc_new_without_constructor", vec![e_var("_statementClass")])),
                        s_expr(e_call("__elephc_initialize_pdo_statement", vec![e_var("_stmt"), e_var("_handle"), e_this_prop("conn"), e_this_prop("errMode"), e_var("query")])),
                        s_expr(e_method_call(e_var("_stmt"), "setOwner", vec![e_this()])),
                        s_expr(e_method_call(e_var("_stmt"), "setDefaultFetchMode", vec![e_this_prop("defaultFetchMode")])),
                        s_expr(e_method_call(e_var("_stmt"), "setStringifyFetches", vec![e_this_prop("stringifyFetches")])),
                        s_expr(e_method_call(e_var("_stmt"), "setDefaultStrParam", vec![e_this_prop("defaultStrParam")])),
                        s_expr(e_method_call(e_var("_stmt"), "setEmulatePrepares", vec![e_binop(e_binop(e_var("_simple"), BinOp::BitAnd, e_int(1)), BinOp::StrictEq, e_int(1))])),
                        s_expr(e_method_call(e_var("_stmt"), "setAttrCase", vec![e_this_prop("attrCase")])),
                        s_expr(e_method_call(e_var("_stmt"), "setOracleNulls", vec![e_this_prop("oracleNulls")])),
                        s_expr(e_method_call(e_var("_stmt"), "setScrollable", vec![e_var("_scrollable")])),
                        s_if(
                            e_var("_hasStatementConstructor"),
                            vec![
                                s_if(
                                    e_call("array_key_exists", vec![e_int(1), e_var("_statementConfig")]),
                                    vec![
                                        s_expr(e_call("__elephc_invoke_pdo_statement_constructor", vec![e_var("_statementClass"), e_var("_stmt"), e_index(e_var("_statementConfig"), e_int(1))])),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_expr(e_call("__elephc_invoke_pdo_statement_constructor", vec![e_var("_statementClass"), e_var("_stmt"), e_array(vec![])])),
                                ]),
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_ignoredOptions", e_var("options")),
                        s_return(e_var("_stmt")),
                    ],
                ]
                .concat(),
            ),
        )
        .method(pdo_query())
        .method(pdo_lastinsertid())
            .when(drivers.cubrid, |class| {
                class.method(
                method("cubrid_schema")
                    .param("schemaType", TypeExpr::Int)
                    .param_default("className", t_nullable(TypeExpr::Str), e_null())
                    .param_default("attributeName", t_nullable(TypeExpr::Str), e_null())
                    .returns(t_union(vec![t_array(), TypeExpr::Bool]))
                    .body(vec![
                        s_if(
                            e_binop(e_call("elephc_pdo_driver_name", vec![e_this_prop("conn")]), BinOp::StrictNotEq, e_str("cubrid")),
                            vec![
                                s_throw(e_new("Error", vec![e_str("Call to undefined method PDO::cubrid_schema()")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_handle", e_call("elephc_pdo_cubrid_schema", vec![e_this_prop("conn"), e_var("schemaType"), e_null_coalesce(e_var("className"), e_str("")), e_null_coalesce(e_var("attributeName"), e_str(""))])),
                        s_if(
                            e_binop(e_var("_handle"), BinOp::Lt, e_int(0)),
                            vec![
                                s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_statement", e_call("__elephc_new_without_constructor", vec![e_str("PDOStatement")])),
                        s_expr(e_call("__elephc_initialize_pdo_statement", vec![e_var("_statement"), e_var("_handle"), e_this_prop("conn"), e_this_prop("errMode"), e_str("__elephc_cubrid_schema__")])),
                        s_expr(e_method_call(e_var("_statement"), "setOwner", vec![e_this()])),
                        s_expr(e_method_call(e_var("_statement"), "setDefaultFetchMode", vec![e_int(2)])),
                        s_return(e_method_call(e_var("_statement"), "fetchAll", vec![e_int(2)])),
                    ]),
            )
            })
        .method(pdo_begintransaction())
        .method(pdo_commit())
        .method(pdo_rollback())
        .method(pdo_intransaction())
        .method(
            method("getAvailableDrivers")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_assign("_drivers", e_array(vec![])),
                    s_assign("_count", e_call("elephc_pdo_available_driver_count", vec![])),
                    // The bridge reports every driver it was linked with, so this filters `sqlsrv`
                    // back OUT when the build cannot use it — otherwise a program is told the
                    // driver is available and then fails to connect with it.
                    s_for(Some(s_assign("_index", e_int(0))), Some(e_binop(e_var("_index"), BinOp::Lt, e_var("_count"))), Some(s_expr(e_post_inc("_index"))), if sqlsrv_usable {
                        vec![
                            s_array_push("_drivers", e_call("elephc_pdo_available_driver_name", vec![e_var("_index")])),
                        ]
                    } else {
                        vec![
                            s_assign("_availableDriver", e_call("elephc_pdo_available_driver_name", vec![e_var("_index")])),
                            s_if(
                                e_binop(e_var("_availableDriver"), BinOp::StrictEq, e_str("sqlsrv")),
                                vec![
                                    s_continue(1),
                                ],
                                vec![],
                                None,
                            ),
                            s_array_push("_drivers", e_var("_availableDriver")),
                        ]
                    }),
                    s_return(e_var("_drivers")),
                ]),
        )
        // `PDO::connect()` — the static factory that returns `static` — arrived in PHP
        // 8.4. Under earlier profiles the method is ABSENT, not present and failing.
        .when(php_version >= PhpVersion::Php84, |class| {
            class.method(
                method("connect")
                    .static_()
                    .param("dsn", TypeExpr::Str)
                    .param_default("username", t_nullable(TypeExpr::Str), e_null())
                    .param_default("password", t_nullable(TypeExpr::Str), e_null())
                    .when(php_version >= PhpVersion::Php82, |method| {
                        method.param_attr("\\SensitiveParameter")
                    })
                    .param_default("options", t_nullable(t_array()), e_null())
                    .returns(t_class("static"))
                    .body([
                        vec![
                            s_assign("calledClass", e_static_class()),
                            s_assign("calledStatus", e_call("__elephc_pdo_called_class_status", vec![e_var("calledClass")])),
                            s_assign("_operation", e_binop(e_var("calledClass"), BinOp::Concat, e_str("::connect"))),
                            s_assign("_dsn", e_self_call("resolveDsnAlias", vec![e_var("dsn"), e_var("_operation")])),
                            s_assign("_dsn", e_self_call("resolveDsnUri", vec![e_var("_dsn"), e_var("_operation")])),
                            s_assign("_driver", e_str("")),
                            s_assign("_driverClass", e_str("")),
                            s_assign("_driverStatus", e_neg(e_int(1))),
                            s_if(
                                e_call("str_starts_with", vec![e_var("_dsn"), e_str("sqlite:")]),
                                vec![
                                    s_assign("_driver", e_str("sqlite")),
                                    s_assign("_driverClass", e_str("Pdo\\Sqlite")),
                                    s_assign("_driverStatus", e_int(1)),
                                ],
                                [
                                    vec![
                                    (e_call("str_starts_with", vec![e_var("_dsn"), e_str("mysql:")]), vec![
                                        s_assign("_driver", e_str("mysql")),
                                        s_assign("_driverClass", e_str("Pdo\\Mysql")),
                                        s_assign("_driverStatus", e_int(2)),
                                    ]),
                                    (e_call("str_starts_with", vec![e_var("_dsn"), e_str("pgsql:")]), vec![
                                        s_assign("_driver", e_str("pgsql")),
                                        s_assign("_driverClass", e_str("Pdo\\Pgsql")),
                                        s_assign("_driverStatus", e_int(3)),
                                    ]),
                                    ],
                                    // Each optional driver contributes its own piece here, in the order the DSN prefixes are tested.
                                    if drivers.dblib {
                                        vec![
                                        (e_call("str_starts_with", vec![e_var("_dsn"), e_str("dblib:")]), vec![
                                            s_assign("_driver", e_str("dblib")),
                                            s_assign("_driverClass", e_str("Pdo\\Dblib")),
                                            s_assign("_driverStatus", e_int(4)),
                                        ]),
                                        ]
                                    } else {
                                        vec![]
                                    },
                                    if drivers.firebird {
                                        vec![
                                        (e_call("str_starts_with", vec![e_var("_dsn"), e_str("firebird:")]), vec![
                                            s_assign("_driver", e_str("firebird")),
                                            s_assign("_driverClass", e_str("Pdo\\Firebird")),
                                            s_assign("_driverStatus", e_int(5)),
                                        ]),
                                        ]
                                    } else {
                                        vec![]
                                    },
                                    if drivers.odbc {
                                        vec![
                                        (e_call("str_starts_with", vec![e_var("_dsn"), e_str("odbc:")]), vec![
                                            s_assign("_driver", e_str("odbc")),
                                            s_assign("_driverClass", e_str("Pdo\\Odbc")),
                                            s_assign("_driverStatus", e_int(6)),
                                        ]),
                                        ]
                                    } else {
                                        vec![]
                                    },
                                    if drivers.ibm {
                                        vec![
                                        (e_call("str_starts_with", vec![e_var("_dsn"), e_str("ibm:")]), vec![
                                            s_assign("_driver", e_str("ibm")),
                                            s_assign("_driverClass", e_str("Pdo\\Ibm")),
                                            s_assign("_driverStatus", e_int(7)),
                                        ]),
                                        ]
                                    } else {
                                        vec![]
                                    },
                                    if drivers.oci {
                                        vec![
                                        (e_call("str_starts_with", vec![e_var("_dsn"), e_str("oci:")]), vec![
                                            s_assign("_driver", e_str("oci")),
                                            s_assign("_driverClass", e_str("PDO")),
                                            s_assign("_driverStatus", e_int(0)),
                                        ]),
                                        ]
                                    } else {
                                        vec![]
                                    },
                                    if sqlsrv_usable {
                                        vec![
                                        (e_call("str_starts_with", vec![e_var("_dsn"), e_str("sqlsrv:")]), vec![
                                            s_assign("_driver", e_str("sqlsrv")),
                                            s_assign("_driverClass", e_str("PDO")),
                                            s_assign("_driverStatus", e_int(0)),
                                        ]),
                                        ]
                                    } else {
                                        vec![]
                                    },
                                    vec![
                                    ],
                                ]
                                .concat(),
                                None,
                            ),
                            s_if(
                                e_binop(e_var("_driver"), BinOp::StrictEq, e_str("")),
                                vec![
                                    s_if(
                                        e_binop(e_var("calledStatus"), BinOp::StrictEq, e_int(0)),
                                        vec![
                                            s_throw(e_new("PDOException", vec![e_str("could not find driver")])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_throw(e_new("PDOException", vec![e_binop(e_var("calledClass"), BinOp::Concat, e_str("::connect() cannot be used for connecting to an unknown driver, call PDO::connect() instead"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("calledStatus"), BinOp::StrictEq, e_var("_driverStatus")),
                                vec![
                                    s_return(e_new_static(vec![e_var("_dsn"), e_var("username"), e_var("password"), e_var("options")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("calledStatus"), BinOp::StrictNotEq, e_int(0)),
                                vec![
                                    s_throw(e_new("PDOException", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_var("calledClass"), BinOp::Concat, e_str("::connect() cannot be used for connecting to the \"")), BinOp::Concat, e_var("_driver")), BinOp::Concat, e_str("\" driver, either call ")), BinOp::Concat, e_var("_driverClass")), BinOp::Concat, e_str("::connect() or PDO::connect() instead"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("_driverStatus"), BinOp::StrictEq, e_int(1)),
                                vec![
                                    s_return(e_new_fq("Pdo\\Sqlite", vec![e_var("_dsn"), e_var("username"), e_var("password"), e_var("options")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("_driverStatus"), BinOp::StrictEq, e_int(2)),
                                vec![
                                    s_return(e_new_fq("Pdo\\Mysql", vec![e_var("_dsn"), e_var("username"), e_var("password"), e_var("options")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("_driverStatus"), BinOp::StrictEq, e_int(3)),
                                vec![
                                    s_return(e_new_fq("Pdo\\Pgsql", vec![e_var("_dsn"), e_var("username"), e_var("password"), e_var("options")])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        // Each optional driver contributes its own piece here, in the order the DSN prefixes are tested.
                        if drivers.dblib {
                            vec![
                                s_if(
                                    e_binop(e_var("_driverStatus"), BinOp::StrictEq, e_int(4)),
                                    vec![
                                        s_return(e_new_fq("Pdo\\Dblib", vec![e_var("_dsn"), e_var("username"), e_var("password"), e_var("options")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]
                        } else {
                            vec![]
                        },
                        if drivers.firebird {
                            vec![
                                s_if(
                                    e_binop(e_var("_driverStatus"), BinOp::StrictEq, e_int(5)),
                                    vec![
                                        s_return(e_new_fq("Pdo\\Firebird", vec![e_var("_dsn"), e_var("username"), e_var("password"), e_var("options")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]
                        } else {
                            vec![]
                        },
                        if drivers.odbc {
                            vec![
                                s_if(
                                    e_binop(e_var("_driverStatus"), BinOp::StrictEq, e_int(6)),
                                    vec![
                                        s_return(e_new_fq("Pdo\\Odbc", vec![e_var("_dsn"), e_var("username"), e_var("password"), e_var("options")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]
                        } else {
                            vec![]
                        },
                        if drivers.ibm {
                            vec![
                                s_if(
                                    e_binop(e_var("_driverStatus"), BinOp::StrictEq, e_int(7)),
                                    vec![
                                        s_return(e_new_fq("Pdo\\Ibm", vec![e_var("_dsn"), e_var("username"), e_var("password"), e_var("options")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]
                        } else {
                            vec![]
                        },
                        vec![
                            s_return(e_new_fq("PDO", vec![e_var("_dsn"), e_var("username"), e_var("password"), e_var("options")])),
                        ],
                    ]
                    .concat()),
            )
        })
        .method(pdo_connectionid())
        .method(pdo_sqlitecreatecollation())
        .method(pdo_sqlitecreatefunction())
        .method(pdo_sqlitecreateaggregate())
        .method(pdo_pdopgsqlcopyoptions())
        .method(pdo_pdopgsqlcopytarget())
        .method(pdo_pgsqlcopyfromarray())
        .method(pdo_pgsqlcopyfromfile())
        .method(pdo_pgsqlcopytoarray())
        .method(pdo_pgsqlcopytofile())
        .method(pdo_pgsqllobcreate())
        .method(pdo_pgsqllobopen())
        .method(pdo_pgsqllobunlink())
        .method(pdo_pgsqlgetnotify())
        .method(pdo_pgsqlgetpid())
        .method(pdo_errorcode())
        .method(pdo_errorinfo())
        .method(pdo_quote())
        .method(
            method("__destruct")
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("inTxn"), BinOp::Or, e_binop(e_call("elephc_pdo_in_transaction", vec![e_this_prop("conn")]), BinOp::StrictEq, e_int(1))),
                        vec![
                            s_expr(e_call("elephc_pdo_rollback", vec![e_this_prop("conn")])),
                            s_prop_assign(e_this(), "inTxn", e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_pdo_clear_callbacks", vec![e_this_prop("conn")])),
                    // PHP 8.6 resets a pooled PostgreSQL session on release; earlier profiles
                    // hand the connection back untouched. Same call, different flag.
                    s_expr(e_call(
                        "elephc_pdo_release",
                        vec![
                            e_this_prop("conn"),
                            e_int(i64::from(php_version >= PhpVersion::Php86)),
                        ],
                    )),
                ]),
        )
        .method(pdo_clone())
        .method(pdo_serialize())
        .method(pdo_sleep())
        .build()
}

/// `PDORow` — transcribed from the PHP form.
fn decl_class_pdorow(php_version: PhpVersion) -> Stmt {
    // `PDORow::$queryString` became a PUBLIC readonly property in PHP 8.1. Under 8.0 the
    // prelude keeps private storage for its own bookkeeping rather than exposing a surface
    // that version does not have.
    let query_string = |builder: crate::synthetic_class::ClassBuilder| {
        if php_version >= PhpVersion::Php81 {
            builder.readonly_prop("queryString", TypeExpr::Str)
        } else {
            builder.private_prop("queryString", TypeExpr::Str, None)
        }
    };
    query_string(class("PDORow").final_().implements("ArrayAccess"))
        .private_prop("columns", t_array(), None)
        .private_prop("names", t_array(), None)
        .method(pdorow_construct())
        .method(pdorow_elephcrefresh())
        .method(pdorow_get())
        .method(pdorow_isset())
        .method(pdorow_set())
        .method(pdorow_unset())
        .method(pdorow_offsetexists())
        .method(pdorow_offsetget())
        .method(pdorow_offsetset())
        .method(pdorow_offsetunset())
        .method(pdorow_serialize())
        .method(pdorow_sleep())
        .build()
}

/// `PDOStatement` — transcribed from the PHP form.
fn decl_class_pdostatement(php_version: PhpVersion) -> Stmt {
    // PHP 8.5 renumbered the fetch FLAGS into the low byte, so every decoder below moves
    // with `PDO::FETCH_*`. Bound once here rather than repeated as literals: the constants
    // and the masks that read them have to be chosen by the SAME condition, and thirteen
    // separate literals is thirteen chances for half of them to be updated.
    let renumbered_flags = php_version >= PhpVersion::Php85;
    let base_mask = if renumbered_flags { 0xF } else { 0xFFFF };
    let classtype = if renumbered_flags { 0x80 } else { 0x40000 };
    let props_late = if renumbered_flags { 0x100 } else { 0x100000 };
    let group = if renumbered_flags { 0x20 } else { 0x10000 };
    let unique = if renumbered_flags { 0x40 } else { 0x30000 };
    // FETCH_GROUP alone before 8.5; from 8.5 the two flags are separate bits and either
    // selects grouping. Bound as a closure because it appears twice and the two must not
    // be updated independently.
    let group_selected = || {
        if renumbered_flags {
            e_binop(e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_int(group)), BinOp::NotEq, e_int(0)), BinOp::Or, e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_int(unique)), BinOp::NotEq, e_int(0)))
        } else {
            e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_int(group)), BinOp::NotEq, e_int(0))
        }
    };
    class("PDOStatement")
        .implements("IteratorAggregate")
        .private_prop("stmt", TypeExpr::Int, None)
        .private_prop("conn", TypeExpr::Int, None)
        .private_prop("errMode", TypeExpr::Int, None)
        .private_prop("fetchMode", TypeExpr::Int, None)
        .private_untyped_prop("fetchTarget", None)
        .private_prop("fetchCtorArgs", t_array(), None)
        .private_prop("fetchPropsLate", TypeExpr::Bool, None)
        .private_prop("boundParams", t_array(), None)
        .private_prop("boundNames", t_array(), None)
        .private_prop("boundValues", t_array(), None)
        .private_prop("boundTypes", t_array(), None)
        .private_prop("boundDriverOptions", t_array(), None)
        .private_prop("boundPhpTypes", t_array(), None)
        .private_prop("boundNormalizedIndexes", t_array(), None)
        .private_prop("boundParamRefIndexes", t_array(), None)
        .private_prop("boundParamRefGetters", t_array(), None)
        .private_prop("boundParamRefStreamReaders", t_array(), None)
        .private_prop("boundParamRefSetters", t_array(), None)
        .private_prop("boundParamMaxLengths", t_array(), None)
        .private_prop("boundColumnKinds", t_array(), None)
        .private_prop("boundColumnIndexes", t_array(), None)
        .private_prop("boundColumnNames", t_array(), None)
        .private_prop("boundColumnSetters", t_array(), None)
        .private_prop("boundColumnTypes", t_array(), None)
        .private_prop("fetchColumn", TypeExpr::Int, None)
        .private_prop("rowCount", TypeExpr::Int, None)
        .private_prop("executed", TypeExpr::Bool, None)
        .private_prop("hasOperation", TypeExpr::Bool, None)
        .private_prop("lazyRow", t_mixed(), None)
        .private_prop("hasPendingStep", TypeExpr::Bool, None)
        .private_prop("pendingStep", TypeExpr::Int, None)
        .private_prop("scrollable", TypeExpr::Bool, None)
        // `PDOStatement::$queryString` became a PUBLIC readonly property in PHP 8.1; under 8.0
        // the prelude keeps private storage for its own SQL bookkeeping rather than exposing a
        // surface that profile does not have.
        .when(php_version >= PhpVersion::Php81, |class| {
            class.readonly_prop("queryString", TypeExpr::Str)
        })
        .when(php_version < PhpVersion::Php81, |class| {
            class.private_prop("queryString", TypeExpr::Str, None)
        })
        .private_prop("stringifyFetches", TypeExpr::Bool, None)
        .private_prop("defaultStrParam", TypeExpr::Int, None)
        .private_prop("emulatePrepares", TypeExpr::Bool, None)
        .private_prop("attrCase", TypeExpr::Int, None)
        .private_prop("oracleNulls", TypeExpr::Int, None)
        .private_prop("owner", t_nullable(t_class("PDO")), None)
        .method(pdostatement_construct())
        .method(pdostatement_elephcinitialize())
        .method(pdostatement_setowner())
        .method(pdostatement_setstringifyfetches())
        .method(pdostatement_setdefaultstrparam())
        .method(pdostatement_setemulateprepares())
        .method(pdostatement_setattrcase())
        .method(pdostatement_setoraclenulls())
        .method(pdostatement_currentstringifyfetches())
        .method(pdostatement_currentattrcase())
        .method(pdostatement_currentoraclenulls())
        .method(pdostatement_setscrollable())
        .method(pdostatement_dblibstatementerrorinfo())
        .method(pdostatement_fail())
        .method(pdostatement_failcode())
        .method(pdostatement_errorcode())
        .method(pdostatement_errorinfo())
        .method(pdostatement_setdefaultfetchmode())
        .method(pdostatement_argvaluetypename())
        .method(pdostatement_copyconstructorargs())
        .method(
            method("setFetchMode")
                .param("mode", TypeExpr::Int)
                .variadic("args", Some(t_mixed()))
                .returns(TypeExpr::Bool)
            .body(
                [
                    vec![
                        s_assign("_argCount", e_call("count", vec![e_var("args")])),
                        s_assign("classOrColumn", e_ternary(e_binop(e_var("_argCount"), BinOp::Gt, e_int(0)), e_index(e_var("args"), e_int(0)), e_null())),
                        s_assign("_constructorArgs", e_ternary(e_binop(e_var("_argCount"), BinOp::Gt, e_int(1)), e_index(e_var("args"), e_int(1)), e_null())),
                        s_assign("_base", e_binop(e_var("mode"), BinOp::BitAnd, e_int(base_mask))),
                        s_if(
                            e_binop(e_binop(e_var("_base"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("_base"), BinOp::Gt, e_int(12))),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str("PDOStatement::setFetchMode(): Argument #1 ($mode) must be a bitmask of PDO::FETCH_* constants")])),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    // 8.5 added this guard outright; earlier profiles have no statement
                    // here at all, so it is SPLICED rather than conditioned.
                    if renumbered_flags {
                        vec![
                            s_if(
                                e_binop(e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_binop(e_binop(e_int(128), BinOp::BitOr, e_int(256)), BinOp::BitOr, e_int(512))), BinOp::NotEq, e_int(0)), BinOp::And, e_binop(e_var("_base"), BinOp::NotEq, e_int(8))),
                                vec![
                                    s_throw(e_new("ValueError", vec![e_str("PDOStatement::setFetchMode(): Argument #1 ($mode) cannot use PDO::FETCH_CLASSTYPE, PDO::FETCH_PROPS_LATE, or PDO::FETCH_SERIALIZE fetch flags with a fetch mode other than PDO::FETCH_CLASS")])),
                                ],
                                vec![],
                                None,
                            ),
                        ]
                    } else {
                        vec![]
                    },
                    vec![
                        s_if(
                            e_binop(e_var("_base"), BinOp::Eq, e_int(10)),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str("Can only use PDO::FETCH_FUNC in PDOStatement::fetchAll()")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_binop(e_var("_base"), BinOp::Eq, e_int(7)), BinOp::And, e_binop(e_var("classOrColumn"), BinOp::StrictNotEq, e_null())), BinOp::And, e_not(e_call("is_int", vec![e_var("classOrColumn")]))),
                            vec![
                                s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("PDOStatement::setFetchMode(): Argument #2 must be of type int, "), BinOp::Concat, e_method_call(e_this(), "argValueTypeName", vec![e_var("classOrColumn")])), BinOp::Concat, e_str(" given"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_binop(e_var("_base"), BinOp::Eq, e_int(7)), BinOp::And, e_binop(e_var("classOrColumn"), BinOp::StrictNotEq, e_null())), BinOp::And, e_binop(e_cast(CastType::Int, e_var("classOrColumn")), BinOp::Lt, e_int(0))),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str("PDOStatement::setFetchMode(): Argument #2 ($args) must be greater than or equal to 0")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_binop(e_var("_base"), BinOp::Eq, e_int(8)), BinOp::And, e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_int(classtype)), BinOp::NotEq, e_int(0))), BinOp::And, e_binop(e_var("_argCount"), BinOp::NotEq, e_int(0))),
                            vec![
                                s_throw(e_new("ValueError", vec![e_binop(e_binop(e_str("PDOStatement::setFetchMode() expects exactly 1 argument for the fetch mode provided, "), BinOp::Concat, e_binop(e_int(1), BinOp::Add, e_var("_argCount"))), BinOp::Concat, e_str(" given"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("_base"), BinOp::Eq, e_int(7)), BinOp::And, e_binop(e_var("_argCount"), BinOp::NotEq, e_int(1))),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str("PDOStatement::setFetchMode() expects exactly 2 arguments for the fetch mode provided, 1 given")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_binop(e_var("_base"), BinOp::Eq, e_int(8)), BinOp::And, e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_int(classtype)), BinOp::Eq, e_int(0))), BinOp::And, e_binop(e_binop(e_var("_argCount"), BinOp::Lt, e_int(1)), BinOp::Or, e_binop(e_var("_argCount"), BinOp::Gt, e_int(2)))),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str("PDOStatement::setFetchMode() expects at least 2 arguments for the fetch mode provided, 1 given")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("_base"), BinOp::Eq, e_int(9)), BinOp::And, e_binop(e_var("_argCount"), BinOp::NotEq, e_int(1))),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str("PDOStatement::setFetchMode() expects exactly 2 arguments for the fetch mode provided, 1 given")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("_base"), BinOp::NotEq, e_int(7)), BinOp::And, e_binop(e_var("_base"), BinOp::NotEq, e_int(8))), BinOp::And, e_binop(e_var("_base"), BinOp::NotEq, e_int(9))), BinOp::And, e_binop(e_var("_argCount"), BinOp::NotEq, e_int(0))),
                            vec![
                                s_throw(e_new("ValueError", vec![e_binop(e_binop(e_str("PDOStatement::setFetchMode() expects exactly 1 argument for the fetch mode provided, "), BinOp::Concat, e_binop(e_int(1), BinOp::Add, e_var("_argCount"))), BinOp::Concat, e_str(" given"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_binop(e_var("_base"), BinOp::Eq, e_int(8)), BinOp::And, e_binop(e_var("_constructorArgs"), BinOp::StrictNotEq, e_null())), BinOp::And, e_not(e_call("is_array", vec![e_var("_constructorArgs")]))),
                            vec![
                                s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("PDOStatement::setFetchMode(): Argument #3 must be of type array, "), BinOp::Concat, e_method_call(e_this(), "argValueTypeName", vec![e_var("_constructorArgs")])), BinOp::Concat, e_str(" given"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "fetchMode", e_var("mode")),
                        s_prop_assign(e_this(), "fetchPropsLate", e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_int(props_late)), BinOp::NotEq, e_int(0))),
                        s_prop_assign(e_this(), "fetchCtorArgs", e_array(vec![])),
                        s_if(
                            e_binop(e_binop(e_var("_base"), BinOp::Eq, e_int(7)), BinOp::And, e_binop(e_var("classOrColumn"), BinOp::StrictNotEq, e_null())),
                            vec![
                                s_prop_assign(e_this(), "fetchColumn", e_cast(CastType::Int, e_var("classOrColumn"))),
                            ],
                            vec![
                            (e_binop(e_binop(e_binop(e_var("_base"), BinOp::Eq, e_int(8)), BinOp::Or, e_binop(e_var("_base"), BinOp::Eq, e_int(9))), BinOp::And, e_binop(e_var("classOrColumn"), BinOp::StrictNotEq, e_null())), vec![
                                s_prop_assign(e_this(), "fetchTarget", e_var("classOrColumn")),
                            ]),
                        ],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("_base"), BinOp::Eq, e_int(8)), BinOp::And, e_call("is_array", vec![e_var("_constructorArgs")])),
                            vec![
                                s_prop_assign(e_this(), "fetchCtorArgs", e_method_call(e_this(), "copyConstructorArgs", vec![e_var("_constructorArgs")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_bool(true)),
                    ],
                ]
                .concat(),
            ),
        )
        .method(pdostatement_bindvalue())
        .method(pdostatement_bindvaluewithdriveroption())
        .method(pdostatement_bindparam())
        .method(pdostatement_bindcolumn())
        .method(pdostatement_syncoutputparameters())
        .method(pdostatement_execute())
        .method(pdostatement_columnvalue())
        .method(pdostatement_columnname())
        .method(pdostatement_assigncolumns())
        .method(pdostatement_assigncolumnsfrom())
        .method(pdostatement_hydrateclass())
        .method(pdostatement_updateboundcolumns())
        .method(pdostatement_stepcursor())
        .method(
            method("fetch")
                .param_default("mode", TypeExpr::Int, e_int(0))
                .param_default("cursorOrientation", TypeExpr::Int, e_int(0))
                .param_default("cursorOffset", TypeExpr::Int, e_int(0))
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_not(e_this_prop("executed")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("mode"), BinOp::Eq, e_int(0)),
                        vec![
                            s_assign("mode", e_this_prop("fetchMode")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_base", e_binop(e_var("mode"), BinOp::BitAnd, e_int(base_mask))),
                    s_if(
                        e_binop(e_var("_base"), BinOp::Eq, e_int(10)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("Can only use PDO::FETCH_FUNC in PDOStatement::fetchAll()")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_base"), BinOp::Eq, e_int(6)),
                        vec![
                            s_assign("_boundRc", e_method_call(e_this(), "stepCursor", vec![e_var("cursorOrientation"), e_var("cursorOffset")])),
                            s_if(
                                e_binop(e_var("_boundRc"), BinOp::Lt, e_int(0)),
                                vec![
                                    s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_binop(e_var("_boundRc"), BinOp::NotEq, e_int(0))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        // 8.5 widened this guard from CLASSTYPE alone to all three class
                        // flags, and reworded it to match. Condition and message move together:
                        // the older wording names a rule the newer condition does not enforce.
                        if renumbered_flags {
                            e_binop(e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_binop(e_binop(e_int(classtype), BinOp::BitOr, e_int(props_late)), BinOp::BitOr, e_int(0x200))), BinOp::NotEq, e_int(0)), BinOp::And, e_binop(e_var("_base"), BinOp::NotEq, e_int(8)))
                        } else {
                            e_binop(e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_int(classtype)), BinOp::NotEq, e_int(0)), BinOp::And, e_binop(e_var("_base"), BinOp::NotEq, e_int(8)))
                        },
                        vec![
                            s_throw(e_new("ValueError", vec![e_str(if renumbered_flags {
                                "PDOStatement::fetch(): Argument #1 ($mode) cannot use PDO::FETCH_CLASSTYPE, PDO::FETCH_PROPS_LATE, or PDO::FETCH_SERIALIZE fetch flags with a fetch mode other than PDO::FETCH_CLASS"
                            } else {
                                "PDOStatement::fetch(): Argument #1 ($mode) must use PDO::FETCH_CLASSTYPE with PDO::FETCH_CLASS"
                            })])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_rc", e_method_call(e_this(), "stepCursor", vec![e_var("cursorOrientation"), e_var("cursorOffset")])),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Eq, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_count", e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")])),
                    s_if(
                        e_binop(e_var("_base"), BinOp::Eq, e_int(1)),
                        vec![
                            s_assign("_lazyValues", e_array(vec![])),
                            s_assign("_lazyNames", e_array(vec![])),
                            s_for(Some(s_assign("_li", e_int(0))), Some(e_binop(e_var("_li"), BinOp::Lt, e_var("_count"))), Some(s_expr(e_post_inc("_li"))), vec![
                                s_array_push("_lazyValues", e_method_call(e_this(), "columnValue", vec![e_var("_li")])),
                                s_array_push("_lazyNames", e_method_call(e_this(), "columnName", vec![e_var("_li")])),
                            ]),
                            s_if(
                                e_not(e_instance_of(e_this_prop("lazyRow"), "PDORow")),
                                vec![
                                    s_prop_assign(e_this(), "lazyRow", e_new("PDORow", vec![e_bool(true), e_this_prop("queryString")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_lazyRow", e_this_prop("lazyRow")),
                            s_if(
                                e_instance_of(e_var("_lazyRow"), "PDORow"),
                                vec![
                                    s_typed_assign(t_class("PDORow"), "_typedLazyRow", e_var("_lazyRow")),
                                    s_expr(e_method_call(e_var("_typedLazyRow"), "__elephcRefresh", vec![e_var("_lazyValues"), e_var("_lazyNames")])),
                                    s_return(e_var("_typedLazyRow")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_base"), BinOp::Eq, e_int(7)),
                        vec![
                            s_return(e_method_call(e_this(), "columnValue", vec![e_this_prop("fetchColumn")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_base"), BinOp::Eq, e_int(12)),
                        vec![
                            s_if(
                                e_binop(e_var("_count"), BinOp::NotEq, e_int(2)),
                                vec![
                                    s_expr(e_method_call(e_this(), "failCode", vec![e_str("HY000"), e_str("PDO::FETCH_KEY_PAIR fetch mode requires the result set to contain exactly 2 columns.")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_pk", e_method_call(e_this(), "columnValue", vec![e_int(0)])),
                            s_assign("_pv", e_method_call(e_this(), "columnValue", vec![e_int(1)])),
                            s_assign("_pair", e_array(vec![])),
                            s_array_assign("_pair", e_var("_pk"), e_var("_pv")),
                            s_return(e_var("_pair")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_base"), BinOp::Eq, e_int(5)),
                        vec![
                            s_return(e_method_call(e_this(), "assignColumns", vec![e_new("stdClass", vec![]), e_var("_count")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_base"), BinOp::Eq, e_int(8)),
                        vec![
                            s_if(
                                e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_int(classtype)), BinOp::NotEq, e_int(0)),
                                vec![
                                    s_assign("_ctName", e_cast(CastType::String, e_method_call(e_this(), "columnValue", vec![e_int(0)]))),
                                    s_return(e_method_call(e_this(), "hydrateClassOrStd", vec![e_var("_ctName"), e_int(1), e_var("_count")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_this_prop("fetchTarget"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_assign("_classTarget", e_this_prop("fetchTarget")),
                                    s_return(e_method_call(e_this(), "hydrateClass", vec![e_var("_classTarget"), e_int(0), e_var("_count")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_method_call(e_this(), "assignColumns", vec![e_new("stdClass", vec![]), e_var("_count")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_base"), BinOp::Eq, e_int(9)),
                        vec![
                            s_if(
                                e_binop(e_this_prop("fetchTarget"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_return(e_method_call(e_this(), "assignColumns", vec![e_this_prop("fetchTarget"), e_var("_count")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "failCode", vec![e_str("HY000"), e_str("No fetch-into object specified.")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_base"), BinOp::Eq, e_int(3)),
                        vec![
                            s_assign("_numRow", e_array(vec![])),
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_array_assign("_numRow", e_var("_i"), e_method_call(e_this(), "columnValue", vec![e_var("_i")])),
                            ]),
                            s_return(e_var("_numRow")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_base"), BinOp::Eq, e_int(2)),
                        vec![
                            s_assign("_assocRow", e_array(vec![])),
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_assign("_name", e_method_call(e_this(), "columnName", vec![e_var("_i")])),
                                s_array_assign("_assocRow", e_var("_name"), e_method_call(e_this(), "columnValue", vec![e_var("_i")])),
                            ]),
                            s_return(e_var("_assocRow")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_base"), BinOp::Eq, e_int(11)),
                        vec![
                            s_assign("_names", e_array(vec![])),
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_array_assign("_names", e_var("_i"), e_method_call(e_this(), "columnName", vec![e_var("_i")])),
                            ]),
                            s_assign("_namedRow", e_array(vec![])),
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_assign("_name", e_index(e_var("_names"), e_var("_i"))),
                                s_assign("_value", e_method_call(e_this(), "columnValue", vec![e_var("_i")])),
                                s_assign("_priorCount", e_int(0)),
                                s_for(Some(s_assign("_j", e_int(0))), Some(e_binop(e_var("_j"), BinOp::Lt, e_var("_i"))), Some(s_expr(e_post_inc("_j"))), vec![
                                    s_if(
                                        e_binop(e_index(e_var("_names"), e_var("_j")), BinOp::StrictEq, e_var("_name")),
                                        vec![
                                            s_assign("_priorCount", e_binop(e_var("_priorCount"), BinOp::Add, e_int(1))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ]),
                                s_if(
                                    e_binop(e_var("_priorCount"), BinOp::Eq, e_int(0)),
                                    vec![
                                        s_array_assign("_namedRow", e_var("_name"), e_var("_value")),
                                    ],
                                    vec![
                                    (e_binop(e_var("_priorCount"), BinOp::Eq, e_int(1)), vec![
                                        s_array_assign("_namedRow", e_var("_name"), e_array(vec![e_index(e_var("_namedRow"), e_var("_name")), e_var("_value")])),
                                    ]),
                                ],
                                    Some(vec![
                                    s_assign("_existing", e_index(e_var("_namedRow"), e_var("_name"))),
                                    s_array_push("_existing", e_var("_value")),
                                    s_array_assign("_namedRow", e_var("_name"), e_var("_existing")),
                                ]),
                                ),
                            ]),
                            s_return(e_var("_namedRow")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_bothRow", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_name", e_method_call(e_this(), "columnName", vec![e_var("_i")])),
                        s_assign("_value", e_method_call(e_this(), "columnValue", vec![e_var("_i")])),
                        s_array_assign("_bothRow", e_var("_name"), e_var("_value")),
                        s_array_assign("_bothRow", e_var("_i"), e_var("_value")),
                    ]),
                    s_return(e_var("_bothRow")),
                ]),
        )
        .method(
            method("fetchAll")
                .param_default("mode", TypeExpr::Int, e_int(0))
                .variadic("args", Some(t_mixed()))
                .returns(t_array())
            .body(
                [
                    vec![
                        s_assign("_fetchAllArgCount", e_call("count", vec![e_var("args")])),
                        s_assign("classOrObject", e_ternary(e_binop(e_var("_fetchAllArgCount"), BinOp::Gt, e_int(0)), e_index(e_var("args"), e_int(0)), e_null())),
                        s_assign("ctorArgs", e_ternary(e_binop(e_var("_fetchAllArgCount"), BinOp::Gt, e_int(1)), e_index(e_var("args"), e_int(1)), e_null())),
                        s_if(
                            e_binop(e_var("mode"), BinOp::Eq, e_int(0)),
                            vec![
                                s_assign("mode", e_this_prop("fetchMode")),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_base", e_binop(e_var("mode"), BinOp::BitAnd, e_int(base_mask))),
                        s_if(
                            e_binop(e_var("_base"), BinOp::Eq, e_int(1)),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str(if renumbered_flags {
                                "PDOStatement::fetchAll(): Argument #1 ($mode) PDO::FETCH_LAZY cannot be used with PDOStatement::fetchAll()"
                            } else {
                                "PDOStatement::fetchAll(): Argument #1 ($mode) cannot be PDO::FETCH_LAZY"
                            })])),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    // The FETCH_INTO rejection is 8.5 and up; earlier profiles let it
                    // through to fail elsewhere, so the statement is ABSENT there.
                    if renumbered_flags {
                        vec![
                            s_if(
                                e_binop(e_var("_base"), BinOp::Eq, e_int(9)),
                                vec![
                                    s_throw(e_new("ValueError", vec![e_str("PDOStatement::fetchAll(): Argument #1 ($mode) PDO::FETCH_INTO cannot be used with PDOStatement::fetchAll()")])),
                                ],
                                vec![],
                                None,
                            ),
                        ]
                    } else {
                        vec![]
                    },
                    vec![
                        s_if(
                            e_binop(e_var("_base"), BinOp::Eq, e_int(10)),
                            vec![
                                s_if(
                                    e_not(e_call("is_callable", vec![e_var("classOrObject")])),
                                    vec![
                                        s_throw(e_new("TypeError", vec![e_str("PDOStatement::fetchAll(): Argument #2 must be a valid callback")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("_fetchFunc", e_var("classOrObject")),
                                s_if(
                                    e_not(e_this_prop("executed")),
                                    vec![
                                        s_return(e_array(vec![])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("_funcRows", e_array(vec![])),
                                s_while(e_bool(true), vec![
                                    s_assign("_frc", e_method_call(e_this(), "stepCursor", vec![])),
                                    s_if(
                                        e_binop(e_var("_frc"), BinOp::Lt, e_int(0)),
                                        vec![
                                            s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                                            s_break(1),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_var("_frc"), BinOp::Eq, e_int(0)),
                                        vec![
                                            s_break(1),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("_funcArgs", e_array(vec![])),
                                    s_assign("_funcCount", e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")])),
                                    s_for(Some(s_assign("_fi", e_int(0))), Some(e_binop(e_var("_fi"), BinOp::Lt, e_var("_funcCount"))), Some(s_expr(e_post_inc("_fi"))), vec![
                                        s_array_push("_funcArgs", e_method_call(e_this(), "columnValue", vec![e_var("_fi")])),
                                    ]),
                                    s_array_push("_funcRows", e_call("call_user_func_array", vec![e_var("_fetchFunc"), e_var("_funcArgs")])),
                                ]),
                                s_return(e_var("_funcRows")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("_base"), BinOp::Eq, e_int(12)),
                            vec![
                                s_if(
                                    e_not(e_this_prop("executed")),
                                    vec![
                                        s_return(e_array(vec![])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("_pairs", e_array(vec![])),
                                s_while(e_bool(true), vec![
                                    s_assign("_krc", e_method_call(e_this(), "stepCursor", vec![])),
                                    s_if(
                                        e_binop(e_var("_krc"), BinOp::Lt, e_int(0)),
                                        vec![
                                            s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                                            s_break(1),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_var("_krc"), BinOp::Eq, e_int(0)),
                                        vec![
                                            s_break(1),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")]), BinOp::NotEq, e_int(2)),
                                        vec![
                                            s_expr(e_method_call(e_this(), "failCode", vec![e_str("HY000"), e_str("PDO::FETCH_KEY_PAIR fetch mode requires the result set to contain exactly 2 columns.")])),
                                            s_break(1),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("_kk", e_method_call(e_this(), "columnValue", vec![e_int(0)])),
                                    s_assign("_vv", e_method_call(e_this(), "columnValue", vec![e_int(1)])),
                                    s_array_assign("_pairs", e_var("_kk"), e_var("_vv")),
                                ]),
                                s_return(e_var("_pairs")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("_base"), BinOp::Eq, e_int(7)),
                            vec![
                                s_if(
                                    e_binop(e_var("classOrObject"), BinOp::StrictNotEq, e_null()),
                                    vec![
                                        s_prop_assign(e_this(), "fetchColumn", e_cast(CastType::Int, e_var("classOrObject"))),
                                    ],
                                    vec![
                                    (group_selected(), vec![
                                        s_prop_assign(e_this(), "fetchColumn", e_int(1)),
                                    ]),
                                ],
                                    Some(vec![
                                    s_prop_assign(e_this(), "fetchColumn", e_int(0)),
                                ]),
                                ),
                            ],
                            vec![
                            (e_binop(e_binop(e_binop(e_var("_base"), BinOp::Eq, e_int(8)), BinOp::Or, e_binop(e_var("_base"), BinOp::Eq, e_int(9))), BinOp::And, e_binop(e_var("classOrObject"), BinOp::StrictNotEq, e_null())), vec![
                                s_prop_assign(e_this(), "fetchTarget", e_var("classOrObject")),
                            ]),
                        ],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("_base"), BinOp::Eq, e_int(8)),
                            vec![
                                s_if(
                                    e_binop(e_var("_fetchAllArgCount"), BinOp::Gt, e_int(2)),
                                    vec![
                                        s_throw(e_new("ValueError", vec![e_binop(e_binop(e_str("PDOStatement::fetchAll() expects at most 3 arguments for the fetch mode provided, "), BinOp::Concat, e_binop(e_int(1), BinOp::Add, e_var("_fetchAllArgCount"))), BinOp::Concat, e_str(" given"))])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_binop(e_binop(e_var("ctorArgs"), BinOp::StrictNotEq, e_null()), BinOp::And, e_not(e_call("is_array", vec![e_var("ctorArgs")]))),
                                    vec![
                                        s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("PDOStatement::fetchAll(): Argument #3 must be of type array, "), BinOp::Concat, e_method_call(e_this(), "argValueTypeName", vec![e_var("ctorArgs")])), BinOp::Concat, e_str(" given"))])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_call("is_array", vec![e_var("ctorArgs")]),
                                    vec![
                                        s_prop_assign(e_this(), "fetchCtorArgs", e_method_call(e_this(), "copyConstructorArgs", vec![e_var("ctorArgs")])),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_prop_assign(e_this(), "fetchCtorArgs", e_array(vec![])),
                                ]),
                                ),
                                s_prop_assign(e_this(), "fetchPropsLate", e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_int(props_late)), BinOp::NotEq, e_int(0))),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            group_selected(),
                            vec![
                                s_if(
                                    e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_int(classtype)), BinOp::NotEq, e_int(0)),
                                    vec![
                                        s_throw(e_new("PDOException", vec![e_str("PDO::FETCH_CLASSTYPE is not supported with PDO::FETCH_GROUP or PDO::FETCH_UNIQUE")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_base"), BinOp::NotEq, e_int(2)), BinOp::And, e_binop(e_var("_base"), BinOp::NotEq, e_int(3))), BinOp::And, e_binop(e_var("_base"), BinOp::NotEq, e_int(4))), BinOp::And, e_binop(e_var("_base"), BinOp::NotEq, e_int(5))), BinOp::And, e_binop(e_var("_base"), BinOp::NotEq, e_int(7))), BinOp::And, e_binop(e_var("_base"), BinOp::NotEq, e_int(8))),
                                    vec![
                                        s_throw(e_new("PDOException", vec![e_str("PDO::FETCH_GROUP and PDO::FETCH_UNIQUE are not supported with this fetch mode")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_not(e_this_prop("executed")),
                                    vec![
                                        s_return(e_array(vec![])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("_unique", if renumbered_flags {
                                    e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_int(unique)), BinOp::NotEq, e_int(0))
                                } else {
                                    e_binop(e_binop(e_var("mode"), BinOp::BitAnd, e_int(unique)), BinOp::Eq, e_int(unique))
                                }),
                                s_assign("_present", e_array(vec![])),
                                s_assign("_groups", e_array(vec![])),
                                s_assign("_order", e_array(vec![])),
                                s_assign("_bn", e_int(0)),
                                s_assign("_out", e_array(vec![])),
                                s_while(e_bool(true), vec![
                                    s_assign("_grc", e_method_call(e_this(), "stepCursor", vec![])),
                                    s_if(
                                        e_binop(e_var("_grc"), BinOp::Lt, e_int(0)),
                                        vec![
                                            s_expr(e_method_call(e_this(), "fail", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                                            s_break(1),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_var("_grc"), BinOp::Eq, e_int(0)),
                                        vec![
                                            s_break(1),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("_gcount", e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")])),
                                    s_assign("_gkeyM", e_method_call(e_this(), "groupKey", vec![e_int(0)])),
                                    s_assign("_gkeyS", e_cast(CastType::String, e_var("_gkeyM"))),
                                    s_assign("_grow", e_method_call(e_this(), "groupRow", vec![e_var("_base"), e_var("_gcount")])),
                                    s_if(
                                        e_var("_unique"),
                                        vec![
                                            s_array_assign("_out", e_var("_gkeyM"), e_var("_grow")),
                                            s_continue(1),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("_before", e_call("count", vec![e_var("_present")])),
                                    s_array_assign("_present", e_var("_gkeyS"), e_int(1)),
                                    s_if(
                                        e_binop(e_call("count", vec![e_var("_present")]), BinOp::Gt, e_var("_before")),
                                        vec![
                                            s_array_assign("_groups", e_var("_gkeyS"), e_array(vec![e_var("_grow")])),
                                            s_array_assign("_order", e_var("_bn"), e_var("_gkeyM")),
                                            s_assign("_bn", e_binop(e_var("_bn"), BinOp::Add, e_int(1))),
                                        ],
                                        vec![],
                                        Some(vec![
                                        s_assign("_bucket", e_index(e_var("_groups"), e_var("_gkeyS"))),
                                        s_expr(e_call("unset", vec![e_index(e_var("_groups"), e_var("_gkeyS"))])),
                                        s_array_push("_bucket", e_var("_grow")),
                                        s_array_assign("_groups", e_var("_gkeyS"), e_var("_bucket")),
                                    ]),
                                    ),
                                ]),
                                s_if(
                                    e_not(e_var("_unique")),
                                    vec![
                                        s_for(Some(s_assign("_gi", e_int(0))), Some(e_binop(e_var("_gi"), BinOp::Lt, e_var("_bn"))), Some(s_expr(e_post_inc("_gi"))), vec![
                                            s_assign("_gkOut", e_index(e_var("_order"), e_var("_gi"))),
                                            s_array_assign("_out", e_var("_gkOut"), e_index(e_var("_groups"), e_cast(CastType::String, e_var("_gkOut")))),
                                        ]),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_return(e_var("_out")),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_rows", e_array(vec![])),
                        s_while(e_bool(true), vec![
                            s_assign("_row", e_method_call(e_this(), "fetch", vec![e_var("mode")])),
                            s_if(
                                e_binop(e_var("_row"), BinOp::StrictEq, e_bool(false)),
                                vec![
                                    s_break(1),
                                ],
                                vec![],
                                None,
                            ),
                            s_array_push("_rows", e_var("_row")),
                        ]),
                        s_return(e_var("_rows")),
                    ],
                ]
                .concat(),
            ),
        )
        .method(pdostatement_assigncolumnsfromorstd())
        .method(pdostatement_hydrateclassorstdwithoutconstructor())
        .method(pdostatement_hydrateclassorstd())
        .method(pdostatement_groupkey())
        .method(pdostatement_grouprow())
        .method(pdostatement_fetchcolumn())
        .method(pdostatement_closecursor())
        .method(pdostatement_fetchobject())
        .method(pdostatement_rowcount())
        .method(pdostatement_columncount())
        .method(pdostatement_getattribute())
        .method(pdostatement_setattribute())
        .method(pdostatement_nextrowset())
        .method(pdostatement_getcolumnmeta())
        .method(pdostatement_debugdumpparams())
        .method(pdostatement_getiterator())
        .method(pdostatement_destruct())
        .method(pdostatement_clone())
        .method(pdostatement_serialize())
        .method(pdostatement_sleep())
        .build()
}

/// `__ElephcPDOStatementIterator` — transcribed from the PHP form.
fn decl_class_elephcpdostatementiterator() -> Stmt {
    class("__ElephcPDOStatementIterator")
        .final_()
        .implements("Iterator")
        .private_prop("statement", t_class("PDOStatement"), None)
        .private_prop("row", t_mixed(), None)
        .private_prop("position", TypeExpr::Int, None)
        .method(elephcpdostatementiterator_construct())
        .method(elephcpdostatementiterator_rewind())
        .method(elephcpdostatementiterator_valid())
        .method(elephcpdostatementiterator_current())
        .method(elephcpdostatementiterator_key())
        .method(elephcpdostatementiterator_next())
        .build()
}

/// `bootstrap 1` — transcribed from the PHP form.
fn decl_stmt_bootstrap_1(php_version: PhpVersion, drivers: OptionalDrivers) -> Stmt {
    s_namespace("Pdo", [
        vec![
        ],
        // Each optional driver adds its own namespaced subclass.
        if drivers.dblib {
            vec![
                class("Dblib")
                    .extends("\\PDO")
                    .constant("ATTR_CONNECTION_TIMEOUT", e_int(1000))
                    .constant("ATTR_QUERY_TIMEOUT", e_int(1001))
                    .constant("ATTR_STRINGIFY_UNIQUEIDENTIFIER", e_int(1002))
                    .constant("ATTR_VERSION", e_int(1003))
                    .constant("ATTR_TDS_VERSION", e_int(1004))
                    .constant("ATTR_SKIP_EMPTY_ROWSETS", e_int(1005))
                    .constant("ATTR_DATETIME_CONVERT", e_int(1006))
                    .method(stmt_bootstrap_1_construct_4())
                    .build(),
            ]
        } else {
            vec![]
        },
        if drivers.firebird {
            vec![
                class("Firebird")
                    .extends("\\PDO")
                    .constant("ATTR_DATE_FORMAT", e_int(1000))
                    .constant("ATTR_TIME_FORMAT", e_int(1001))
                    .constant("ATTR_TIMESTAMP_FORMAT", e_int(1002))
                    .constant("TRANSACTION_ISOLATION_LEVEL", e_int(1003))
                    .constant("READ_COMMITTED", e_int(1004))
                    .constant("REPEATABLE_READ", e_int(1005))
                    .constant("SERIALIZABLE", e_int(1006))
                    .constant("WRITABLE_TRANSACTION", e_int(1007))
                    .method(stmt_bootstrap_1_construct_3())
                    .method(stmt_bootstrap_1_getapiversion())
                    .build(),
            ]
        } else {
            vec![]
        },
        if drivers.odbc {
            vec![
                class("Odbc")
                    .extends("\\PDO")
                    .constant("ATTR_USE_CURSOR_LIBRARY", e_int(1000))
                    .constant("ATTR_ASSUME_UTF8", e_int(1001))
                    .constant("SQL_USE_IF_NEEDED", e_int(0))
                    .constant("SQL_USE_ODBC", e_int(1))
                    .constant("SQL_USE_DRIVER", e_int(2))
                    .method(stmt_bootstrap_1_construct_2())
                    .build(),
            ]
        } else {
            vec![]
        },
        if drivers.ibm {
            vec![
                class("Ibm")
                    .extends("\\PDO")
                    .constant("ATTR_INFO_USERID", e_int(1281))
                    .constant("ATTR_INFO_ACCTSTR", e_int(1282))
                    .constant("ATTR_INFO_APPLNAME", e_int(1283))
                    .constant("ATTR_INFO_WRKSTNNAME", e_int(1284))
                    .constant("ATTR_USE_TRUSTED_CONTEXT", e_int(2561))
                    .constant("ATTR_TRUSTED_CONTEXT_USERID", e_int(2562))
                    .constant("ATTR_TRUSTED_CONTEXT_PASSWORD", e_int(2563))
                    .method(stmt_bootstrap_1_construct())
                    .build(),
            ]
        } else {
            vec![]
        },
        vec![
            class("Sqlite")
                .extends("\\PDO")
                .constant("DETERMINISTIC", e_int(2048))
                .constant("OPEN_READONLY", e_int(1))
                .constant("OPEN_READWRITE", e_int(2))
                .constant("OPEN_CREATE", e_int(4))
                .constant("ATTR_OPEN_FLAGS", e_int(1000))
                .constant("ATTR_READONLY_STATEMENT", e_int(1001))
                .constant("ATTR_EXTENDED_RESULT_CODES", e_int(1002))
                // The busy/explain/authorizer surface arrived with PHP 8.5; 8.4 has none of it.
                .when(php_version >= PhpVersion::Php85, |class| {
                    class.constant("ATTR_BUSY_STATEMENT", e_int(1003))
                    .constant("ATTR_EXPLAIN_STATEMENT", e_int(1004))
                    .constant("ATTR_TRANSACTION_MODE", e_int(1005))
                    .constant("TRANSACTION_MODE_DEFERRED", e_int(0))
                    .constant("TRANSACTION_MODE_IMMEDIATE", e_int(1))
                    .constant("TRANSACTION_MODE_EXCLUSIVE", e_int(2))
                    .constant("EXPLAIN_MODE_PREPARED", e_int(0))
                    .constant("EXPLAIN_MODE_EXPLAIN", e_int(1))
                    .constant("EXPLAIN_MODE_EXPLAIN_QUERY_PLAN", e_int(2))
                    .constant("OK", e_int(0))
                    .constant("DENY", e_int(1))
                    .constant("IGNORE", e_int(2))
                })
                .private_untyped_prop("authorizerCallback", None)
                .method(
                    method("__construct")
                        .param("dsn", TypeExpr::Str)
                        .param_default("username", t_nullable(TypeExpr::Str), e_null())
                        .param_default("password", t_nullable(TypeExpr::Str), e_null())
                        .when(php_version >= PhpVersion::Php82, |method| {
                            method.param_attr("\\SensitiveParameter")
                        })
                        .param_default("options", t_nullable(t_array()), e_null())
                        .body(vec![
                            s_assign("_operation", e_binop(e_call("get_class", vec![e_this()]), BinOp::Concat, e_str("::__construct"))),
                            s_assign("_sqliteDsn", e_self_call("resolveDsnAlias", vec![e_var("dsn"), e_var("_operation")])),
                            s_assign("_sqliteDsn", e_self_call("resolveDsnUri", vec![e_var("_sqliteDsn"), e_var("_operation")])),
                            s_expr(e_method_call(e_this(), "checkDriverSubclassDsn", vec![e_var("_sqliteDsn"), e_str("Pdo\\Sqlite"), e_str("sqlite")])),
                            s_expr(e_parent_call("__construct", vec![e_var("_sqliteDsn"), e_var("username"), e_var("password"), e_var("options")])),
                            s_prop_assign(e_this(), "authorizerCallback", closure()
                                .body(vec![
                                    s_return(e_int(0)),
                                ])
                                .build()),
                        ]),
                )
                .method(stmt_bootstrap_1_loadextension())
                .method(stmt_bootstrap_1_openblob())
                .method(stmt_bootstrap_1_createcollation())
                .method(stmt_bootstrap_1_createfunction())
                .method(stmt_bootstrap_1_createaggregate())
                // `Pdo\Sqlite::setAuthorizer()` is PHP 8.5 and up; 8.4 has no such method.
                .when(php_version >= PhpVersion::Php85, |class| {
                    class.method(
                        method("setAuthorizer")
                            .param("callback", t_nullable(t_class("callable")))
                            .returns(TypeExpr::Void)
                            .body(vec![
                                s_if(
                                    e_binop(e_var("callback"), BinOp::StrictEq, e_null()),
                                    vec![
                                        s_expr(e_call("\\elephc_pdo_clear_authorizer", vec![e_method_call(e_this(), "connectionId", vec![])])),
                                        s_prop_assign(e_this(), "authorizerCallback", closure()
                                            .body(vec![
                                                s_return(e_int(0)),
                                            ])
                                            .build()),
                                        s_return_void(),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_not(e_call("\\is_callable", vec![e_var("callback")])),
                                    vec![
                                        s_throw(e_new_fq("TypeError", vec![e_str("Pdo\\Sqlite::setAuthorizer(): Argument #1 ($callback) must be a valid callback or null")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("_normalized", e_call("\\__elephc_normalize_callable", vec![e_var("callback")])),
                                s_assign("_descriptor", e_call("\\__elephc_callable_ptr", vec![e_var("_normalized")])),
                                s_assign("_adapter", e_call("\\__elephc_pdo_adapter_addr", vec![e_int(1)])),
                                s_if(
                                    e_binop(e_call("\\elephc_pdo_set_authorizer", vec![e_method_call(e_this(), "connectionId", vec![]), e_var("_descriptor"), e_var("_adapter")]), BinOp::StrictNotEq, e_int(1)),
                                    vec![
                                        s_throw(e_new_fq("PDOException", vec![e_str("Failed to register SQLite authorizer")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_prop_assign(e_this(), "authorizerCallback", e_var("_normalized")),
                            ]),
                    )
                })
                .build(),
            class("Mysql")
                .extends("\\PDO")
                .constant("ATTR_USE_BUFFERED_QUERY", e_int(1000))
                .constant("ATTR_LOCAL_INFILE", e_int(1001))
                .constant("ATTR_INIT_COMMAND", e_int(1002))
                .constant("ATTR_COMPRESS", e_int(1003))
                .constant("ATTR_DIRECT_QUERY", e_int(1004))
                .constant("ATTR_FOUND_ROWS", e_int(1005))
                .constant("ATTR_IGNORE_SPACE", e_int(1006))
                .constant("ATTR_SSL_KEY", e_int(1007))
                .constant("ATTR_SSL_CERT", e_int(1008))
                .constant("ATTR_SSL_CA", e_int(1009))
                .constant("ATTR_SSL_CAPATH", e_int(1010))
                .constant("ATTR_SSL_CIPHER", e_int(1011))
                .constant("ATTR_SERVER_PUBLIC_KEY", e_int(1012))
                .constant("ATTR_MULTI_STATEMENTS", e_int(1013))
                .constant("ATTR_SSL_VERIFY_SERVER_CERT", e_int(1014))
                .constant("ATTR_LOCAL_INFILE_DIRECTORY", e_int(1015))
                .method(
                    method("__construct")
                        .param("dsn", TypeExpr::Str)
                        .param_default("username", t_nullable(TypeExpr::Str), e_null())
                        .param_default("password", t_nullable(TypeExpr::Str), e_null())
                        .when(php_version >= PhpVersion::Php82, |method| {
                            method.param_attr("\\SensitiveParameter")
                        })
                        .param_default("options", t_nullable(t_array()), e_null())
                        .body(vec![
                            s_assign("_operation", e_binop(e_call("get_class", vec![e_this()]), BinOp::Concat, e_str("::__construct"))),
                            s_assign("_mysqlDsn", e_self_call("resolveDsnAlias", vec![e_var("dsn"), e_var("_operation")])),
                            s_assign("_mysqlDsn", e_self_call("resolveDsnUri", vec![e_var("_mysqlDsn"), e_var("_operation")])),
                            s_expr(e_method_call(e_this(), "checkDriverSubclassDsn", vec![e_var("_mysqlDsn"), e_str("Pdo\\Mysql"), e_str("mysql")])),
                            s_expr(e_parent_call("__construct", vec![e_var("_mysqlDsn"), e_var("username"), e_var("password"), e_var("options")])),
                        ]),
                )
                .method(stmt_bootstrap_1_getwarningcount())
                .build(),
            class("Pgsql")
                .extends("\\PDO")
                .constant("ATTR_DISABLE_PREPARES", e_int(1000))
                .constant("ATTR_RESULT_MEMORY_SIZE", e_int(1001))
                // PHP 8.5 marked the Pgsql transaction-state constants `#[\Deprecated]` — they
                // report a state the driver never updates. The values stay; the annotation is
                // what reference PHP warns from, so it has to appear on exactly those profiles.
                .when(php_version >= PhpVersion::Php85, |class| {
                    let deprecated =
                        || vec![attr("\\Deprecated", vec![e_str("as it has no effect")])];
                    class
                        .constant_attributed("TRANSACTION_IDLE", e_int(0), deprecated())
                        .constant_attributed("TRANSACTION_ACTIVE", e_int(1), deprecated())
                        .constant_attributed("TRANSACTION_INTRANS", e_int(2), deprecated())
                        .constant_attributed("TRANSACTION_INERROR", e_int(3), deprecated())
                        .constant_attributed("TRANSACTION_UNKNOWN", e_int(4), deprecated())
                })
                .when(php_version < PhpVersion::Php85, |class| {
                    class
                        .constant("TRANSACTION_IDLE", e_int(0))
                        .constant("TRANSACTION_ACTIVE", e_int(1))
                        .constant("TRANSACTION_INTRANS", e_int(2))
                        .constant("TRANSACTION_INERROR", e_int(3))
                        .constant("TRANSACTION_UNKNOWN", e_int(4))
                })
                .private_untyped_prop("noticeCallback", None)
                .method(
                    method("__construct")
                        .param("dsn", TypeExpr::Str)
                        .param_default("username", t_nullable(TypeExpr::Str), e_null())
                        .param_default("password", t_nullable(TypeExpr::Str), e_null())
                        .when(php_version >= PhpVersion::Php82, |method| {
                            method.param_attr("\\SensitiveParameter")
                        })
                        .param_default("options", t_nullable(t_array()), e_null())
                        .body(vec![
                            s_assign("_operation", e_binop(e_call("get_class", vec![e_this()]), BinOp::Concat, e_str("::__construct"))),
                            s_assign("_pgsqlDsn", e_self_call("resolveDsnAlias", vec![e_var("dsn"), e_var("_operation")])),
                            s_assign("_pgsqlDsn", e_self_call("resolveDsnUri", vec![e_var("_pgsqlDsn"), e_var("_operation")])),
                            s_expr(e_method_call(e_this(), "checkDriverSubclassDsn", vec![e_var("_pgsqlDsn"), e_str("Pdo\\Pgsql"), e_str("pgsql")])),
                            s_expr(e_parent_call("__construct", vec![e_var("_pgsqlDsn"), e_var("username"), e_var("password"), e_var("options")])),
                            s_prop_assign(e_this(), "noticeCallback", closure()
                                .param_untyped("_message")
                                .body(vec![])
                                .build()),
                        ]),
                )
                .method(stmt_bootstrap_1_setnoticecallback())
                .method(stmt_bootstrap_1_elephcdrainpgsqlnotices())
                .method(stmt_bootstrap_1_exec())
                .method(stmt_bootstrap_1_query())
                .method(stmt_bootstrap_1_escapeidentifier())
                .method(stmt_bootstrap_1_getpid())
                .method(stmt_bootstrap_1_lobcreate())
                .method(stmt_bootstrap_1_lobunlink())
                .method(stmt_bootstrap_1_lobopen())
                .method(stmt_bootstrap_1_copyoptions())
                .method(stmt_bootstrap_1_copytarget())
                .method(stmt_bootstrap_1_copyfromarray())
                .method(stmt_bootstrap_1_copyfromfile())
                .method(stmt_bootstrap_1_copytoarray())
                .method(stmt_bootstrap_1_copytofile())
                .method(stmt_bootstrap_1_getnotify())
                .build()
        ],
    ]
    .concat())
}

/// `PDO_ODBC_TYPE` — the ODBC driver manager this build links against.
///
/// Declared only when the ODBC driver is enabled: reference PHP defines it from the manager it
/// was built with, and a program can test for its existence to tell whether ODBC is there at
/// all. Defining it unconditionally would answer yes for a build with no ODBC.
fn decl_const_pdo_odbc_type() -> Stmt {
    s_const("PDO_ODBC_TYPE", e_str("unixODBC"))
}

/// Builds the whole surface for one PHP profile, one declaration per helper above.
///
/// Four of the 188 helpers take the profile; the rest are identical on every one of
/// them. That ratio is measured rather than assumed — see the module docs.
pub(crate) fn pdo_declarations(php_version: PhpVersion, drivers: OptionalDrivers) -> Program {
    internal_declarations(move || {
        let mut declarations = vec![
            decl_extern_elephc_pdo_available_driver_count(),
            decl_extern_elephc_pdo_available_driver_name(),
            decl_extern_elephc_pdo_ini_dsn_defined(),
            decl_extern_elephc_pdo_ini_dsn_value(),
            decl_extern_elephc_pdo_open(),
            decl_extern_elephc_pdo_open_persistent(),
            decl_extern_elephc_pdo_last_open_error(),
            decl_extern_elephc_pdo_last_open_sqlstate(),
            decl_extern_elephc_pdo_last_open_native_code(),
            decl_extern_elephc_pdo_close(),
            decl_extern_elephc_pdo_release(),
            decl_extern_elephc_pdo_clear_callbacks(),
            decl_extern_elephc_pdo_exec(),
            decl_extern_elephc_pdo_last_insert_id(),
            decl_extern_elephc_pdo_changes(),
            decl_extern_elephc_pdo_begin(),
            decl_extern_elephc_pdo_commit(),
            decl_extern_elephc_pdo_rollback(),
            decl_extern_elephc_pdo_errcode(),
            decl_extern_elephc_pdo_errmsg(),
            decl_extern_elephc_pdo_prepare(),
            decl_extern_elephc_pdo_bind_parameter_index(),
            decl_extern_elephc_pdo_bind_int(),
            decl_extern_elephc_pdo_bind_double(),
            decl_extern_elephc_pdo_bind_text(),
            decl_extern_elephc_pdo_bind_text_national(),
            decl_extern_elephc_pdo_bind_blob(),
            decl_extern_elephc_pdo_bind_null(),
            decl_extern_elephc_pdo_bind_output(),
            decl_extern_elephc_pdo_output_data(),
            decl_extern_elephc_pdo_output_is_lob(),
            decl_extern_elephc_pdo_output_is_numeric(),
            decl_extern_elephc_pdo_reset(),
            decl_extern_elephc_pdo_clear_bindings(),
            decl_extern_elephc_pdo_step(),
            decl_extern_elephc_pdo_step_oriented(),
            decl_extern_elephc_pdo_result_memory_size(),
            decl_extern_elephc_pdo_next_rowset(),
            decl_extern_elephc_pdo_column_count(),
            decl_extern_elephc_pdo_column_name(),
            decl_extern_elephc_pdo_column_type(),
            decl_extern_elephc_pdo_column_int(),
            decl_extern_elephc_pdo_column_double(),
            decl_extern_elephc_pdo_column_data_len(),
            decl_extern_elephc_pdo_column_data_ptr(),
            decl_extern_elephc_pdo_column_data_byte(),
            decl_extern_elephc_pdo_finalize(),
            decl_extern_elephc_pdo_driver_name(),
            decl_extern_elephc_pdo_sqlstate(),
            decl_extern_elephc_pdo_stmt_errcode(),
            decl_extern_elephc_pdo_stmt_errmsg(),
            decl_extern_elephc_pdo_stmt_sqlstate(),
            decl_extern_elephc_pdo_stmt_sent_sql(),
            decl_extern_elephc_pdo_bind_bool(),
            decl_extern_elephc_pdo_set_busy_timeout(),
            decl_extern_elephc_pdo_dblib_set_attribute(),
            decl_extern_elephc_pdo_dblib_attribute_bool(),
            decl_extern_elephc_pdo_dblib_os_errcode(),
            decl_extern_elephc_pdo_dblib_severity(),
            decl_extern_elephc_pdo_dblib_os_errmsg(),
            decl_extern_elephc_pdo_dblib_stmt_os_errcode(),
            decl_extern_elephc_pdo_dblib_stmt_severity(),
            decl_extern_elephc_pdo_dblib_stmt_os_errmsg(),
            decl_extern_elephc_pdo_firebird_set_attribute_int(),
            decl_extern_elephc_pdo_firebird_set_attribute_text(),
            decl_extern_elephc_pdo_firebird_attribute_int(),
            decl_extern_elephc_pdo_firebird_attribute_text(),
            decl_extern_elephc_pdo_firebird_column_pdo_type(),
            decl_extern_elephc_pdo_firebird_stmt_set_cursor_name(),
            decl_extern_elephc_pdo_firebird_stmt_cursor_name(),
            decl_extern_elephc_pdo_odbc_set_attribute(),
            decl_extern_elephc_pdo_odbc_attribute(),
            decl_extern_elephc_pdo_odbc_stmt_set_cursor_name(),
            decl_extern_elephc_pdo_odbc_stmt_cursor_name(),
            decl_extern_elephc_pdo_odbc_stmt_set_assume_utf8(),
            decl_extern_elephc_pdo_odbc_stmt_assume_utf8(),
            decl_extern_elephc_pdo_oci_set_attribute_int(),
            decl_extern_elephc_pdo_oci_set_attribute_text(),
            decl_extern_elephc_pdo_oci_attribute_int(),
            decl_extern_elephc_pdo_oci_column_pdo_type(),
            decl_extern_elephc_pdo_oci_column_scale(),
            decl_extern_elephc_pdo_oci_column_flags(),
            decl_extern_elephc_pdo_informix_column_scale(),
            decl_extern_elephc_pdo_informix_column_pdo_type(),
            decl_extern_elephc_pdo_ibm_set_attribute_text(),
            decl_extern_elephc_pdo_ibm_attribute_text(),
            decl_extern_elephc_pdo_ibm_attribute_int(),
            decl_extern_elephc_pdo_ibm_column_scale(),
            decl_extern_elephc_pdo_ibm_column_pdo_type(),
            decl_extern_elephc_pdo_sqlsrv_stmt_set_attribute(),
            decl_extern_elephc_pdo_sqlsrv_stmt_configure(),
            decl_extern_elephc_pdo_sqlsrv_stmt_attribute(),
            decl_extern_elephc_pdo_sqlsrv_column_is_datetime(),
            decl_extern_elephc_pdo_sqlsrv_info(),
            decl_extern_elephc_pdo_sqlsrv_classification_pair_count(),
            decl_extern_elephc_pdo_sqlsrv_classification_text(),
            decl_extern_elephc_pdo_sqlsrv_classification_pair_rank(),
            decl_extern_elephc_pdo_sqlsrv_classification_query_rank(),
            decl_extern_elephc_pdo_cubrid_set_attribute(),
            decl_extern_elephc_pdo_cubrid_attribute(),
            decl_extern_elephc_pdo_cubrid_quote(),
            decl_extern_elephc_pdo_cubrid_bind_typed(),
            decl_extern_elephc_pdo_cubrid_schema(),
            decl_extern_elephc_pdo_cubrid_column_scale(),
            decl_extern_elephc_pdo_cubrid_column_default(),
            decl_extern_elephc_pdo_server_version(),
            decl_extern_elephc_pdo_client_version(),
            decl_extern_elephc_pdo_server_info(),
            decl_extern_elephc_pdo_connection_status(),
            decl_extern_elephc_pdo_last_insert_id_text(),
            decl_extern_elephc_pdo_backend_pid(),
            decl_extern_elephc_pdo_warning_count(),
            decl_extern_elephc_pdo_lob_create(),
            decl_extern_elephc_pdo_lob_unlink(),
            decl_extern_elephc_pdo_copy_in(),
            decl_extern_elephc_pdo_copy_out(),
            decl_extern_elephc_pdo_column_decltype(),
            decl_extern_elephc_pdo_load_extension(),
            decl_extern_elephc_pdo_get_notify(),
            decl_extern_elephc_pdo_blob_read(),
            decl_extern_elephc_pdo_lob_get(),
            decl_extern_elephc_pdo_blob_byte(),
            decl_extern_elephc_pdo_blob_write(),
            decl_extern_elephc_pdo_lob_put(),
            decl_extern_elephc_pdo_lob_size(),
            decl_extern_elephc_pdo_lob_read_at(),
            decl_extern_elephc_pdo_lob_write_at(),
            decl_extern_elephc_pdo_blob_size(),
            decl_extern_elephc_pdo_blob_read_at(),
            decl_extern_elephc_pdo_blob_write_at(),
            decl_extern_elephc_pdo_create_collation(),
            decl_extern_elephc_pdo_create_function(),
            decl_extern_elephc_pdo_create_aggregate(),
            decl_extern_elephc_pdo_get_notice(),
            decl_extern_elephc_pdo_stmt_readonly(),
            decl_extern_elephc_pdo_no_backslash_escapes(),
            decl_extern_elephc_pdo_in_transaction(),
            decl_extern_elephc_pdo_set_autocommit(),
            decl_extern_elephc_pdo_autocommit(),
            decl_extern_elephc_pdo_set_fetch_table_names(),
            decl_extern_elephc_pdo_fetch_table_names(),
            decl_extern_elephc_pdo_set_buffered_query(),
            decl_extern_elephc_pdo_buffered_query(),
            decl_extern_elephc_pdo_set_prefetch(),
            decl_extern_elephc_pdo_stmt_set_prefetch(),
            decl_extern_elephc_pdo_stmt_enable_simple_streaming(),
            decl_extern_elephc_pdo_column_native_type(),
            decl_extern_elephc_pdo_column_type_oid(),
            decl_extern_elephc_pdo_column_table_name(),
            decl_extern_elephc_pdo_column_flags(),
            decl_extern_elephc_pdo_blob_data_ptr(),
            decl_extern_elephc_pdo_set_extended_result_codes(),
            decl_extern_elephc_pdo_set_transaction_mode(),
            decl_extern_elephc_pdo_transaction_mode(),
            decl_extern_elephc_pdo_stmt_busy(),
            decl_extern_elephc_pdo_stmt_explain_mode(),
            decl_extern_elephc_pdo_stmt_set_explain_mode(),
            decl_extern_elephc_pdo_set_authorizer(),
            decl_extern_elephc_pdo_clear_authorizer(),
            decl_extern_elephc_pdo_take_authorizer_error(),
            decl_extern_elephc_pdo_column_table_oid(),
            decl_extern_elephc_pdo_column_len(),
            decl_extern_elephc_pdo_column_precision(),
            decl_extern_elephc_pdo_dblib_column_native_type_id(),
            decl_extern_elephc_pdo_dblib_column_user_type_id(),
            decl_extern_elephc_pdo_dblib_column_scale(),
            decl_extern_elephc_pdo_dblib_column_source(),
            decl_fn_pdo_drivers(php_version, drivers),
            decl_fn_elephc_pdo_sqlstate_description_0(),
            decl_fn_elephc_pdo_sqlstate_description_2(),
            decl_fn_elephc_pdo_sqlstate_description_3(),
            decl_fn_elephc_pdo_sqlstate_description_4(),
            decl_fn_elephc_pdo_sqlstate_description_5(),
            decl_fn_elephc_pdo_sqlstate_description_f(),
            decl_fn_elephc_pdo_sqlstate_description_h(),
            decl_fn_elephc_pdo_sqlstate_description_i(),
            decl_fn_elephc_pdo_sqlstate_description_p(),
            decl_fn_elephc_pdo_sqlstate_description_x(),
            decl_fn_elephc_pdo_sqlstate_description(),
            decl_fn_elephc_pdo_impl_error_message(),
            decl_class_pdoexception(),
            decl_class_elephcpdosqliteblobstream(),
            decl_class_elephcpdopgsqllobstream(),
        ];
        // `PDO_ODBC_TYPE` exists only with ODBC — a program tests for it to tell whether
        // the driver is there at all, so defining it always would answer yes for a build
        // that has none.
        if drivers.odbc {
            declarations.push(decl_const_pdo_odbc_type());
        }
        declarations.extend([
            decl_class_pdo(php_version, drivers),
            decl_class_pdorow(php_version),
            decl_class_pdostatement(php_version),
            decl_class_elephcpdostatementiterator(),
        ]);
        // The namespaced driver subclasses (`Pdo\Sqlite` and friends) arrived in PHP
        // 8.4; before that the block is ABSENT, not present and empty.
        if php_version >= PhpVersion::Php84 {
            declarations.push(decl_stmt_bootstrap_1(php_version, drivers));
        }
        declarations
    })
}
