//! Purpose:
//! Builds the `mysqli` connection class as AST: DSN/TLS construction over
//! `elephc_pdo_open_persistent`, error bookkeeping, `mysqli_report` dispatch,
//! escaping, charset, transactions, change_user / kill / refresh, MYSQLI_ASYNC
//! queries with `poll()` / `reap_async_query()`, warnings, and statistics.
//!
//! Called from:
//! - `crate::mysqli_prelude::build::mysqli_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from `mysqli_prelude::connection::SRC` (`synthetic_class::transcribe`);
//!   the oracle `built_declarations_match_the_php_for_every_version` compares the
//!   built class against that PHP for every profile. The version gates are
//!   conditionals here: `$reportMode`'s default (0 under 8.0, 3 from 8.1),
//!   `execute_query` (8.2+), and `quote_string` (8.6+).
//! - Method-local variables use the `$_` prefix (same checker clash rule as the
//!   PDO prelude); the PHP fragment carries the explanatory comments.

use crate::php_version::PhpVersion;
use crate::parser::ast::{BinOp, CastType, TypeExpr, Stmt};
use crate::synthetic_class::{
    class,
    e_array,
    e_array_assoc,
    e_assign,
    e_binop,
    e_bool,
    e_call,
    e_cast,
    e_const,
    e_dyn_prop,
    e_index,
    e_instance_of,
    e_int,
    e_method_call,
    e_neg,
    e_new,
    e_not,
    e_null,
    e_null_coalesce,
    e_post_inc,
    e_prop,
    e_self_call,
    e_static_call,
    e_static_prop,
    e_str,
    e_ternary,
    e_this,
    e_this_prop,
    e_var,
    method,
    s_array_assign,
    s_array_push,
    s_assign,
    s_break,
    s_continue,
    s_expr,
    s_for,
    s_foreach,
    s_if,
    s_prop_array_assign,
    s_prop_array_push,
    s_prop_assign,
    s_return,
    s_return_void,
    s_static_prop_assign,
    s_throw,
    s_while,
    t_array,
    t_class,
    t_mixed,
    t_nullable,
    t_union,
};

/// `mysqli` — transcribed from the PHP form.
pub(super) fn decl_class_mysqli(php_version: PhpVersion) -> Stmt {
    class("mysqli")
        .private_prop("conn", TypeExpr::Int, Some(e_neg(e_int(1))))
        .static_prop("reportMode", TypeExpr::Int, Some(e_int(if php_version >= PhpVersion::Php81 { 3 } else { 0 })))
        .static_prop("lastConnectErrno", TypeExpr::Int, Some(e_int(0)))
        .static_prop("lastConnectError", TypeExpr::Str, Some(e_str("")))
        .static_prop("clientStats", t_array(), Some(e_array(vec![])))
        .static_prop("openLinks", TypeExpr::Int, Some(e_int(0)))
        .static_prop("openPersistentLinks", TypeExpr::Int, Some(e_int(0)))
        .static_prop("cachedPlinks", TypeExpr::Int, Some(e_int(0)))
        .static_prop("seenPlinks", t_array(), Some(e_array(vec![])))
        .static_prop("plinkStats", t_array(), Some(e_array(vec![])))
        .prop("affected_rows", t_union(vec![TypeExpr::Int, TypeExpr::Str]), Some(e_int(0)))
        .prop("client_info", TypeExpr::Str, Some(e_str("")))
        .prop("client_version", TypeExpr::Int, Some(e_int(0)))
        .prop("connect_errno", TypeExpr::Int, Some(e_int(0)))
        .prop("connect_error", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("errno", TypeExpr::Int, Some(e_int(0)))
        .prop("error", TypeExpr::Str, Some(e_str("")))
        .prop("error_list", t_array(), Some(e_array(vec![])))
        .prop("field_count", TypeExpr::Int, Some(e_int(0)))
        .prop("host_info", TypeExpr::Str, Some(e_str("")))
        .prop("info", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("insert_id", t_union(vec![TypeExpr::Int, TypeExpr::Str]), Some(e_int(0)))
        .prop("server_info", TypeExpr::Str, Some(e_str("")))
        .prop("server_version", TypeExpr::Int, Some(e_int(0)))
        .prop("sqlstate", TypeExpr::Str, Some(e_str("00000")))
        .prop("protocol_version", TypeExpr::Int, Some(e_int(10)))
        .prop("thread_id", TypeExpr::Int, Some(e_int(0)))
        .prop("warning_count", TypeExpr::Int, Some(e_int(0)))
        .private_prop("optConnectTimeout", TypeExpr::Int, Some(e_int(0)))
        .private_prop("optInitCommands", t_array(), Some(e_array(vec![])))
        .private_prop("optCharsetName", TypeExpr::Str, Some(e_str("")))
        .private_prop("optLocalInfile", TypeExpr::Int, Some(e_int(0)))
        .private_prop("optLocalInfileDir", TypeExpr::Str, Some(e_str("")))
        .private_prop("optServerPublicKey", TypeExpr::Str, Some(e_str("")))
        .private_prop("optSslVerify", TypeExpr::Int, Some(e_neg(e_int(1))))
        .private_prop("optIntAndFloatNative", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("optReadTimeout", TypeExpr::Int, Some(e_int(0)))
        .private_prop("sslRequested", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("sslKey", TypeExpr::Str, Some(e_str("")))
        .private_prop("sslCert", TypeExpr::Str, Some(e_str("")))
        .private_prop("sslCa", TypeExpr::Str, Some(e_str("")))
        .private_prop("sslCapath", TypeExpr::Str, Some(e_str("")))
        .private_prop("sslCipher", TypeExpr::Str, Some(e_str("")))
        .private_prop("persistent", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("linkStatus", TypeExpr::Int, Some(e_int(0)))
        .private_prop("handleAllocated", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("handlePersistent", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("linkGone", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("liveStmt", TypeExpr::Int, Some(e_neg(e_int(1))))
        .private_prop("liveStep", TypeExpr::Int, Some(e_int(0)))
        .private_prop("liveSetPending", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("streaming", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("multiMore", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("asyncPending", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("busyStatement", TypeExpr::Int, Some(e_neg(e_int(1))))
        .private_prop("multiStatementsOn", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("asyncUnbuffered", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("connStats", t_array(), Some(e_array(vec![])))
        .method(
            method("__construct")
                .param_default("hostname", t_nullable(TypeExpr::Str), e_null())
                .param_default("username", t_nullable(TypeExpr::Str), e_null())
                .param_default("password", t_nullable(TypeExpr::Str), e_null())
                .param_default("database", t_nullable(TypeExpr::Str), e_null())
                .param_default("port", t_nullable(TypeExpr::Int), e_null())
                .param_default("socket", t_nullable(TypeExpr::Str), e_null())
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("hostname"), BinOp::StrictEq, e_null()), BinOp::And, e_binop(e_var("username"), BinOp::StrictEq, e_null())), BinOp::And, e_binop(e_var("password"), BinOp::StrictEq, e_null())), BinOp::And, e_binop(e_var("database"), BinOp::StrictEq, e_null())), BinOp::And, e_binop(e_var("port"), BinOp::StrictEq, e_null())), BinOp::And, e_binop(e_var("socket"), BinOp::StrictEq, e_null())),
                        vec![
                            s_prop_assign(e_this(), "handleAllocated", e_bool(true)),
                            s_prop_assign(e_this(), "handlePersistent", e_bool(true)),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "connectLink", vec![e_var("hostname"), e_var("username"), e_var("password"), e_var("database"), e_var("port"), e_var("socket"), e_int(0), e_bool(false)])),
                ]),
        )
        .method(
            method("real_connect")
                .param_default("hostname", t_nullable(TypeExpr::Str), e_null())
                .param_default("username", t_nullable(TypeExpr::Str), e_null())
                .param_default("password", t_nullable(TypeExpr::Str), e_null())
                .param_default("database", t_nullable(TypeExpr::Str), e_null())
                .param_default("port", t_nullable(TypeExpr::Int), e_null())
                .param_default("socket", t_nullable(TypeExpr::Str), e_null())
                .param_default("flags", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "connectLink", vec![e_var("hostname"), e_var("username"), e_var("password"), e_var("database"), e_var("port"), e_var("socket"), e_var("flags"), e_bool(true)])),
                ]),
        )
        .method(
            method("__elephcDetachHandle")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "handleAllocated", e_bool(false)),
                    s_prop_assign(e_this(), "handlePersistent", e_bool(false)),
                ]),
        )
        .method(
            method("__elephcConnectProcedural")
                .private()
                .param("hostname", t_nullable(TypeExpr::Str))
                .param("username", t_nullable(TypeExpr::Str))
                .param("password", t_nullable(TypeExpr::Str))
                .param("database", t_nullable(TypeExpr::Str))
                .param("port", t_nullable(TypeExpr::Int))
                .param("socket", t_nullable(TypeExpr::Str))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "connectLink", vec![e_var("hostname"), e_var("username"), e_var("password"), e_var("database"), e_var("port"), e_var("socket"), e_int(0), e_bool(false)])),
                ]),
        )
        .method(
            method("adminReply")
                .private()
                .param("okPacket", TypeExpr::Bool)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "affected_rows", e_neg(e_int(1))),
                    s_if(
                        e_var("okPacket"),
                        vec![
                            s_prop_assign(e_this(), "info", e_null()),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("requireNotClosed")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("linkStatus"), BinOp::Eq, e_int(2)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("mysqli object is already closed")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("connectLink")
                .private()
                .param("hostname", t_nullable(TypeExpr::Str))
                .param("username", t_nullable(TypeExpr::Str))
                .param("password", t_nullable(TypeExpr::Str))
                .param("database", t_nullable(TypeExpr::Str))
                .param("port", t_nullable(TypeExpr::Int))
                .param("socket", t_nullable(TypeExpr::Str))
                .param("flags", TypeExpr::Int)
                .param("isRealConnect", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("linkStatus"), BinOp::Eq, e_int(2)),
                        vec![
                            s_if(
                                e_var("isRealConnect"),
                                vec![
                                    s_throw(e_new("Error", vec![e_str("mysqli object is already closed")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "linkStatus", e_int(0)),
                            s_expr(e_method_call(e_this(), "resetOptions", vec![])),
                            s_prop_assign(e_this(), "connStats", e_array(vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_host", e_ternary(e_binop(e_var("hostname"), BinOp::StrictEq, e_null()), e_str("localhost"), e_var("hostname"))),
                    s_assign("_persistent", e_int(0)),
                    s_if(
                        e_binop(e_binop(e_call("strlen", vec![e_var("_host")]), BinOp::GtEq, e_int(2)), BinOp::And, e_binop(e_call("strtolower", vec![e_call("substr", vec![e_var("_host"), e_int(0), e_int(2)])]), BinOp::StrictEq, e_str("p:"))),
                        vec![
                            s_assign("_persistent", e_int(1)),
                            s_assign("_host", e_call("substr", vec![e_var("_host"), e_int(2)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("linkStatus"), BinOp::Eq, e_int(1)),
                        vec![
                            s_expr(e_method_call(e_this(), "closeLink", vec![e_bool(false)])),
                            s_expr(e_method_call(e_this(), "resetOptions", vec![])),
                            s_prop_assign(e_this(), "connStats", e_array(vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_this_prop("handleAllocated")),
                        vec![
                            s_prop_assign(e_this(), "handleAllocated", e_bool(true)),
                            s_prop_assign(e_this(), "handlePersistent", e_binop(e_var("_persistent"), BinOp::Eq, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_port", e_ternary(e_binop(e_var("port"), BinOp::StrictEq, e_null()), e_int(3306), e_var("port"))),
                    s_assign("_socket", e_ternary(e_binop(e_var("socket"), BinOp::StrictEq, e_null()), e_str(""), e_var("socket"))),
                    s_if(
                        e_binop(e_binop(e_binop(e_call("strpos", vec![e_var("_host"), e_str(";")]), BinOp::StrictNotEq, e_bool(false)), BinOp::Or, e_binop(e_binop(e_var("database"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("strpos", vec![e_var("database"), e_str(";")]), BinOp::StrictNotEq, e_bool(false)))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("_socket"), e_str(";")]), BinOp::StrictNotEq, e_bool(false))),
                        vec![
                            s_return(e_method_call(e_this(), "connectFailure", vec![e_int(2002), e_str("elephc mysqli: host, database, and socket must not contain ';'"), e_str("HY000")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_dsn", e_str("mysql:")),
                    s_if(
                        e_binop(e_binop(e_var("_socket"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_binop(e_var("_host"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("_host"), BinOp::StrictEq, e_str("localhost")))),
                        vec![
                            s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str("unix_socket=")), BinOp::Concat, e_var("_socket"))),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("_dsn", e_binop(e_binop(e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str("host=")), BinOp::Concat, e_ternary(e_binop(e_var("_host"), BinOp::StrictEq, e_str("")), e_str("localhost"), e_var("_host"))), BinOp::Concat, e_str(";port=")), BinOp::Concat, e_var("_port"))),
                    ]),
                    ),
                    s_if(
                        e_binop(e_binop(e_var("database"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("database"), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";dbname=")), BinOp::Concat, e_var("database"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("username"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";user=")), BinOp::Concat, e_call("str_replace", vec![e_str(";"), e_str("%3B"), e_call("str_replace", vec![e_str("%"), e_str("%25"), e_var("username")])]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("password"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";password=")), BinOp::Concat, e_call("str_replace", vec![e_str(";"), e_str("%3B"), e_call("str_replace", vec![e_str("%"), e_str("%25"), e_var("password")])]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("optConnectTimeout"), BinOp::Gt, e_int(0)),
                        vec![
                            s_assign("_dsn", e_binop(e_binop(e_var("_dsn"), BinOp::Concat, e_str(";connect_timeout=")), BinOp::Concat, e_this_prop("optConnectTimeout"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_sslConfig", e_str("")),
                    s_assign("_sslFiles", e_binop(e_binop(e_binop(e_binop(e_binop(e_this_prop("sslKey"), BinOp::StrictNotEq, e_str("")), BinOp::Or, e_binop(e_this_prop("sslCert"), BinOp::StrictNotEq, e_str(""))), BinOp::Or, e_binop(e_this_prop("sslCa"), BinOp::StrictNotEq, e_str(""))), BinOp::Or, e_binop(e_this_prop("sslCapath"), BinOp::StrictNotEq, e_str(""))), BinOp::Or, e_binop(e_this_prop("sslCipher"), BinOp::StrictNotEq, e_str("")))),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2048)), BinOp::NotEq, e_int(0)), BinOp::Or, e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(1073741824)), BinOp::NotEq, e_int(0))), BinOp::Or, e_this_prop("sslRequested")),
                        vec![
                            s_if(
                                e_binop(e_call("strpos", vec![e_binop(e_binop(e_this_prop("sslKey"), BinOp::Concat, e_this_prop("sslCert")), BinOp::Concat, e_this_prop("sslCa")), e_str(";")]), BinOp::StrictNotEq, e_bool(false)),
                                vec![
                                    s_return(e_method_call(e_this(), "connectFailure", vec![e_int(2026), e_str("elephc mysqli: ssl_set() paths must not contain ';'"), e_str("HY000")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_verify", e_ternary(e_var("_sslFiles"), e_int(1), e_int(0))),
                            s_if(
                                e_binop(e_this_prop("optSslVerify"), BinOp::GtEq, e_int(0)),
                                vec![
                                    s_assign("_verify", e_this_prop("optSslVerify")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(1073741824)), BinOp::NotEq, e_int(0)),
                                vec![
                                    s_assign("_verify", e_int(1)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(64)), BinOp::NotEq, e_int(0)),
                                vec![
                                    s_assign("_verify", e_int(0)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_this_prop("sslCa"), BinOp::StrictNotEq, e_str("")),
                                vec![
                                    s_assign("_sslConfig", e_binop(e_binop(e_binop(e_var("_sslConfig"), BinOp::Concat, e_str("ca=")), BinOp::Concat, e_this_prop("sslCa")), BinOp::Concat, e_str(";"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_this_prop("sslCert"), BinOp::StrictNotEq, e_str("")),
                                vec![
                                    s_assign("_sslConfig", e_binop(e_binop(e_binop(e_var("_sslConfig"), BinOp::Concat, e_str("cert=")), BinOp::Concat, e_this_prop("sslCert")), BinOp::Concat, e_str(";"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_this_prop("sslKey"), BinOp::StrictNotEq, e_str("")),
                                vec![
                                    s_assign("_sslConfig", e_binop(e_binop(e_binop(e_var("_sslConfig"), BinOp::Concat, e_str("key=")), BinOp::Concat, e_this_prop("sslKey")), BinOp::Concat, e_str(";"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_sslConfig", e_binop(e_binop(e_binop(e_var("_sslConfig"), BinOp::Concat, e_str("verify=")), BinOp::Concat, e_var("_verify")), BinOp::Concat, e_str(";"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_driverConfig", e_str("")),
                    s_assign("_driverConfig", e_binop(e_var("_driverConfig"), BinOp::Concat, e_str("servermulti=0;"))),
                    s_assign("_driverConfig", e_binop(e_var("_driverConfig"), BinOp::Concat, e_str("mariadbext=0;"))),
                    s_assign("_driverConfig", e_binop(e_var("_driverConfig"), BinOp::Concat, e_str("stmtcache=0;"))),
                    s_assign("_driverConfig", e_binop(e_var("_driverConfig"), BinOp::Concat, e_str("lazysets=1;"))),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(32)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_assign("_driverConfig", e_binop(e_var("_driverConfig"), BinOp::Concat, e_str("compress=1;"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(256)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_assign("_driverConfig", e_binop(e_var("_driverConfig"), BinOp::Concat, e_str("ignore=1;"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("optLocalInfile"), BinOp::NotEq, e_int(0)),
                        vec![
                            s_assign("_driverConfig", e_binop(e_var("_driverConfig"), BinOp::Concat, e_str("local=1;"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("optLocalInfileDir"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("_driverConfig", e_binop(e_binop(e_binop(e_var("_driverConfig"), BinOp::Concat, e_str("dir=")), BinOp::Concat, e_method_call(e_this(), "encodeDriverOption", vec![e_this_prop("optLocalInfileDir")])), BinOp::Concat, e_str(";"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("sslCapath"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_var("_sslConfig"), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_assign("_driverConfig", e_binop(e_binop(e_binop(e_var("_driverConfig"), BinOp::Concat, e_str("capath=")), BinOp::Concat, e_method_call(e_this(), "encodeDriverOption", vec![e_this_prop("sslCapath")])), BinOp::Concat, e_str(";"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("sslCipher"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_var("_sslConfig"), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_assign("_driverConfig", e_binop(e_binop(e_binop(e_var("_driverConfig"), BinOp::Concat, e_str("cipher=")), BinOp::Concat, e_method_call(e_this(), "encodeDriverOption", vec![e_this_prop("sslCipher")])), BinOp::Concat, e_str(";"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("optReadTimeout"), BinOp::Gt, e_int(0)),
                        vec![
                            s_assign("_driverConfig", e_binop(e_binop(e_binop(e_var("_driverConfig"), BinOp::Concat, e_str("iotimeout=")), BinOp::Concat, e_this_prop("optReadTimeout")), BinOp::Concat, e_str(";"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("optServerPublicKey"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("_driverConfig", e_binop(e_binop(e_binop(e_var("_driverConfig"), BinOp::Concat, e_str("serverkey=")), BinOp::Concat, e_method_call(e_this(), "encodeDriverOption", vec![e_this_prop("optServerPublicKey")])), BinOp::Concat, e_str(";"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_foundRows", e_ternary(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2)), BinOp::NotEq, e_int(0)), e_int(1), e_int(0))),
                    s_assign("_initCommand", e_str("")),
                    s_if(
                        e_binop(e_call("count", vec![e_this_prop("optInitCommands")]), BinOp::Gt, e_int(0)),
                        vec![
                            s_assign("_initCommand", e_cast(CastType::String, e_index(e_this_prop("optInitCommands"), e_int(0)))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_conn", e_call("elephc_pdo_open_persistent", vec![e_var("_dsn"), e_var("_persistent"), e_int(0), e_var("_initCommand"), e_var("_sslConfig"), e_var("_foundRows"), e_str(""), e_var("_driverConfig")])),
                    s_if(
                        e_binop(e_var("_conn"), BinOp::Lt, e_int(0)),
                        vec![
                            s_assign("_raw", e_call("elephc_pdo_last_open_error", vec![])),
                            s_assign("_message", e_static_call("mysqli", "__elephcServerMessage", vec![e_var("_raw")])),
                            s_assign("_state", e_call("elephc_pdo_last_open_sqlstate", vec![])),
                            s_if(
                                e_binop(e_var("_state"), BinOp::StrictEq, e_str("")),
                                vec![
                                    s_assign("_state", e_str("HY000")),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_code", e_call("elephc_pdo_last_open_native_code", vec![])),
                            s_if(
                                e_binop(e_var("_code"), BinOp::Eq, e_int(0)),
                                vec![
                                    s_assign("_marker", e_call("strpos", vec![e_var("_raw"), e_str("ERROR ")])),
                                    s_if(
                                        e_binop(e_var("_marker"), BinOp::StrictNotEq, e_bool(false)),
                                        vec![
                                            s_assign("_code", e_cast(CastType::Int, e_call("substr", vec![e_var("_raw"), e_binop(e_var("_marker"), BinOp::Add, e_int(6)), e_int(5)]))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("_code"), BinOp::Eq, e_int(0)),
                                vec![
                                    s_assign("_code", e_int(2002)),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("connect_failure")])),
                            s_if(
                                e_not(e_var("isRealConnect")),
                                vec![
                                    s_expr(e_method_call(e_this(), "freeHandle", vec![])),
                                    s_prop_assign(e_this(), "linkStatus", e_int(2)),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_method_call(e_this(), "connectFailure", vec![e_var("_code"), e_var("_message"), e_var("_state")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("elephc_pdo_driver_name", vec![e_var("_conn")]), BinOp::StrictNotEq, e_str("mysql")),
                        vec![
                            s_expr(e_call("elephc_pdo_release", vec![e_var("_conn"), e_int(0)])),
                            s_return(e_method_call(e_this(), "connectFailure", vec![e_int(2002), e_str("elephc mysqli opened a non-mysql bridge connection"), e_str("HY000")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "conn", e_var("_conn")),
                    s_assign("_priorStatus", e_this_prop("linkStatus")),
                    s_prop_assign(e_this(), "linkStatus", e_int(1)),
                    s_prop_assign(e_this(), "linkGone", e_bool(false)),
                    s_prop_assign(e_this(), "multiStatementsOn", e_bool(false)),
                    s_prop_assign(e_this(), "persistent", e_binop(e_var("_persistent"), BinOp::Eq, e_int(1))),
                    s_prop_assign(e_this(), "connect_errno", e_int(0)),
                    s_prop_assign(e_this(), "connect_error", e_null()),
                    s_static_prop_assign("mysqli", "lastConnectErrno", e_int(0)),
                    s_static_prop_assign("mysqli", "lastConnectError", e_str("")),
                    s_static_prop_assign("mysqli", "openLinks", e_binop(e_static_prop("mysqli", "openLinks"), BinOp::Add, e_int(1))),
                    s_assign("_pooled", e_bool(false)),
                    s_if(
                        e_this_prop("persistent"),
                        vec![
                            s_static_prop_assign("mysqli", "openPersistentLinks", e_binop(e_static_prop("mysqli", "openPersistentLinks"), BinOp::Add, e_int(1))),
                            s_assign("_pooled", e_call("in_array", vec![e_var("_conn"), e_static_prop("mysqli", "seenPlinks"), e_bool(true)])),
                            s_if(
                                e_var("_pooled"),
                                vec![
                                    s_static_prop_assign("mysqli", "cachedPlinks", e_binop(e_static_prop("mysqli", "cachedPlinks"), BinOp::Sub, e_int(1))),
                                    s_prop_assign(e_this(), "connStats", e_cast(CastType::Array, e_null_coalesce(e_index(e_static_prop("mysqli", "plinkStats"), e_var("_conn")), e_array(vec![])))),
                                ],
                                vec![],
                                Some(vec![
                                s_assign("_seen", e_static_prop("mysqli", "seenPlinks")),
                                s_array_push("_seen", e_var("_conn")),
                                s_static_prop_assign("mysqli", "seenPlinks", e_var("_seen")),
                            ]),
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_var("_pooled")),
                        vec![
                            s_if(
                                e_this_prop("handlePersistent"),
                                vec![
                                    s_expr(e_method_call(e_this(), "bump", vec![e_str("pconnect_success")])),
                                    s_expr(e_method_call(e_this(), "bump", vec![e_str("active_persistent_connections")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("connect_success")])),
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("active_connections")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_prop_assign(e_this(), "host_info", e_call("elephc_pdo_connection_status", vec![e_var("_conn")])),
                    s_prop_assign(e_this(), "server_info", e_call("elephc_pdo_server_version", vec![e_var("_conn")])),
                    s_prop_assign(e_this(), "server_version", e_method_call(e_this(), "versionStringToInt", vec![e_this_prop("server_info")])),
                    s_prop_assign(e_this(), "client_info", e_call("elephc_pdo_mysql_client_info", vec![])),
                    s_prop_assign(e_this(), "client_version", e_method_call(e_this(), "versionStringToInt", vec![e_this_prop("client_info")])),
                    s_prop_assign(e_this(), "thread_id", e_call("elephc_pdo_mysql_thread_id", vec![e_var("_conn")])),
                    s_prop_assign(e_this(), "warning_count", e_call("elephc_pdo_warning_count", vec![e_var("_conn")])),
                    s_prop_assign(e_this(), "info", e_null()),
                    s_assign("_n", e_call("count", vec![e_this_prop("optInitCommands")])),
                    s_for(Some(s_assign("_i", e_int(1))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_if(
                            e_binop(e_call("elephc_pdo_exec", vec![e_var("_conn"), e_cast(CastType::String, e_index(e_this_prop("optInitCommands"), e_var("_i")))]), BinOp::Lt, e_int(0)),
                            vec![
                                s_expr(e_method_call(e_this(), "__elephcAdoptBridgeError", vec![])),
                                s_static_prop_assign("mysqli", "openLinks", e_binop(e_static_prop("mysqli", "openLinks"), BinOp::Sub, e_int(1))),
                                s_expr(e_method_call(e_this(), "dropActive", vec![])),
                                s_expr(e_method_call(e_this(), "releaseConnection", vec![])),
                                s_if(
                                    e_not(e_var("isRealConnect")),
                                    vec![
                                        s_expr(e_method_call(e_this(), "freeHandle", vec![])),
                                        s_prop_assign(e_this(), "linkStatus", e_int(2)),
                                    ],
                                    vec![],
                                    Some(vec![
                                        s_prop_assign(e_this(), "linkStatus", e_var("_priorStatus")),
                                    ]),
                                ),
                                s_expr(e_method_call(e_this(), "__elephcReportError", vec![])),
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_binop(e_this_prop("optCharsetName"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_expr(e_method_call(e_this(), "set_charset", vec![e_this_prop("optCharsetName")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("connect")
                .param_default("hostname", t_nullable(TypeExpr::Str), e_null())
                .param_default("username", t_nullable(TypeExpr::Str), e_null())
                .param_default("password", t_nullable(TypeExpr::Str), e_null())
                .param_default("database", t_nullable(TypeExpr::Str), e_null())
                .param_default("port", t_nullable(TypeExpr::Int), e_null())
                .param_default("socket", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "connectLink", vec![e_var("hostname"), e_var("username"), e_var("password"), e_var("database"), e_var("port"), e_var("socket"), e_int(0), e_bool(false)])),
                ]),
        )
        .method(
            method("init")
                .body(vec![
                    s_return(e_null()),
                ]),
        )
        .method(
            method("close")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("linkStatus"), BinOp::Eq, e_int(2)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("mysqli object is already closed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "closeLink", vec![e_bool(true)])),
                    s_prop_assign(e_this(), "linkStatus", e_int(2)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("linkStatus"), BinOp::NotEq, e_int(2)),
                        vec![
                            s_expr(e_method_call(e_this(), "closeLink", vec![e_bool(true)])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("closeLink")
                .private()
                .param("explicit", TypeExpr::Bool)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("linkStatus"), BinOp::Eq, e_int(1)),
                        vec![
                            s_static_prop_assign("mysqli", "openLinks", e_binop(e_static_prop("mysqli", "openLinks"), BinOp::Sub, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("conn"), BinOp::GtEq, e_int(0)), BinOp::And, e_this_prop("persistent")),
                        vec![
                            s_assign("_handle", e_this_prop("conn")),
                            s_expr(e_method_call(e_this(), "releaseConnection", vec![])),
                            s_if(
                                e_binop(e_call("elephc_pdo_driver_name", vec![e_var("_handle")]), BinOp::StrictEq, e_str("mysql")),
                                vec![
                                    s_assign("_stats", e_static_prop("mysqli", "plinkStats")),
                                    s_array_assign("_stats", e_var("_handle"), e_this_prop("connStats")),
                                    s_static_prop_assign("mysqli", "plinkStats", e_var("_stats")),
                                    s_static_prop_assign("mysqli", "cachedPlinks", e_binop(e_static_prop("mysqli", "cachedPlinks"), BinOp::Add, e_int(1))),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "handleAllocated", e_bool(false)),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("conn"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "bump", vec![e_ternary(e_var("explicit"), e_str("explicit_close"), e_str("implicit_close"))])),
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("com_quit")])),
                            s_expr(e_method_call(e_this(), "dropActive", vec![])),
                            s_expr(e_method_call(e_this(), "releaseConnection", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "freeHandle", vec![])),
                ]),
        )
        .method(
            method("freeHandle")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_not(e_this_prop("handleAllocated")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "handleAllocated", e_bool(false)),
                    s_assign("_client", e_static_prop("mysqli", "clientStats")),
                    s_array_assign("_client", e_str("active_connections"), e_binop(e_cast(CastType::Int, e_null_coalesce(e_index(e_var("_client"), e_str("active_connections")), e_int(0))), BinOp::Sub, e_int(1))),
                    s_if(
                        e_this_prop("handlePersistent"),
                        vec![
                            s_array_assign("_client", e_str("active_persistent_connections"), e_binop(e_cast(CastType::Int, e_null_coalesce(e_index(e_var("_client"), e_str("active_persistent_connections")), e_int(0))), BinOp::Sub, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_static_prop_assign("mysqli", "clientStats", e_var("_client")),
                ]),
        )
        .method(
            method("resetOptions")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "optConnectTimeout", e_int(0)),
                    s_prop_assign(e_this(), "optInitCommands", e_array(vec![])),
                    s_prop_assign(e_this(), "optCharsetName", e_str("")),
                    s_prop_assign(e_this(), "optLocalInfile", e_int(0)),
                    s_prop_assign(e_this(), "optLocalInfileDir", e_str("")),
                    s_prop_assign(e_this(), "optServerPublicKey", e_str("")),
                    s_prop_assign(e_this(), "optSslVerify", e_neg(e_int(1))),
                    s_prop_assign(e_this(), "optIntAndFloatNative", e_bool(false)),
                    s_prop_assign(e_this(), "optReadTimeout", e_int(0)),
                    s_prop_assign(e_this(), "sslRequested", e_bool(false)),
                    s_prop_assign(e_this(), "sslKey", e_str("")),
                    s_prop_assign(e_this(), "sslCert", e_str("")),
                    s_prop_assign(e_this(), "sslCa", e_str("")),
                    s_prop_assign(e_this(), "sslCapath", e_str("")),
                    s_prop_assign(e_this(), "sslCipher", e_str("")),
                ]),
        )
        .method(
            method("ping")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("com_ping")])),
                    s_if(
                        e_binop(e_call("elephc_pdo_mysql_ping", vec![e_this_prop("conn")]), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "adminReply", vec![e_bool(false)])),
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_expr(e_method_call(e_this(), "adminReply", vec![e_bool(true)])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("change_user")
                .param("username", TypeExpr::Str)
                .param("password", TypeExpr::Str)
                .param("database", t_nullable(TypeExpr::Str))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("com_change_user")])),
                    s_assign("_db", e_ternary(e_binop(e_var("database"), BinOp::StrictEq, e_null()), e_str(""), e_var("database"))),
                    s_assign("_hasDb", e_ternary(e_binop(e_var("database"), BinOp::StrictEq, e_null()), e_int(0), e_int(1))),
                    s_if(
                        e_binop(e_call("elephc_pdo_mysql_change_user", vec![e_this_prop("conn"), e_var("username"), e_var("password"), e_var("_db"), e_var("_hasDb")]), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "closeLive", vec![])),
                    s_prop_assign(e_this(), "thread_id", e_call("elephc_pdo_mysql_thread_id", vec![e_this_prop("conn")])),
                    s_prop_assign(e_this(), "affected_rows", e_int(0)),
                    s_prop_assign(e_this(), "insert_id", e_int(0)),
                    s_prop_assign(e_this(), "field_count", e_int(0)),
                    s_prop_assign(e_this(), "warning_count", e_int(0)),
                    s_prop_assign(e_this(), "info", e_null()),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("kill")
                .param("process_id", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("process_id"), BinOp::LtEq, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli::kill(): Argument #1 ($process_id) must be greater than 0")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("com_process_kill")])),
                    s_assign("_own", e_binop(e_var("process_id"), BinOp::Eq, e_this_prop("thread_id"))),
                    s_if(
                        e_binop(e_call("elephc_pdo_mysql_process_kill", vec![e_this_prop("conn"), e_var("process_id")]), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "adminReply", vec![e_bool(false)])),
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_expr(e_method_call(e_this(), "adminReply", vec![e_not(e_var("_own"))])),
                    s_if(
                        e_var("_own"),
                        vec![
                            s_prop_assign(e_this(), "linkGone", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("refresh")
                .param("flags", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("com_refresh")])),
                    s_if(
                        e_binop(e_call("elephc_pdo_mysql_refresh", vec![e_this_prop("conn"), e_var("flags")]), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephcAdoptBridgeError", vec![])),
                            s_expr(e_method_call(e_this(), "adminReply", vec![e_bool(false)])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_expr(e_method_call(e_this(), "adminReply", vec![e_bool(true)])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("ssl_set")
                .param("key", t_nullable(TypeExpr::Str))
                .param("certificate", t_nullable(TypeExpr::Str))
                .param("ca_certificate", t_nullable(TypeExpr::Str))
                .param("ca_path", t_nullable(TypeExpr::Str))
                .param("cipher_algos", t_nullable(TypeExpr::Str))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireNotClosed", vec![])),
                    s_prop_assign(e_this(), "sslRequested", e_bool(true)),
                    s_prop_assign(e_this(), "sslKey", e_ternary(e_binop(e_var("key"), BinOp::StrictEq, e_null()), e_str(""), e_var("key"))),
                    s_prop_assign(e_this(), "sslCert", e_ternary(e_binop(e_var("certificate"), BinOp::StrictEq, e_null()), e_str(""), e_var("certificate"))),
                    s_prop_assign(e_this(), "sslCa", e_ternary(e_binop(e_var("ca_certificate"), BinOp::StrictEq, e_null()), e_str(""), e_var("ca_certificate"))),
                    s_prop_assign(e_this(), "sslCapath", e_ternary(e_binop(e_var("ca_path"), BinOp::StrictEq, e_null()), e_str(""), e_var("ca_path"))),
                    s_prop_assign(e_this(), "sslCipher", e_ternary(e_binop(e_var("cipher_algos"), BinOp::StrictEq, e_null()), e_str(""), e_var("cipher_algos"))),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("debug")
                .param("options", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_ignoredOptions", e_var("options")),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("dump_debug_info")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_if(
                        e_method_call(e_this(), "__elephcHasPendingResults", vec![]),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephcOutOfSync", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("com_debug")])),
                    s_if(
                        e_binop(e_call("elephc_pdo_mysql_dump_debug_info", vec![e_this_prop("conn")]), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephcAdoptBridgeError", vec![])),
                            s_expr(e_method_call(e_this(), "adminReply", vec![e_bool(false)])),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_expr(e_method_call(e_this(), "adminReply", vec![e_bool(true)])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("get_connection_stats")
                .returns(t_array())
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_assign("_out", e_array(vec![])),
                    s_foreach(e_static_call("mysqli", "__elephcStatKeys", vec![]), None, "_key", vec![
                        s_assign("_name", e_cast(CastType::String, e_var("_key"))),
                        s_array_assign("_out", e_var("_name"), e_null_coalesce(e_static_call("mysqli", "__elephcWireStat", vec![e_ternary(e_binop(e_this_prop("conn"), BinOp::Lt, e_int(0)), e_neg(e_int(2)), e_this_prop("conn")), e_var("_name")]), e_static_call("mysqli", "__elephcStatString", vec![e_cast(CastType::Int, e_null_coalesce(e_index(e_this_prop("connStats"), e_var("_name")), e_int(0)))]))),
                    ]),
                    s_return(e_var("_out")),
                ]),
        )
        .method(
            method("__elephcStatString")
                .private()
                .static_()
                .param("value", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_binop(e_var("value"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_return(e_cast(CastType::String, e_var("value"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_magnitude", e_neg(e_var("value"))),
                    s_assign("_low", e_binop(e_int(709551616), BinOp::Sub, e_binop(e_var("_magnitude"), BinOp::Mod, e_int(1000000000)))),
                    s_assign("_high", e_binop(e_int(18446744073), BinOp::Sub, e_call("intdiv", vec![e_var("_magnitude"), e_int(1000000000)]))),
                    s_if(
                        e_binop(e_var("_low"), BinOp::Lt, e_int(0)),
                        vec![
                            s_assign("_low", e_binop(e_var("_low"), BinOp::Add, e_int(1000000000))),
                            s_assign("_high", e_binop(e_var("_high"), BinOp::Sub, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_cast(CastType::String, e_var("_high")), BinOp::Concat, e_call("str_pad", vec![e_cast(CastType::String, e_var("_low")), e_int(9), e_str("0"), e_const("STR_PAD_LEFT")]))),
                ]),
        )
        .method(
            method("__elephcWireStat")
                .private()
                .static_()
                .param("handle", TypeExpr::Int)
                .param("key", TypeExpr::Str)
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_if(
                        e_binop(e_var("handle"), BinOp::Lt, e_neg(e_int(1))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_index", e_null_coalesce(e_index(e_array_assoc(vec![(e_str("bytes_sent"), e_int(0)), (e_str("bytes_received"), e_int(1)), (e_str("packets_sent"), e_int(2)), (e_str("packets_received"), e_int(3)), (e_str("protocol_overhead_in"), e_int(4)), (e_str("protocol_overhead_out"), e_int(5)), (e_str("packets_sent_command"), e_int(6)), (e_str("bytes_received_ok_packet"), e_int(7)), (e_str("bytes_received_eof_packet"), e_int(8)), (e_str("bytes_received_rset_header_packet"), e_int(9)), (e_str("bytes_received_rset_field_meta_packet"), e_int(10)), (e_str("bytes_received_rset_row_packet"), e_int(11)), (e_str("bytes_received_prepare_response_packet"), e_int(12)), (e_str("bytes_received_change_user_packet"), e_int(13)), (e_str("packets_received_ok"), e_int(14)), (e_str("packets_received_eof"), e_int(15)), (e_str("packets_received_rset_header"), e_int(16)), (e_str("packets_received_rset_field_meta"), e_int(17)), (e_str("packets_received_rset_row"), e_int(18)), (e_str("packets_received_prepare_response"), e_int(19)), (e_str("packets_received_change_user"), e_int(20))]), e_var("key")), e_neg(e_int(1)))),
                    s_if(
                        e_binop(e_var("_index"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_cast(CastType::String, e_call("elephc_pdo_mysql_io_stat", vec![e_var("handle"), e_var("_index")]))),
                ]),
        )
        .method(
            method("get_warnings")
                .returns(t_union(vec![t_class("mysqli_warning"), TypeExpr::False]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_if(
                        e_binop(e_this_prop("warning_count"), BinOp::Eq, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "__elephcFetchWarnings", vec![])),
                ]),
        )
        .method(
            method("thread_safe")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("select_db")
                .param("database", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("com_init_db")])),
                    s_if(
                        e_binop(e_call("elephc_pdo_mysql_init_db", vec![e_this_prop("conn"), e_var("database"), e_call("strlen", vec![e_var("database")])]), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "adminReply", vec![e_bool(false)])),
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_expr(e_method_call(e_this(), "adminReply", vec![e_bool(true)])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("set_charset")
                .param("charset", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "charsetIdentIsValid", vec![e_var("charset")])),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2019), e_str("Invalid characterset or character set not supported"), e_str("HY000")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("elephc_pdo_exec", vec![e_this_prop("conn"), e_binop(e_str("SET NAMES "), BinOp::Concat, e_var("charset"))]), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_prop_assign(e_this(), "affected_rows", e_int(0)),
                    s_prop_assign(e_this(), "info", e_null()),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("character_set_name")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_return(e_call("elephc_pdo_mysql_charset", vec![e_this_prop("conn")])),
                ]),
        )
        .method(
            method("real_escape_string")
                .param("string", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_assign("_len", e_call("elephc_pdo_real_escape_string", vec![e_this_prop("conn"), e_var("string"), e_call("strlen", vec![e_var("string")])])),
                    s_if(
                        e_binop(e_var("_len"), BinOp::LtEq, e_int(0)),
                        vec![
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_ptr_read_string", vec![e_call("elephc_pdo_blob_data_ptr", vec![]), e_var("_len")])),
                ]),
        )
        .method(
            method("escape_string")
                .param("string", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_method_call(e_this(), "real_escape_string", vec![e_var("string")])),
                ]),
        )
        // `mysqli::quote_string` is PHP 8.6+; earlier profiles do not declare it.
        .when(php_version >= PhpVersion::Php86, |class| {
            class.method(method("quote_string")
                    .param("string", TypeExpr::Str)
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_return(e_binop(e_binop(e_str("'"), BinOp::Concat, e_method_call(e_this(), "real_escape_string", vec![e_var("string")])), BinOp::Concat, e_str("'"))),
                    ]),
        )
        })
        .method(
            method("begin_transaction")
                .param_default("flags", TypeExpr::Int, e_int(0))
                .param_default("name", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("flags"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli::begin_transaction(): Argument #1 ($flags) must be one of the MYSQLI_TRANS_* constants")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_comment", e_method_call(e_this(), "transactionComment", vec![e_str("mysqli::begin_transaction"), e_var("name"), e_bool(true)])),
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_parts", e_array(vec![])),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(1)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_array_push("_parts", e_str("WITH CONSISTENT SNAPSHOT")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_array_push("_parts", e_str("READ WRITE")),
                        ],
                        vec![
                        (e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(4)), BinOp::NotEq, e_int(0)), vec![
                            s_array_push("_parts", e_str("READ ONLY")),
                        ]),
                    ],
                        None,
                    ),
                    s_assign("_sql", e_str("START TRANSACTION")),
                    s_if(
                        e_binop(e_call("count", vec![e_var("_parts")]), BinOp::Gt, e_int(0)),
                        vec![
                            s_assign("_sql", e_binop(e_binop(e_var("_sql"), BinOp::Concat, e_str(" ")), BinOp::Concat, e_call("implode", vec![e_str(", "), e_var("_parts")]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_sql", e_binop(e_var("_sql"), BinOp::Concat, e_var("_comment"))),
                    s_if(
                        e_binop(e_call("elephc_pdo_exec", vec![e_this_prop("conn"), e_var("_sql")]), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "refreshStatus", vec![])),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("commit")
                .param_default("flags", TypeExpr::Int, e_int(0))
                .param_default("name", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_comment", e_method_call(e_this(), "transactionComment", vec![e_str("mysqli::commit"), e_var("name"), e_bool(false)])),
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_sql", e_binop(e_binop(e_str("COMMIT"), BinOp::Concat, e_method_call(e_this(), "transactionCorFlags", vec![e_var("flags")])), BinOp::Concat, e_var("_comment"))),
                    s_if(
                        e_binop(e_call("elephc_pdo_exec", vec![e_this_prop("conn"), e_var("_sql")]), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "refreshStatus", vec![])),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("rollback")
                .param_default("flags", TypeExpr::Int, e_int(0))
                .param_default("name", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_comment", e_method_call(e_this(), "transactionComment", vec![e_str("mysqli::rollback"), e_var("name"), e_bool(false)])),
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_sql", e_binop(e_binop(e_str("ROLLBACK"), BinOp::Concat, e_method_call(e_this(), "transactionCorFlags", vec![e_var("flags")])), BinOp::Concat, e_var("_comment"))),
                    s_if(
                        e_binop(e_call("elephc_pdo_exec", vec![e_this_prop("conn"), e_var("_sql")]), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "refreshStatus", vec![])),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("savepoint")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli::savepoint(): Argument #1 ($name) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("elephc_pdo_exec", vec![e_this_prop("conn"), e_binop(e_binop(e_str("SAVEPOINT `"), BinOp::Concat, e_call("str_replace", vec![e_str("`"), e_str("``"), e_var("name")])), BinOp::Concat, e_str("`"))]), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "refreshStatus", vec![])),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("release_savepoint")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli::release_savepoint(): Argument #1 ($name) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("elephc_pdo_exec", vec![e_this_prop("conn"), e_binop(e_binop(e_str("RELEASE SAVEPOINT `"), BinOp::Concat, e_call("str_replace", vec![e_str("`"), e_str("``"), e_var("name")])), BinOp::Concat, e_str("`"))]), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "refreshStatus", vec![])),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("autocommit")
                .param("enable", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("elephc_pdo_set_autocommit", vec![e_this_prop("conn"), e_ternary(e_var("enable"), e_int(1), e_int(0))]), BinOp::NotEq, e_int(1)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_prop_assign(e_this(), "affected_rows", e_int(0)),
                    s_prop_assign(e_this(), "info", e_null()),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("options")
                .param("option", TypeExpr::Int)
                .param("value", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireNotClosed", vec![])),
                    s_if(
                        e_binop(e_var("option"), BinOp::Eq, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "optConnectTimeout", e_cast(CastType::Int, e_var("value"))),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::Eq, e_int(3)),
                        vec![
                            s_prop_array_push(e_this(), "optInitCommands", e_cast(CastType::String, e_var("value"))),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::Eq, e_int(7)),
                        vec![
                            s_prop_assign(e_this(), "optCharsetName", e_cast(CastType::String, e_var("value"))),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::Eq, e_int(8)),
                        vec![
                            s_prop_assign(e_this(), "optLocalInfile", e_ternary(e_binop(e_cast(CastType::Int, e_var("value")), BinOp::NotEq, e_int(0)), e_int(1), e_int(0))),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::Eq, e_int(43)),
                        vec![
                            s_prop_assign(e_this(), "optLocalInfileDir", e_ternary(e_binop(e_var("value"), BinOp::StrictEq, e_null()), e_str(""), e_cast(CastType::String, e_var("value")))),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::Eq, e_int(21)),
                        vec![
                            s_prop_assign(e_this(), "optSslVerify", e_ternary(e_binop(e_cast(CastType::Int, e_var("value")), BinOp::NotEq, e_int(0)), e_int(1), e_int(0))),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::Eq, e_int(35)),
                        vec![
                            s_prop_assign(e_this(), "optServerPublicKey", e_cast(CastType::String, e_var("value"))),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::Eq, e_int(201)),
                        vec![
                            s_prop_assign(e_this(), "optIntAndFloatNative", e_binop(e_cast(CastType::Int, e_var("value")), BinOp::NotEq, e_int(0))),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::Eq, e_int(11)),
                        vec![
                            s_prop_assign(e_this(), "optReadTimeout", e_cast(CastType::Int, e_var("value"))),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("option"), BinOp::Eq, e_int(1)), BinOp::Or, e_binop(e_var("option"), BinOp::Eq, e_int(4))), BinOp::Or, e_binop(e_var("option"), BinOp::Eq, e_int(5))), BinOp::Or, e_binop(e_var("option"), BinOp::Eq, e_int(37))), BinOp::Or, e_binop(e_var("option"), BinOp::Eq, e_int(202))), BinOp::Or, e_binop(e_var("option"), BinOp::Eq, e_int(203))),
                        vec![
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("set_opt")
                .param("option", TypeExpr::Int)
                .param("value", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "options", vec![e_var("option"), e_var("value")])),
                ]),
        )
        .method(
            method("query")
                .param("query", TypeExpr::Str)
                .param_default("result_mode", TypeExpr::Int, e_int(0))
                .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::Bool]))
                .body(vec![
                    s_if(
                        e_binop(e_var("query"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli::query(): Argument #1 ($query) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_base", e_ternary(e_binop(e_binop(e_var("result_mode"), BinOp::BitAnd, e_int(8)), BinOp::NotEq, e_int(0)), e_binop(e_var("result_mode"), BinOp::Sub, e_int(8)), e_var("result_mode"))),
                    s_assign("_storeBase", e_ternary(e_binop(e_binop(e_var("_base"), BinOp::BitAnd, e_int(16)), BinOp::NotEq, e_int(0)), e_binop(e_var("_base"), BinOp::Sub, e_int(16)), e_var("_base"))),
                    s_if(
                        e_binop(e_binop(e_var("_base"), BinOp::NotEq, e_int(1)), BinOp::And, e_binop(e_var("_storeBase"), BinOp::NotEq, e_int(0))),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli::query(): Argument #2 ($result_mode) must be either MYSQLI_USE_RESULT or MYSQLI_STORE_RESULT with MYSQLI_ASYNC as an optional bitmask flag")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("result_mode"), BinOp::BitAnd, e_int(8)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "sendAsyncQuery", vec![e_var("query"), e_binop(e_var("_base"), BinOp::Eq, e_int(1))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_unbuffered", e_binop(e_var("_base"), BinOp::Eq, e_int(1))),
                    s_assign("_code", e_method_call(e_this(), "runQuery", vec![e_var("query"), e_var("_unbuffered")])),
                    s_if(
                        e_binop(e_var("_code"), BinOp::Eq, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_code"), BinOp::Eq, e_int(1)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephcReportIndex", vec![e_var("query")])),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_result", e_ternary(e_var("_unbuffered"), e_method_call(e_this(), "claimStream", vec![]), e_method_call(e_this(), "claimStored", vec![]))),
                    s_if(
                        e_binop(e_var("_result"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "__elephcReportIndex", vec![e_var("query")])),
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("reap_async_query")
                .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::Bool]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_if(
                        e_not(e_this_prop("asyncPending")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "asyncPending", e_bool(false)),
                    s_assign("_stmt", e_call("elephc_pdo_mysql_async_reap", vec![e_this_prop("conn")])),
                    s_if(
                        e_binop(e_var("_stmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_rc", e_int(2)),
                    s_if(
                        e_not(e_this_prop("asyncUnbuffered")),
                        vec![
                            s_assign("_rc", e_call("elephc_pdo_step", vec![e_var("_stmt")])),
                            s_if(
                                e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                                vec![
                                    s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                                    s_return(e_method_call(e_this(), "opFailed", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_columnCount", e_call("elephc_pdo_column_count", vec![e_var("_stmt")])),
                    s_expr(e_method_call(e_this(), "__elephcHeaderStats", vec![e_var("_columnCount"), e_bool(true)])),
                    s_if(
                        e_binop(e_var("_columnCount"), BinOp::Eq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "refreshStatus", vec![])),
                            s_prop_assign(e_this(), "field_count", e_int(0)),
                            s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                            s_expr(e_method_call(e_this(), "clearError", vec![])),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "liveStmt", e_var("_stmt")),
                    s_prop_assign(e_this(), "liveStep", e_var("_rc")),
                    s_prop_assign(e_this(), "liveSetPending", e_bool(true)),
                    s_prop_assign(e_this(), "field_count", e_var("_columnCount")),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_assign("_result", e_ternary(e_this_prop("asyncUnbuffered"), e_method_call(e_this(), "claimStream", vec![]), e_method_call(e_this(), "claimStored", vec![]))),
                    s_if(
                        e_binop(e_var("_result"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("poll")
                .static_()
                .param_by_ref("read", Some(t_nullable(t_array())))
                .param_by_ref("error", Some(t_nullable(t_array())))
                .param_by_ref("reject", Some(t_array()))
                .param("seconds", TypeExpr::Int)
                .param_default("microseconds", TypeExpr::Int, e_int(0))
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
                .body(vec![
                    s_if(
                        e_binop(e_var("seconds"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli::poll(): Argument #4 ($seconds) must be greater than or equal to 0")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("microseconds"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli::poll(): Argument #5 ($microseconds) must be greater than or equal to 0")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("read"), BinOp::StrictEq, e_null()), BinOp::And, e_binop(e_var("error"), BinOp::StrictEq, e_null())),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("No stream arrays were passed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_pending", e_array(vec![])),
                    s_assign("_rejected", e_array(vec![])),
                    s_if(
                        e_binop(e_var("read"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_foreach(e_var("read"), None, "_link", vec![
                                s_if(
                                    e_not(e_instance_of(e_var("_link"), "mysqli")),
                                    vec![
                                        s_throw(e_new("TypeError", vec![e_str("mysqli::poll(): Argument #1 ($read) must only contain objects of type mysqli")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_method_call(e_var("_link"), "__elephcAsyncPending", vec![]),
                                    vec![
                                        s_array_push("_pending", e_var("_link")),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_array_push("_rejected", e_var("_link")),
                                ]),
                                ),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_watched", e_int(0)),
                    s_if(
                        e_binop(e_var("error"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_foreach(e_var("error"), None, "_link", vec![
                                s_if(
                                    e_not(e_instance_of(e_var("_link"), "mysqli")),
                                    vec![
                                        s_throw(e_new("TypeError", vec![e_str("mysqli::poll(): Argument #1 ($read) must only contain objects of type mysqli")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("_watched", e_binop(e_var("_watched"), BinOp::Add, e_int(1))),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_ready", e_array(vec![])),
                    s_assign("_budget", e_binop(e_binop(e_var("seconds"), BinOp::Mul, e_int(1000000)), BinOp::Add, e_var("microseconds"))),
                    s_assign("_deadline", e_binop(e_call("microtime", vec![e_bool(true)]), BinOp::Add, e_binop(e_var("_budget"), BinOp::Div, e_int(1000000)))),
                    s_assign("_count", e_call("count", vec![e_var("_pending")])),
                    s_while(e_bool(true), vec![
                        s_foreach(e_var("_pending"), None, "_link", vec![
                            s_if(
                                e_method_call(e_var("_link"), "__elephcAsyncReady", vec![e_int(0)]),
                                vec![
                                    s_array_push("_ready", e_var("_link")),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                        s_if(
                            e_binop(e_binop(e_call("count", vec![e_var("_ready")]), BinOp::Gt, e_int(0)), BinOp::Or, e_binop(e_binop(e_var("_count"), BinOp::Eq, e_int(0)), BinOp::And, e_binop(e_var("_watched"), BinOp::Eq, e_int(0)))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_left", e_cast(CastType::Int, e_binop(e_binop(e_var("_deadline"), BinOp::Sub, e_call("microtime", vec![e_bool(true)])), BinOp::Mul, e_int(1000000)))),
                        s_if(
                            e_binop(e_var("_left"), BinOp::LtEq, e_int(0)),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("_count"), BinOp::Eq, e_int(0)),
                            vec![
                                s_expr(e_call("usleep", vec![e_var("_left")])),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_slice", e_ternary(e_binop(e_var("_left"), BinOp::Lt, e_int(2000)), e_var("_left"), e_int(2000))),
                        s_foreach(e_var("_pending"), None, "_link", vec![
                            s_expr(e_method_call(e_var("_link"), "__elephcAsyncReady", vec![e_cast(CastType::Int, e_binop(e_var("_slice"), BinOp::Div, e_var("_count")))])),
                        ]),
                    ]),
                    s_if(
                        e_binop(e_var("read"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("read", e_var("_ready")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("error"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("error", e_array(vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("reject", e_var("_rejected")),
                    s_return(e_call("count", vec![e_var("_ready")])),
                ]),
        )
        .method(
            method("prepare")
                .param("query", TypeExpr::Str)
                .returns(t_union(vec![t_class("mysqli_stmt"), TypeExpr::False]))
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_statement", e_new("mysqli_stmt", vec![e_this()])),
                    s_if(
                        e_not(e_method_call(e_var("_statement"), "__elephcPrepareOrCopyError", vec![e_var("query")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_statement")),
                ]),
        )
        .method(
            method("stmt_init")
                .returns(t_union(vec![t_class("mysqli_stmt"), TypeExpr::False]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_return(e_new("mysqli_stmt", vec![e_this()])),
                ]),
        )
        .method(
            method("get_charset")
                .returns(t_mixed())
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_assign("_charset", e_call("strtolower", vec![e_call("elephc_pdo_mysql_charset", vec![e_this_prop("conn")])])),
                    s_assign("_entry", e_static_call("mysqli", "__elephcCharsetEntry", vec![e_var("_charset")])),
                    s_assign("_info", e_new("stdClass", vec![])),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("charset")), e_var("_charset"))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("collation")), e_cast(CastType::String, e_index(e_var("_entry"), e_int(0))))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("dir")), e_str(""))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("min_length")), e_cast(CastType::Int, e_index(e_var("_entry"), e_int(2))))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("max_length")), e_cast(CastType::Int, e_index(e_var("_entry"), e_int(3))))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("number")), e_cast(CastType::Int, e_index(e_var("_entry"), e_int(1))))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("state")), e_int(1))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("comment")), e_cast(CastType::String, e_index(e_var("_entry"), e_int(4))))),
                    s_return(e_var("_info")),
                ]),
        )
        .method(
            method("__elephcCharsetEntry")
                .private()
                .static_()
                .param("name", TypeExpr::Str)
                .returns(t_array())
                .body(vec![
                    s_assign("_table", e_array_assoc(vec![(e_str("big5"), e_array(vec![e_str("big5_chinese_ci"), e_int(1), e_int(1), e_int(2), e_str("")])), (e_str("dec8"), e_array(vec![e_str("dec8_swedish_ci"), e_int(3), e_int(1), e_int(1), e_str("")])), (e_str("cp850"), e_array(vec![e_str("cp850_general_ci"), e_int(4), e_int(1), e_int(1), e_str("")])), (e_str("hp8"), e_array(vec![e_str("hp8_english_ci"), e_int(6), e_int(1), e_int(1), e_str("")])), (e_str("koi8r"), e_array(vec![e_str("koi8r_general_ci"), e_int(7), e_int(1), e_int(1), e_str("")])), (e_str("latin1"), e_array(vec![e_str("latin1_swedish_ci"), e_int(8), e_int(1), e_int(1), e_str("")])), (e_str("latin2"), e_array(vec![e_str("latin2_general_ci"), e_int(9), e_int(1), e_int(1), e_str("")])), (e_str("swe7"), e_array(vec![e_str("swe7_swedish_ci"), e_int(10), e_int(1), e_int(1), e_str("")])), (e_str("ascii"), e_array(vec![e_str("ascii_general_ci"), e_int(11), e_int(1), e_int(1), e_str("")])), (e_str("ujis"), e_array(vec![e_str("ujis_japanese_ci"), e_int(12), e_int(1), e_int(3), e_str("")])), (e_str("sjis"), e_array(vec![e_str("sjis_japanese_ci"), e_int(13), e_int(1), e_int(2), e_str("")])), (e_str("hebrew"), e_array(vec![e_str("hebrew_general_ci"), e_int(16), e_int(1), e_int(1), e_str("")])), (e_str("filename"), e_array(vec![e_str("filename"), e_int(17), e_int(1), e_int(5), e_str("")])), (e_str("tis620"), e_array(vec![e_str("tis620_thai_ci"), e_int(18), e_int(1), e_int(1), e_str("")])), (e_str("euckr"), e_array(vec![e_str("euckr_korean_ci"), e_int(19), e_int(1), e_int(2), e_str("")])), (e_str("koi8u"), e_array(vec![e_str("koi8u_general_ci"), e_int(22), e_int(1), e_int(1), e_str("")])), (e_str("gb2312"), e_array(vec![e_str("gb2312_chinese_ci"), e_int(24), e_int(1), e_int(2), e_str("")])), (e_str("greek"), e_array(vec![e_str("greek_general_ci"), e_int(25), e_int(1), e_int(1), e_str("")])), (e_str("cp1250"), e_array(vec![e_str("cp1250_general_ci"), e_int(26), e_int(1), e_int(1), e_str("")])), (e_str("gbk"), e_array(vec![e_str("gbk_chinese_ci"), e_int(28), e_int(1), e_int(2), e_str("")])), (e_str("latin5"), e_array(vec![e_str("latin5_turkish_ci"), e_int(30), e_int(1), e_int(1), e_str("")])), (e_str("armscii8"), e_array(vec![e_str("armscii8_general_ci"), e_int(32), e_int(1), e_int(1), e_str("")])), (e_str("utf8"), e_array(vec![e_str("utf8_general_ci"), e_int(33), e_int(1), e_int(3), e_str("UTF-8 Unicode")])), (e_str("ucs2"), e_array(vec![e_str("ucs2_general_ci"), e_int(35), e_int(2), e_int(2), e_str("UCS-2 Unicode")])), (e_str("cp866"), e_array(vec![e_str("cp866_general_ci"), e_int(36), e_int(1), e_int(1), e_str("")])), (e_str("keybcs2"), e_array(vec![e_str("keybcs2_general_ci"), e_int(37), e_int(1), e_int(1), e_str("")])), (e_str("macce"), e_array(vec![e_str("macce_general_ci"), e_int(38), e_int(1), e_int(1), e_str("")])), (e_str("macroman"), e_array(vec![e_str("macroman_general_ci"), e_int(39), e_int(1), e_int(1), e_str("")])), (e_str("cp852"), e_array(vec![e_str("cp852_general_ci"), e_int(40), e_int(1), e_int(1), e_str("")])), (e_str("latin7"), e_array(vec![e_str("latin7_general_ci"), e_int(41), e_int(1), e_int(1), e_str("")])), (e_str("utf8mb4"), e_array(vec![e_str("utf8mb4_general_ci"), e_int(45), e_int(1), e_int(4), e_str("UTF-8 Unicode")])), (e_str("cp1251"), e_array(vec![e_str("cp1251_general_ci"), e_int(51), e_int(1), e_int(1), e_str("")])), (e_str("utf16"), e_array(vec![e_str("utf16_general_ci"), e_int(54), e_int(2), e_int(4), e_str("UTF-16 Unicode")])), (e_str("utf16le"), e_array(vec![e_str("utf16le_general_ci"), e_int(56), e_int(2), e_int(4), e_str("UTF-16LE Unicode")])), (e_str("cp1256"), e_array(vec![e_str("cp1256_general_ci"), e_int(57), e_int(1), e_int(1), e_str("")])), (e_str("cp1257"), e_array(vec![e_str("cp1257_general_ci"), e_int(59), e_int(1), e_int(1), e_str("")])), (e_str("utf32"), e_array(vec![e_str("utf32_general_ci"), e_int(60), e_int(4), e_int(4), e_str("UTF-32 Unicode")])), (e_str("binary"), e_array(vec![e_str("binary"), e_int(63), e_int(1), e_int(1), e_str("")])), (e_str("geostd8"), e_array(vec![e_str("geostd8_general_ci"), e_int(92), e_int(1), e_int(1), e_str("")])), (e_str("cp932"), e_array(vec![e_str("cp932_japanese_ci"), e_int(95), e_int(1), e_int(2), e_str("")])), (e_str("eucjpms"), e_array(vec![e_str("eucjpms_japanese_ci"), e_int(97), e_int(1), e_int(3), e_str("")])), (e_str("gb18030"), e_array(vec![e_str("gb18030_chinese_ci"), e_int(248), e_int(1), e_int(4), e_str("")]))])),
                    s_return(e_null_coalesce(e_index(e_var("_table"), e_var("name")), e_array(vec![e_str(""), e_int(0), e_int(0), e_int(0), e_str("")]))),
                ]),
        )
        // `mysqli::execute_query` is PHP 8.2+; earlier profiles do not declare it.
        .when(php_version >= PhpVersion::Php82, |class| {
            class.method(method("execute_query")
                    .param("query", TypeExpr::Str)
                    .param_default("params", t_nullable(t_array()), e_null())
                    .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::Bool]))
                    .body(vec![
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
                            e_not(e_method_call(e_var("_statement"), "execute", vec![e_var("params")])),
                            vec![
                                s_expr(e_method_call(e_var("_statement"), "close", vec![])),
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_prop(e_var("_statement"), "field_count"), BinOp::Eq, e_int(0)),
                            vec![
                                s_prop_assign(e_this(), "affected_rows", e_prop(e_var("_statement"), "affected_rows")),
                                s_prop_assign(e_this(), "insert_id", e_prop(e_var("_statement"), "insert_id")),
                                s_prop_assign(e_this(), "field_count", e_int(0)),
                                s_expr(e_method_call(e_var("_statement"), "close", vec![])),
                                s_return(e_bool(true)),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_result", e_method_call(e_var("_statement"), "get_result", vec![])),
                        s_expr(e_method_call(e_var("_statement"), "close", vec![])),
                        s_if(
                            e_binop(e_var("_result"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "field_count", e_prop(e_var("_result"), "field_count")),
                        s_return(e_var("_result")),
                    ]),
        )
        })
        .method(
            method("real_query")
                .param("query", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_code", e_method_call(e_this(), "runQuery", vec![e_var("query"), e_bool(true)])),
                    s_if(
                        e_binop(e_var("_code"), BinOp::Eq, e_int(1)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephcReportIndex", vec![e_var("query")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_var("_code"), BinOp::NotEq, e_int(0))),
                ]),
        )
        .method(
            method("multi_query")
                .param("query", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "enableMultiStatements", vec![])),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("com_query")])),
                    s_assign("_stmt", e_method_call(e_this(), "prepareText", vec![e_var("query"), e_bool(true)])),
                    s_if(
                        e_binop(e_var("_stmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephcAdoptBridgeError", vec![])),
                            s_expr(e_method_call(e_this(), "disableMultiStatements", vec![])),
                            s_expr(e_method_call(e_this(), "__elephcReportError", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_rc", e_call("elephc_pdo_mysql_execute", vec![e_var("_stmt")])),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Eq, e_int(0)),
                        vec![
                            s_assign("_rc", e_int(2)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephcAdoptBridgeError", vec![])),
                            s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                            s_expr(e_method_call(e_this(), "__elephcReportError", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "liveStmt", e_var("_stmt")),
                    s_expr(e_method_call(e_this(), "enterSet", vec![e_var("_rc")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("more_results")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_return(e_this_prop("multiMore")),
                ]),
        )
        .method(
            method("next_result")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_if(
                        e_binop(e_this_prop("liveSetPending"), BinOp::Or, e_this_prop("streaming")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_not(e_this_prop("multiMore")), BinOp::Or, e_binop(e_this_prop("liveStmt"), BinOp::Lt, e_int(0))),
                        vec![
                            s_prop_assign(e_this(), "multiMore", e_bool(false)),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "multiMore", e_bool(false)),
                    s_assign("_advanced", e_call("elephc_pdo_next_rowset", vec![e_this_prop("liveStmt")])),
                    s_if(
                        e_binop(e_var("_advanced"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephcAdoptBridgeError", vec![])),
                            s_expr(e_method_call(e_this(), "closeLive", vec![])),
                            s_expr(e_method_call(e_this(), "__elephcReportError", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_advanced"), BinOp::Eq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "closeLive", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "enterSet", vec![e_int(2)])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("store_result")
                .param_default("mode", TypeExpr::Int, e_int(0))
                .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_assign("_ignoredMode", e_var("mode")),
                    s_assign("_result", e_method_call(e_this(), "claimStored", vec![])),
                    s_if(
                        e_binop(e_var("_result"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "__elephcReportIndex", vec![e_str("from previous query")])),
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("use_result")
                .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_assign("_result", e_method_call(e_this(), "claimStream", vec![])),
                    s_if(
                        e_binop(e_var("_result"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("get_server_info")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_return(e_this_prop("server_info")),
                ]),
        )
        .method(
            method("get_client_info")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_call("elephc_pdo_mysql_client_info", vec![])),
                ]),
        )
        .method(
            method("stat")
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("com_statistics")])),
                    s_assign("_stat", e_call("elephc_pdo_server_info", vec![e_this_prop("conn")])),
                    s_expr(e_method_call(e_this(), "adminReply", vec![e_bool(false)])),
                    s_if(
                        e_binop(e_var("_stat"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_stat")),
                ]),
        )
        .method(
            method("__elephcStatKeys")
                .private()
                .static_()
                .returns(t_array())
                .body(vec![
                    s_return(e_call("explode", vec![e_str(" "), e_str("bytes_sent bytes_received packets_sent packets_received protocol_overhead_in protocol_overhead_out bytes_received_ok_packet bytes_received_eof_packet bytes_received_rset_header_packet bytes_received_rset_field_meta_packet bytes_received_rset_row_packet bytes_received_prepare_response_packet bytes_received_change_user_packet packets_sent_command packets_received_ok packets_received_eof packets_received_rset_header packets_received_rset_field_meta packets_received_rset_row packets_received_prepare_response packets_received_change_user result_set_queries non_result_set_queries no_index_used bad_index_used slow_queries buffered_sets unbuffered_sets ps_buffered_sets ps_unbuffered_sets flushed_normal_sets flushed_ps_sets ps_prepared_never_executed ps_prepared_once_executed rows_fetched_from_server_normal rows_fetched_from_server_ps rows_buffered_from_client_normal rows_buffered_from_client_ps rows_fetched_from_client_normal_buffered rows_fetched_from_client_normal_unbuffered rows_fetched_from_client_ps_buffered rows_fetched_from_client_ps_unbuffered rows_fetched_from_client_ps_cursor rows_affected_normal rows_affected_ps rows_skipped_normal rows_skipped_ps copy_on_write_saved copy_on_write_performed command_buffer_too_small connect_success connect_failure connection_reused reconnect pconnect_success active_connections active_persistent_connections explicit_close implicit_close disconnect_close in_middle_of_command_close explicit_free_result implicit_free_result explicit_stmt_close implicit_stmt_close mem_emalloc_count mem_emalloc_amount mem_ecalloc_count mem_ecalloc_amount mem_erealloc_count mem_erealloc_amount mem_efree_count mem_efree_amount mem_malloc_count mem_malloc_amount mem_calloc_count mem_calloc_amount mem_realloc_count mem_realloc_amount mem_free_count mem_free_amount mem_estrndup_count mem_strndup_count mem_estrdup_count mem_strdup_count mem_edupl_count mem_dupl_count proto_text_fetched_null proto_text_fetched_bit proto_text_fetched_tinyint proto_text_fetched_short proto_text_fetched_int24 proto_text_fetched_int proto_text_fetched_bigint proto_text_fetched_decimal proto_text_fetched_float proto_text_fetched_double proto_text_fetched_date proto_text_fetched_year proto_text_fetched_time proto_text_fetched_datetime proto_text_fetched_timestamp proto_text_fetched_string proto_text_fetched_blob proto_text_fetched_enum proto_text_fetched_set proto_text_fetched_geometry proto_text_fetched_other proto_binary_fetched_null proto_binary_fetched_bit proto_binary_fetched_tinyint proto_binary_fetched_short proto_binary_fetched_int24 proto_binary_fetched_int proto_binary_fetched_bigint proto_binary_fetched_decimal proto_binary_fetched_float proto_binary_fetched_double proto_binary_fetched_date proto_binary_fetched_year proto_binary_fetched_time proto_binary_fetched_datetime proto_binary_fetched_timestamp proto_binary_fetched_string proto_binary_fetched_json proto_binary_fetched_blob proto_binary_fetched_enum proto_binary_fetched_set proto_binary_fetched_geometry proto_binary_fetched_other init_command_executed_count init_command_failed_count com_quit com_init_db com_query com_field_list com_create_db com_drop_db com_refresh com_shutdown com_statistics com_process_info com_connect com_process_kill com_debug com_ping com_time com_delayed_insert com_change_user com_binlog_dump com_table_dump com_connect_out com_register_slave com_stmt_prepare com_stmt_execute com_stmt_send_long_data com_stmt_close com_stmt_reset com_stmt_set_option com_stmt_fetch com_deamon bytes_received_real_data_normal bytes_received_real_data_ps")])),
                ]),
        )
        .method(
            method("__elephcClientErrno")
                .private()
                .static_()
                .param("errno", TypeExpr::Int)
                .param("message", TypeExpr::Str)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_var("errno"), BinOp::Eq, e_int(1)), BinOp::And, e_binop(e_var("message"), BinOp::StrictEq, e_str("MySQL server has gone away"))),
                        vec![
                            s_return(e_int(2006)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("errno")),
                ]),
        )
        .method(
            method("__elephcServerMessage")
                .private()
                .static_()
                .param("message", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_call("strpos", vec![e_var("message"), e_str("IO error")]), BinOp::StrictNotEq, e_bool(false)), BinOp::Or, e_binop(e_call("strpos", vec![e_var("message"), e_str("IoError")]), BinOp::StrictNotEq, e_bool(false))),
                        vec![
                            s_return(e_str("MySQL server has gone away")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_prefix", e_str("MySqlError { ERROR ")),
                    s_if(
                        e_binop(e_binop(e_binop(e_call("strlen", vec![e_var("message")]), BinOp::Gt, e_binop(e_call("strlen", vec![e_var("_prefix")]), BinOp::Add, e_int(2))), BinOp::And, e_binop(e_call("substr", vec![e_var("message"), e_int(0), e_call("strlen", vec![e_var("_prefix")])]), BinOp::StrictEq, e_var("_prefix"))), BinOp::And, e_binop(e_call("substr", vec![e_var("message"), e_neg(e_int(2))]), BinOp::StrictEq, e_str(" }"))),
                        vec![
                            s_assign("_at", e_call("strpos", vec![e_var("message"), e_str("): ")])),
                            s_if(
                                e_binop(e_var("_at"), BinOp::StrictNotEq, e_bool(false)),
                                vec![
                                    s_return(e_call("substr", vec![e_var("message"), e_binop(e_var("_at"), BinOp::Add, e_int(3)), e_binop(e_binop(e_call("strlen", vec![e_var("message")]), BinOp::Sub, e_var("_at")), BinOp::Sub, e_int(5))])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("message")),
                ]),
        )
        .method(
            method("__elephcReportIndex")
                .private()
                .param("query", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("conn"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_status", e_call("elephc_pdo_mysql_server_status", vec![e_this_prop("conn")])),
                    s_assign("_index", e_str("")),
                    s_if(
                        e_binop(e_binop(e_var("_status"), BinOp::BitAnd, e_int(16)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_assign("_index", e_str("Bad index")),
                        ],
                        vec![
                        (e_binop(e_binop(e_var("_status"), BinOp::BitAnd, e_int(32)), BinOp::NotEq, e_int(0)), vec![
                            s_assign("_index", e_str("No index")),
                        ]),
                    ],
                        Some(vec![
                        s_return_void(),
                    ]),
                    ),
                    s_if(
                        e_binop(e_binop(e_static_prop("mysqli", "reportMode"), BinOp::BitAnd, e_int(4)), BinOp::Eq, e_int(0)),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_message", e_binop(e_binop(e_var("_index"), BinOp::Concat, e_str(" used in query/prepared statement ")), BinOp::Concat, e_var("query"))),
                    s_if(
                        e_binop(e_binop(e_static_prop("mysqli", "reportMode"), BinOp::BitAnd, e_int(2)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_throw(e_static_call("mysqli_sql_exception", "__elephcCreate", vec![e_var("_message"), e_int(0), e_str("00000")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_static_call("mysqli", "__elephcWarn", vec![e_var("_message")])),
                ]),
        )
        .method(
            method("enableMultiStatements")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_not(e_this_prop("multiStatementsOn")),
                        vec![
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("com_stmt_set_option")])),
                            s_expr(e_call("elephc_pdo_mysql_set_server_option", vec![e_this_prop("conn"), e_int(0)])),
                            s_prop_assign(e_this(), "multiStatementsOn", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("disableMultiStatements")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_this_prop("multiStatementsOn"),
                        vec![
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("com_stmt_set_option")])),
                            s_expr(e_call("elephc_pdo_mysql_set_server_option", vec![e_this_prop("conn"), e_int(1)])),
                            s_prop_assign(e_this(), "multiStatementsOn", e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("bump")
                .private()
                .param("key", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_array_assign(e_this(), "connStats", e_var("key"), e_binop(e_cast(CastType::Int, e_null_coalesce(e_index(e_this_prop("connStats"), e_var("key")), e_int(0))), BinOp::Add, e_int(1))),
                    s_assign("_client", e_static_prop("mysqli", "clientStats")),
                    s_array_assign("_client", e_var("key"), e_binop(e_cast(CastType::Int, e_null_coalesce(e_index(e_var("_client"), e_var("key")), e_int(0))), BinOp::Add, e_int(1))),
                    s_static_prop_assign("mysqli", "clientStats", e_var("_client")),
                ]),
        )
        .method(
            method("__elephcStat")
                .private()
                .param("key", TypeExpr::Str)
                .param("by", TypeExpr::Int)
                .param("connection", TypeExpr::Bool)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_var("by"), BinOp::Eq, e_int(0)),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_var("connection"),
                        vec![
                            s_prop_array_assign(e_this(), "connStats", e_var("key"), e_binop(e_cast(CastType::Int, e_null_coalesce(e_index(e_this_prop("connStats"), e_var("key")), e_int(0))), BinOp::Add, e_var("by"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_client", e_static_prop("mysqli", "clientStats")),
                    s_array_assign("_client", e_var("key"), e_binop(e_cast(CastType::Int, e_null_coalesce(e_index(e_var("_client"), e_var("key")), e_int(0))), BinOp::Add, e_var("by"))),
                    s_static_prop_assign("mysqli", "clientStats", e_var("_client")),
                ]),
        )
        .method(
            method("__elephcHeaderStats")
                .private()
                .param("cols", TypeExpr::Int)
                .param("normal", TypeExpr::Bool)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_var("cols"), BinOp::Gt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("result_set_queries")])),
                            s_assign("_status", e_call("elephc_pdo_mysql_server_status", vec![e_this_prop("conn")])),
                            s_if(
                                e_binop(e_binop(e_var("_status"), BinOp::BitAnd, e_int(16)), BinOp::NotEq, e_int(0)),
                                vec![
                                    s_expr(e_method_call(e_this(), "bump", vec![e_str("bad_index_used")])),
                                ],
                                vec![
                                (e_binop(e_binop(e_var("_status"), BinOp::BitAnd, e_int(32)), BinOp::NotEq, e_int(0)), vec![
                                    s_expr(e_method_call(e_this(), "bump", vec![e_str("no_index_used")])),
                                ]),
                                (e_binop(e_binop(e_var("_status"), BinOp::BitAnd, e_int(2048)), BinOp::NotEq, e_int(0)), vec![
                                    s_expr(e_method_call(e_this(), "bump", vec![e_str("slow_queries")])),
                                ]),
                            ],
                                None,
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("non_result_set_queries")])),
                    s_if(
                        e_var("normal"),
                        vec![
                            s_assign("_affected", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                            s_if(
                                e_binop(e_var("_affected"), BinOp::Gt, e_int(0)),
                                vec![
                                    s_expr(e_method_call(e_this(), "__elephcStat", vec![e_str("rows_affected_normal"), e_var("_affected"), e_bool(true)])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephcBump")
                .private()
                .param("key", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "bump", vec![e_var("key")])),
                ]),
        )
        .method(
            method("dropActive")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_client", e_static_prop("mysqli", "clientStats")),
                    s_array_assign("_client", e_str("active_connections"), e_binop(e_cast(CastType::Int, e_null_coalesce(e_index(e_var("_client"), e_str("active_connections")), e_int(0))), BinOp::Sub, e_int(1))),
                    s_if(
                        e_this_prop("handlePersistent"),
                        vec![
                            s_array_assign("_client", e_str("active_persistent_connections"), e_binop(e_cast(CastType::Int, e_null_coalesce(e_index(e_var("_client"), e_str("active_persistent_connections")), e_int(0))), BinOp::Sub, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_static_prop_assign("mysqli", "clientStats", e_var("_client")),
                ]),
        )
        .method(
            method("__elephcAdoptBridgeError")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "error", e_static_call("mysqli", "__elephcServerMessage", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                    s_prop_assign(e_this(), "errno", e_static_call("mysqli", "__elephcClientErrno", vec![e_call("elephc_pdo_errcode", vec![e_this_prop("conn")]), e_this_prop("error")])),
                    s_prop_assign(e_this(), "sqlstate", e_call("elephc_pdo_sqlstate", vec![e_this_prop("conn")])),
                    s_if(
                        e_binop(e_this_prop("sqlstate"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "sqlstate", e_str("HY000")),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "error_list", e_array(vec![e_array_assoc(vec![(e_str("errno"), e_this_prop("errno")), (e_str("sqlstate"), e_this_prop("sqlstate")), (e_str("error"), e_this_prop("error"))])])),
                ]),
        )
        .method(
            method("__elephcReportError")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "report", vec![e_this_prop("error"), e_this_prop("errno"), e_this_prop("sqlstate")])),
                ]),
        )
        .method(
            method("__elephcConn")
                .private()
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("conn")),
                ]),
        )
        .method(
            method("__elephcAsyncPending")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_this_prop("asyncPending")),
                ]),
        )
        .method(
            method("__elephcAsyncReady")
                .private()
                .param("timeoutUs", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_this_prop("asyncPending"), BinOp::And, e_binop(e_call("elephc_pdo_mysql_async_wait", vec![e_this_prop("conn"), e_var("timeoutUs")]), BinOp::Eq, e_int(1)))),
                ]),
        )
        .method(
            method("releaseConnection")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("conn"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_this_prop("persistent")),
                        vec![
                            s_expr(e_call("elephc_pdo_mysql_cancel", vec![e_this_prop("conn")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "closeLive", vec![])),
                    s_if(
                        e_binop(e_call("elephc_pdo_in_transaction", vec![e_this_prop("conn")]), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_expr(e_call("elephc_pdo_rollback", vec![e_this_prop("conn")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_pdo_release", vec![e_this_prop("conn"), e_int(0)])),
                    s_prop_assign(e_this(), "conn", e_neg(e_int(1))),
                    s_prop_assign(e_this(), "asyncPending", e_bool(false)),
                    s_if(
                        e_this_prop("persistent"),
                        vec![
                            s_static_prop_assign("mysqli", "openPersistentLinks", e_binop(e_static_prop("mysqli", "openPersistentLinks"), BinOp::Sub, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "persistent", e_bool(false)),
                ]),
        )
        .method(
            method("sendAsyncQuery")
                .private()
                .param("query", TypeExpr::Str)
                .param("unbuffered", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "disableMultiStatements", vec![])),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("com_query")])),
                    s_if(
                        e_binop(e_call("elephc_pdo_mysql_async_send", vec![e_this_prop("conn"), e_var("query"), e_cast(CastType::Int, e_var("unbuffered"))]), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "asyncPending", e_bool(true)),
                    s_prop_assign(e_this(), "asyncUnbuffered", e_var("unbuffered")),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephcFetchWarnings")
                .private()
                .returns(t_union(vec![t_class("mysqli_warning"), TypeExpr::False]))
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_this_prop("conn"), BinOp::Lt, e_int(0)), BinOp::Or, e_method_call(e_this(), "__elephcHasPendingResults", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_stmt", e_call("elephc_pdo_prepare", vec![e_this_prop("conn"), e_str("SHOW WARNINGS"), e_int(1)])),
                    s_if(
                        e_binop(e_var("_stmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_codes", e_array(vec![])),
                    s_assign("_messages", e_array(vec![])),
                    s_assign("_rc", e_call("elephc_pdo_step", vec![e_var("_stmt")])),
                    s_while(e_binop(e_var("_rc"), BinOp::Eq, e_int(1)), vec![
                        s_array_push("_codes", e_cast(CastType::Int, e_method_call(e_this(), "columnValue", vec![e_var("_stmt"), e_int(1)]))),
                        s_array_push("_messages", e_cast(CastType::String, e_method_call(e_this(), "columnValue", vec![e_var("_stmt"), e_int(2)]))),
                        s_assign("_rc", e_call("elephc_pdo_step", vec![e_var("_stmt")])),
                    ]),
                    s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                    s_if(
                        e_binop(e_call("count", vec![e_var("_codes")]), BinOp::Eq, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("mysqli_warning", "__elephcFromRows", vec![e_var("_codes"), e_var("_messages")])),
                ]),
        )
        .method(
            method("requireNoPendingResults")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_method_call(e_this(), "__elephcHasPendingResults", vec![]),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2014), e_str("Commands out of sync; you can't run this command now"), e_str("HY000")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephcHasPendingResults")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_binop(e_binop(e_this_prop("liveStmt"), BinOp::GtEq, e_int(0)), BinOp::Or, e_this_prop("asyncPending")), BinOp::Or, e_binop(e_this_prop("busyStatement"), BinOp::GtEq, e_int(0)))),
                ]),
        )
        .method(
            method("__elephcBusyFor")
                .private()
                .param("handle", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_binop(e_binop(e_this_prop("liveStmt"), BinOp::GtEq, e_int(0)), BinOp::Or, e_this_prop("asyncPending")), BinOp::Or, e_binop(e_binop(e_this_prop("busyStatement"), BinOp::GtEq, e_int(0)), BinOp::And, e_binop(e_this_prop("busyStatement"), BinOp::NotEq, e_var("handle"))))),
                ]),
        )
        .method(
            method("__elephcOutOfSync")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "errno", e_int(2014)),
                    s_prop_assign(e_this(), "error", e_str("Commands out of sync; you can't run this command now")),
                    s_prop_assign(e_this(), "sqlstate", e_str("HY000")),
                    s_prop_assign(e_this(), "error_list", e_array(vec![e_array_assoc(vec![(e_str("errno"), e_int(2014)), (e_str("sqlstate"), e_str("HY000")), (e_str("error"), e_str("Commands out of sync; you can't run this command now"))])])),
                ]),
        )
        .method(
            method("__elephcStatementBusy")
                .private()
                .param("handle", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "busyStatement", e_var("handle")),
                ]),
        )
        .method(
            method("__elephcStatementIdle")
                .private()
                .param("handle", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("busyStatement"), BinOp::Eq, e_var("handle")),
                        vec![
                            s_prop_assign(e_this(), "busyStatement", e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("encodeDriverOption")
                .private()
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_call("str_replace", vec![e_str("="), e_str("%3D"), e_call("str_replace", vec![e_str(";"), e_str("%3B"), e_call("str_replace", vec![e_str("%"), e_str("%25"), e_var("value")])])])),
                ]),
        )
        .method(
            method("transactionComment")
                .private()
                .param("context", TypeExpr::Str)
                .param("name", t_nullable(TypeExpr::Str))
                .param("emptyThrows", TypeExpr::Bool)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_if(
                                e_var("emptyThrows"),
                                vec![
                                    s_throw(e_new("ValueError", vec![e_binop(e_var("context"), BinOp::Concat, e_str("(): Argument #2 ($name) must not be empty"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_str(" /**/")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_clean", e_str("")),
                    s_assign("_len", e_call("strlen", vec![e_var("name")])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_len"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_c", e_call("substr", vec![e_var("name"), e_var("_i"), e_int(1)])),
                        s_assign("_o", e_call("ord", vec![e_var("_c")])),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_o"), BinOp::GtEq, e_int(48)), BinOp::And, e_binop(e_var("_o"), BinOp::LtEq, e_int(57))), BinOp::Or, e_binop(e_binop(e_var("_o"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("_o"), BinOp::LtEq, e_int(90)))), BinOp::Or, e_binop(e_binop(e_var("_o"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("_o"), BinOp::LtEq, e_int(122)))), BinOp::Or, e_binop(e_var("_c"), BinOp::StrictEq, e_str(" "))), BinOp::Or, e_binop(e_var("_c"), BinOp::StrictEq, e_str("-"))), BinOp::Or, e_binop(e_var("_c"), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_var("_c"), BinOp::StrictEq, e_str("="))),
                            vec![
                                s_assign("_clean", e_binop(e_var("_clean"), BinOp::Concat, e_var("_c"))),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_binop(e_binop(e_str(" /*"), BinOp::Concat, e_var("_clean")), BinOp::Concat, e_str("*/"))),
                ]),
        )
        .method(
            method("transactionCorFlags")
                .private()
                .param("flags", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_sql", e_str("")),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(1)), BinOp::NotEq, e_int(0)), BinOp::And, e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2)), BinOp::Eq, e_int(0))),
                        vec![
                            s_assign("_sql", e_binop(e_var("_sql"), BinOp::Concat, e_str(" AND CHAIN"))),
                        ],
                        vec![
                        (e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2)), BinOp::NotEq, e_int(0)), BinOp::And, e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(1)), BinOp::Eq, e_int(0))), vec![
                            s_assign("_sql", e_binop(e_var("_sql"), BinOp::Concat, e_str(" AND NO CHAIN"))),
                        ]),
                    ],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(4)), BinOp::NotEq, e_int(0)), BinOp::And, e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(8)), BinOp::Eq, e_int(0))),
                        vec![
                            s_assign("_sql", e_binop(e_var("_sql"), BinOp::Concat, e_str(" RELEASE"))),
                        ],
                        vec![
                        (e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(8)), BinOp::NotEq, e_int(0)), BinOp::And, e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(4)), BinOp::Eq, e_int(0))), vec![
                            s_assign("_sql", e_binop(e_var("_sql"), BinOp::Concat, e_str(" NO RELEASE"))),
                        ]),
                    ],
                        None,
                    ),
                    s_return(e_var("_sql")),
                ]),
        )
        .method(
            method("__elephcUnsignedId")
                .private()
                .static_()
                .param("text", TypeExpr::Str)
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::Str]))
                .body(vec![
                    s_if(
                        e_binop(e_var("text"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_id", e_cast(CastType::Int, e_var("text"))),
                    s_return(e_ternary(e_binop(e_cast(CastType::String, e_var("_id")), BinOp::StrictEq, e_var("text")), e_var("_id"), e_var("text"))),
                ]),
        )
        .method(
            method("refreshStatus")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "affected_rows", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                    s_prop_assign(e_this(), "insert_id", e_self_call("__elephcUnsignedId", vec![e_call("elephc_pdo_last_insert_id_text", vec![e_this_prop("conn"), e_str("")])])),
                    s_prop_assign(e_this(), "warning_count", e_call("elephc_pdo_warning_count", vec![e_this_prop("conn")])),
                    s_assign("_info", e_call("elephc_pdo_mysql_info", vec![e_this_prop("conn")])),
                    s_prop_assign(e_this(), "info", e_ternary(e_binop(e_var("_info"), BinOp::StrictEq, e_str("")), e_null(), e_var("_info"))),
                ]),
        )
        .method(
            method("__elephcRequireValid")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                ]),
        )
        .method(
            method("requireInitialized")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("linkStatus"), BinOp::Eq, e_int(2)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("mysqli object is already closed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("linkStatus"), BinOp::Eq, e_int(0)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("mysqli object is not fully initialized")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("requireConnection")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_if(
                        e_this_prop("linkGone"),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2006), e_str("MySQL server has gone away"), e_str("HY000")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("conn"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2014), e_str("Commands out of sync; you can't run this command now"), e_str("HY000")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("syntheticFailure")
                .private()
                .param("errno", TypeExpr::Int)
                .param("message", TypeExpr::Str)
                .param("sqlstate", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "errno", e_var("errno")),
                    s_prop_assign(e_this(), "error", e_var("message")),
                    s_prop_assign(e_this(), "sqlstate", e_var("sqlstate")),
                    s_prop_assign(e_this(), "error_list", e_array(vec![e_array_assoc(vec![(e_str("errno"), e_var("errno")), (e_str("sqlstate"), e_var("sqlstate")), (e_str("error"), e_var("message"))])])),
                    s_expr(e_method_call(e_this(), "report", vec![e_var("message"), e_var("errno"), e_var("sqlstate")])),
                ]),
        )
        .method(
            method("connectFailure")
                .private()
                .param("errno", TypeExpr::Int)
                .param("message", TypeExpr::Str)
                .param("sqlstate", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "connect_errno", e_var("errno")),
                    s_prop_assign(e_this(), "connect_error", e_var("message")),
                    s_prop_assign(e_this(), "errno", e_var("errno")),
                    s_prop_assign(e_this(), "error", e_var("message")),
                    s_prop_assign(e_this(), "sqlstate", e_var("sqlstate")),
                    s_prop_assign(e_this(), "error_list", e_array(vec![e_array_assoc(vec![(e_str("errno"), e_var("errno")), (e_str("sqlstate"), e_var("sqlstate")), (e_str("error"), e_var("message"))])])),
                    s_static_prop_assign("mysqli", "lastConnectErrno", e_var("errno")),
                    s_static_prop_assign("mysqli", "lastConnectError", e_var("message")),
                    s_if(
                        e_binop(e_binop(e_static_prop("mysqli", "reportMode"), BinOp::BitAnd, e_int(2)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_throw(e_static_call("mysqli_sql_exception", "__elephcCreate", vec![e_var("message"), e_var("errno"), e_var("sqlstate")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_static_call("mysqli", "__elephcWarn", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_str("("), BinOp::Concat, e_var("sqlstate")), BinOp::Concat, e_str("/")), BinOp::Concat, e_var("errno")), BinOp::Concat, e_str("): ")), BinOp::Concat, e_var("message"))])),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("opFailed")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "error", e_static_call("mysqli", "__elephcServerMessage", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                    s_prop_assign(e_this(), "errno", e_static_call("mysqli", "__elephcClientErrno", vec![e_call("elephc_pdo_errcode", vec![e_this_prop("conn")]), e_this_prop("error")])),
                    s_prop_assign(e_this(), "sqlstate", e_call("elephc_pdo_sqlstate", vec![e_this_prop("conn")])),
                    s_if(
                        e_binop(e_this_prop("sqlstate"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "sqlstate", e_str("HY000")),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "error_list", e_array(vec![e_array_assoc(vec![(e_str("errno"), e_this_prop("errno")), (e_str("sqlstate"), e_this_prop("sqlstate")), (e_str("error"), e_this_prop("error"))])])),
                    s_expr(e_method_call(e_this(), "report", vec![e_this_prop("error"), e_this_prop("errno"), e_this_prop("sqlstate")])),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("clearError")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "errno", e_int(0)),
                    s_prop_assign(e_this(), "error", e_str("")),
                    s_prop_assign(e_this(), "sqlstate", e_str("00000")),
                    s_prop_assign(e_this(), "error_list", e_array(vec![])),
                ]),
        )
        .method(
            method("report")
                .private()
                .param("message", TypeExpr::Str)
                .param("errno", TypeExpr::Int)
                .param("sqlstate", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_static_prop("mysqli", "reportMode"), BinOp::BitAnd, e_int(2)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_throw(e_static_call("mysqli_sql_exception", "__elephcCreate", vec![e_var("message"), e_var("errno"), e_var("sqlstate")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_static_prop("mysqli", "reportMode"), BinOp::BitAnd, e_int(1)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_expr(e_static_call("mysqli", "__elephcWarn", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_str("("), BinOp::Concat, e_var("sqlstate")), BinOp::Concat, e_str("/")), BinOp::Concat, e_var("errno")), BinOp::Concat, e_str("): ")), BinOp::Concat, e_var("message"))])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephcWarn")
                .private()
                .static_()
                .param("message", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_entry", e_str("")),
                    s_foreach(e_call("debug_backtrace", vec![e_const("DEBUG_BACKTRACE_IGNORE_ARGS")]), None, "_frame", vec![
                        s_assign("_function", e_cast(CastType::String, e_null_coalesce(e_index(e_var("_frame"), e_str("function")), e_str("")))),
                        s_assign("_class", e_cast(CastType::String, e_null_coalesce(e_index(e_var("_frame"), e_str("class")), e_str("")))),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("_class"), BinOp::StrictEq, e_str("mysqli")), BinOp::Or, e_binop(e_var("_class"), BinOp::StrictEq, e_str("mysqli_stmt"))), BinOp::Or, e_binop(e_var("_class"), BinOp::StrictEq, e_str("mysqli_result"))), BinOp::Or, e_binop(e_var("_class"), BinOp::StrictEq, e_str("mysqli_driver"))),
                            vec![
                                s_assign("_entry", e_binop(e_binop(e_var("_class"), BinOp::Concat, e_str("::")), BinOp::Concat, e_var("_function"))),
                            ],
                            vec![
                            (e_binop(e_binop(e_var("_class"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_call("strncmp", vec![e_var("_function"), e_str("mysqli_"), e_int(7)]), BinOp::Eq, e_int(0))), vec![
                                s_assign("_entry", e_var("_function")),
                            ]),
                        ],
                            None,
                        ),
                    ]),
                    s_expr(e_call("__elephc_builtin_warning", vec![e_ternary(e_binop(e_var("_entry"), BinOp::StrictEq, e_str("")), e_var("message"), e_binop(e_binop(e_var("_entry"), BinOp::Concat, e_str("(): ")), BinOp::Concat, e_var("message")))])),
                ]),
        )
        .method(
            method("runQuery")
                .private()
                .param("query", TypeExpr::Str)
                .param("unbuffered", TypeExpr::Bool)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_not(e_method_call(e_this(), "requireConnection", vec![])),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireNoPendingResults", vec![])),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "disableMultiStatements", vec![])),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("com_query")])),
                    s_assign("_stmt", e_method_call(e_this(), "prepareText", vec![e_var("query"), e_var("unbuffered")])),
                    s_if(
                        e_binop(e_var("_stmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "opFailed", vec![])),
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_rc", e_ternary(e_var("unbuffered"), e_call("elephc_pdo_mysql_execute", vec![e_var("_stmt")]), e_call("elephc_pdo_step", vec![e_var("_stmt")]))),
                    s_if(
                        e_binop(e_binop(e_var("_rc"), BinOp::Eq, e_int(0)), BinOp::And, e_var("unbuffered")),
                        vec![
                            s_assign("_rc", e_int(2)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephcAdoptBridgeError", vec![])),
                            s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                            s_expr(e_method_call(e_this(), "__elephcReportError", vec![])),
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_columnCount", e_call("elephc_pdo_column_count", vec![e_var("_stmt")])),
                    s_expr(e_method_call(e_this(), "__elephcHeaderStats", vec![e_var("_columnCount"), e_bool(true)])),
                    s_if(
                        e_binop(e_var("_columnCount"), BinOp::Eq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "refreshStatus", vec![])),
                            s_prop_assign(e_this(), "field_count", e_int(0)),
                            s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                            s_expr(e_method_call(e_this(), "clearError", vec![])),
                            s_return(e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "liveStmt", e_var("_stmt")),
                    s_prop_assign(e_this(), "liveStep", e_var("_rc")),
                    s_prop_assign(e_this(), "liveSetPending", e_bool(true)),
                    s_prop_assign(e_this(), "field_count", e_var("_columnCount")),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_int(2)),
                ]),
        )
        .method(
            method("prepareText")
                .private()
                .param("query", TypeExpr::Str)
                .param("unbuffered", TypeExpr::Bool)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_not(e_var("unbuffered")),
                        vec![
                            s_return(e_call("elephc_pdo_prepare", vec![e_this_prop("conn"), e_var("query"), e_int(1)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_pdo_set_buffered_query", vec![e_this_prop("conn"), e_int(0)])),
                    s_assign("_stmt", e_call("elephc_pdo_prepare", vec![e_this_prop("conn"), e_var("query"), e_int(1)])),
                    s_expr(e_call("elephc_pdo_set_buffered_query", vec![e_this_prop("conn"), e_int(1)])),
                    s_return(e_var("_stmt")),
                ]),
        )
        .method(
            method("enterSet")
                .private()
                .param("firstStep", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_cols", e_call("elephc_pdo_column_count", vec![e_this_prop("liveStmt")])),
                    s_expr(e_method_call(e_this(), "__elephcHeaderStats", vec![e_var("_cols"), e_bool(true)])),
                    s_if(
                        e_binop(e_var("_cols"), BinOp::Eq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "refreshStatus", vec![])),
                            s_prop_assign(e_this(), "field_count", e_int(0)),
                            s_expr(e_method_call(e_this(), "clearError", vec![])),
                            s_expr(e_method_call(e_this(), "finishSet", vec![])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "liveStep", e_var("firstStep")),
                    s_prop_assign(e_this(), "liveSetPending", e_bool(true)),
                    s_prop_assign(e_this(), "field_count", e_var("_cols")),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                ]),
        )
        .method(
            method("claimStored")
                .private()
                .returns(t_nullable(t_class("mysqli_result")))
                .body(vec![
                    s_if(
                        e_not(e_this_prop("liveSetPending")),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "liveSetPending", e_bool(false)),
                    s_assign("_cols", e_call("elephc_pdo_column_count", vec![e_this_prop("liveStmt")])),
                    s_assign("_result", e_method_call(e_this(), "drainResultSet", vec![e_this_prop("liveStmt"), e_this_prop("liveStep"), e_var("_cols")])),
                    s_if(
                        e_binop(e_var("_result"), BinOp::StrictEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_this(), "closeLive", vec![])),
                            s_expr(e_method_call(e_this(), "__elephcReportError", vec![])),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "finishSet", vec![])),
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("claimStream")
                .private()
                .returns(t_nullable(t_class("mysqli_result")))
                .body(vec![
                    s_if(
                        e_not(e_this_prop("liveSetPending")),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "liveSetPending", e_bool(false)),
                    s_prop_assign(e_this(), "streaming", e_bool(true)),
                    s_assign("_stmt", e_this_prop("liveStmt")),
                    s_assign("_cols", e_call("elephc_pdo_column_count", vec![e_var("_stmt")])),
                    s_assign("_names", e_array(vec![])),
                    s_assign("_tables", e_array(vec![])),
                    s_assign("_natives", e_array(vec![])),
                    s_assign("_flags", e_array(vec![])),
                    s_assign("_lens", e_array(vec![])),
                    s_assign("_decimals", e_array(vec![])),
                    s_assign("_kinds", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_cols"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_push("_names", e_call("elephc_pdo_column_name", vec![e_var("_stmt"), e_var("_i")])),
                        s_array_push("_tables", e_call("elephc_pdo_column_table_name", vec![e_var("_stmt"), e_var("_i")])),
                        s_assign("_native", e_call("elephc_pdo_column_native_type", vec![e_var("_stmt"), e_var("_i")])),
                        s_array_push("_natives", e_var("_native")),
                        s_array_push("_flags", e_call("elephc_pdo_column_flags", vec![e_var("_stmt"), e_var("_i")])),
                        s_array_push("_lens", e_call("elephc_pdo_column_len", vec![e_var("_stmt"), e_var("_i")])),
                        s_array_push("_decimals", e_call("elephc_pdo_column_precision", vec![e_var("_stmt"), e_var("_i")])),
                        s_array_push("_kinds", e_static_call("mysqli_result", "__elephcCellKind", vec![e_var("_native"), e_cast(CastType::Int, e_index(e_var("_flags"), e_var("_i"))), e_this_prop("optIntAndFloatNative"), e_bool(false)])),
                    ]),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("unbuffered_sets")])),
                    s_prop_assign(e_this(), "affected_rows", e_neg(e_int(1))),
                    s_prop_assign(e_this(), "insert_id", e_int(0)),
                    s_prop_assign(e_this(), "field_count", e_var("_cols")),
                    s_prop_assign(e_this(), "info", e_null()),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_static_call("mysqli_result", "__elephcFromStream", vec![e_this(), e_var("_stmt"), e_this_prop("liveStep"), e_var("_names"), e_var("_tables"), e_var("_natives"), e_var("_flags"), e_var("_lens"), e_var("_decimals"), e_var("_kinds")])),
                ]),
        )
        .method(
            method("__elephcStreamFinished")
                .private()
                .param("stmt", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_not(e_this_prop("streaming")), BinOp::Or, e_binop(e_this_prop("liveStmt"), BinOp::Lt, e_int(0))), BinOp::Or, e_binop(e_this_prop("liveStmt"), BinOp::NotEq, e_var("stmt"))),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "affected_rows", e_var("rows")),
                    s_expr(e_method_call(e_this(), "finishSet", vec![])),
                ]),
        )
        .method(
            method("finishSet")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "liveSetPending", e_bool(false)),
                    s_prop_assign(e_this(), "streaming", e_bool(false)),
                    s_prop_assign(e_this(), "warning_count", e_call("elephc_pdo_warning_count", vec![e_this_prop("conn")])),
                    s_prop_assign(e_this(), "multiMore", e_binop(e_call("elephc_pdo_mysql_more_results", vec![e_this_prop("liveStmt")]), BinOp::Eq, e_int(1))),
                    s_if(
                        e_not(e_this_prop("multiMore")),
                        vec![
                            s_expr(e_call("elephc_pdo_finalize", vec![e_this_prop("liveStmt")])),
                            s_prop_assign(e_this(), "liveStmt", e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("closeLive")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("liveStmt"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_expr(e_call("elephc_pdo_finalize", vec![e_this_prop("liveStmt")])),
                            s_prop_assign(e_this(), "liveStmt", e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "liveSetPending", e_bool(false)),
                    s_prop_assign(e_this(), "streaming", e_bool(false)),
                    s_prop_assign(e_this(), "multiMore", e_bool(false)),
                ]),
        )
        .method(
            method("drainResultSet")
                .private()
                .param("stmt", TypeExpr::Int)
                .param("firstStep", TypeExpr::Int)
                .param("cols", TypeExpr::Int)
                .returns(t_nullable(t_class("mysqli_result")))
                .body(vec![
                    s_assign("_names", e_array(vec![])),
                    s_assign("_tables", e_array(vec![])),
                    s_assign("_natives", e_array(vec![])),
                    s_assign("_flags", e_array(vec![])),
                    s_assign("_lens", e_array(vec![])),
                    s_assign("_decimals", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("cols"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_push("_names", e_call("elephc_pdo_column_name", vec![e_var("stmt"), e_var("_i")])),
                        s_array_push("_tables", e_call("elephc_pdo_column_table_name", vec![e_var("stmt"), e_var("_i")])),
                        s_array_push("_natives", e_call("elephc_pdo_column_native_type", vec![e_var("stmt"), e_var("_i")])),
                        s_array_push("_flags", e_call("elephc_pdo_column_flags", vec![e_var("stmt"), e_var("_i")])),
                        s_array_push("_lens", e_call("elephc_pdo_column_len", vec![e_var("stmt"), e_var("_i")])),
                        s_array_push("_decimals", e_call("elephc_pdo_column_precision", vec![e_var("stmt"), e_var("_i")])),
                    ]),
                    s_assign("_native", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("cols"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_push("_native", e_static_call("mysqli_result", "__elephcCellKind", vec![e_cast(CastType::String, e_index(e_var("_natives"), e_var("_i"))), e_cast(CastType::Int, e_index(e_var("_flags"), e_var("_i"))), e_this_prop("optIntAndFloatNative"), e_bool(false)])),
                    ]),
                    s_assign("_cells", e_array(vec![])),
                    s_assign("_rowCount", e_int(0)),
                    s_assign("_rc", e_ternary(e_binop(e_var("firstStep"), BinOp::Eq, e_int(2)), e_call("elephc_pdo_step", vec![e_var("stmt")]), e_var("firstStep"))),
                    s_while(e_binop(e_var("_rc"), BinOp::Eq, e_int(1)), vec![
                        s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("cols"))), Some(s_expr(e_post_inc("_i"))), vec![
                            s_array_push("_cells", e_static_call("mysqli_result", "__elephcConvertCell", vec![e_method_call(e_this(), "columnValue", vec![e_var("stmt"), e_var("_i")]), e_cast(CastType::Int, e_index(e_var("_native"), e_var("_i"))), e_cast(CastType::Int, e_index(e_var("_lens"), e_var("_i")))])),
                        ]),
                        s_assign("_rowCount", e_binop(e_var("_rowCount"), BinOp::Add, e_int(1))),
                        s_assign("_rc", e_call("elephc_pdo_step", vec![e_var("stmt")])),
                    ]),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephcAdoptBridgeError", vec![])),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("buffered_sets")])),
                    s_expr(e_method_call(e_this(), "__elephcStat", vec![e_str("rows_fetched_from_server_normal"), e_var("_rowCount"), e_bool(true)])),
                    s_expr(e_method_call(e_this(), "__elephcStat", vec![e_str("rows_buffered_from_client_normal"), e_var("_rowCount"), e_bool(true)])),
                    s_prop_assign(e_this(), "affected_rows", e_var("_rowCount")),
                    s_prop_assign(e_this(), "insert_id", e_int(0)),
                    s_prop_assign(e_this(), "field_count", e_var("cols")),
                    s_prop_assign(e_this(), "warning_count", e_call("elephc_pdo_warning_count", vec![e_this_prop("conn")])),
                    s_prop_assign(e_this(), "info", e_null()),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_assign("_result", e_static_call("mysqli_result", "__elephcFromDrain", vec![e_var("_cells"), e_var("_rowCount"), e_var("_names"), e_var("_tables"), e_var("_natives"), e_var("_flags"), e_var("_lens"), e_var("_decimals")])),
                    s_expr(e_method_call(e_var("_result"), "__elephcCountFor", vec![e_this(), e_bool(false)])),
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("columnValue")
                .private()
                .param("stmt", TypeExpr::Int)
                .param("index", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_assign("_type", e_call("elephc_pdo_column_type", vec![e_var("stmt"), e_var("index")])),
                    s_if(
                        e_binop(e_var("_type"), BinOp::Eq, e_int(1)),
                        vec![
                            s_return(e_call("elephc_pdo_column_int", vec![e_var("stmt"), e_var("index")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_type"), BinOp::Eq, e_int(2)),
                        vec![
                            s_return(e_call("elephc_pdo_column_double", vec![e_var("stmt"), e_var("index")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_type"), BinOp::Eq, e_int(5)),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_len", e_call("elephc_pdo_column_data_len", vec![e_var("stmt"), e_var("index")])),
                    s_if(
                        e_binop(e_var("_len"), BinOp::Gt, e_int(0)),
                        vec![
                            s_return(e_call("__elephc_ptr_read_string", vec![e_call("elephc_pdo_column_data_ptr", vec![e_var("stmt"), e_var("index")]), e_var("_len")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_str("")),
                ]),
        )
        .method(
            method("versionStringToInt")
                .private()
                .param("version", TypeExpr::Str)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_call("str_starts_with", vec![e_var("version"), e_str("mysql ")]),
                        vec![
                            s_assign("version", e_call("substr", vec![e_var("version"), e_int(6)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_parts", e_call("explode", vec![e_str("."), e_var("version")])),
                    s_assign("_major", e_cast(CastType::Int, e_index(e_var("_parts"), e_int(0)))),
                    s_assign("_minor", e_ternary(e_binop(e_call("count", vec![e_var("_parts")]), BinOp::Gt, e_int(1)), e_cast(CastType::Int, e_index(e_var("_parts"), e_int(1))), e_int(0))),
                    s_assign("_patch", e_ternary(e_binop(e_call("count", vec![e_var("_parts")]), BinOp::Gt, e_int(2)), e_cast(CastType::Int, e_index(e_var("_parts"), e_int(2))), e_int(0))),
                    s_return(e_binop(e_binop(e_binop(e_var("_major"), BinOp::Mul, e_int(10000)), BinOp::Add, e_binop(e_var("_minor"), BinOp::Mul, e_int(100))), BinOp::Add, e_var("_patch"))),
                ]),
        )
        .method(
            method("charsetIdentIsValid")
                .private()
                .param("charset", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_len", e_call("strlen", vec![e_var("charset")])),
                    s_if(
                        e_binop(e_var("_len"), BinOp::Eq, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_len"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_c", e_call("ord", vec![e_call("substr", vec![e_var("charset"), e_var("_i"), e_int(1)])])),
                        s_assign("_ok", e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_c"), BinOp::GtEq, e_int(48)), BinOp::And, e_binop(e_var("_c"), BinOp::LtEq, e_int(57))), BinOp::Or, e_binop(e_binop(e_var("_c"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("_c"), BinOp::LtEq, e_int(90)))), BinOp::Or, e_binop(e_binop(e_var("_c"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("_c"), BinOp::LtEq, e_int(122)))), BinOp::Or, e_binop(e_var("_c"), BinOp::Eq, e_int(95)))),
                        s_if(
                            e_not(e_var("_ok")),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_bool(true)),
                ]),
        )
        .build()
}
