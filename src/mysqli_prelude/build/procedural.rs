//! Purpose:
//! Builds the `mysqli_*` procedural aliases as AST — ordinary functions
//! forwarding to the `mysqli` / `mysqli_result` / `mysqli_stmt` objects, so
//! `function_exists('mysqli_query')` is true once the prelude is injected.
//!
//! Called from:
//! - `crate::mysqli_prelude::build::mysqli_declarations`, which also applies the
//!   version gates (`mysqli_fetch_column` 8.1+, `mysqli_execute_query` 8.2+,
//!   `mysqli_quote_string` 8.6+).
//!
//! Key details:
//! - TRANSCRIBED from `mysqli_prelude::procedural::SRC` (`synthetic_class::transcribe`);
//!   the oracle compares each built node against that PHP.
//! - Link/result/statement parameters are `mixed` and validated with an inline
//!   `instanceof` guard that throws `TypeError` (see the PHP fragment).

use crate::parser::ast::{BinOp, CastType, TypeExpr, Stmt};
use crate::synthetic_class::{
    closure,
    e_array,
    e_array_assoc,
    e_binop,
    e_bool,
    e_call,
    e_cast,
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
    e_var,
    function,
    s_array_assign,
    s_array_push,
    s_assign,
    s_expr,
    s_for,
    s_foreach,
    s_if,
    s_return,
    s_throw,
    t_array,
    t_class,
    t_mixed,
    t_nullable,
    t_union,
};

/// `mysqli_connect` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_connect() -> Stmt {
    function("mysqli_connect")
        .param_default("hostname", t_nullable(TypeExpr::Str), e_null())
        .param_default("username", t_nullable(TypeExpr::Str), e_null())
        .param_default("password", t_nullable(TypeExpr::Str), e_null())
        .param_default("database", t_nullable(TypeExpr::Str), e_null())
        .param_default("port", t_nullable(TypeExpr::Int), e_null())
        .param_default("socket", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![t_class("mysqli"), TypeExpr::False]))
        .body(vec![
            s_assign("_link", e_new("mysqli", vec![])),
            s_expr(e_method_call(e_var("_link"), "__elephcDetachHandle", vec![])),
            s_if(
                e_method_call(e_var("_link"), "__elephcConnectProcedural", vec![e_var("hostname"), e_var("username"), e_var("password"), e_var("database"), e_var("port"), e_var("socket")]),
                vec![
                    s_return(e_var("_link")),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(false)),
        ])
        .build()
}

/// `mysqli_init` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_init() -> Stmt {
    function("mysqli_init")
        .returns(t_class("mysqli"))
        .body(vec![
            s_return(e_new("mysqli", vec![])),
        ])
        .build()
}

/// `mysqli_real_connect` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_real_connect() -> Stmt {
    function("mysqli_real_connect")
        .param("mysql", t_mixed())
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
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_real_connect(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "real_connect", vec![e_var("hostname"), e_var("username"), e_var("password"), e_var("database"), e_var("port"), e_var("socket"), e_var("flags")])),
        ])
        .build()
}

/// `mysqli_close` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_close() -> Stmt {
    function("mysqli_close")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_close(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "close", vec![])),
        ])
        .build()
}

/// `mysqli_ping` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_ping() -> Stmt {
    function("mysqli_ping")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_ping(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "ping", vec![])),
        ])
        .build()
}

/// `mysqli_select_db` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_select_db() -> Stmt {
    function("mysqli_select_db")
        .param("mysql", t_mixed())
        .param("database", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_select_db(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "select_db", vec![e_var("database")])),
        ])
        .build()
}

/// `mysqli_set_charset` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_set_charset() -> Stmt {
    function("mysqli_set_charset")
        .param("mysql", t_mixed())
        .param("charset", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_set_charset(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "set_charset", vec![e_var("charset")])),
        ])
        .build()
}

/// `mysqli_character_set_name` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_character_set_name() -> Stmt {
    function("mysqli_character_set_name")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_character_set_name(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "character_set_name", vec![])),
        ])
        .build()
}

/// `mysqli_real_escape_string` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_real_escape_string() -> Stmt {
    function("mysqli_real_escape_string")
        .param("mysql", t_mixed())
        .param("string", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_real_escape_string(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "real_escape_string", vec![e_var("string")])),
        ])
        .build()
}

/// `mysqli_escape_string` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_escape_string() -> Stmt {
    function("mysqli_escape_string")
        .param("mysql", t_mixed())
        .param("string", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_escape_string(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "real_escape_string", vec![e_var("string")])),
        ])
        .build()
}

/// `mysqli_quote_string` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_quote_string() -> Stmt {
    function("mysqli_quote_string")
        .param("mysql", t_mixed())
        .param("string", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_quote_string(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_binop(e_str("'"), BinOp::Concat, e_method_call(e_var("mysql"), "real_escape_string", vec![e_var("string")])), BinOp::Concat, e_str("'"))),
        ])
        .build()
}

/// `mysqli_begin_transaction` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_begin_transaction() -> Stmt {
    function("mysqli_begin_transaction")
        .param("mysql", t_mixed())
        .param_default("flags", TypeExpr::Int, e_int(0))
        .param_default("name", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_begin_transaction(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "begin_transaction", vec![e_var("flags"), e_var("name")])),
        ])
        .build()
}

/// `mysqli_commit` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_commit() -> Stmt {
    function("mysqli_commit")
        .param("mysql", t_mixed())
        .param_default("flags", TypeExpr::Int, e_int(0))
        .param_default("name", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_commit(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "commit", vec![e_var("flags"), e_var("name")])),
        ])
        .build()
}

/// `mysqli_rollback` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_rollback() -> Stmt {
    function("mysqli_rollback")
        .param("mysql", t_mixed())
        .param_default("flags", TypeExpr::Int, e_int(0))
        .param_default("name", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_rollback(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "rollback", vec![e_var("flags"), e_var("name")])),
        ])
        .build()
}

/// `mysqli_savepoint` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_savepoint() -> Stmt {
    function("mysqli_savepoint")
        .param("mysql", t_mixed())
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_savepoint(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "savepoint", vec![e_var("name")])),
        ])
        .build()
}

/// `mysqli_release_savepoint` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_release_savepoint() -> Stmt {
    function("mysqli_release_savepoint")
        .param("mysql", t_mixed())
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_release_savepoint(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "release_savepoint", vec![e_var("name")])),
        ])
        .build()
}

/// `mysqli_autocommit` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_autocommit() -> Stmt {
    function("mysqli_autocommit")
        .param("mysql", t_mixed())
        .param("enable", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_autocommit(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "autocommit", vec![e_var("enable")])),
        ])
        .build()
}

/// `mysqli_options` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_options() -> Stmt {
    function("mysqli_options")
        .param("mysql", t_mixed())
        .param("option", TypeExpr::Int)
        .param("value", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_options(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "options", vec![e_var("option"), e_var("value")])),
        ])
        .build()
}

/// `mysqli_set_opt` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_set_opt() -> Stmt {
    function("mysqli_set_opt")
        .param("mysql", t_mixed())
        .param("option", TypeExpr::Int)
        .param("value", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_set_opt(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "options", vec![e_var("option"), e_var("value")])),
        ])
        .build()
}

/// `mysqli_ssl_set` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_ssl_set() -> Stmt {
    function("mysqli_ssl_set")
        .param("mysql", t_mixed())
        .param("key", t_nullable(TypeExpr::Str))
        .param("certificate", t_nullable(TypeExpr::Str))
        .param("ca_certificate", t_nullable(TypeExpr::Str))
        .param("ca_path", t_nullable(TypeExpr::Str))
        .param("cipher_algos", t_nullable(TypeExpr::Str))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_ssl_set(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "ssl_set", vec![e_var("key"), e_var("certificate"), e_var("ca_certificate"), e_var("ca_path"), e_var("cipher_algos")])),
        ])
        .build()
}

/// `mysqli_change_user` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_change_user() -> Stmt {
    function("mysqli_change_user")
        .param("mysql", t_mixed())
        .param("username", TypeExpr::Str)
        .param("password", TypeExpr::Str)
        .param("database", t_nullable(TypeExpr::Str))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_change_user(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "change_user", vec![e_var("username"), e_var("password"), e_var("database")])),
        ])
        .build()
}

/// `mysqli_kill` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_kill() -> Stmt {
    function("mysqli_kill")
        .param("mysql", t_mixed())
        .param("process_id", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_kill(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "kill", vec![e_var("process_id")])),
        ])
        .build()
}

/// `mysqli_refresh` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_refresh() -> Stmt {
    function("mysqli_refresh")
        .param("mysql", t_mixed())
        .param("flags", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_refresh(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "refresh", vec![e_var("flags")])),
        ])
        .build()
}

/// `mysqli_dump_debug_info` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_dump_debug_info() -> Stmt {
    function("mysqli_dump_debug_info")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_dump_debug_info(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "dump_debug_info", vec![])),
        ])
        .build()
}

/// `mysqli_debug` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_debug() -> Stmt {
    function("mysqli_debug")
        .param("options", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_ignoredOptions", e_var("options")),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `mysqli_get_server_info` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_get_server_info() -> Stmt {
    function("mysqli_get_server_info")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_get_server_info(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "get_server_info", vec![])),
        ])
        .build()
}

/// `mysqli_get_client_info` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_get_client_info() -> Stmt {
    function("mysqli_get_client_info")
        .param_default("mysql", t_mixed(), e_null())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("mysql"), BinOp::StrictNotEq, e_null()), BinOp::And, e_not(e_instance_of(e_var("mysql"), "mysqli"))),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_get_client_info(): Argument #1 ($mysql) must be of type ?mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("elephc_pdo_mysql_client_info", vec![])),
        ])
        .build()
}

/// `mysqli_get_client_version` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_get_client_version() -> Stmt {
    function("mysqli_get_client_version")
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("_version", e_call("elephc_pdo_mysql_client_info", vec![])),
            s_if(
                e_call("str_starts_with", vec![e_var("_version"), e_str("mysql ")]),
                vec![
                    s_assign("_version", e_call("substr", vec![e_var("_version"), e_int(6)])),
                ],
                vec![],
                None,
            ),
            s_assign("_parts", e_call("explode", vec![e_str("."), e_var("_version")])),
            s_assign("_major", e_cast(CastType::Int, e_index(e_var("_parts"), e_int(0)))),
            s_assign("_minor", e_ternary(e_binop(e_call("count", vec![e_var("_parts")]), BinOp::Gt, e_int(1)), e_cast(CastType::Int, e_index(e_var("_parts"), e_int(1))), e_int(0))),
            s_assign("_patch", e_ternary(e_binop(e_call("count", vec![e_var("_parts")]), BinOp::Gt, e_int(2)), e_cast(CastType::Int, e_index(e_var("_parts"), e_int(2))), e_int(0))),
            s_return(e_binop(e_binop(e_binop(e_var("_major"), BinOp::Mul, e_int(10000)), BinOp::Add, e_binop(e_var("_minor"), BinOp::Mul, e_int(100))), BinOp::Add, e_var("_patch"))),
        ])
        .build()
}

/// `mysqli_get_host_info` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_get_host_info() -> Stmt {
    function("mysqli_get_host_info")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_get_host_info(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "host_info")),
        ])
        .build()
}

/// `mysqli_get_proto_info` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_get_proto_info() -> Stmt {
    function("mysqli_get_proto_info")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_get_proto_info(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "protocol_version")),
        ])
        .build()
}

/// `mysqli_get_server_version` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_get_server_version() -> Stmt {
    function("mysqli_get_server_version")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_get_server_version(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "server_version")),
        ])
        .build()
}

/// `mysqli_get_connection_stats` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_get_connection_stats() -> Stmt {
    function("mysqli_get_connection_stats")
        .param("mysql", t_mixed())
        .returns(t_array())
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_get_connection_stats(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_assign("_out", e_array(vec![])),
            s_foreach(e_method_call(e_var("mysql"), "get_connection_stats", vec![]), Some("_key"), "_value", vec![
                s_array_assign("_out", e_cast(CastType::String, e_var("_key")), e_cast(CastType::String, e_var("_value"))),
            ]),
            s_return(e_var("_out")),
        ])
        .build()
}

/// `mysqli_get_client_stats` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_get_client_stats() -> Stmt {
    function("mysqli_get_client_stats")
        .returns(t_array())
        .body(vec![
            s_assign("_counters", e_static_prop("mysqli", "clientStats")),
            s_assign("_out", e_array(vec![])),
            s_foreach(e_static_call("mysqli", "__elephcStatKeys", vec![]), None, "_key", vec![
                s_assign("_name", e_cast(CastType::String, e_var("_key"))),
                s_array_assign("_out", e_var("_name"), e_null_coalesce(e_static_call("mysqli", "__elephcWireStat", vec![e_neg(e_int(1)), e_var("_name")]), e_static_call("mysqli", "__elephcStatString", vec![e_cast(CastType::Int, e_null_coalesce(e_index(e_var("_counters"), e_var("_name")), e_int(0)))]))),
            ]),
            s_return(e_var("_out")),
        ])
        .build()
}

/// `mysqli_get_links_stats` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_get_links_stats() -> Stmt {
    function("mysqli_get_links_stats")
        .returns(t_array())
        .body(vec![
            s_return(e_array_assoc(vec![(e_str("total"), e_static_prop("mysqli", "openLinks")), (e_str("active_plinks"), e_static_prop("mysqli", "openPersistentLinks")), (e_str("cached_plinks"), e_static_prop("mysqli", "cachedPlinks"))])),
        ])
        .build()
}

/// `mysqli_get_warnings` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_get_warnings() -> Stmt {
    function("mysqli_get_warnings")
        .param("mysql", t_mixed())
        .returns(t_union(vec![t_class("mysqli_warning"), TypeExpr::False]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_get_warnings(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "get_warnings", vec![])),
        ])
        .build()
}

/// `mysqli_stat` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stat() -> Stmt {
    function("mysqli_stat")
        .param("mysql", t_mixed())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stat(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "stat", vec![])),
        ])
        .build()
}

/// `mysqli_thread_id` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_thread_id() -> Stmt {
    function("mysqli_thread_id")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_thread_id(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "thread_id")),
        ])
        .build()
}

/// `mysqli_connect_errno` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_connect_errno() -> Stmt {
    function("mysqli_connect_errno")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_static_prop("mysqli", "lastConnectErrno")),
        ])
        .build()
}

/// `mysqli_connect_error` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_connect_error() -> Stmt {
    function("mysqli_connect_error")
        .returns(t_nullable(TypeExpr::Str))
        .body(vec![
            s_if(
                e_binop(e_static_prop("mysqli", "lastConnectErrno"), BinOp::Eq, e_int(0)),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_return(e_static_prop("mysqli", "lastConnectError")),
        ])
        .build()
}

/// `mysqli_errno` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_errno() -> Stmt {
    function("mysqli_errno")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_errno(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "errno")),
        ])
        .build()
}

/// `mysqli_error` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_error() -> Stmt {
    function("mysqli_error")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_error(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "error")),
        ])
        .build()
}

/// `mysqli_error_list` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_error_list() -> Stmt {
    function("mysqli_error_list")
        .param("mysql", t_mixed())
        .returns(t_array())
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_error_list(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "error_list")),
        ])
        .build()
}

/// `mysqli_sqlstate` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_sqlstate() -> Stmt {
    function("mysqli_sqlstate")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_sqlstate(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "sqlstate")),
        ])
        .build()
}

/// `mysqli_affected_rows` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_affected_rows() -> Stmt {
    function("mysqli_affected_rows")
        .param("mysql", t_mixed())
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::Str]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_affected_rows(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "affected_rows")),
        ])
        .build()
}

/// `mysqli_insert_id` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_insert_id() -> Stmt {
    function("mysqli_insert_id")
        .param("mysql", t_mixed())
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::Str]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_insert_id(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "insert_id")),
        ])
        .build()
}

/// `mysqli_field_count` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_field_count() -> Stmt {
    function("mysqli_field_count")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_field_count(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "field_count")),
        ])
        .build()
}

/// `mysqli_warning_count` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_warning_count() -> Stmt {
    function("mysqli_warning_count")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_warning_count(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "warning_count")),
        ])
        .build()
}

/// `mysqli_info` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_info() -> Stmt {
    function("mysqli_info")
        .param("mysql", t_mixed())
        .returns(t_nullable(TypeExpr::Str))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_info(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("mysql"), "__elephcRequireValid", vec![])),
            s_return(e_prop(e_var("mysql"), "info")),
        ])
        .build()
}

/// `mysqli_query` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_query() -> Stmt {
    function("mysqli_query")
        .param("mysql", t_mixed())
        .param("query", TypeExpr::Str)
        .param_default("result_mode", TypeExpr::Int, e_int(0))
        .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::Bool]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_query(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "query", vec![e_var("query"), e_var("result_mode")])),
        ])
        .build()
}

/// `mysqli_real_query` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_real_query() -> Stmt {
    function("mysqli_real_query")
        .param("mysql", t_mixed())
        .param("query", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_real_query(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "real_query", vec![e_var("query")])),
        ])
        .build()
}

/// `mysqli_multi_query` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_multi_query() -> Stmt {
    function("mysqli_multi_query")
        .param("mysql", t_mixed())
        .param("query", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_multi_query(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "multi_query", vec![e_var("query")])),
        ])
        .build()
}

/// `mysqli_more_results` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_more_results() -> Stmt {
    function("mysqli_more_results")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_more_results(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "more_results", vec![])),
        ])
        .build()
}

/// `mysqli_next_result` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_next_result() -> Stmt {
    function("mysqli_next_result")
        .param("mysql", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_next_result(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "next_result", vec![])),
        ])
        .build()
}

/// `mysqli_store_result` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_store_result() -> Stmt {
    function("mysqli_store_result")
        .param("mysql", t_mixed())
        .param_default("mode", TypeExpr::Int, e_int(0))
        .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_store_result(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "store_result", vec![e_var("mode")])),
        ])
        .build()
}

/// `mysqli_use_result` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_use_result() -> Stmt {
    function("mysqli_use_result")
        .param("mysql", t_mixed())
        .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_use_result(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "use_result", vec![])),
        ])
        .build()
}

/// `mysqli_reap_async_query` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_reap_async_query() -> Stmt {
    function("mysqli_reap_async_query")
        .param("mysql", t_mixed())
        .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::Bool]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_reap_async_query(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "reap_async_query", vec![])),
        ])
        .build()
}

/// `mysqli_poll` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_poll() -> Stmt {
    function("mysqli_poll")
        .param_by_ref("read", Some(t_nullable(t_array())))
        .param_by_ref("error", Some(t_nullable(t_array())))
        .param_by_ref("reject", Some(t_array()))
        .param("seconds", TypeExpr::Int)
        .param_default("microseconds", TypeExpr::Int, e_int(0))
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
        .body(vec![
            s_return(e_static_call("mysqli", "poll", vec![e_var("read"), e_var("error"), e_var("reject"), e_var("seconds"), e_var("microseconds")])),
        ])
        .build()
}

/// `mysqli_fetch_assoc` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_fetch_assoc() -> Stmt {
    function("mysqli_fetch_assoc")
        .param("result", t_mixed())
        .returns(t_nullable(t_array()))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_fetch_assoc(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("result"), "fetch_assoc", vec![])),
        ])
        .build()
}

/// `mysqli_fetch_row` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_fetch_row() -> Stmt {
    function("mysqli_fetch_row")
        .param("result", t_mixed())
        .returns(t_nullable(t_array()))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_fetch_row(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("result"), "fetch_row", vec![])),
        ])
        .build()
}

/// `mysqli_fetch_array` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_fetch_array() -> Stmt {
    function("mysqli_fetch_array")
        .param("result", t_mixed())
        .param_default("mode", TypeExpr::Int, e_int(3))
        .returns(t_nullable(t_array()))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_fetch_array(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("result"), "fetch_array", vec![e_var("mode")])),
        ])
        .build()
}

/// `mysqli_fetch_object` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_fetch_object() -> Stmt {
    function("mysqli_fetch_object")
        .param("result", t_mixed())
        .param_default("class", TypeExpr::Str, e_str("stdClass"))
        .param_default("constructor_args", t_array(), e_array(vec![]))
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_fetch_object(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("result"), "fetch_object", vec![e_var("class"), e_var("constructor_args")])),
        ])
        .build()
}

/// `mysqli_fetch_all` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_fetch_all() -> Stmt {
    function("mysqli_fetch_all")
        .param("result", t_mixed())
        .param_default("mode", TypeExpr::Int, e_int(2))
        .returns(t_array())
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_fetch_all(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("result"), "fetch_all", vec![e_var("mode")])),
        ])
        .build()
}

/// `mysqli_fetch_column` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_fetch_column() -> Stmt {
    function("mysqli_fetch_column")
        .param("result", t_mixed())
        .param_default("column", TypeExpr::Int, e_int(0))
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_fetch_column(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("result"), "fetch_column", vec![e_var("column")])),
        ])
        .build()
}

/// `mysqli_num_rows` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_num_rows() -> Stmt {
    function("mysqli_num_rows")
        .param("result", t_mixed())
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::Str]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_num_rows(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_prop(e_var("result"), "type"), BinOp::Eq, e_int(1)),
                vec![
                    s_throw(e_new("Error", vec![e_str("mysqli_num_rows() cannot be used in MYSQLI_USE_RESULT mode")])),
                ],
                vec![],
                None,
            ),
            s_return(e_prop(e_var("result"), "num_rows")),
        ])
        .build()
}

/// `mysqli_num_fields` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_num_fields() -> Stmt {
    function("mysqli_num_fields")
        .param("result", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_num_fields(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_prop(e_var("result"), "field_count")),
        ])
        .build()
}

/// `mysqli_data_seek` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_data_seek() -> Stmt {
    function("mysqli_data_seek")
        .param("result", t_mixed())
        .param("offset", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_data_seek(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("result"), "data_seek", vec![e_var("offset")])),
        ])
        .build()
}

/// `mysqli_fetch_field` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_fetch_field() -> Stmt {
    function("mysqli_fetch_field")
        .param("result", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_fetch_field(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("result"), "fetch_field", vec![])),
        ])
        .build()
}

/// `mysqli_fetch_fields` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_fetch_fields() -> Stmt {
    function("mysqli_fetch_fields")
        .param("result", t_mixed())
        .returns(t_array())
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_fetch_fields(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("result"), "fetch_fields", vec![])),
        ])
        .build()
}

/// `mysqli_fetch_field_direct` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_fetch_field_direct() -> Stmt {
    function("mysqli_fetch_field_direct")
        .param("result", t_mixed())
        .param("index", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_fetch_field_direct(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("result"), "fetch_field_direct", vec![e_var("index")])),
        ])
        .build()
}

/// `mysqli_fetch_lengths` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_fetch_lengths() -> Stmt {
    function("mysqli_fetch_lengths")
        .param("result", t_mixed())
        .returns(t_union(vec![t_array(), TypeExpr::False]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_fetch_lengths(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_assign("_lengths", e_prop(e_var("result"), "lengths")),
            s_if(
                e_binop(e_var("_lengths"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_lengths")),
        ])
        .build()
}

/// `mysqli_field_seek` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_field_seek() -> Stmt {
    function("mysqli_field_seek")
        .param("result", t_mixed())
        .param("index", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_field_seek(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("result"), "field_seek", vec![e_var("index")])),
        ])
        .build()
}

/// `mysqli_field_tell` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_field_tell() -> Stmt {
    function("mysqli_field_tell")
        .param("result", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_field_tell(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_prop(e_var("result"), "current_field")),
        ])
        .build()
}

/// `mysqli_free_result` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_free_result() -> Stmt {
    function("mysqli_free_result")
        .param("result", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("result"), "mysqli_result")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_free_result(): Argument #1 ($result) must be of type mysqli_result, "), BinOp::Concat, e_call("gettype", vec![e_var("result")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("result"), "free", vec![])),
        ])
        .build()
}

/// `mysqli_prepare` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_prepare() -> Stmt {
    function("mysqli_prepare")
        .param("mysql", t_mixed())
        .param("query", TypeExpr::Str)
        .returns(t_union(vec![t_class("mysqli_stmt"), TypeExpr::False]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_prepare(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "prepare", vec![e_var("query")])),
        ])
        .build()
}

/// `mysqli_execute_query` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_execute_query() -> Stmt {
    function("mysqli_execute_query")
        .param("mysql", t_mixed())
        .param("query", TypeExpr::Str)
        .param_default("params", t_nullable(t_array()), e_null())
        .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::Bool]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_execute_query(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "execute_query", vec![e_var("query"), e_var("params")])),
        ])
        .build()
}

/// `mysqli_stmt_init` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_init() -> Stmt {
    function("mysqli_stmt_init")
        .param("mysql", t_mixed())
        .returns(t_union(vec![t_class("mysqli_stmt"), TypeExpr::False]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_init(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "stmt_init", vec![])),
        ])
        .build()
}

/// `mysqli_stmt_bind_param` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_bind_param() -> Stmt {
    function("mysqli_stmt_bind_param")
        .param("statement", t_mixed())
        .param("types", TypeExpr::Str)
        .variadic_by_ref("vars", Some(t_mixed()))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_bind_param(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "__elephcBindParamReader", vec![e_str("mysqli_stmt_bind_param"), e_int(2), e_var("types"), e_call("count", vec![e_var("vars")]), closure()
                .captures_ref("vars")
                .returns(t_array())
                .body(vec![
                    s_assign("_values", e_array(vec![])),
                    s_assign("_n", e_call("count", vec![e_var("vars")])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_push("_values", e_index(e_var("vars"), e_var("_i"))),
                    ]),
                    s_return(e_var("_values")),
                ])
                .build()])),
        ])
        .build()
}

/// `mysqli_stmt_bind_result` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_bind_result() -> Stmt {
    function("mysqli_stmt_bind_result")
        .param("statement", t_mixed())
        .variadic_by_ref("vars", Some(t_mixed()))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_bind_result(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "__elephcBindResultWriter", vec![e_call("count", vec![e_var("vars")]), closure()
                .param("row", t_array())
                .captures_ref("vars")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_n", e_call("count", vec![e_var("vars")])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_assign("vars", e_var("_i"), e_index(e_var("row"), e_var("_i"))),
                    ]),
                ])
                .build()])),
        ])
        .build()
}

/// `mysqli_stmt_execute` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_execute() -> Stmt {
    function("mysqli_stmt_execute")
        .param("statement", t_mixed())
        .param_default("params", t_nullable(t_array()), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_execute(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "execute", vec![e_var("params")])),
        ])
        .build()
}

/// `mysqli_execute` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_execute() -> Stmt {
    function("mysqli_execute")
        .param("statement", t_mixed())
        .param_default("params", t_nullable(t_array()), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_execute(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "execute", vec![e_var("params")])),
        ])
        .build()
}

/// `mysqli_stmt_prepare` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_prepare() -> Stmt {
    function("mysqli_stmt_prepare")
        .param("statement", t_mixed())
        .param("query", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_prepare(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "prepare", vec![e_var("query")])),
        ])
        .build()
}

/// `mysqli_stmt_fetch` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_fetch() -> Stmt {
    function("mysqli_stmt_fetch")
        .param("statement", t_mixed())
        .returns(t_nullable(TypeExpr::Bool))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_fetch(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "fetch", vec![])),
        ])
        .build()
}

/// `mysqli_stmt_get_result` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_get_result() -> Stmt {
    function("mysqli_stmt_get_result")
        .param("statement", t_mixed())
        .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_get_result(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "get_result", vec![])),
        ])
        .build()
}

/// `mysqli_stmt_store_result` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_store_result() -> Stmt {
    function("mysqli_stmt_store_result")
        .param("statement", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_store_result(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "store_result", vec![])),
        ])
        .build()
}

/// `mysqli_stmt_result_metadata` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_result_metadata() -> Stmt {
    function("mysqli_stmt_result_metadata")
        .param("statement", t_mixed())
        .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_result_metadata(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "result_metadata", vec![])),
        ])
        .build()
}

/// `mysqli_stmt_data_seek` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_data_seek() -> Stmt {
    function("mysqli_stmt_data_seek")
        .param("statement", t_mixed())
        .param("offset", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_data_seek(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("statement"), "data_seek", vec![e_var("offset")])),
        ])
        .build()
}

/// `mysqli_stmt_more_results` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_more_results() -> Stmt {
    function("mysqli_stmt_more_results")
        .param("statement", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_more_results(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "more_results", vec![])),
        ])
        .build()
}

/// `mysqli_stmt_next_result` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_next_result() -> Stmt {
    function("mysqli_stmt_next_result")
        .param("statement", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_next_result(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "next_result", vec![])),
        ])
        .build()
}

/// `mysqli_stmt_send_long_data` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_send_long_data() -> Stmt {
    function("mysqli_stmt_send_long_data")
        .param("statement", t_mixed())
        .param("param_num", TypeExpr::Int)
        .param("data", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_send_long_data(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "send_long_data", vec![e_var("param_num"), e_var("data")])),
        ])
        .build()
}

/// `mysqli_stmt_attr_get` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_attr_get() -> Stmt {
    function("mysqli_stmt_attr_get")
        .param("statement", t_mixed())
        .param("attribute", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_attr_get(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "attr_get", vec![e_var("attribute")])),
        ])
        .build()
}

/// `mysqli_stmt_attr_set` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_attr_set() -> Stmt {
    function("mysqli_stmt_attr_set")
        .param("statement", t_mixed())
        .param("attribute", TypeExpr::Int)
        .param("value", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_attr_set(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "attr_set", vec![e_var("attribute"), e_var("value")])),
        ])
        .build()
}

/// `mysqli_stmt_get_warnings` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_get_warnings() -> Stmt {
    function("mysqli_stmt_get_warnings")
        .param("statement", t_mixed())
        .returns(t_union(vec![t_class("mysqli_warning"), TypeExpr::False]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_get_warnings(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "get_warnings", vec![])),
        ])
        .build()
}

/// `mysqli_stmt_reset` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_reset() -> Stmt {
    function("mysqli_stmt_reset")
        .param("statement", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_reset(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "reset", vec![])),
        ])
        .build()
}

/// `mysqli_stmt_close` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_close() -> Stmt {
    function("mysqli_stmt_close")
        .param("statement", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_close(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "close", vec![])),
        ])
        .build()
}

/// `mysqli_stmt_free_result` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_free_result() -> Stmt {
    function("mysqli_stmt_free_result")
        .param("statement", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_free_result(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_var("statement"), "free_result", vec![])),
        ])
        .build()
}

/// `mysqli_stmt_affected_rows` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_affected_rows() -> Stmt {
    function("mysqli_stmt_affected_rows")
        .param("statement", t_mixed())
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::Str]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_affected_rows(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_prop(e_var("statement"), "affected_rows")),
        ])
        .build()
}

/// `mysqli_stmt_errno` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_errno() -> Stmt {
    function("mysqli_stmt_errno")
        .param("statement", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_errno(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_prop(e_var("statement"), "errno")),
        ])
        .build()
}

/// `mysqli_stmt_error` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_error() -> Stmt {
    function("mysqli_stmt_error")
        .param("statement", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_error(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_prop(e_var("statement"), "error")),
        ])
        .build()
}

/// `mysqli_stmt_error_list` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_error_list() -> Stmt {
    function("mysqli_stmt_error_list")
        .param("statement", t_mixed())
        .returns(t_array())
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_error_list(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_prop(e_var("statement"), "error_list")),
        ])
        .build()
}

/// `mysqli_stmt_num_rows` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_num_rows() -> Stmt {
    function("mysqli_stmt_num_rows")
        .param("statement", t_mixed())
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::Str]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_num_rows(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("statement"), "num_rows", vec![])),
        ])
        .build()
}

/// `mysqli_stmt_param_count` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_param_count() -> Stmt {
    function("mysqli_stmt_param_count")
        .param("statement", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_param_count(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_prop(e_var("statement"), "param_count")),
        ])
        .build()
}

/// `mysqli_stmt_field_count` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_field_count() -> Stmt {
    function("mysqli_stmt_field_count")
        .param("statement", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_field_count(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_prop(e_var("statement"), "field_count")),
        ])
        .build()
}

/// `mysqli_stmt_insert_id` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_insert_id() -> Stmt {
    function("mysqli_stmt_insert_id")
        .param("statement", t_mixed())
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::Str]))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_insert_id(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_prop(e_var("statement"), "insert_id")),
        ])
        .build()
}

/// `mysqli_stmt_sqlstate` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_stmt_sqlstate() -> Stmt {
    function("mysqli_stmt_sqlstate")
        .param("statement", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("statement"), "mysqli_stmt")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_stmt_sqlstate(): Argument #1 ($statement) must be of type mysqli_stmt, "), BinOp::Concat, e_call("gettype", vec![e_var("statement")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_prop(e_var("statement"), "sqlstate")),
        ])
        .build()
}

/// `mysqli_get_charset` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_get_charset() -> Stmt {
    function("mysqli_get_charset")
        .param("mysql", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("mysql"), "mysqli")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("mysqli_get_charset(): Argument #1 ($mysql) must be of type mysqli, "), BinOp::Concat, e_call("gettype", vec![e_var("mysql")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("mysql"), "get_charset", vec![])),
        ])
        .build()
}

/// `mysqli_thread_safe` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_thread_safe() -> Stmt {
    function("mysqli_thread_safe")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_bool(false)),
        ])
        .build()
}
