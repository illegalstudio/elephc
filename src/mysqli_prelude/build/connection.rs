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
        .prop("affected_rows", TypeExpr::Int, Some(e_int(0)))
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
        .prop("insert_id", TypeExpr::Int, Some(e_int(0)))
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
        .private_prop("pendingResult", t_nullable(t_class("mysqli_result")), Some(e_null()))
        .private_prop("multiStmt", TypeExpr::Int, Some(e_neg(e_int(1))))
        .private_prop("multiMore", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("asyncState", TypeExpr::Int, Some(e_int(0)))
        .private_prop("asyncResult", t_nullable(t_class("mysqli_result")), Some(e_null()))
        .private_prop("asyncErrno", TypeExpr::Int, Some(e_int(0)))
        .private_prop("asyncError", TypeExpr::Str, Some(e_str("")))
        .private_prop("asyncSqlstate", TypeExpr::Str, Some(e_str("")))
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
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "real_connect", vec![e_var("hostname"), e_var("username"), e_var("password"), e_var("database"), e_var("port"), e_var("socket"), e_int(0)])),
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
                    s_if(
                        e_binop(e_this_prop("conn"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "releaseConnection", vec![])),
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
                            s_assign("_message", e_static_call("mysqli", "__elephcServerMessage", vec![e_call("elephc_pdo_last_open_error", vec![])])),
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
                                    s_assign("_code", e_int(2002)),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("connect_failure")])),
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
                    s_prop_assign(e_this(), "persistent", e_binop(e_var("_persistent"), BinOp::Eq, e_int(1))),
                    s_prop_assign(e_this(), "connect_errno", e_int(0)),
                    s_prop_assign(e_this(), "connect_error", e_null()),
                    s_static_prop_assign("mysqli", "lastConnectErrno", e_int(0)),
                    s_static_prop_assign("mysqli", "lastConnectError", e_str("")),
                    s_static_prop_assign("mysqli", "openLinks", e_binop(e_static_prop("mysqli", "openLinks"), BinOp::Add, e_int(1))),
                    s_if(
                        e_this_prop("persistent"),
                        vec![
                            s_static_prop_assign("mysqli", "openPersistentLinks", e_binop(e_static_prop("mysqli", "openPersistentLinks"), BinOp::Add, e_int(1))),
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("pconnect_success")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("connect_success")])),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("active_connections")])),
                    s_if(
                        e_this_prop("persistent"),
                        vec![
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("active_persistent_connections")])),
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
                                s_expr(e_method_call(e_this(), "opFailed", vec![])),
                                s_expr(e_method_call(e_this(), "releaseConnection", vec![])),
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
                    s_return(e_method_call(e_this(), "real_connect", vec![e_var("hostname"), e_var("username"), e_var("password"), e_var("database"), e_var("port"), e_var("socket"), e_int(0)])),
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
                        e_binop(e_this_prop("conn"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("explicit_close")])),
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("com_quit")])),
                            s_expr(e_method_call(e_this(), "releaseConnection", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("conn"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("implicit_close")])),
                            s_expr(e_method_call(e_this(), "releaseConnection", vec![])),
                        ],
                        vec![],
                        None,
                    ),
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
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
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
                    s_expr(e_method_call(e_this(), "multiClose", vec![])),
                    s_prop_assign(e_this(), "pendingResult", e_null()),
                    s_prop_assign(e_this(), "asyncState", e_int(0)),
                    s_prop_assign(e_this(), "asyncResult", e_null()),
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
                    s_if(
                        e_binop(e_call("elephc_pdo_exec", vec![e_this_prop("conn"), e_binop(e_str("KILL CONNECTION "), BinOp::Concat, e_var("process_id"))]), BinOp::Lt, e_int(0)),
                        vec![
                            s_if(
                                e_binop(e_var("process_id"), BinOp::Eq, e_this_prop("thread_id")),
                                vec![
                                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                                    s_return(e_bool(true)),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
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
                    s_assign("_sql", e_array(vec![])),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(1)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_array_push("_sql", e_str("FLUSH PRIVILEGES")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_array_push("_sql", e_str("FLUSH LOGS")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(4)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_array_push("_sql", e_str("FLUSH TABLES")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(8)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_array_push("_sql", e_str("FLUSH HOSTS")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(16)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_array_push("_sql", e_str("FLUSH STATUS")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(64)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_array_push("_sql", e_str("RESET SLAVE")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(128)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_array_push("_sql", e_str("RESET MASTER")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_n", e_call("count", vec![e_var("_sql")])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_if(
                            e_binop(e_call("elephc_pdo_exec", vec![e_this_prop("conn"), e_cast(CastType::String, e_index(e_var("_sql"), e_var("_i")))]), BinOp::Lt, e_int(0)),
                            vec![
                                s_return(e_method_call(e_this(), "opFailed", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
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
                        s_array_assign("_out", e_var("_name"), e_cast(CastType::String, e_null_coalesce(e_index(e_this_prop("connStats"), e_var("_name")), e_int(0)))),
                    ]),
                    s_return(e_var("_out")),
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
                    s_assign("_ident", e_call("str_replace", vec![e_str("`"), e_str("``"), e_var("database")])),
                    s_if(
                        e_binop(e_call("elephc_pdo_exec", vec![e_this_prop("conn"), e_binop(e_binop(e_str("USE `"), BinOp::Concat, e_var("_ident")), BinOp::Concat, e_str("`"))]), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
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
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("options")
                .param("option", TypeExpr::Int)
                .param("value", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
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
                            s_return(e_method_call(e_this(), "sendAsyncQuery", vec![e_var("query")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_code", e_method_call(e_this(), "runQuery", vec![e_var("query")])),
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
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_result", e_this_prop("pendingResult")),
                    s_prop_assign(e_this(), "pendingResult", e_null()),
                    s_if(
                        e_binop(e_var("_result"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_var("_result"), "__elephcSetType", vec![e_ternary(e_binop(e_var("_base"), BinOp::Eq, e_int(1)), e_int(1), e_int(0))])),
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("reap_async_query")
                .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::Bool]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_assign("_state", e_this_prop("asyncState")),
                    s_prop_assign(e_this(), "asyncState", e_int(0)),
                    s_if(
                        e_binop(e_var("_state"), BinOp::Eq, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_state"), BinOp::Eq, e_int(3)),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_this_prop("asyncErrno"), e_this_prop("asyncError"), e_this_prop("asyncSqlstate")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_state"), BinOp::Eq, e_int(1)),
                        vec![
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_result", e_this_prop("asyncResult")),
                    s_prop_assign(e_this(), "asyncResult", e_null()),
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
                .param_by_ref("read", Some(t_array()))
                .param_by_ref("error", Some(t_array()))
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
                    s_assign("_ready", e_array(vec![])),
                    s_assign("_rejected", e_array(vec![])),
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
                                s_array_push("_ready", e_var("_link")),
                            ],
                            vec![],
                            Some(vec![
                            s_array_push("_rejected", e_var("_link")),
                        ]),
                        ),
                    ]),
                    s_assign("read", e_var("_ready")),
                    s_assign("error", e_array(vec![])),
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
                    s_assign("_charset", e_call("elephc_pdo_mysql_charset", vec![e_this_prop("conn")])),
                    s_assign("_info", e_new("stdClass", vec![])),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("charset")), e_var("_charset"))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("collation")), e_str(""))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("dir")), e_str(""))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("min_length")), e_int(0))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("max_length")), e_int(0))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("number")), e_int(0))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("state")), e_int(0))),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("comment")), e_str(""))),
                    s_return(e_var("_info")),
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
                    s_return(e_binop(e_method_call(e_this(), "runQuery", vec![e_var("query")]), BinOp::NotEq, e_int(0))),
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
                    s_expr(e_method_call(e_this(), "multiClose", vec![])),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("com_query")])),
                    s_assign("_stmt", e_call("elephc_pdo_prepare", vec![e_this_prop("conn"), e_var("query"), e_int(1)])),
                    s_if(
                        e_binop(e_var("_stmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_rc", e_call("elephc_pdo_step", vec![e_var("_stmt")])),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "opFailed", vec![])),
                            s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "multiStmt", e_var("_stmt")),
                    s_return(e_method_call(e_this(), "multiDrainCurrent", vec![e_var("_rc")])),
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
                        e_binop(e_not(e_this_prop("multiMore")), BinOp::Or, e_binop(e_this_prop("multiStmt"), BinOp::Lt, e_int(0))),
                        vec![
                            s_prop_assign(e_this(), "multiMore", e_bool(false)),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_rc", e_call("elephc_pdo_step", vec![e_this_prop("multiStmt")])),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "opFailed", vec![])),
                            s_expr(e_method_call(e_this(), "multiClose", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "multiDrainCurrent", vec![e_var("_rc")])),
                ]),
        )
        .method(
            method("store_result")
                .param_default("mode", TypeExpr::Int, e_int(0))
                .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_assign("_ignoredMode", e_var("mode")),
                    s_assign("_result", e_this_prop("pendingResult")),
                    s_if(
                        e_binop(e_var("_result"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "pendingResult", e_null()),
                    s_expr(e_method_call(e_var("_result"), "__elephcSetType", vec![e_int(0)])),
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("use_result")
                .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![])),
                    s_assign("_result", e_this_prop("pendingResult")),
                    s_if(
                        e_binop(e_var("_result"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "pendingResult", e_null()),
                    s_expr(e_method_call(e_var("_result"), "__elephcSetType", vec![e_int(1)])),
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
            method("__elephcServerMessage")
                .private()
                .static_()
                .param("message", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
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
                        e_this_prop("persistent"),
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
                    s_prop_assign(e_this(), "errno", e_call("elephc_pdo_errcode", vec![e_this_prop("conn")])),
                    s_prop_assign(e_this(), "error", e_static_call("mysqli", "__elephcServerMessage", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
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
                    s_return(e_binop(e_this_prop("asyncState"), BinOp::NotEq, e_int(0))),
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
                    s_expr(e_method_call(e_this(), "multiClose", vec![])),
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
                    s_prop_assign(e_this(), "pendingResult", e_null()),
                    s_prop_assign(e_this(), "asyncState", e_int(0)),
                    s_prop_assign(e_this(), "asyncResult", e_null()),
                    s_static_prop_assign("mysqli", "openLinks", e_binop(e_static_prop("mysqli", "openLinks"), BinOp::Sub, e_int(1))),
                    s_if(
                        e_this_prop("persistent"),
                        vec![
                            s_static_prop_assign("mysqli", "openPersistentLinks", e_binop(e_static_prop("mysqli", "openPersistentLinks"), BinOp::Sub, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "dropActive", vec![])),
                    s_prop_assign(e_this(), "persistent", e_bool(false)),
                ]),
        )
        .method(
            method("sendAsyncQuery")
                .private()
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
                    s_assign("_mode", e_static_prop("mysqli", "reportMode")),
                    s_static_prop_assign("mysqli", "reportMode", e_int(0)),
                    s_assign("_code", e_method_call(e_this(), "runQuery", vec![e_var("query")])),
                    s_static_prop_assign("mysqli", "reportMode", e_var("_mode")),
                    s_if(
                        e_binop(e_var("_code"), BinOp::Eq, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "asyncState", e_int(3)),
                            s_prop_assign(e_this(), "asyncErrno", e_this_prop("errno")),
                            s_prop_assign(e_this(), "asyncError", e_this_prop("error")),
                            s_prop_assign(e_this(), "asyncSqlstate", e_this_prop("sqlstate")),
                            s_expr(e_method_call(e_this(), "clearError", vec![])),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_code"), BinOp::Eq, e_int(1)),
                        vec![
                            s_prop_assign(e_this(), "asyncState", e_int(1)),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "asyncState", e_int(2)),
                    s_prop_assign(e_this(), "asyncResult", e_this_prop("pendingResult")),
                    s_prop_assign(e_this(), "pendingResult", e_null()),
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
                    s_return(e_binop(e_binop(e_binop(e_this_prop("multiStmt"), BinOp::GtEq, e_int(0)), BinOp::Or, e_this_prop("multiMore")), BinOp::Or, e_binop(e_this_prop("pendingResult"), BinOp::StrictNotEq, e_null()))),
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
            method("refreshStatus")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "affected_rows", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                    s_prop_assign(e_this(), "insert_id", e_call("elephc_pdo_last_insert_id", vec![e_this_prop("conn"), e_str("")])),
                    s_prop_assign(e_this(), "warning_count", e_call("elephc_pdo_warning_count", vec![e_this_prop("conn")])),
                    s_assign("_info", e_call("elephc_pdo_mysql_info", vec![e_this_prop("conn")])),
                    s_prop_assign(e_this(), "info", e_ternary(e_binop(e_var("_info"), BinOp::StrictEq, e_str("")), e_null(), e_var("_info"))),
                ]),
        )
        .method(
            method("requireInitialized")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("conn"), BinOp::Lt, e_int(0)),
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
                    s_expr(e_method_call(e_this(), "report", vec![e_var("message"), e_var("errno"), e_var("sqlstate")])),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("opFailed")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "errno", e_call("elephc_pdo_errcode", vec![e_this_prop("conn")])),
                    s_prop_assign(e_this(), "error", e_static_call("mysqli", "__elephcServerMessage", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
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
                            s_expr(e_call("fwrite", vec![e_const("STDERR"), e_binop(e_binop(e_str("mysqli error: "), BinOp::Concat, e_var("message")), BinOp::Concat, e_str("\n"))])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("runQuery")
                .private()
                .param("query", TypeExpr::Str)
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
                    s_if(
                        e_binop(e_binop(e_call("strpos", vec![e_var("query"), e_str(";")]), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_call("elephc_pdo_sql_has_multiple_statements", vec![e_this_prop("conn"), e_var("query"), e_call("strlen", vec![e_var("query")])]), BinOp::NotEq, e_int(0))),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(1064), e_str("elephc mysqli does not support multiple statements in mysqli::query(); use mysqli::multi_query()"), e_str("42000")])),
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("com_query")])),
                    s_assign("_stmt", e_call("elephc_pdo_prepare", vec![e_this_prop("conn"), e_var("query"), e_int(1)])),
                    s_if(
                        e_binop(e_var("_stmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "opFailed", vec![])),
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_rc", e_call("elephc_pdo_step", vec![e_var("_stmt")])),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "opFailed", vec![])),
                            s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_columnCount", e_call("elephc_pdo_column_count", vec![e_var("_stmt")])),
                    s_if(
                        e_binop(e_var("_columnCount"), BinOp::Eq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("non_result_set_queries")])),
                            s_expr(e_method_call(e_this(), "refreshStatus", vec![])),
                            s_prop_assign(e_this(), "field_count", e_int(0)),
                            s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                            s_expr(e_method_call(e_this(), "clearError", vec![])),
                            s_prop_assign(e_this(), "pendingResult", e_null()),
                            s_return(e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_result", e_method_call(e_this(), "drainResultSet", vec![e_var("_stmt"), e_var("_rc"), e_var("_columnCount")])),
                    s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                    s_if(
                        e_binop(e_var("_result"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "pendingResult", e_var("_result")),
                    s_return(e_int(2)),
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
                        s_array_push("_native", e_ternary(e_this_prop("optIntAndFloatNative"), e_method_call(e_this(), "nativeKind", vec![e_cast(CastType::String, e_index(e_var("_natives"), e_var("_i")))]), e_int(0))),
                    ]),
                    s_assign("_cells", e_array(vec![])),
                    s_assign("_rowCount", e_int(0)),
                    s_assign("_rc", e_var("firstStep")),
                    s_while(e_binop(e_var("_rc"), BinOp::Eq, e_int(1)), vec![
                        s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("cols"))), Some(s_expr(e_post_inc("_i"))), vec![
                            s_assign("_value", e_method_call(e_this(), "columnValue", vec![e_var("stmt"), e_var("_i")])),
                            s_assign("_kind", e_cast(CastType::Int, e_index(e_var("_native"), e_var("_i")))),
                            s_if(
                                e_binop(e_binop(e_var("_value"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("_kind"), BinOp::Eq, e_int(1))),
                                vec![
                                    s_array_push("_cells", e_cast(CastType::Int, e_var("_value"))),
                                ],
                                vec![
                                (e_binop(e_binop(e_var("_value"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("_kind"), BinOp::Eq, e_int(2))), vec![
                                    s_array_push("_cells", e_cast(CastType::Float, e_var("_value"))),
                                ]),
                            ],
                                Some(vec![
                                s_array_push("_cells", e_var("_value")),
                            ]),
                            ),
                        ]),
                        s_assign("_rowCount", e_binop(e_var("_rowCount"), BinOp::Add, e_int(1))),
                        s_assign("_rc", e_call("elephc_pdo_step", vec![e_var("stmt")])),
                    ]),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "opFailed", vec![])),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("result_set_queries")])),
                    s_expr(e_method_call(e_this(), "bump", vec![e_str("buffered_sets")])),
                    s_prop_array_assign(e_this(), "connStats", e_str("rows_fetched_from_server_normal"), e_binop(e_cast(CastType::Int, e_null_coalesce(e_index(e_this_prop("connStats"), e_str("rows_fetched_from_server_normal")), e_int(0))), BinOp::Add, e_var("_rowCount"))),
                    s_prop_array_assign(e_this(), "connStats", e_str("rows_buffered_from_client_normal"), e_binop(e_cast(CastType::Int, e_null_coalesce(e_index(e_this_prop("connStats"), e_str("rows_buffered_from_client_normal")), e_int(0))), BinOp::Add, e_var("_rowCount"))),
                    s_prop_assign(e_this(), "affected_rows", e_var("_rowCount")),
                    s_prop_assign(e_this(), "insert_id", e_int(0)),
                    s_prop_assign(e_this(), "field_count", e_var("cols")),
                    s_prop_assign(e_this(), "warning_count", e_call("elephc_pdo_warning_count", vec![e_this_prop("conn")])),
                    s_prop_assign(e_this(), "info", e_null()),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_static_call("mysqli_result", "__elephcFromDrain", vec![e_var("_cells"), e_var("_rowCount"), e_var("_names"), e_var("_tables"), e_var("_natives"), e_var("_flags"), e_var("_lens"), e_var("_decimals")])),
                ]),
        )
        .method(
            method("nativeKind")
                .private()
                .param("native", TypeExpr::Str)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("native"), BinOp::StrictEq, e_str("TINY")), BinOp::Or, e_binop(e_var("native"), BinOp::StrictEq, e_str("SHORT"))), BinOp::Or, e_binop(e_var("native"), BinOp::StrictEq, e_str("LONG"))), BinOp::Or, e_binop(e_var("native"), BinOp::StrictEq, e_str("LONGLONG"))), BinOp::Or, e_binop(e_var("native"), BinOp::StrictEq, e_str("INT24"))), BinOp::Or, e_binop(e_var("native"), BinOp::StrictEq, e_str("YEAR"))),
                        vec![
                            s_return(e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("native"), BinOp::StrictEq, e_str("FLOAT")), BinOp::Or, e_binop(e_var("native"), BinOp::StrictEq, e_str("DOUBLE"))),
                        vec![
                            s_return(e_int(2)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_int(0)),
                ]),
        )
        .method(
            method("multiDrainCurrent")
                .private()
                .param("firstStep", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_stmt", e_this_prop("multiStmt")),
                    s_assign("_cols", e_call("elephc_pdo_column_count", vec![e_var("_stmt")])),
                    s_if(
                        e_binop(e_var("_cols"), BinOp::Eq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "bump", vec![e_str("non_result_set_queries")])),
                            s_expr(e_method_call(e_this(), "refreshStatus", vec![])),
                            s_prop_assign(e_this(), "field_count", e_int(0)),
                            s_prop_assign(e_this(), "pendingResult", e_null()),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("_result", e_method_call(e_this(), "drainResultSet", vec![e_var("_stmt"), e_var("firstStep"), e_var("_cols")])),
                        s_if(
                            e_binop(e_var("_result"), BinOp::StrictEq, e_null()),
                            vec![
                                s_expr(e_method_call(e_this(), "multiClose", vec![])),
                                s_prop_assign(e_this(), "pendingResult", e_null()),
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "pendingResult", e_var("_result")),
                    ]),
                    ),
                    s_prop_assign(e_this(), "warning_count", e_call("elephc_pdo_warning_count", vec![e_this_prop("conn")])),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_prop_assign(e_this(), "multiMore", e_binop(e_call("elephc_pdo_next_rowset", vec![e_var("_stmt")]), BinOp::Eq, e_int(1))),
                    s_if(
                        e_not(e_this_prop("multiMore")),
                        vec![
                            s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                            s_prop_assign(e_this(), "multiStmt", e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("multiClose")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("multiStmt"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_expr(e_call("elephc_pdo_finalize", vec![e_this_prop("multiStmt")])),
                            s_prop_assign(e_this(), "multiStmt", e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "multiMore", e_bool(false)),
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
