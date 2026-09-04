//! Purpose:
//! Builds the `mysqli` connection class as AST: DSN construction over
//! `elephc_pdo_open_persistent`, connect-time and per-op error bookkeeping
//! (`connect_errno`/`errno`/`error_list`), `mysqli_report` dispatch, escaping,
//! charset, `select_db`, ping, transactions, and the multi_query batch state.
//!
//! Called from:
//! - `crate::mysqli_prelude::build::mysqli_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from `mysqli_prelude::connection::SRC` (`synthetic_class::transcribe`);
//!   the oracle `built_declarations_match_the_php_for_every_version` compares the
//!   built class against that PHP for every profile. The two version gates the
//!   PHP assembly used to apply as text rewrites are conditionals here:
//!   `$reportMode`'s default (0 under 8.0, 3 from 8.1) and `execute_query` (8.2+).
//! - `$conn = -1` means "not connected" (`mysqli_init()` / argument-less
//!   `new mysqli()`); a successful `real_connect` stores the bridge handle. The
//!   DSN prefix is always forced to `mysql:` and a successful open whose
//!   `elephc_pdo_driver_name` is not "mysql" is rejected (belt-and-braces).
//! - Failure dispatch follows `mysqli::$reportMode` (see `exception.rs`):
//!   STRICT throws `mysqli_sql_exception`, ERROR writes to STDERR and the
//!   caller returns `false`, OFF is silent — mirroring PHP 8.1+'s default.
//!   `PDOException` is never thrown.
//! - `real_escape_string` reuses the MySQL branch of `PDO::quote()` minus the
//!   wrapping quotes and the `_binary` introducer, including the
//!   `NO_BACKSLASH_ESCAPES` quote-doubling fallback (mysqlnd's own behavior).
//! - Method-local variables use the `$_` prefix (same checker clash rule as the
//!   PDO prelude). Public properties are refreshed after operations; writes to
//!   them stick (documented divergence, no write barriers in v1).

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
    e_int,
    e_method_call,
    e_neg,
    e_new,
    e_not,
    e_null,
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
    s_array_push,
    s_assign,
    s_expr,
    s_for,
    s_if,
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
        // Opaque elephc_pdo bridge connection handle; -1 = not connected. Not part of PHP's
        // surface: private, handed to mysqli_stmt through its factory.
        .private_prop("conn", TypeExpr::Int, Some(e_neg(e_int(1))))
        // Process-wide mysqli_report() mode. PHP 8.1 changed the default from
        // MYSQLI_REPORT_OFF (0) to MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT (3);
        // the literal default is baked per `--php-version` here (the PHP form
        // rewrote it at assembly time).
        .static_prop(
            "reportMode",
            TypeExpr::Int,
            Some(e_int(if php_version >= PhpVersion::Php81 { 3 } else { 0 })),
        )
        // Process-wide last-connect failure, read by the no-argument procedural
        // mysqli_connect_errno() / mysqli_connect_error() exactly like PHP's globals; updated by
        // every construct / real_connect attempt.
        .static_prop("lastConnectErrno", TypeExpr::Int, Some(e_int(0)))
        .static_prop("lastConnectError", TypeExpr::Str, Some(e_str("")))
        // Public properties refreshed after operations (writes stick; documented divergence — no
        // write barriers in v1).
        .prop("affected_rows", TypeExpr::Int, Some(e_int(0)))
        .prop("connect_errno", TypeExpr::Int, Some(e_int(0)))
        .prop("connect_error", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("errno", TypeExpr::Int, Some(e_int(0)))
        .prop("error", TypeExpr::Str, Some(e_str("")))
        .prop("error_list", t_array(), Some(e_array(vec![])))
        .prop("field_count", TypeExpr::Int, Some(e_int(0)))
        .prop("client_info", TypeExpr::Str, Some(e_str("")))
        .prop("client_version", TypeExpr::Int, Some(e_int(0)))
        .prop("host_info", TypeExpr::Str, Some(e_str("")))
        .prop("protocol_version", TypeExpr::Int, Some(e_int(10)))
        .prop("server_info", TypeExpr::Str, Some(e_str("")))
        .prop("server_version", TypeExpr::Int, Some(e_int(0)))
        .prop("info", TypeExpr::Str, Some(e_str("")))
        .prop("insert_id", TypeExpr::Int, Some(e_int(0)))
        .prop("sqlstate", TypeExpr::Str, Some(e_str("00000")))
        .prop("thread_id", TypeExpr::Int, Some(e_int(0)))
        .prop("warning_count", TypeExpr::Int, Some(e_int(0)))
        // mysqli_options() values collected before real_connect applies them.
        .private_prop("optConnectTimeout", TypeExpr::Int, Some(e_int(0)))
        .private_prop("optInitCommand", TypeExpr::Str, Some(e_str("")))
        .private_prop("optCharsetName", TypeExpr::Str, Some(e_str("")))
        // Buffered result produced by real_query() and picked up by store_result() (and, in the
        // multi_query path, by next_result()).
        .private_prop("pendingResult", t_nullable(t_class("mysqli_result")), Some(e_null()))
        // multi_query() batch state: the live bridge statement (kept alive until every result set
        // is consumed) and the eager next_rowset probe verdict.
        .private_prop("multiStmt", TypeExpr::Int, Some(e_neg(e_int(1))))
        .private_prop("multiMore", TypeExpr::Bool, Some(e_bool(false)))
        .method(
            method("__construct")
                .param_default("hostname", t_nullable(TypeExpr::Str), e_null())
                .param_default("username", t_nullable(TypeExpr::Str), e_null())
                .param_default("password", t_nullable(TypeExpr::Str), e_null())
                .param_default("database", t_nullable(TypeExpr::Str), e_null())
                .param_default("port", t_nullable(TypeExpr::Int), e_null())
                .param_default("socket", t_nullable(TypeExpr::Str), e_null())
                .body(vec![
                    // The argument-less constructor is mysqli_init(): no connection attempt until
                    // real_connect() (php-src behavior).
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
                    // MYSQLI_CLIENT_SSL is declared but unsupported: fail loudly rather than
                    // silently connecting in cleartext.
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2048)), BinOp::NotEq, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "connectFailure", vec![e_int(2054), e_str("elephc mysqli does not support MYSQLI_CLIENT_SSL; use PDO MySQL TLS attributes"), e_str("HY000")])),
                        ],
                        vec![],
                        None,
                    ),
                    // php-src reconnect semantics: mysqlnd closes the existing connection before
                    // dialing the new one, so a second real_connect never strands the previous
                    // bridge handle (or leaves a persistent handle checked out of the pool).
                    s_if(
                        e_binop(e_this_prop("conn"), BinOp::GtEq, e_int(0)),
                        vec![
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
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_host", e_ternary(e_binop(e_var("hostname"), BinOp::StrictEq, e_null()), e_str("localhost"), e_var("hostname"))),
                    s_assign("_persistent", e_int(0)),
                    // php-src compares the `p:` persistent-connection prefix case-insensitively,
                    // so `P:host` is persistent too; the real host is the remainder.
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
                    // SECURITY: host, dbname, and unix_socket are folded verbatim into the bridge
                    // DSN, which splits on ';' and applies the LAST duplicate of a directive;
                    // unlike user/password these three are NOT percent-decoded by the bridge, so a
                    // ';' in any of them would inject a second directive (e.g.
                    // host="localhost;host=attacker" redirects the connection). None of the three
                    // ever legitimately contains a ';', so reject it rather than silently trusting
                    // the crafted value.
                    s_if(
                        e_binop(e_binop(e_binop(e_call("strpos", vec![e_var("_host"), e_str(";")]), BinOp::StrictNotEq, e_bool(false)), BinOp::Or, e_binop(e_binop(e_var("database"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("strpos", vec![e_var("database"), e_str(";")]), BinOp::StrictNotEq, e_bool(false)))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("_socket"), e_str(";")]), BinOp::StrictNotEq, e_bool(false))),
                        vec![
                            s_return(e_method_call(e_this(), "connectFailure", vec![e_int(2002), e_str("elephc mysqli: host, database, and socket must not contain ';'"), e_str("HY000")])),
                        ],
                        vec![],
                        None,
                    ),
                    // php-src mysqli honors the socket only when the host is empty or exactly
                    // "localhost"; any other host goes over TCP.
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
                    // '%' first, so the '%' introduced by encoding ';' is not itself re-encoded;
                    // the bridge percent-decodes user=/password= DSN values (F-CORE-02), so a ';'
                    // or '%' inside a credential survives the DSN's split on ';'. An
                    // explicitly-passed empty credential is still transmitted (no !== "" gate).
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
                    // Client flags map onto the bridge's packed driver config (same format PDO
                    // packs for $my_driver_config); FOUND_ROWS rides its own argument.
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
                    s_assign("_foundRows", e_ternary(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2)), BinOp::NotEq, e_int(0)), e_int(1), e_int(0))),
                    s_assign("_conn", e_call("elephc_pdo_open_persistent", vec![e_var("_dsn"), e_var("_persistent"), e_int(0), e_this_prop("optInitCommand"), e_str(""), e_var("_foundRows"), e_str(""), e_var("_driverConfig")])),
                    s_if(
                        e_binop(e_var("_conn"), BinOp::Lt, e_int(0)),
                        vec![
                            s_assign("_message", e_call("elephc_pdo_last_open_error", vec![])),
                            s_assign("_state", e_call("elephc_pdo_last_open_sqlstate", vec![])),
                            s_if(
                                e_binop(e_var("_state"), BinOp::StrictEq, e_str("")),
                                vec![
                                    // Connect-time network failure default, same class the PDO
                                    // mysql driver falls back to.
                                    s_assign("_state", e_str("HY000")),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_code", e_call("elephc_pdo_last_open_native_code", vec![])),
                            s_if(
                                e_binop(e_var("_code"), BinOp::Eq, e_int(0)),
                                vec![
                                    // CR_CONN_HOST_ERROR: generic client-side connect failure.
                                    s_assign("_code", e_int(2002)),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_method_call(e_this(), "connectFailure", vec![e_var("_code"), e_var("_message"), e_var("_state")])),
                        ],
                        vec![],
                        None,
                    ),
                    // Cannot happen while the DSN prefix is forced to mysql:, but keep the guard:
                    // a non-mysql handle must never become a mysqli connection.
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
                    s_prop_assign(e_this(), "connect_errno", e_int(0)),
                    s_prop_assign(e_this(), "connect_error", e_null()),
                    s_static_prop_assign("mysqli", "lastConnectErrno", e_int(0)),
                    s_static_prop_assign("mysqli", "lastConnectError", e_str("")),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    // Connection information, refreshed once per connect.
                    s_prop_assign(e_this(), "host_info", e_call("elephc_pdo_connection_status", vec![e_var("_conn")])),
                    s_prop_assign(e_this(), "server_info", e_call("elephc_pdo_server_version", vec![e_var("_conn")])),
                    s_prop_assign(e_this(), "server_version", e_method_call(e_this(), "versionStringToInt", vec![e_this_prop("server_info")])),
                    s_prop_assign(e_this(), "client_info", e_call("elephc_pdo_client_version", vec![e_var("_conn")])),
                    s_prop_assign(e_this(), "client_version", e_method_call(e_this(), "versionStringToInt", vec![e_this_prop("client_info")])),
                    // Both come from the handshake with ZERO round-trips, like php-src (which
                    // reads them off the handshake packet): the server thread id straight from the
                    // bridge, and the negotiated client charset is utf8mb4 (the charset the mysql
                    // client sends in its handshake response for any server >= 5.5.3).
                    // character_set_name() then answers from this client-side state without ever
                    // issuing a statement (so it stays usable mid-batch).
                    s_prop_assign(e_this(), "thread_id", e_call("elephc_pdo_mysql_thread_id", vec![e_var("_conn")])),
                    s_prop_assign(e_this(), "warning_count", e_call("elephc_pdo_warning_count", vec![e_var("_conn")])),
                    s_if(
                        e_binop(e_this_prop("optCharsetName"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            // MYSQLI_SET_CHARSET_NAME collected before connect applies now.
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
                    // Alias of the constructor's connect path (php-src keeps it for backwards
                    // compatibility with the old procedural object).
                    s_return(e_method_call(e_this(), "real_connect", vec![e_var("hostname"), e_var("username"), e_var("password"), e_var("database"), e_var("port"), e_var("socket"), e_int(0)])),
                ]),
        )
        .method(
            method("close")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("conn"), BinOp::GtEq, e_int(0)),
                        vec![
                            // Finalize any live multi_query batch statement before the connection
                            // handle goes back to the bridge.
                            s_expr(e_method_call(e_this(), "multiClose", vec![])),
                            // Roll an open transaction back first (matching PHP and keeping a
                            // persistent handle clean when it returns to the pool).
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
                            // An unconsumed buffered result dies with the connection (php-src
                            // frees all pending state on close): without this, a later
                            // real_connect() on the same object — which skips its own cleanup
                            // because $conn is already -1 — would inherit the stale pending result
                            // and raise a spurious 2014 on the first statement.
                            s_prop_assign(e_this(), "pendingResult", e_null()),
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
                    s_expr(e_method_call(e_this(), "close", vec![])),
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
                    // A cheap round-trip; see the plan's elephc_pdo_ping escape hatch if this ever
                    // eats a pending multi_query result.
                    s_if(
                        e_binop(e_call("elephc_pdo_exec", vec![e_this_prop("conn"), e_str("SELECT 1")]), BinOp::Lt, e_int(0)),
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
                    // Same [A-Za-z0-9_] identifier filter the PDO DSN charset key uses: a charset
                    // name is an identifier, never quoted, so anything else is rejected before it
                    // can smuggle SQL.
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
                    // php 8: an Error on an unconnected object (not "").
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![e_str("mysqli::character_set_name")])),
                    // The connection's live charset, tracked by the bridge from the handshake +
                    // every SET NAMES (no round-trip); true even after a MYSQLI_INIT_COMMAND "SET
                    // NAMES …" or a reused persistent connection.
                    s_return(e_call("elephc_pdo_mysql_charset", vec![e_this_prop("conn")])),
                ]),
        )
        .method(
            method("real_escape_string")
                .param("string", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    // php 8: real_escape_string on an unconnected object is an Error.
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![e_str("mysqli::real_escape_string")])),
                    // SECURITY: charset-aware escaping through the bridge, which uses the
                    // connection's OWN tracked charset (never assuming utf8mb4) and its
                    // NO_BACKSLASH_ESCAPES state — pure byte substitution is injectable under
                    // gbk/big5/sjis/cp932/euckr/… (the trailing-byte breakout). The length-counted
                    // read preserves embedded NUL bytes.
                    s_assign("_len", e_call("elephc_pdo_real_escape_string", vec![e_this_prop("conn"), e_var("string"), e_call("strlen", vec![e_var("string")])])),
                    s_if(
                        e_binop(e_var("_len"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_len"), BinOp::Eq, e_int(0)),
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
        .method(
            method("begin_transaction")
                .param_default("flags", TypeExpr::Int, e_int(0))
                .param_default("name", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    // php-src: $name is a SQL COMMENT on START TRANSACTION, never a savepoint; the
                    // empty-name ValueError is raised BEFORE any SQL goes to the server (unlike
                    // the old ordering, which left an open transaction).
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
                    // php composes the whole statement once (captured wire: "START TRANSACTION
                    // WITH CONSISTENT SNAPSHOT, READ WRITE"); the bridge's note_transaction_sql
                    // updates in_transaction from raw START TRANSACTION too, so close()/__destruct
                    // still auto-rollback.
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
                    // php: COMMIT [AND [NO] CHAIN] [[NO] RELEASE] /*name*/ — a real COMMIT, never
                    // a RELEASE SAVEPOINT (the old code never actually committed).
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
                            s_throw(e_new("ValueError", vec![e_str("mysqli::savepoint(): Argument #1 ($name) cannot be empty")])),
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
                            s_throw(e_new("ValueError", vec![e_str("mysqli::release_savepoint(): Argument #1 ($name) cannot be empty")])),
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
                    // The three locked option ids; anything else is unsupported and fails loudly
                    // with `false` (php-src also returns false for unknown options).
                    // MYSQLI_OPT_CONNECT_TIMEOUT (0) -> DSN connect_timeout at connect.
                    s_if(
                        e_binop(e_var("option"), BinOp::Eq, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "optConnectTimeout", e_cast(CastType::Int, e_var("value"))),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    // MYSQLI_INIT_COMMAND (3) -> $my_init_command at connect.
                    s_if(
                        e_binop(e_var("option"), BinOp::Eq, e_int(3)),
                        vec![
                            s_prop_assign(e_this(), "optInitCommand", e_cast(CastType::String, e_var("value"))),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    // MYSQLI_SET_CHARSET_NAME (7) -> SET NAMES after connect.
                    s_if(
                        e_binop(e_var("option"), BinOp::Eq, e_int(7)),
                        vec![
                            s_prop_assign(e_this(), "optCharsetName", e_cast(CastType::String, e_var("value"))),
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
                .param_default("resultmode", TypeExpr::Int, e_int(0))
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
                    s_if(
                        e_binop(e_binop(e_var("resultmode"), BinOp::NotEq, e_int(0)), BinOp::And, e_binop(e_var("resultmode"), BinOp::NotEq, e_int(1))),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli::query(): Argument #2 ($result_mode) must be either MYSQLI_STORE_RESULT or MYSQLI_USE_RESULT")])),
                        ],
                        vec![],
                        None,
                    ),
                    // MYSQLI_USE_RESULT (1) is accepted but still buffered — documented
                    // divergence; true unbuffered use_result is out of scope.
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
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("prepare")
                .param("query", TypeExpr::Str)
                .returns(t_union(vec![t_class("mysqli_stmt"), TypeExpr::False]))
                .body(vec![
                    s_if(
                        e_binop(e_var("query"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli::prepare(): Argument #1 ($query) must not be empty")])),
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
                    // Native (non-emulated) prepare: real `?` placeholders on the server.
                    s_assign("_handle", e_call("elephc_pdo_prepare", vec![e_this_prop("conn"), e_var("query"), e_int(0)])),
                    s_if(
                        e_binop(e_var("_handle"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "opFailed", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_static_call("mysqli_stmt", "__elephcFromPrepare", vec![e_this(), e_this_prop("conn"), e_var("_handle"), e_var("query")])),
                ]),
        )
        .method(
            method("stmt_init")
                .returns(t_class("mysqli_stmt"))
                .body(vec![
                    // The two-step form: an unprepared statement to be prepared with
                    // $stmt->prepare($sql) (or mysqli_stmt_prepare()).
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![e_str("mysqli::stmt_init")])),
                    s_return(e_static_call("mysqli_stmt", "__elephcInit", vec![e_this(), e_this_prop("conn")])),
                ]),
        )
        .method(
            method("get_charset")
                .returns(t_mixed())
                .body(vec![
                    // php returns a stdClass describing the connection charset. elephc knows the
                    // negotiated name (utf8mb4 by default / whatever set_charset set); the numeric
                    // collation/state/comment fields the bridge does not expose are reported as 0
                    // / "" (documented). The common `->charset` read is exact.
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![e_str("mysqli::get_charset")])),
                    s_assign("_info", e_new("stdClass", vec![])),
                    s_expr(e_assign(e_dyn_prop(e_var("_info"), e_str("charset")), e_call("elephc_pdo_mysql_charset", vec![e_this_prop("conn")]))),
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
        // `mysqli::execute_query` — prepare + execute($params) + get_result in one
        // call — is PHP 8.2+; under earlier profiles the method is ABSENT, not a stub.
        .when(php_version >= PhpVersion::Php82, |class| {
            class.method(
                method("execute_query")
                    .param("query", TypeExpr::Str)
                    .param_default("params", t_nullable(t_array()), e_null())
                    .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::Bool]))
                    .body(vec![
                        // prepare + execute($params) + get_result in one call (PHP 8.2+).
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
                        // Non-select: mirror the statement's outcome on the connection and report
                        // success as `true`, like mysqli::query does.
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
                    s_if(
                        e_binop(e_var("query"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli::real_query(): Argument #1 ($query) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    // Same drain as query(); a produced result set stays pending until
                    // store_result() picks it up.
                    s_return(e_binop(e_method_call(e_this(), "runQuery", vec![e_var("query")]), BinOp::NotEq, e_int(0))),
                ]),
        )
        .method(
            method("multi_query")
                .param("query", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("query"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli::multi_query(): Argument #1 ($query) must not be empty")])),
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
                    // One server round-trip for the whole batch: the bridge's emulated prepare +
                    // step executes the string with multi-statements enabled (mysqlnd's own
                    // default, mirrored by the bridge) and retains every wire result set for
                    // elephc_pdo_next_rowset.
                    s_expr(e_method_call(e_this(), "multiClose", vec![])),
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
                    s_return(e_this_prop("multiMore")),
                ]),
        )
        .method(
            method("next_result")
                .returns(TypeExpr::Bool)
                .body(vec![
                    // The probe in multiDrainCurrent already advanced the statement onto the next
                    // retained result set; step its first row and drain it.
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
                // `$mode` is accepted for signature compatibility and ignored: results are always
                // buffered, so MYSQLI_STORE_RESULT and MYSQLI_USE_RESULT behave the same. The PHP
                // form left it unread, and the oracle holds the built body to that.
                .keep_unread_params()
                .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
                .body(vec![
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
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("use_result")
                .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
                .body(vec![
                    // Alias of store_result: results are always buffered (documented divergence;
                    // true unbuffered streaming is out of scope).
                    s_return(e_method_call(e_this(), "store_result", vec![])),
                ]),
        )
        .method(
            method("get_server_info")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_this_prop("server_info")),
                ]),
        )
        .method(
            method("get_client_info")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_this_prop("client_info")),
                ]),
        )
        .method(
            method("get_host_info")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_this_prop("host_info")),
                ]),
        )
        .method(
            method("get_proto_info")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("protocol_version")),
                ]),
        )
        .method(
            method("get_server_version")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("server_version")),
                ]),
        )
        .method(
            method("get_client_version")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("client_version")),
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
                    // The bridge's server_info is MySQL's own "Uptime: … Questions: …" statistics
                    // line — exactly what mysqli::stat() returns.
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
        // -- internal helpers ($_-prefixed locals; same checker rule as PDO) --
        //
        // Guards statement-issuing operations while multi_query result sets (or a real_query
        // result) remain unconsumed: php-src raises CR_COMMANDS_OUT_OF_SYNC (2014) there, and
        // silently mixing the pending batch state with a new statement would corrupt
        // store_result/next_result.
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
        // Read-only pending probe for mysqli_stmt's two-step prepare()/execute() guard: the stmt
        // records the 2014 on ITS OWN error state (php-src puts a busy-connection failure on the
        // statement), so it needs the condition, not requireNoPendingResults' connection-level
        // failure. Private — the checker's mysqli friend channel exposes it to mysqli_stmt only,
        // keeping it off PHP's mysqli surface.
        .method(
            method("__elephcHasPendingResults")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_binop(e_binop(e_this_prop("multiStmt"), BinOp::GtEq, e_int(0)), BinOp::Or, e_this_prop("multiMore")), BinOp::Or, e_binop(e_this_prop("pendingResult"), BinOp::StrictNotEq, e_null()))),
                ]),
        )
        // Renders a transaction $name as php-src's ` /*name*/` SQL comment suffix ("" when $name
        // is null). An empty $name is a ValueError ONLY for begin_transaction ($emptyThrows) — php
        // throws there but for commit / rollback it does not, sending `COMMIT /**/` / `ROLLBACK
        // /**/` instead. A non-empty name is STRIPPED to php's allowlist [A-Za-z0-9 -_=] before it
        // is wrapped: blocklisting `*/` was not enough because a name starting with `!` (or
        // MariaDB's `M!`) opens an EXECUTABLE `/*! … */` comment whose body the server runs — a
        // `;` inside it would then execute a second statement, and this path (exec, not runQuery)
        // never sees the multi-statement guard. Stripping to the allowlist closes every comment
        // dialect at once, exactly as php sanitises the name.
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
                        // php's kept set (empirically, feeding every printable byte): space, `-`,
                        // 0-9 (48..=57), `=`, A-Z (65..=90), `_`, a-z (97..=122). Backslash is NOT
                        // kept, though php's own warning text lists it; the empirical behaviour
                        // wins.
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
                    // php raises an E_WARNING ("Transaction name has been truncated …") when it
                    // drops characters; elephc has no E_WARNING channel, so the stripping is
                    // silent (documented divergence — the security behaviour is identical).
                    s_return(e_binop(e_binop(e_str(" /*"), BinOp::Concat, e_var("_clean")), BinOp::Concat, e_str("*/"))),
                ]),
        )
        // Renders the COMMIT/ROLLBACK completion flags (MYSQLI_TRANS_COR_*), matching php-src's
        // mysqlnd: AND [NO] CHAIN and [NO] RELEASE, each emitted only when its bit is set and its
        // opposite is not.
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
        // Refreshes the connection status fields from the last command's OK packet, matching php
        // (which updates them after EVERY command). A control statement like COMMIT / BEGIN / SET
        // NAMES reports affected_rows = 0, so this resets the DML count a prior query() left
        // behind — e.g. `$db->affected_rows` read after commit() is 0, as in php, not the previous
        // statement's count.
        .method(
            method("refreshStatus")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "affected_rows", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                    s_prop_assign(e_this(), "insert_id", e_call("elephc_pdo_last_insert_id", vec![e_this_prop("conn"), e_str("")])),
                    s_prop_assign(e_this(), "warning_count", e_call("elephc_pdo_warning_count", vec![e_this_prop("conn")])),
                ]),
        )
        // php 8 raises `Error: mysqli object is not fully initialized` for any operation on a
        // `mysqli_init()` / argument-less `new mysqli()` object (or a failed/closed connection),
        // regardless of mysqli_report mode — it is an Error, not a connection warning. Throwing
        // here (rather than the old silent 2006 `false`) closes the divergence where an
        // unconnected real_escape_string returned a value with no signal at all.
        .method(
            method("requireInitialized")
                .private()
                .param("context", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("conn"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("Error", vec![e_binop(e_var("context"), BinOp::Concat, e_str("(): mysqli object is not fully initialized"))])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        // Guards every operation that needs a live connection. An unconnected object raises the
        // same php Error as requireInitialized; always returns true when it does not throw so
        // existing `if (!requireConnection())` call sites stay correct.
        .method(
            method("requireConnection")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireInitialized", vec![e_str("mysqli")])),
                    s_return(e_bool(true)),
                ]),
        )
        // Records a client-side failure that has no live bridge error state (unconnected handle,
        // invalid charset) and dispatches mysqli_report.
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
        // Records a connect-time failure on both the instance (connect_errno / connect_error,
        // distinct from errno/error) and the process-wide statics behind
        // mysqli_connect_errno()/mysqli_connect_error(), then dispatches mysqli_report. Always
        // returns false so connect paths can tail-call it.
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
        // Refreshes errno/error/sqlstate/error_list from the live bridge error state after a
        // failed operation, then dispatches mysqli_report. Always returns false so callers can
        // tail-call it.
        .method(
            method("opFailed")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "errno", e_call("elephc_pdo_errcode", vec![e_this_prop("conn")])),
                    s_prop_assign(e_this(), "error", e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])),
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
        // Clears the per-operation error state after a successful operation.
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
        // mysqli_report dispatch: STRICT throws mysqli_sql_exception (never PDOException), ERROR
        // alone writes the message to STDERR, OFF is silent.
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
                            s_assign("_e", e_new("mysqli_sql_exception", vec![e_var("message"), e_var("errno")])),
                            s_prop_assign(e_var("_e"), "sqlstate", e_var("sqlstate")),
                            s_throw(e_var("_e")),
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
        // Executes one statement through the bridge and fully buffers any result set (result
        // identity: a later query must never invalidate an earlier mysqli_result, so every row is
        // drained and the statement finalized before this returns). Returns 0 = failure (error
        // state recorded and reported), 1 = success with no result set (DML/DDL:
        // affected_rows/insert_id set), 2 = success with a result buffered into
        // $this->pendingResult.
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
                    // php-src rejects multi-statement strings in mysqli_query (mysqlnd toggles
                    // CLIENT_MULTI_STATEMENTS per multi_query call via COM_SET_OPTION; the bridge
                    // keeps it enabled for the whole connection), so a classic "1; DROP TABLE …"
                    // injection would otherwise EXECUTE here. The rejection uses the ONE
                    // authoritative bridge scanner (which treats `/*! … */` executable comments as
                    // live SQL, closing the comment-hidden separator bypass); a fast strpos skips
                    // the scan for the overwhelming majority of statements that carry no ';' at
                    // all. multi_query() is the one multi-statement path.
                    s_if(
                        e_binop(e_binop(e_call("strpos", vec![e_var("query"), e_str(";")]), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_call("elephc_pdo_sql_has_multiple_statements", vec![e_this_prop("conn"), e_var("query"), e_call("strlen", vec![e_var("query")])]), BinOp::NotEq, e_int(0))),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(1064), e_str("elephc mysqli does not support multiple statements in mysqli::query(); use mysqli::multi_query()"), e_str("42000")])),
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
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
                    // Column metadata is definitely known after the first step, including for
                    // emulated prepares that only execute at step time.
                    s_assign("_columnCount", e_call("elephc_pdo_column_count", vec![e_var("_stmt")])),
                    s_if(
                        e_binop(e_var("_columnCount"), BinOp::Eq, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "affected_rows", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                            s_prop_assign(e_this(), "insert_id", e_call("elephc_pdo_last_insert_id", vec![e_this_prop("conn"), e_str("")])),
                            s_prop_assign(e_this(), "field_count", e_int(0)),
                            s_prop_assign(e_this(), "warning_count", e_call("elephc_pdo_warning_count", vec![e_this_prop("conn")])),
                            s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                            s_expr(e_method_call(e_this(), "clearError", vec![])),
                            s_prop_assign(e_this(), "pendingResult", e_null()),
                            s_return(e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_names", e_array(vec![])),
                    s_assign("_tables", e_array(vec![])),
                    s_assign("_natives", e_array(vec![])),
                    s_assign("_flags", e_array(vec![])),
                    s_assign("_lens", e_array(vec![])),
                    s_assign("_decimals", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_columnCount"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_push("_names", e_call("elephc_pdo_column_name", vec![e_var("_stmt"), e_var("_i")])),
                        s_array_push("_tables", e_call("elephc_pdo_column_table_name", vec![e_var("_stmt"), e_var("_i")])),
                        s_array_push("_natives", e_call("elephc_pdo_column_native_type", vec![e_var("_stmt"), e_var("_i")])),
                        s_array_push("_flags", e_call("elephc_pdo_column_flags", vec![e_var("_stmt"), e_var("_i")])),
                        s_array_push("_lens", e_call("elephc_pdo_column_len", vec![e_var("_stmt"), e_var("_i")])),
                        s_array_push("_decimals", e_call("elephc_pdo_column_precision", vec![e_var("_stmt"), e_var("_i")])),
                    ]),
                    // Cells are buffered FLAT in row-major order (see mysqli_result): every
                    // buffered value stays a Mixed scalar and fetches build fresh rows.
                    s_assign("_cells", e_array(vec![])),
                    s_assign("_rowCount", e_int(0)),
                    s_while(e_binop(e_var("_rc"), BinOp::Eq, e_int(1)), vec![
                        s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_columnCount"))), Some(s_expr(e_post_inc("_i"))), vec![
                            s_array_push("_cells", e_method_call(e_this(), "columnValue", vec![e_var("_stmt"), e_var("_i")])),
                        ]),
                        s_assign("_rowCount", e_binop(e_var("_rowCount"), BinOp::Add, e_int(1))),
                        s_assign("_rc", e_call("elephc_pdo_step", vec![e_var("_stmt")])),
                    ]),
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
                    s_expr(e_call("elephc_pdo_finalize", vec![e_var("_stmt")])),
                    // php-src: for a SELECT, affected_rows mirrors num_rows and insert_id resets
                    // to 0 (the statement generated no AUTO_INCREMENT value).
                    s_prop_assign(e_this(), "affected_rows", e_var("_rowCount")),
                    s_prop_assign(e_this(), "insert_id", e_int(0)),
                    s_prop_assign(e_this(), "field_count", e_var("_columnCount")),
                    s_prop_assign(e_this(), "warning_count", e_call("elephc_pdo_warning_count", vec![e_this_prop("conn")])),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_prop_assign(e_this(), "pendingResult", e_static_call("mysqli_result", "__elephcFromDrain", vec![e_var("_cells"), e_var("_rowCount"), e_var("_names"), e_var("_tables"), e_var("_natives"), e_var("_flags"), e_var("_lens"), e_var("_decimals")])),
                    s_return(e_int(2)),
                ]),
        )
        // Drains the CURRENT result set of the active multi_query batch into $this->pendingResult
        // (or records affected_rows/insert_id for a non-select set), then eagerly probes
        // elephc_pdo_next_rowset so more_results() can answer without consuming; the batch
        // statement is finalized as soon as the probe reports no further set. Returns false —
        // error recorded, batch closed, nothing buffered — if a step fails mid-drain (same
        // contract as runQuery's fetch loop; with the bridge's buffered execution the first step
        // surfaces server errors, so this is the defensive twin for a mid-stream failure, e.g. a
        // dropped connection).
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
                            s_prop_assign(e_this(), "affected_rows", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                            s_prop_assign(e_this(), "insert_id", e_call("elephc_pdo_last_insert_id", vec![e_this_prop("conn"), e_str("")])),
                            s_prop_assign(e_this(), "field_count", e_int(0)),
                            s_prop_assign(e_this(), "pendingResult", e_null()),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("_names", e_array(vec![])),
                        s_assign("_tables", e_array(vec![])),
                        s_assign("_natives", e_array(vec![])),
                        s_assign("_flags", e_array(vec![])),
                        s_assign("_lens", e_array(vec![])),
                        s_assign("_decimals", e_array(vec![])),
                        s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_cols"))), Some(s_expr(e_post_inc("_i"))), vec![
                            s_array_push("_names", e_call("elephc_pdo_column_name", vec![e_var("_stmt"), e_var("_i")])),
                            s_array_push("_tables", e_call("elephc_pdo_column_table_name", vec![e_var("_stmt"), e_var("_i")])),
                            s_array_push("_natives", e_call("elephc_pdo_column_native_type", vec![e_var("_stmt"), e_var("_i")])),
                            s_array_push("_flags", e_call("elephc_pdo_column_flags", vec![e_var("_stmt"), e_var("_i")])),
                            s_array_push("_lens", e_call("elephc_pdo_column_len", vec![e_var("_stmt"), e_var("_i")])),
                            s_array_push("_decimals", e_call("elephc_pdo_column_precision", vec![e_var("_stmt"), e_var("_i")])),
                        ]),
                        s_assign("_cells", e_array(vec![])),
                        s_assign("_rowCount", e_int(0)),
                        s_assign("_rc", e_var("firstStep")),
                        s_while(e_binop(e_var("_rc"), BinOp::Eq, e_int(1)), vec![
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_cols"))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_array_push("_cells", e_method_call(e_this(), "columnValue", vec![e_var("_stmt"), e_var("_i")])),
                            ]),
                            s_assign("_rowCount", e_binop(e_var("_rowCount"), BinOp::Add, e_int(1))),
                            s_assign("_rc", e_call("elephc_pdo_step", vec![e_var("_stmt")])),
                        ]),
                        s_if(
                            e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                            vec![
                                // A step failed mid-set: report the error instead of passing off
                                // the truncated buffer as a complete result, and close the batch —
                                // its remaining sets are not trustworthy.
                                s_expr(e_method_call(e_this(), "opFailed", vec![])),
                                s_expr(e_method_call(e_this(), "multiClose", vec![])),
                                s_prop_assign(e_this(), "pendingResult", e_null()),
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "affected_rows", e_var("_rowCount")),
                        s_prop_assign(e_this(), "insert_id", e_int(0)),
                        s_prop_assign(e_this(), "field_count", e_var("_cols")),
                        s_prop_assign(e_this(), "pendingResult", e_static_call("mysqli_result", "__elephcFromDrain", vec![e_var("_cells"), e_var("_rowCount"), e_var("_names"), e_var("_tables"), e_var("_natives"), e_var("_flags"), e_var("_lens"), e_var("_decimals")])),
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
        // Finalizes any active multi_query batch statement and clears its state.
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
        // Decodes one cell of the current row, same type dispatch as PDOStatement::fetch's
        // columnValue (int / float / null / length-counted TEXT-or-BLOB copy so embedded NUL bytes
        // survive); mysqli has no stringify/oracle-nulls modes, so those branches are absent.
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
                    // The $_len == 0 guard is load-bearing: the bridge returns a NULL pointer for
                    // an empty buffer and ptr_read_string fatals on NULL.
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
        // "8.0.36-log" -> 80036, php-src's major*10000 + minor*100 + patch. The bridge's client
        // string is "mysql x.y.z"; strip that prefix first.
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
        // [A-Za-z0-9_] identifier filter shared by set_charset, same rule the PDO DSN charset key
        // enforces bridge-side.
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
