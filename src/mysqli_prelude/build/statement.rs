//! Purpose:
//! Builds the `mysqli_stmt` class as AST: native prepares over
//! `elephc_pdo_prepare(…, 0)`, parameter binding, execute/step, and
//! `get_result` draining into an independent `mysqli_result`.
//!
//! Called from:
//! - `crate::mysqli_prelude::build::mysqli_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from `mysqli_prelude::statement::SRC` (`synthetic_class::transcribe`);
//!   the oracle `built_declarations_match_the_php_for_every_version` compares the
//!   built class against that PHP for every profile.
//! - `bind_param(string $types, mixed &...$vars)` keeps PHP's signature
//!   (literals are rejected, the variable count is validated against the type
//!   string), but the variable VALUES are captured at bind time — a documented
//!   divergence: elephc cannot alias caller variables past the call (variadic
//!   by-ref elements alias only inside the callee frame, and per-variable
//!   closure captures need fixed parameter names, whose defaulted-by-ref
//!   lowering the backend does not support). Re-executing with fresh values
//!   means re-calling bind_param or using `execute($params)`.
//! - `bind_result` / `fetch` are deliberately NOT declared: writing a fetched
//!   row back into caller variables after the binding call is exactly the
//!   post-call aliasing elephc cannot express, and a silently inert binding
//!   would be worse than an honest "undefined method". `get_result()` +
//!   `mysqli_result` fetches cover row reading; `method_exists` stays honest.
//! - `execute` steps once to learn whether a result set exists; `get_result`
//!   drains the rest and resets the statement so it can be re-executed;
//!   `store_result` consumes the pending rows so `num_rows` is valid.

use crate::parser::ast::{BinOp, CastType, TypeExpr, Stmt};
use crate::synthetic_class::{
    class,
    e_array,
    e_array_assoc,
    e_binop,
    e_bool,
    e_call,
    e_cast,
    e_const,
    e_index,
    e_int,
    e_method_call,
    e_neg,
    e_new,
    e_not,
    e_null,
    e_post_inc,
    e_static_call,
    e_static_prop,
    e_str,
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
    s_throw,
    s_while,
    t_array,
    t_class,
    t_mixed,
    t_nullable,
    t_union,
};

/// `mysqli_stmt` — transcribed from the PHP form.
pub(super) fn decl_class_mysqli_stmt() -> Stmt {
    class("mysqli_stmt")
        .prop("affected_rows", TypeExpr::Int, Some(e_int(0)))
        .prop("errno", TypeExpr::Int, Some(e_int(0)))
        .prop("error", TypeExpr::Str, Some(e_str("")))
        .prop("field_count", TypeExpr::Int, Some(e_int(0)))
        .prop("insert_id", TypeExpr::Int, Some(e_int(0)))
        .prop("num_rows", TypeExpr::Int, Some(e_int(0)))
        .prop("param_count", TypeExpr::Int, Some(e_int(0)))
        .prop("sqlstate", TypeExpr::Str, Some(e_str("00000")))
        .prop("error_list", t_array(), Some(e_array(vec![])))
        // Bridge handles: the prepared-statement handle and the owning connection.
        .private_prop("stmt", TypeExpr::Int, Some(e_neg(e_int(1))))
        .private_prop("conn", TypeExpr::Int, Some(e_neg(e_int(1))))
        .private_prop("link", t_nullable(t_class("mysqli")), Some(e_null()))
        // bind_param state: the type string and the values captured at bind time.
        .private_prop("bindTypes", TypeExpr::Str, Some(e_str("")))
        .private_prop("boundParams", t_array(), Some(e_array(vec![])))
        // Cursor state: execute() pre-steps once; get_result/store_result drain.
        .private_prop("executedOnce", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("hasPending", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("pendingStep", TypeExpr::Int, Some(e_int(0)))
        // Internal factory used by mysqli::prepare (a user never constructs mysqli_stmt directly
        // in the v1 subset).
        .method(
            method("__elephcFromPrepare")
                .private()
                .static_()
                .param("link", t_class("mysqli"))
                .param("conn", TypeExpr::Int)
                .param("stmt", TypeExpr::Int)
                .param("query", TypeExpr::Str)
                // `$query` is not stored: the bridge reports the parameter count itself (below),
                // so the SQL text has no further use here. The PHP form left it unread, and the
                // oracle holds the built body to that.
                .keep_unread_params()
                .returns(t_class("mysqli_stmt"))
                .body(vec![
                    s_assign("_statement", e_new("mysqli_stmt", vec![])),
                    s_prop_assign(e_var("_statement"), "link", e_var("link")),
                    s_prop_assign(e_var("_statement"), "conn", e_var("conn")),
                    s_prop_assign(e_var("_statement"), "stmt", e_var("stmt")),
                    // The bridge reports the server's exact parameter marker count off the
                    // prepared statement (no client-side `?`-scanning, which used to diverge from
                    // the multi-statement scanner on `--` comments / backslash rules).
                    s_prop_assign(e_var("_statement"), "param_count", e_call("elephc_pdo_mysql_param_count", vec![e_var("stmt")])),
                    s_return(e_var("_statement")),
                ]),
        )
        // Internal factory for the two-step mysqli::stmt_init() + prepare() form: an unprepared
        // statement bound to a connection, ready for prepare(). Private like __elephcFromPrepare —
        // the checker's mysqli friend channel exposes it to mysqli::stmt_init only.
        .method(
            method("__elephcInit")
                .private()
                .static_()
                .param("link", t_class("mysqli"))
                .param("conn", TypeExpr::Int)
                .returns(t_class("mysqli_stmt"))
                .body(vec![
                    s_assign("_statement", e_new("mysqli_stmt", vec![])),
                    s_prop_assign(e_var("_statement"), "link", e_var("link")),
                    s_prop_assign(e_var("_statement"), "conn", e_var("conn")),
                    s_return(e_var("_statement")),
                ]),
        )
        .method(
            method("prepare")
                .param("query", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("conn"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2006), e_str("mysqli_stmt object is not associated with a connection"), e_str("HY000")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("query"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli_stmt::prepare(): Argument #1 ($query) cannot be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireLinkNotBusy", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("stmt"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_expr(e_call("elephc_pdo_finalize", vec![e_this_prop("stmt")])),
                            s_prop_assign(e_this(), "stmt", e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_handle", e_call("elephc_pdo_prepare", vec![e_this_prop("conn"), e_var("query"), e_int(0)])),
                    s_if(
                        e_binop(e_var("_handle"), BinOp::Lt, e_int(0)),
                        vec![
                            // stmt is still -1, so opFailed reads the connection's error state
                            // (its stmt-errno fallback), which is where a prepare error lives.
                            s_expr(e_method_call(e_this(), "opFailed", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "stmt", e_var("_handle")),
                    s_prop_assign(e_this(), "param_count", e_call("elephc_pdo_mysql_param_count", vec![e_var("_handle")])),
                    s_prop_assign(e_this(), "executedOnce", e_bool(false)),
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        // Guards the two-step prepare() and execute() while the LINK has unconsumed results (a
        // multi_query batch or a real_query result): php-src raises CR_COMMANDS_OUT_OF_SYNC (2014)
        // on the statement there — sending COM_STMT_PREPARE / COM_STMT_EXECUTE on a busy
        // connection would corrupt the pending batch. One-shot mysqli::prepare() has its own
        // connection-level guard; this is the statement-side twin, reading the link's private
        // state through the checker's mysqli friend channel.
        .method(
            method("requireLinkNotBusy")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_link", e_this_prop("link")),
                    s_if(
                        e_binop(e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()), BinOp::And, e_method_call(e_var("_link"), "__elephcHasPendingResults", vec![])),
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
            method("free_result")
                .returns(TypeExpr::Void)
                .body(vec![
                    // Discards a pending buffered result so the statement can be re-executed (php
                    // frees the client-side buffer). We buffer per-statement in the bridge, so
                    // draining/resetting the cursor is the equivalent.
                    s_if(
                        e_binop(e_this_prop("hasPending"), BinOp::And, e_binop(e_this_prop("stmt"), BinOp::GtEq, e_int(0))),
                        vec![
                            s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                ]),
        )
        .method(
            method("bind_param")
                .param("types", TypeExpr::Str)
                .variadic_by_ref("vars", Some(t_mixed()))
                .returns(TypeExpr::Bool)
                .body(vec![
                    // Variadic by-ref keeps PHP's signature contract (a literal argument is
                    // rejected with "must be passed a variable"); the values are captured HERE —
                    // see the module preamble for the documented divergence from PHP's
                    // read-at-execute reference semantics.
                    s_assign("_values", e_array(vec![])),
                    s_assign("_given", e_call("count", vec![e_var("vars")])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_given"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_push("_values", e_index(e_var("vars"), e_var("_i"))),
                    ]),
                    s_return(e_method_call(e_this(), "__elephcBindParamValues", vec![e_var("types"), e_var("_values")])),
                ]),
        )
        // Shared validation + snapshot behind bind_param (the procedural mysqli_stmt_bind_param
        // alias forwards through it directly). Private: not part of PHP's mysqli surface.
        .method(
            method("__elephcBindParamValues")
                .private()
                .param("types", TypeExpr::Str)
                .param("values", t_array())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_want", e_call("strlen", vec![e_var("types")])),
                    s_if(
                        e_binop(e_var("_want"), BinOp::Eq, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli_stmt::bind_param(): Argument #1 ($types) cannot be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_want"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_char", e_call("substr", vec![e_var("types"), e_var("_i"), e_int(1)])),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("_char"), BinOp::StrictNotEq, e_str("i")), BinOp::And, e_binop(e_var("_char"), BinOp::StrictNotEq, e_str("d"))), BinOp::And, e_binop(e_var("_char"), BinOp::StrictNotEq, e_str("s"))), BinOp::And, e_binop(e_var("_char"), BinOp::StrictNotEq, e_str("b"))),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str("mysqli_stmt::bind_param(): Argument #1 ($types) must only contain the \"b\", \"d\", \"i\", \"s\" type specifiers")])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_binop(e_call("count", vec![e_var("values")]), BinOp::NotEq, e_var("_want")),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2031), e_str("The number of variables must match the number of parameters in the prepared statement"), e_str("HY000")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "bindTypes", e_var("types")),
                    s_prop_assign(e_this(), "boundParams", e_var("values")),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("execute")
                .param_default("params", t_nullable(t_array()), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("stmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2006), e_str("mysqli_stmt object is already closed"), e_str("HY000")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "requireLinkNotBusy", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    // Re-execution: rewind the server-side cursor and drop old binds.
                    s_if(
                        e_this_prop("executedOnce"),
                        vec![
                            s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
                            s_expr(e_call("elephc_pdo_clear_bindings", vec![e_this_prop("stmt")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "executedOnce", e_bool(true)),
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_prop_assign(e_this(), "pendingStep", e_int(0)),
                    // PHP 8.1+: execute's own $params bind as strings, in order.
                    s_if(
                        e_binop(e_var("params"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("_given", e_call("count", vec![e_var("params")])),
                            s_if(
                                e_binop(e_var("_given"), BinOp::NotEq, e_this_prop("param_count")),
                                vec![
                                    s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2031), e_binop(e_binop(e_binop(e_binop(e_str("mysqli_stmt::execute(): Argument #1 ($params) must consist of exactly "), BinOp::Concat, e_this_prop("param_count")), BinOp::Concat, e_str(" elements, ")), BinOp::Concat, e_var("_given")), BinOp::Concat, e_str(" given")), e_str("HY000")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_given"))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_assign("_value", e_index(e_var("params"), e_var("_i"))),
                                s_if(
                                    e_binop(e_var("_value"), BinOp::StrictEq, e_null()),
                                    vec![
                                        s_expr(e_call("elephc_pdo_bind_null", vec![e_this_prop("stmt"), e_binop(e_var("_i"), BinOp::Add, e_int(1))])),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_assign("_text", e_cast(CastType::String, e_var("_value"))),
                                    s_expr(e_call("elephc_pdo_bind_text", vec![e_this_prop("stmt"), e_binop(e_var("_i"), BinOp::Add, e_int(1)), e_var("_text"), e_call("strlen", vec![e_var("_text")])])),
                                ]),
                                ),
                            ]),
                        ],
                        vec![
                        (e_binop(e_this_prop("bindTypes"), BinOp::StrictNotEq, e_str("")), vec![
                            s_if(
                                e_binop(e_call("strlen", vec![e_this_prop("bindTypes")]), BinOp::NotEq, e_this_prop("param_count")),
                                vec![
                                    s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2034), e_str("Number of variables doesn't match number of parameters in prepared statement"), e_str("HY000")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_n", e_call("strlen", vec![e_this_prop("bindTypes")])),
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_i"))), vec![
                                // The values captured by the most recent bind_param call.
                                s_assign("_value", e_index(e_this_prop("boundParams"), e_var("_i"))),
                                s_assign("_char", e_call("substr", vec![e_this_prop("bindTypes"), e_var("_i"), e_int(1)])),
                                s_if(
                                    e_binop(e_var("_value"), BinOp::StrictEq, e_null()),
                                    vec![
                                        s_expr(e_call("elephc_pdo_bind_null", vec![e_this_prop("stmt"), e_binop(e_var("_i"), BinOp::Add, e_int(1))])),
                                    ],
                                    vec![
                                    (e_binop(e_var("_char"), BinOp::StrictEq, e_str("i")), vec![
                                        s_expr(e_call("elephc_pdo_bind_int", vec![e_this_prop("stmt"), e_binop(e_var("_i"), BinOp::Add, e_int(1)), e_cast(CastType::Int, e_var("_value"))])),
                                    ]),
                                    (e_binop(e_var("_char"), BinOp::StrictEq, e_str("d")), vec![
                                        s_expr(e_call("elephc_pdo_bind_double", vec![e_this_prop("stmt"), e_binop(e_var("_i"), BinOp::Add, e_int(1)), e_cast(CastType::Float, e_var("_value"))])),
                                    ]),
                                    (e_binop(e_var("_char"), BinOp::StrictEq, e_str("b")), vec![
                                        s_assign("_blob", e_cast(CastType::String, e_var("_value"))),
                                        s_expr(e_call("elephc_pdo_bind_blob", vec![e_this_prop("stmt"), e_binop(e_var("_i"), BinOp::Add, e_int(1)), e_var("_blob"), e_call("strlen", vec![e_var("_blob")])])),
                                    ]),
                                ],
                                    Some(vec![
                                    s_assign("_text", e_cast(CastType::String, e_var("_value"))),
                                    s_expr(e_call("elephc_pdo_bind_text", vec![e_this_prop("stmt"), e_binop(e_var("_i"), BinOp::Add, e_int(1)), e_var("_text"), e_call("strlen", vec![e_var("_text")])])),
                                ]),
                                ),
                            ]),
                        ]),
                        (e_binop(e_this_prop("param_count"), BinOp::Gt, e_int(0)), vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2031), e_str("No data supplied for parameters in prepared statement"), e_str("HY000")])),
                            s_return(e_bool(false)),
                        ]),
                    ],
                        None,
                    ),
                    s_assign("_rc", e_call("elephc_pdo_step", vec![e_this_prop("stmt")])),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_cols", e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")])),
                    s_prop_assign(e_this(), "field_count", e_var("_cols")),
                    s_if(
                        e_binop(e_var("_cols"), BinOp::Eq, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "affected_rows", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                            s_prop_assign(e_this(), "insert_id", e_call("elephc_pdo_last_insert_id", vec![e_this_prop("conn"), e_str("")])),
                            s_expr(e_method_call(e_this(), "refreshLink", vec![])),
                            // Rewind now so the statement is immediately re-executable.
                            s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
                            s_expr(e_method_call(e_this(), "clearError", vec![])),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    // A result set exists: the first step's verdict (1 = row, 0 = empty) is kept
                    // pending; get_result()/store_result() drain from here.
                    s_prop_assign(e_this(), "hasPending", e_bool(true)),
                    s_prop_assign(e_this(), "pendingStep", e_var("_rc")),
                    s_prop_assign(e_this(), "num_rows", e_int(0)),
                    s_expr(e_method_call(e_this(), "refreshLink", vec![])),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        // php refreshes the OWNING connection's affected_rows / insert_id / warning_count from
        // every command's OK packet, so the canonical `$stmt->execute(); $db->insert_id;` idiom
        // reads the statement's value — not a stale one. Mirror the statement's freshly-updated
        // copies onto the link (a no-op if the statement was detached from its connection).
        .method(
            method("refreshLink")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("link"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_prop_assign(e_this_prop("link"), "affected_rows", e_this_prop("affected_rows")),
                            s_prop_assign(e_this_prop("link"), "insert_id", e_this_prop("insert_id")),
                            s_prop_assign(e_this_prop("link"), "warning_count", e_call("elephc_pdo_warning_count", vec![e_this_prop("conn")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("get_result")
                .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("stmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2050), e_str("mysqli_stmt object is already closed"), e_str("HY000")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_this_prop("hasPending")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_cols", e_this_prop("field_count")),
                    s_assign("_names", e_array(vec![])),
                    s_assign("_tables", e_array(vec![])),
                    s_assign("_natives", e_array(vec![])),
                    s_assign("_flags", e_array(vec![])),
                    s_assign("_lens", e_array(vec![])),
                    s_assign("_decimals", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_cols"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_push("_names", e_call("elephc_pdo_column_name", vec![e_this_prop("stmt"), e_var("_i")])),
                        s_array_push("_tables", e_call("elephc_pdo_column_table_name", vec![e_this_prop("stmt"), e_var("_i")])),
                        s_array_push("_natives", e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("_i")])),
                        s_array_push("_flags", e_call("elephc_pdo_column_flags", vec![e_this_prop("stmt"), e_var("_i")])),
                        s_array_push("_lens", e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("_i")])),
                        s_array_push("_decimals", e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("_i")])),
                    ]),
                    s_assign("_cells", e_array(vec![])),
                    s_assign("_rowCount", e_int(0)),
                    s_assign("_rc", e_this_prop("pendingStep")),
                    s_while(e_binop(e_var("_rc"), BinOp::Eq, e_int(1)), vec![
                        s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_cols"))), Some(s_expr(e_post_inc("_i"))), vec![
                            s_array_push("_cells", e_method_call(e_this(), "columnValue", vec![e_var("_i")])),
                        ]),
                        s_assign("_rowCount", e_binop(e_var("_rowCount"), BinOp::Add, e_int(1))),
                        s_assign("_rc", e_call("elephc_pdo_step", vec![e_this_prop("stmt")])),
                    ]),
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "opFailed", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    // The statement stays alive and rewound, so it can be re-executed.
                    s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
                    s_prop_assign(e_this(), "num_rows", e_var("_rowCount")),
                    s_prop_assign(e_this(), "affected_rows", e_var("_rowCount")),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_static_call("mysqli_result", "__elephcFromDrain", vec![e_var("_cells"), e_var("_rowCount"), e_var("_names"), e_var("_tables"), e_var("_natives"), e_var("_flags"), e_var("_lens"), e_var("_decimals")])),
                ]),
        )
        .method(
            method("store_result")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("stmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2050), e_str("mysqli_stmt object is already closed"), e_str("HY000")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_this_prop("hasPending")),
                        vec![
                            // Nothing pending (non-select execute, or already consumed): php-src
                            // treats this as a successful no-op.
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    // Without bind_result/fetch (see the module preamble) the buffered rows have
                    // no reader; consume and count them so num_rows is valid.
                    s_assign("_rowCount", e_int(0)),
                    s_assign("_rc", e_this_prop("pendingStep")),
                    s_while(e_binop(e_var("_rc"), BinOp::Eq, e_int(1)), vec![
                        s_assign("_rowCount", e_binop(e_var("_rowCount"), BinOp::Add, e_int(1))),
                        s_assign("_rc", e_call("elephc_pdo_step", vec![e_this_prop("stmt")])),
                    ]),
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
                    s_prop_assign(e_this(), "num_rows", e_var("_rowCount")),
                    s_prop_assign(e_this(), "affected_rows", e_var("_rowCount")),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("reset")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("stmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
                    s_expr(e_call("elephc_pdo_clear_bindings", vec![e_this_prop("stmt")])),
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_prop_assign(e_this(), "num_rows", e_int(0)),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("close")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("stmt"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_expr(e_call("elephc_pdo_finalize", vec![e_this_prop("stmt")])),
                            s_prop_assign(e_this(), "stmt", e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    // Clear the pending-result flag too: a post-close get_result() /
                    // store_result() must see a closed statement (and raise the already-closed
                    // error), not drive the bridge with handle -1.
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_prop_assign(e_this(), "link", e_null()),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    // The bridge ignores an unknown/already-finalized handle, so this is safe even
                    // when the owning mysqli connection was closed first.
                    s_if(
                        e_binop(e_this_prop("stmt"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_expr(e_call("elephc_pdo_finalize", vec![e_this_prop("stmt")])),
                            s_prop_assign(e_this(), "stmt", e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_prop_assign(e_this(), "link", e_null()),
                ]),
        )
        // -- internal helpers ($_-prefixed locals; same checker rule as PDO) --
        //
        // Decodes one cell of the statement's current row; same dispatch as the connection's
        // columnValue (int / float / null / length-counted bytes).
        .method(
            method("columnValue")
                .private()
                .param("index", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_assign("_type", e_call("elephc_pdo_column_type", vec![e_this_prop("stmt"), e_var("index")])),
                    s_if(
                        e_binop(e_var("_type"), BinOp::Eq, e_int(1)),
                        vec![
                            s_return(e_call("elephc_pdo_column_int", vec![e_this_prop("stmt"), e_var("index")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_type"), BinOp::Eq, e_int(2)),
                        vec![
                            s_return(e_call("elephc_pdo_column_double", vec![e_this_prop("stmt"), e_var("index")])),
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
                    s_assign("_len", e_call("elephc_pdo_column_data_len", vec![e_this_prop("stmt"), e_var("index")])),
                    s_if(
                        e_binop(e_var("_len"), BinOp::Gt, e_int(0)),
                        vec![
                            s_return(e_call("__elephc_ptr_read_string", vec![e_call("elephc_pdo_column_data_ptr", vec![e_this_prop("stmt"), e_var("index")]), e_var("_len")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_str("")),
                ]),
        )
        // Records a client-side failure that has no live bridge error state and dispatches
        // mysqli_report (same contract as mysqli::syntheticFailure).
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
        // Refreshes errno/error/sqlstate from the live statement error state (falling back to the
        // connection for prepare-level failures), then dispatches mysqli_report. Always returns
        // false so callers tail-call it.
        .method(
            method("opFailed")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "errno", e_call("elephc_pdo_stmt_errcode", vec![e_this_prop("stmt")])),
                    s_prop_assign(e_this(), "error", e_call("elephc_pdo_stmt_errmsg", vec![e_this_prop("stmt")])),
                    s_prop_assign(e_this(), "sqlstate", e_call("elephc_pdo_stmt_sqlstate", vec![e_this_prop("stmt")])),
                    s_if(
                        e_binop(e_binop(e_this_prop("errno"), BinOp::Eq, e_int(0)), BinOp::And, e_binop(e_this_prop("conn"), BinOp::GtEq, e_int(0))),
                        vec![
                            s_prop_assign(e_this(), "errno", e_call("elephc_pdo_errcode", vec![e_this_prop("conn")])),
                            s_prop_assign(e_this(), "error", e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])),
                            s_prop_assign(e_this(), "sqlstate", e_call("elephc_pdo_sqlstate", vec![e_this_prop("conn")])),
                        ],
                        vec![],
                        None,
                    ),
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
        // mysqli_report dispatch: STRICT throws mysqli_sql_exception, ERROR alone writes to
        // STDERR, OFF is silent (same contract as mysqli::report).
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
        .build()
}
