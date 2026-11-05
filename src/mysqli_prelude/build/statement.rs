//! Purpose:
//! Builds the `mysqli_stmt` class as AST: native prepares, reference parameter
//! binding, result binding with `fetch()`, `store_result()` buffering with
//! `data_seek()`, long data, attributes, and multiple result sets.
//!
//! Called from:
//! - `crate::mysqli_prelude::build::mysqli_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from `mysqli_prelude::statement::SRC` (`synthetic_class::transcribe`);
//!   the oracle compares the built class against that PHP for every profile.
//!   `$legacyAttributes` (true below PHP 8.4) is the one version conditional.
//! - `bind_param` / `bind_result` keep php's reference semantics through
//!   closures that capture the by-ref variadic by reference.

use crate::php_version::PhpVersion;
use crate::parser::ast::{BinOp, CastType, TypeExpr, Stmt};
use crate::synthetic_class::{
    class,
    closure,
    e_array,
    e_array_assoc,
    e_binop,
    e_bool,
    e_call,
    e_cast,
    e_closure_call,
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
    s_if,
    s_prop_array_assign,
    s_prop_array_push,
    s_prop_assign,
    s_return,
    s_return_void,
    s_throw,
    s_while,
    t_array,
    t_class,
    t_mixed,
    t_nullable,
    t_union,
};

/// `mysqli_stmt` — transcribed from the PHP form.
pub(super) fn decl_class_mysqli_stmt(php_version: PhpVersion) -> Stmt {
    class("mysqli_stmt")
        .prop("affected_rows", t_union(vec![TypeExpr::Int, TypeExpr::Str]), Some(e_int(0)))
        .prop("insert_id", t_union(vec![TypeExpr::Int, TypeExpr::Str]), Some(e_int(0)))
        .prop("num_rows", t_union(vec![TypeExpr::Int, TypeExpr::Str]), Some(e_int(0)))
        .prop("param_count", TypeExpr::Int, Some(e_int(0)))
        .prop("field_count", TypeExpr::Int, Some(e_int(0)))
        .private_prop("cellKinds", t_array(), Some(e_array(vec![])))
        .private_prop("cellLengths", t_array(), Some(e_array(vec![])))
        .prop("errno", TypeExpr::Int, Some(e_int(0)))
        .prop("error_list", t_array(), Some(e_array(vec![])))
        .prop("error", TypeExpr::Str, Some(e_str("")))
        .prop("sqlstate", TypeExpr::Str, Some(e_str("00000")))
        .prop("id", TypeExpr::Int, Some(e_int(0)))
        .private_static_prop("legacyAttributes", TypeExpr::Bool, Some(e_bool(php_version < PhpVersion::Php84)))
        .private_prop("stmt", TypeExpr::Int, Some(e_neg(e_int(1))))
        .private_prop("conn", TypeExpr::Int, Some(e_neg(e_int(1))))
        .private_prop("link", t_nullable(t_class("mysqli")), Some(e_null()))
        .private_prop("closed", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("queryText", TypeExpr::Str, Some(e_str("")))
        .private_prop("bindTypes", TypeExpr::Str, Some(e_str("")))
        .private_prop("paramReader", t_nullable(t_class("Closure")), Some(e_null()))
        .private_prop("resultWriter", t_nullable(t_class("Closure")), Some(e_null()))
        .private_prop("longData", t_array(), Some(e_array(vec![])))
        .private_prop("longDataSent", t_array(), Some(e_array(vec![])))
        .private_prop("executedOnce", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("executeCount", TypeExpr::Int, Some(e_int(0)))
        .private_prop("unbufferedCounted", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("hasPending", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("pendingStep", TypeExpr::Int, Some(e_int(0)))
        .private_prop("stored", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("storedCells", t_array(), Some(e_array(vec![])))
        .private_prop("storedCount", TypeExpr::Int, Some(e_int(0)))
        .private_prop("storedPos", TypeExpr::Int, Some(e_int(0)))
        .private_prop("moreResults", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("warningCount", TypeExpr::Int, Some(e_int(0)))
        .private_prop("attrUpdateMaxLength", TypeExpr::Int, Some(e_int(0)))
        .private_prop("attrCursorType", TypeExpr::Int, Some(e_int(0)))
        .private_prop("cursorOpen", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("cursorSpent", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("attrPrefetchRows", TypeExpr::Int, Some(e_int(1)))
        .method(
            method("__construct")
                .param("mysql", t_class("mysqli"))
                .param_default("query", t_nullable(TypeExpr::Str), e_null())
                .body(vec![
                    s_assign("_conn", e_method_call(e_var("mysql"), "__elephcConn", vec![])),
                    s_if(
                        e_binop(e_var("_conn"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("mysqli object is not fully initialized")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "link", e_var("mysql")),
                    s_prop_assign(e_this(), "conn", e_var("_conn")),
                    s_if(
                        e_binop(e_var("query"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_this(), "prepare", vec![e_var("query")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephcPrepareOrCopyError")
                .private()
                .param("query", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "prepareHandle", vec![e_var("query"), e_bool(true)])),
                ]),
        )
        .method(
            method("prepare")
                .param("query", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_this_prop("closed"),
                        vec![
                            s_throw(e_new("Error", vec![e_str("mysqli_stmt object is already closed")])),
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
                    s_return(e_method_call(e_this(), "prepareHandle", vec![e_var("query"), e_bool(false)])),
                ]),
        )
        .method(
            method("prepareHandle")
                .private()
                .param("query", TypeExpr::Str)
                .param("copyToLink", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("stmt"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_assign("_previousLink", e_this_prop("link")),
                            s_if(
                                e_binop(e_var("_previousLink"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_expr(e_method_call(e_var("_previousLink"), "__elephcStatementIdle", vec![e_this_prop("stmt")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_call("elephc_pdo_finalize", vec![e_this_prop("stmt")])),
                            s_prop_assign(e_this(), "stmt", e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "linkBump", vec![e_str("com_stmt_prepare")])),
                    s_expr(e_call("elephc_pdo_set_buffered_query", vec![e_this_prop("conn"), e_int(0)])),
                    s_assign("_handle", e_call("elephc_pdo_prepare", vec![e_this_prop("conn"), e_var("query"), e_int(0)])),
                    s_expr(e_call("elephc_pdo_set_buffered_query", vec![e_this_prop("conn"), e_int(1)])),
                    s_if(
                        e_binop(e_var("_handle"), BinOp::Lt, e_int(0)),
                        vec![
                            s_assign("_link", e_this_prop("link")),
                            s_if(
                                e_binop(e_var("copyToLink"), BinOp::And, e_binop(e_var("_link"), BinOp::StrictNotEq, e_null())),
                                vec![
                                    s_expr(e_method_call(e_var("_link"), "__elephcAdoptBridgeError", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "opFailed", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "stmt", e_var("_handle")),
                    s_prop_assign(e_this(), "queryText", e_var("query")),
                    s_prop_assign(e_this(), "param_count", e_call("elephc_pdo_mysql_param_count", vec![e_var("_handle")])),
                    s_assign("_head", e_call("strtoupper", vec![e_call("ltrim", vec![e_var("query")])])),
                    s_prop_assign(e_this(), "field_count", e_ternary(e_call("str_starts_with", vec![e_var("_head"), e_str("CALL")]), e_int(0), e_call("elephc_pdo_column_count", vec![e_var("_handle")]))),
                    s_prop_assign(e_this(), "cellKinds", e_array(vec![])),
                    s_prop_assign(e_this(), "id", e_call("elephc_pdo_mysql_stmt_id", vec![e_var("_handle")])),
                    s_prop_assign(e_this(), "bindTypes", e_str("")),
                    s_prop_assign(e_this(), "paramReader", e_null()),
                    s_prop_assign(e_this(), "resultWriter", e_null()),
                    s_expr(e_method_call(e_this(), "clearLongData", vec![])),
                    s_prop_assign(e_this(), "executedOnce", e_bool(false)),
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_expr(e_method_call(e_this(), "dropStored", vec![])),
                    s_prop_assign(e_this(), "moreResults", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_prop_assign(e_this(), "affected_rows", e_int(0)),
                    s_prop_assign(e_this(), "insert_id", e_int(0)),
                    s_prop_assign(e_this(), "num_rows", e_int(0)),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("bind_param")
                .param("types", TypeExpr::Str)
                .variadic_by_ref("vars", Some(t_mixed()))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephcBindParamReader", vec![e_str("mysqli_stmt::bind_param"), e_int(1), e_var("types"), e_call("count", vec![e_var("vars")]), closure()
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
                ]),
        )
        .method(
            method("__elephcBindParamReader")
                .private()
                .param("context", TypeExpr::Str)
                .param("typesArg", TypeExpr::Int)
                .param("types", TypeExpr::Str)
                .param("given", TypeExpr::Int)
                .param("reader", t_class("Closure"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_assign("_want", e_call("strlen", vec![e_var("types")])),
                    s_if(
                        e_binop(e_var("_want"), BinOp::Eq, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_binop(e_binop(e_binop(e_var("context"), BinOp::Concat, e_str("(): Argument #")), BinOp::Concat, e_var("typesArg")), BinOp::Concat, e_str(" ($types) must not be empty"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_want"), BinOp::NotEq, e_var("given")),
                        vec![
                            s_throw(e_new("ArgumentCountError", vec![e_str("The number of elements in the type definition string must match the number of bind variables")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_want"), BinOp::NotEq, e_this_prop("param_count")),
                        vec![
                            s_throw(e_new("ArgumentCountError", vec![e_str("The number of variables must match the number of parameters in the prepared statement")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_want"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_char", e_call("substr", vec![e_var("types"), e_var("_i"), e_int(1)])),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("_char"), BinOp::StrictNotEq, e_str("i")), BinOp::And, e_binop(e_var("_char"), BinOp::StrictNotEq, e_str("d"))), BinOp::And, e_binop(e_var("_char"), BinOp::StrictNotEq, e_str("s"))), BinOp::And, e_binop(e_var("_char"), BinOp::StrictNotEq, e_str("b"))),
                            vec![
                                s_throw(e_new("ValueError", vec![e_binop(e_binop(e_binop(e_var("context"), BinOp::Concat, e_str("(): Argument #")), BinOp::Concat, e_var("typesArg")), BinOp::Concat, e_str(" ($types) must only contain the \"b\", \"d\", \"i\", \"s\" type specifiers"))])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_prop_assign(e_this(), "bindTypes", e_var("types")),
                    s_prop_assign(e_this(), "paramReader", e_var("reader")),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("bind_result")
                .variadic_by_ref("vars", Some(t_mixed()))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephcBindResultWriter", vec![e_call("count", vec![e_var("vars")]), closure()
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
                ]),
        )
        .method(
            method("__elephcBindResultWriter")
                .private()
                .param("given", TypeExpr::Int)
                .param("writer", t_class("Closure"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_if(
                        e_binop(e_var("given"), BinOp::NotEq, e_this_prop("field_count")),
                        vec![
                            s_throw(e_new("ArgumentCountError", vec![e_str("Number of bind variables doesn't match number of fields in prepared statement")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "resultWriter", e_var("writer")),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("execute")
                .param_default("params", t_nullable(t_array()), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_if(
                        e_binop(e_var("params"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_if(
                                e_not(e_call("array_is_list", vec![e_var("params")])),
                                vec![
                                    s_throw(e_new("ValueError", vec![e_str("mysqli_stmt::execute(): Argument #1 ($params) must be a list array")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_given", e_call("count", vec![e_var("params")])),
                            s_if(
                                e_binop(e_var("_given"), BinOp::NotEq, e_this_prop("param_count")),
                                vec![
                                    s_throw(e_new("ValueError", vec![e_binop(e_binop(e_binop(e_binop(e_str("mysqli_stmt::execute(): Argument #1 ($params) must consist of exactly "), BinOp::Concat, e_this_prop("param_count")), BinOp::Concat, e_str(" elements, ")), BinOp::Concat, e_var("_given")), BinOp::Concat, e_str(" present"))])),
                                ],
                                vec![],
                                None,
                            ),
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
                        e_binop(e_this_prop("hasPending"), BinOp::And, e_this_prop("cursorOpen")),
                        vec![
                            s_expr(e_method_call(e_this(), "drainCursor", vec![])),
                        ],
                        vec![],
                        None,
                    ),
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
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_prop_assign(e_this(), "pendingStep", e_int(0)),
                    s_expr(e_method_call(e_this(), "dropStored", vec![])),
                    s_prop_assign(e_this(), "moreResults", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_if(
                        e_binop(e_var("params"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("_n", e_call("count", vec![e_var("params")])),
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_i"))), vec![
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
                            s_assign("_values", e_method_call(e_this(), "readBoundParams", vec![])),
                            s_assign("_n", e_call("strlen", vec![e_this_prop("bindTypes")])),
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_assign("_value", e_index(e_var("_values"), e_var("_i"))),
                                s_assign("_char", e_call("substr", vec![e_this_prop("bindTypes"), e_var("_i"), e_int(1)])),
                                s_if(
                                    e_binop(e_var("_char"), BinOp::StrictEq, e_str("b")),
                                    vec![
                                        s_assign("_sent", e_cast(CastType::Bool, e_index(e_this_prop("longDataSent"), e_var("_i")))),
                                        s_if(
                                            e_binop(e_binop(e_var("_value"), BinOp::StrictEq, e_null()), BinOp::And, e_not(e_var("_sent"))),
                                            vec![
                                                s_expr(e_call("elephc_pdo_bind_null", vec![e_this_prop("stmt"), e_binop(e_var("_i"), BinOp::Add, e_int(1))])),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_assign("_blob", e_cast(CastType::String, e_index(e_this_prop("longData"), e_var("_i")))),
                                            s_expr(e_call("elephc_pdo_bind_blob", vec![e_this_prop("stmt"), e_binop(e_var("_i"), BinOp::Add, e_int(1)), e_var("_blob"), e_call("strlen", vec![e_var("_blob")])])),
                                        ]),
                                        ),
                                    ],
                                    vec![
                                    (e_binop(e_var("_value"), BinOp::StrictEq, e_null()), vec![
                                        s_expr(e_call("elephc_pdo_bind_null", vec![e_this_prop("stmt"), e_binop(e_var("_i"), BinOp::Add, e_int(1))])),
                                    ]),
                                    (e_binop(e_var("_char"), BinOp::StrictEq, e_str("i")), vec![
                                        s_expr(e_call("elephc_pdo_bind_int", vec![e_this_prop("stmt"), e_binop(e_var("_i"), BinOp::Add, e_int(1)), e_cast(CastType::Int, e_var("_value"))])),
                                    ]),
                                    (e_binop(e_var("_char"), BinOp::StrictEq, e_str("d")), vec![
                                        s_expr(e_call("elephc_pdo_bind_double", vec![e_this_prop("stmt"), e_binop(e_var("_i"), BinOp::Add, e_int(1)), e_cast(CastType::Float, e_var("_value"))])),
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
                    s_expr(e_method_call(e_this(), "clearLongData", vec![])),
                    s_expr(e_method_call(e_this(), "linkBump", vec![e_str("com_stmt_execute")])),
                    s_expr(e_call("elephc_pdo_mysql_set_cursor_type", vec![e_this_prop("stmt"), e_this_prop("attrCursorType")])),
                    s_assign("_rc", e_call("elephc_pdo_mysql_execute", vec![e_this_prop("stmt")])),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "cursorOpen", e_binop(e_call("elephc_pdo_mysql_cursor_open", vec![e_this_prop("stmt")]), BinOp::Eq, e_int(1))),
                    s_prop_assign(e_this(), "cursorSpent", e_bool(false)),
                    s_assign("_cols", e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")])),
                    s_prop_assign(e_this(), "field_count", e_var("_cols")),
                    s_prop_assign(e_this(), "cellKinds", e_array(vec![])),
                    s_prop_assign(e_this(), "executeCount", e_binop(e_this_prop("executeCount"), BinOp::Add, e_int(1))),
                    s_prop_assign(e_this(), "unbufferedCounted", e_bool(false)),
                    s_assign("_statLink", e_this_prop("link")),
                    s_if(
                        e_binop(e_var("_statLink"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_var("_statLink"), "__elephcHeaderStats", vec![e_var("_cols"), e_bool(false)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "warningCount", e_call("elephc_pdo_warning_count", vec![e_this_prop("conn")])),
                    s_if(
                        e_binop(e_var("_cols"), BinOp::Eq, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "affected_rows", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                            s_prop_assign(e_this(), "insert_id", e_static_call("mysqli", "__elephcUnsignedId", vec![e_call("elephc_pdo_last_insert_id_text", vec![e_this_prop("conn"), e_str("")])])),
                            s_expr(e_method_call(e_this(), "refreshLink", vec![])),
                            s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
                            s_expr(e_method_call(e_this(), "clearError", vec![])),
                            s_expr(e_method_call(e_this(), "reportIndex", vec![])),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "hasPending", e_bool(true)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_prop_assign(e_this(), "pendingStep", e_int(2)),
                    s_prop_assign(e_this(), "num_rows", e_int(0)),
                    s_expr(e_method_call(e_this(), "refreshLink", vec![])),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_expr(e_method_call(e_this(), "reportIndex", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("reportIndex")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_link", e_this_prop("link")),
                    s_if(
                        e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_var("_link"), "__elephcReportIndex", vec![e_this_prop("queryText")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("fetch")
                .returns(t_nullable(TypeExpr::Bool))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_if(
                        e_this_prop("stored"),
                        vec![
                            s_if(
                                e_binop(e_this_prop("storedPos"), BinOp::GtEq, e_this_prop("storedCount")),
                                vec![
                                    s_return(e_null()),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_row", e_array(vec![])),
                            s_assign("_base", e_binop(e_this_prop("storedPos"), BinOp::Mul, e_this_prop("field_count"))),
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_array_push("_row", e_index(e_this_prop("storedCells"), e_binop(e_var("_base"), BinOp::Add, e_var("_i")))),
                            ]),
                            s_prop_assign(e_this(), "storedPos", e_binop(e_this_prop("storedPos"), BinOp::Add, e_int(1))),
                            s_expr(e_method_call(e_this(), "linkStat", vec![e_str("rows_fetched_from_client_ps_buffered"), e_int(1), e_bool(false)])),
                            s_expr(e_method_call(e_this(), "writeRow", vec![e_var("_row")])),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_this_prop("hasPending")),
                        vec![
                            s_if(
                                e_not(e_this_prop("executedOnce")),
                                vec![
                                    s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2014), e_str("Commands out of sync; you can't run this command now"), e_str("HY000")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_this_prop("cursorSpent"),
                                vec![
                                    s_expr(e_method_call(e_this(), "linkBump", vec![e_str("com_stmt_fetch")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("cursorOpen"), BinOp::And, e_method_call(e_this(), "cursorLinkBusy", vec![])),
                        vec![s_return(e_bool(false))],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_this_prop("unbufferedCounted")),
                        vec![
                            s_prop_assign(e_this(), "unbufferedCounted", e_bool(true)),
                            s_expr(e_method_call(e_this(), "linkStat", vec![e_str("ps_unbuffered_sets"), e_int(1), e_bool(true)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_this_prop("cursorOpen"),
                        vec![
                            s_expr(e_method_call(e_this(), "linkBump", vec![e_str("com_stmt_fetch")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "pendingRow", vec![])),
                    s_if(
                        e_binop(e_this_prop("pendingStep"), BinOp::Lt, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "hasPending", e_bool(false)),
                            s_expr(e_method_call(e_this(), "syncLink", vec![])),
                            s_expr(e_method_call(e_this(), "opFailed", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("pendingStep"), BinOp::Eq, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "cursorSpent", e_this_prop("cursorOpen")),
                            s_expr(e_method_call(e_this(), "finishResultSet", vec![])),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_row", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_push("_row", e_method_call(e_this(), "columnValue", vec![e_var("_i")])),
                    ]),
                    s_prop_assign(e_this(), "num_rows", e_binop(e_this_prop("num_rows"), BinOp::Add, e_int(1))),
                    s_expr(e_method_call(e_this(), "linkStat", vec![e_str("rows_fetched_from_server_ps"), e_int(1), e_bool(true)])),
                    s_expr(e_method_call(e_this(), "linkStat", vec![e_ternary(e_this_prop("cursorOpen"), e_str("rows_fetched_from_client_ps_cursor"), e_str("rows_fetched_from_client_ps_unbuffered")), e_int(1), e_bool(true)])),
                    s_prop_assign(e_this(), "pendingStep", e_int(2)),
                    s_expr(e_method_call(e_this(), "writeRow", vec![e_var("_row")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("get_result")
                .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    // mysqlnd refuses get_result() once store_result() buffered the set: the
                    // two APIs cannot be combined, and the stored rows stay fetchable.
                    s_if(
                        e_this_prop("stored"),
                        vec![
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2014), e_str("Commands out of sync; you can't run this command now"), e_str("HY000")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_this_prop("hasPending")),
                        vec![
                            s_if(
                                e_not(e_this_prop("executedOnce")),
                                vec![
                                    s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2014), e_str("Commands out of sync; you can't run this command now"), e_str("HY000")])),
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
                        e_this_prop("cursorOpen"),
                        vec![
                            s_if(
                                e_method_call(e_this(), "cursorLinkBusy", vec![]),
                                vec![s_return(e_bool(false))],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "linkBump", vec![e_str("com_stmt_fetch")])),
                            s_if(
                                e_binop(e_call("elephc_pdo_mysql_cursor_fetch_all", vec![e_this_prop("stmt")]), BinOp::Lt, e_int(0)),
                                vec![
                                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                                    s_expr(e_method_call(e_this(), "opFailed", vec![])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_cells", e_array(vec![])),
                    s_assign("_rowCount", e_int(0)),
                    s_assign("_rc", e_method_call(e_this(), "pendingRow", vec![])),
                    s_while(e_binop(e_var("_rc"), BinOp::Eq, e_int(1)), vec![
                        s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                            s_array_push("_cells", e_method_call(e_this(), "columnValue", vec![e_var("_i")])),
                        ]),
                        s_assign("_rowCount", e_binop(e_var("_rowCount"), BinOp::Add, e_int(1))),
                        s_assign("_rc", e_call("elephc_pdo_step", vec![e_this_prop("stmt")])),
                    ]),
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "opFailed", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "linkBump", vec![e_str("buffered_sets")])),
                    s_expr(e_method_call(e_this(), "linkStat", vec![e_str("rows_fetched_from_server_ps"), e_var("_rowCount"), e_bool(true)])),
                    s_expr(e_method_call(e_this(), "linkStat", vec![e_str("rows_buffered_from_client_ps"), e_var("_rowCount"), e_bool(true)])),
                    s_prop_assign(e_this(), "num_rows", e_var("_rowCount")),
                    s_prop_assign(e_this(), "affected_rows", e_var("_rowCount")),
                    s_expr(e_method_call(e_this(), "finishResultSet", vec![])),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_method_call(e_this(), "buildResult", vec![e_var("_cells"), e_var("_rowCount")])),
                ]),
        )
        .method(
            method("store_result")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_if(
                        e_not(e_this_prop("hasPending")),
                        vec![
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_this_prop("cursorOpen"),
                        vec![
                            s_if(
                                e_method_call(e_this(), "cursorLinkBusy", vec![]),
                                vec![s_return(e_bool(false))],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "linkBump", vec![e_str("com_stmt_fetch")])),
                            s_if(
                                e_binop(e_call("elephc_pdo_mysql_cursor_fetch_all", vec![e_this_prop("stmt")]), BinOp::Lt, e_int(0)),
                                vec![
                                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                                    s_return(e_method_call(e_this(), "opFailed", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_cells", e_array(vec![])),
                    s_assign("_rowCount", e_int(0)),
                    s_assign("_rc", e_method_call(e_this(), "pendingRow", vec![])),
                    s_while(e_binop(e_var("_rc"), BinOp::Eq, e_int(1)), vec![
                        s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                            s_array_push("_cells", e_method_call(e_this(), "columnValue", vec![e_var("_i")])),
                        ]),
                        s_assign("_rowCount", e_binop(e_var("_rowCount"), BinOp::Add, e_int(1))),
                        s_assign("_rc", e_call("elephc_pdo_step", vec![e_this_prop("stmt")])),
                    ]),
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "linkBump", vec![e_str("ps_buffered_sets")])),
                    s_expr(e_method_call(e_this(), "linkStat", vec![e_str("rows_fetched_from_server_ps"), e_var("_rowCount"), e_bool(true)])),
                    s_expr(e_method_call(e_this(), "linkStat", vec![e_str("rows_buffered_from_client_ps"), e_var("_rowCount"), e_bool(true)])),
                    s_prop_assign(e_this(), "stored", e_bool(true)),
                    s_prop_assign(e_this(), "storedCells", e_var("_cells")),
                    s_prop_assign(e_this(), "storedCount", e_var("_rowCount")),
                    s_prop_assign(e_this(), "storedPos", e_int(0)),
                    s_prop_assign(e_this(), "num_rows", e_var("_rowCount")),
                    s_prop_assign(e_this(), "affected_rows", e_var("_rowCount")),
                    s_expr(e_method_call(e_this(), "finishResultSet", vec![])),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("data_seek")
                .param("offset", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_var("offset"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli_stmt::data_seek(): Argument #1 ($offset) must be greater than or equal to 0")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_if(
                        e_this_prop("stored"),
                        vec![
                            s_prop_assign(e_this(), "storedPos", e_ternary(e_binop(e_var("offset"), BinOp::Gt, e_this_prop("storedCount")), e_this_prop("storedCount"), e_var("offset"))),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("num_rows")
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::Str]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_return(e_this_prop("num_rows")),
                ]),
        )
        .method(
            method("result_metadata")
                .returns(t_union(vec![t_class("mysqli_result"), TypeExpr::False]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_if(
                        e_binop(e_this_prop("field_count"), BinOp::Eq, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "buildResult", vec![e_call("array_slice", vec![e_this_prop("storedCells"), e_int(0), e_int(0)]), e_int(0)])),
                ]),
        )
        .method(
            method("more_results")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_return(e_this_prop("moreResults")),
                ]),
        )
        .method(
            method("next_result")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_if(
                        e_not(e_this_prop("moreResults")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "moreResults", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_assign("_advanced", e_call("elephc_pdo_next_rowset", vec![e_this_prop("stmt")])),
                    s_if(
                        e_binop(e_var("_advanced"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_method_call(e_this(), "opFailed", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_advanced"), BinOp::Eq, e_int(0)),
                        vec![
                            s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_rc", e_int(2)),
                    s_expr(e_method_call(e_this(), "dropStored", vec![])),
                    s_prop_assign(e_this(), "field_count", e_call("elephc_pdo_column_count", vec![e_this_prop("stmt")])),
                    s_prop_assign(e_this(), "cellKinds", e_array(vec![])),
                    s_prop_assign(e_this(), "num_rows", e_int(0)),
                    s_if(
                        e_binop(e_this_prop("field_count"), BinOp::Eq, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "hasPending", e_bool(false)),
                            s_expr(e_method_call(e_this(), "syncLink", vec![])),
                            s_prop_assign(e_this(), "affected_rows", e_call("elephc_pdo_changes", vec![e_this_prop("conn")])),
                            s_expr(e_method_call(e_this(), "finishResultSet", vec![])),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "hasPending", e_bool(true)),
                        s_expr(e_method_call(e_this(), "syncLink", vec![])),
                        s_prop_assign(e_this(), "pendingStep", e_var("_rc")),
                    ]),
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("send_long_data")
                .param("param_num", TypeExpr::Int)
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("param_num"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli_stmt::send_long_data(): Argument #1 ($param_num) must be greater than or equal to 0")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    // mysqlnd's order: no bound parameters is "out of sync", then the
                    // parameter number, then a parameter not bound as `b` (MYSQL_TYPE_LONG_BLOB).
                    // None of them reaches mysqli_report(), like php.
                    s_if(
                        e_binop(e_this_prop("bindTypes"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_expr(e_method_call(e_this(), "unreportedFailure", vec![e_int(2014), e_str("Commands out of sync; you can't run this command now")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("param_num"), BinOp::GtEq, e_this_prop("param_count")),
                        vec![
                            s_expr(e_method_call(e_this(), "unreportedFailure", vec![e_int(2034), e_str("Invalid parameter number")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("substr", vec![e_this_prop("bindTypes"), e_var("param_num"), e_int(1)]), BinOp::StrictNotEq, e_str("b")),
                        vec![
                            s_expr(e_method_call(e_this(), "unreportedFailure", vec![e_int(2035), e_str("Can't send long data for non-string/non-binary data types")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_prop_array_assign(e_this(), "longData", e_var("param_num"), e_binop(e_cast(CastType::String, e_index(e_this_prop("longData"), e_var("param_num"))), BinOp::Concat, e_var("data"))),
                    s_prop_array_assign(e_this(), "longDataSent", e_var("param_num"), e_bool(true)),
                    s_expr(e_method_call(e_this(), "linkBump", vec![e_str("com_stmt_send_long_data")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("attr_get")
                .param("attribute", TypeExpr::Int)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_if(
                        e_binop(e_var("attribute"), BinOp::Eq, e_int(0)),
                        vec![
                            s_return(e_this_prop("attrUpdateMaxLength")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("attribute"), BinOp::Eq, e_int(1)),
                        vec![
                            s_return(e_this_prop("attrCursorType")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(2)), BinOp::And, e_static_prop("mysqli_stmt", "legacyAttributes")),
                        vec![
                            s_return(e_this_prop("attrPrefetchRows")),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("ValueError", vec![e_method_call(e_this(), "attributeError", vec![e_str("mysqli_stmt::attr_get")])])),
                ]),
        )
        .method(
            method("attr_set")
                .param("attribute", TypeExpr::Int)
                .param("value", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_if(
                        e_binop(e_var("attribute"), BinOp::Eq, e_int(0)),
                        vec![
                            s_if(
                                e_binop(e_binop(e_var("value"), BinOp::NotEq, e_int(0)), BinOp::And, e_binop(e_var("value"), BinOp::NotEq, e_int(1))),
                                vec![
                                    s_throw(e_new("ValueError", vec![e_str("mysqli_stmt::attr_set(): Argument #2 ($value) must be 0 or 1 for attribute MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "attrUpdateMaxLength", e_var("value")),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("attribute"), BinOp::Eq, e_int(1)),
                        vec![
                            s_assign("_legal", e_binop(e_binop(e_binop(e_var("value"), BinOp::Eq, e_int(0)), BinOp::Or, e_binop(e_var("value"), BinOp::Eq, e_int(1))), BinOp::Or, e_binop(e_static_prop("mysqli_stmt", "legacyAttributes"), BinOp::And, e_binop(e_binop(e_var("value"), BinOp::Eq, e_int(2)), BinOp::Or, e_binop(e_var("value"), BinOp::Eq, e_int(4)))))),
                            s_if(
                                e_not(e_var("_legal")),
                                vec![
                                    s_throw(e_new("ValueError", vec![e_str("mysqli_stmt::attr_set(): Argument #2 ($value) must be one of the MYSQLI_CURSOR_TYPE_* constants for attribute MYSQLI_STMT_ATTR_CURSOR_TYPE")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("value"), BinOp::Gt, e_int(1)),
                                vec![
                                    s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2054), e_str("Not implemented"), e_str("HY000")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "attrCursorType", e_var("value")),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("attribute"), BinOp::Eq, e_int(2)), BinOp::And, e_static_prop("mysqli_stmt", "legacyAttributes")),
                        vec![
                            s_if(
                                e_binop(e_var("value"), BinOp::Lt, e_int(1)),
                                vec![
                                    s_throw(e_new("ValueError", vec![e_str("mysqli_stmt::attr_set(): Argument #2 ($value) must be greater than 0 for attribute MYSQLI_STMT_ATTR_PREFETCH_ROWS")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("value"), BinOp::Gt, e_int(1)),
                                vec![
                                    s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2054), e_str("Not implemented"), e_str("HY000")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "attrPrefetchRows", e_var("value")),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("ValueError", vec![e_method_call(e_this(), "attributeError", vec![e_str("mysqli_stmt::attr_set")])])),
                ]),
        )
        .method(
            method("get_warnings")
                .returns(t_union(vec![t_class("mysqli_warning"), TypeExpr::False]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_assign("_link", e_this_prop("link")),
                    s_if(
                        e_binop(e_binop(e_var("_link"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_this_prop("warningCount"), BinOp::Eq, e_int(0))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_var("_link"), "__elephcFetchWarnings", vec![])),
                ]),
        )
        .method(
            method("free_result")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_if(
                        e_binop(e_this_prop("hasPending"), BinOp::And, e_this_prop("cursorOpen")),
                        vec![
                            s_expr(e_method_call(e_this(), "drainCursor", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("hasPending"), BinOp::And, e_binop(e_this_prop("stmt"), BinOp::GtEq, e_int(0))),
                        vec![
                            s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_prop_assign(e_this(), "moreResults", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_expr(e_method_call(e_this(), "dropStored", vec![])),
                ]),
        )
        .method(
            method("reset")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_expr(e_method_call(e_this(), "linkBump", vec![e_str("com_stmt_reset")])),
                    s_if(
                        e_binop(e_this_prop("hasPending"), BinOp::And, e_this_prop("cursorOpen")),
                        vec![
                            s_expr(e_method_call(e_this(), "drainCursor", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
                    s_expr(e_method_call(e_this(), "clearLongData", vec![])),
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_prop_assign(e_this(), "moreResults", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_expr(e_method_call(e_this(), "dropStored", vec![])),
                    s_prop_assign(e_this(), "num_rows", e_int(0)),
                    s_expr(e_method_call(e_this(), "clearError", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("close")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "requireValid", vec![])),
                    s_expr(e_method_call(e_this(), "countClose", vec![e_bool(true)])),
                    s_expr(e_method_call(e_this(), "linkBump", vec![e_str("com_stmt_close")])),
                    s_expr(e_method_call(e_this(), "release", vec![])),
                    s_prop_assign(e_this(), "closed", e_bool(true)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_if(
                        e_binop(e_not(e_this_prop("closed")), BinOp::And, e_binop(e_this_prop("stmt"), BinOp::GtEq, e_int(0))),
                        vec![
                            s_expr(e_method_call(e_this(), "countClose", vec![e_bool(false)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "release", vec![])),
                ]),
        )
        .method(
            method("countClose")
                .private()
                .param("explicit", TypeExpr::Bool)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("link"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("hasPending"), BinOp::And, e_not(e_this_prop("stored"))), BinOp::And, e_this_prop("cursorOpen")),
                        vec![
                            s_expr(e_method_call(e_this(), "drainCursor", vec![])),
                        ],
                        vec![
                            (e_binop(e_this_prop("hasPending"), BinOp::And, e_not(e_this_prop("stored"))), vec![
                                s_expr(e_method_call(e_this(), "linkStat", vec![e_str("flushed_ps_sets"), e_int(1), e_bool(true)])),
                                s_assign("_rc", e_method_call(e_this(), "pendingRow", vec![])),
                                s_while(e_binop(e_var("_rc"), BinOp::Eq, e_int(1)), vec![
                                    s_expr(e_method_call(e_this(), "linkStat", vec![e_str("rows_fetched_from_server_ps"), e_int(1), e_bool(true)])),
                                    s_expr(e_method_call(e_this(), "linkStat", vec![e_str("rows_fetched_from_client_ps_unbuffered"), e_int(1), e_bool(true)])),
                                    s_expr(e_method_call(e_this(), "linkStat", vec![e_str("rows_skipped_ps"), e_int(1), e_bool(true)])),
                                    s_assign("_rc", e_call("elephc_pdo_step", vec![e_this_prop("stmt")])),
                                ]),
                                s_prop_assign(e_this(), "pendingStep", e_var("_rc")),
                            ]),
                        ],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("field_count"), BinOp::Gt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "linkStat", vec![e_str("implicit_free_result"), e_int(1), e_bool(true)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "linkStat", vec![e_ternary(e_var("explicit"), e_str("explicit_free_result"), e_str("implicit_free_result")), e_int(1), e_bool(false)])),
                    s_if(
                        e_binop(e_this_prop("executeCount"), BinOp::Eq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "linkStat", vec![e_str("ps_prepared_never_executed"), e_int(1), e_bool(true)])),
                        ],
                        vec![
                        (e_binop(e_this_prop("executeCount"), BinOp::Eq, e_int(1)), vec![
                            s_expr(e_method_call(e_this(), "linkStat", vec![e_str("ps_prepared_once_executed"), e_int(1), e_bool(true)])),
                        ]),
                    ],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "linkStat", vec![e_ternary(e_var("explicit"), e_str("explicit_stmt_close"), e_str("implicit_stmt_close")), e_int(1), e_bool(false)])),
                ]),
        )
        .method(
            method("pendingRow")
                .private()
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("pendingStep"), BinOp::Eq, e_int(2)),
                        vec![
                            s_prop_assign(e_this(), "pendingStep", e_call("elephc_pdo_step", vec![e_this_prop("stmt")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this_prop("pendingStep")),
                ]),
        )
        .method(
            method("drainCursor")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "linkStat", vec![e_str("flushed_ps_sets"), e_int(1), e_bool(true)])),
                    s_expr(e_method_call(e_this(), "linkBump", vec![e_str("com_stmt_fetch")])),
                    s_assign("_rc", e_method_call(e_this(), "pendingRow", vec![])),
                    s_while(e_binop(e_var("_rc"), BinOp::Eq, e_int(1)), vec![
                        s_expr(e_method_call(e_this(), "linkStat", vec![e_str("rows_fetched_from_server_ps"), e_int(1), e_bool(true)])),
                        s_expr(e_method_call(e_this(), "linkStat", vec![e_str("rows_fetched_from_client_ps_cursor"), e_int(1), e_bool(true)])),
                        s_expr(e_method_call(e_this(), "linkStat", vec![e_str("rows_skipped_ps"), e_int(1), e_bool(true)])),
                        s_expr(e_method_call(e_this(), "linkBump", vec![e_str("com_stmt_fetch")])),
                        s_assign("_rc", e_call("elephc_pdo_step", vec![e_this_prop("stmt")])),
                    ]),
                    s_prop_assign(e_this(), "pendingStep", e_var("_rc")),
                ]),
        )
        .method(
            method("syncLink")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_link", e_this_prop("link")),
                    s_if(
                        e_binop(e_binop(e_var("_link"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_this_prop("stmt"), BinOp::Lt, e_int(0))),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("hasPending"), BinOp::And, e_not(e_this_prop("cursorOpen"))), BinOp::Or, e_this_prop("moreResults")),
                        vec![
                            s_expr(e_method_call(e_var("_link"), "__elephcStatementBusy", vec![e_this_prop("stmt")])),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_method_call(e_var("_link"), "__elephcStatementIdle", vec![e_this_prop("stmt")])),
                    ]),
                    ),
                ]),
        )
        .method(
            method("release")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_link", e_this_prop("link")),
                    s_if(
                        e_binop(e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_this_prop("stmt"), BinOp::GtEq, e_int(0))),
                        vec![
                            s_expr(e_method_call(e_var("_link"), "__elephcStatementIdle", vec![e_this_prop("stmt")])),
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
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_prop_assign(e_this(), "moreResults", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_expr(e_method_call(e_this(), "dropStored", vec![])),
                    s_prop_assign(e_this(), "paramReader", e_null()),
                    s_prop_assign(e_this(), "resultWriter", e_null()),
                    s_prop_assign(e_this(), "link", e_null()),
                ]),
        )
        .method(
            method("requireValid")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_this_prop("closed"),
                        vec![
                            s_throw(e_new("Error", vec![e_str("mysqli_stmt object is already closed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("stmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("mysqli_stmt object is not fully initialized")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("attributeError")
                .private()
                .param("context", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_static_prop("mysqli_stmt", "legacyAttributes"),
                        vec![
                            s_return(e_binop(e_var("context"), BinOp::Concat, e_str("(): Argument #1 ($attribute) must be one of MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH, MYSQLI_STMT_ATTR_PREFETCH_ROWS, or STMT_ATTR_CURSOR_TYPE"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_var("context"), BinOp::Concat, e_str("(): Argument #1 ($attribute) must be either MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH or MYSQLI_STMT_ATTR_CURSOR_TYPE"))),
                ]),
        )
        .method(
            method("readBoundParams")
                .private()
                .returns(t_array())
                .body(vec![
                    s_assign("_reader", e_this_prop("paramReader")),
                    s_if(
                        e_binop(e_var("_reader"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_array(vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_cast(CastType::Array, e_closure_call("_reader", vec![]))),
                ]),
        )
        .method(
            method("writeRow")
                .private()
                .param("row", t_array())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_writer", e_this_prop("resultWriter")),
                    s_assign("_row", e_var("row")),
                    s_if(
                        e_binop(e_var("_writer"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_closure_call("_writer", vec![e_var("_row")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("clearLongData")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_data", e_array(vec![])),
                    s_assign("_sent", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("param_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_push("_data", e_str("")),
                        s_array_push("_sent", e_bool(false)),
                    ]),
                    s_prop_assign(e_this(), "longData", e_var("_data")),
                    s_prop_assign(e_this(), "longDataSent", e_var("_sent")),
                ]),
        )
        .method(
            method("dropStored")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "stored", e_bool(false)),
                    s_prop_assign(e_this(), "storedCells", e_array(vec![])),
                    s_prop_assign(e_this(), "storedCount", e_int(0)),
                    s_prop_assign(e_this(), "storedPos", e_int(0)),
                ]),
        )
        .method(
            method("finishResultSet")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "hasPending", e_bool(false)),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_prop_assign(e_this(), "moreResults", e_binop(e_call("elephc_pdo_mysql_more_results", vec![e_this_prop("stmt")]), BinOp::Eq, e_int(1))),
                    s_expr(e_method_call(e_this(), "syncLink", vec![])),
                    s_if(
                        e_not(e_this_prop("moreResults")),
                        vec![
                            s_expr(e_call("elephc_pdo_reset", vec![e_this_prop("stmt")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("buildResult")
                .private()
                .param("cells", t_array())
                .param("rowCount", TypeExpr::Int)
                .returns(t_class("mysqli_result"))
                .body(vec![
                    s_assign("_names", e_array(vec![])),
                    s_assign("_tables", e_array(vec![])),
                    s_assign("_natives", e_array(vec![])),
                    s_assign("_flags", e_array(vec![])),
                    s_assign("_lens", e_array(vec![])),
                    s_assign("_decimals", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_push("_names", e_call("elephc_pdo_column_name", vec![e_this_prop("stmt"), e_var("_i")])),
                        s_array_push("_tables", e_call("elephc_pdo_column_table_name", vec![e_this_prop("stmt"), e_var("_i")])),
                        s_array_push("_natives", e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("_i")])),
                        s_array_push("_flags", e_call("elephc_pdo_column_flags", vec![e_this_prop("stmt"), e_var("_i")])),
                        s_array_push("_lens", e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("_i")])),
                        s_array_push("_decimals", e_call("elephc_pdo_column_precision", vec![e_this_prop("stmt"), e_var("_i")])),
                    ]),
                    s_assign("_result", e_static_call("mysqli_result", "__elephcFromDrain", vec![e_var("cells"), e_var("rowCount"), e_var("_names"), e_var("_tables"), e_var("_natives"), e_var("_flags"), e_var("_lens"), e_var("_decimals")])),
                    s_assign("_statLink", e_this_prop("link")),
                    s_if(
                        e_binop(e_var("_statLink"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_var("_result"), "__elephcCountFor", vec![e_var("_statLink"), e_bool(true)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("requireLinkNotBusy")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_link", e_this_prop("link")),
                    s_if(
                        e_binop(e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()), BinOp::And, e_method_call(e_var("_link"), "__elephcBusyFor", vec![e_this_prop("stmt")])),
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
            method("cursorLinkBusy")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_link", e_this_prop("link")),
                    s_if(
                        e_binop(e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()), BinOp::And, e_method_call(e_var("_link"), "__elephcBusyFor", vec![e_this_prop("stmt")])),
                        vec![
                            s_expr(e_method_call(e_var("_link"), "__elephcOutOfSync", vec![])),
                            s_expr(e_method_call(e_this(), "syntheticFailure", vec![e_int(2014), e_str("Commands out of sync; you can't run this command now"), e_str("HY000")])),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("linkStat")
                .private()
                .param("key", TypeExpr::Str)
                .param("by", TypeExpr::Int)
                .param("connection", TypeExpr::Bool)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_link", e_this_prop("link")),
                    s_if(
                        e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_var("_link"), "__elephcStat", vec![e_var("key"), e_var("by"), e_var("connection")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("linkBump")
                .private()
                .param("key", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_link", e_this_prop("link")),
                    s_if(
                        e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_var("_link"), "__elephcBump", vec![e_var("key")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("refreshLink")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_link", e_this_prop("link")),
                    s_if(
                        e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_prop_assign(e_var("_link"), "affected_rows", e_this_prop("affected_rows")),
                            s_prop_assign(e_var("_link"), "insert_id", e_this_prop("insert_id")),
                            s_prop_assign(e_var("_link"), "warning_count", e_this_prop("warningCount")),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("columnValue")
                .private()
                .param("index", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_call("count", vec![e_this_prop("cellKinds")]), BinOp::NotEq, e_this_prop("field_count")),
                        vec![
                            s_prop_assign(e_this(), "cellKinds", e_array(vec![])),
                            s_prop_assign(e_this(), "cellLengths", e_array(vec![])),
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_prop_array_push(e_this(), "cellKinds", e_static_call("mysqli_result", "__elephcCellKind", vec![e_call("elephc_pdo_column_native_type", vec![e_this_prop("stmt"), e_var("_i")]), e_call("elephc_pdo_column_flags", vec![e_this_prop("stmt"), e_var("_i")]), e_bool(false), e_bool(true)])),
                                s_prop_array_push(e_this(), "cellLengths", e_call("elephc_pdo_column_len", vec![e_this_prop("stmt"), e_var("_i")])),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("mysqli_result", "__elephcConvertCell", vec![e_method_call(e_this(), "rawColumnValue", vec![e_var("index")]), e_cast(CastType::Int, e_index(e_this_prop("cellKinds"), e_var("index"))), e_cast(CastType::Int, e_index(e_this_prop("cellLengths"), e_var("index")))])),
                ]),
        )
        .method(
            method("rawColumnValue")
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
            method("opFailed")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "error", e_static_call("mysqli", "__elephcServerMessage", vec![e_call("elephc_pdo_stmt_errmsg", vec![e_this_prop("stmt")])])),
                    s_prop_assign(e_this(), "errno", e_static_call("mysqli", "__elephcClientErrno", vec![e_call("elephc_pdo_stmt_errcode", vec![e_this_prop("stmt")]), e_this_prop("error")])),
                    s_prop_assign(e_this(), "sqlstate", e_call("elephc_pdo_stmt_sqlstate", vec![e_this_prop("stmt")])),
                    s_if(
                        e_binop(e_binop(e_binop(e_this_prop("errno"), BinOp::Eq, e_int(0)), BinOp::Or, e_binop(e_this_prop("stmt"), BinOp::Lt, e_int(0))), BinOp::And, e_binop(e_this_prop("conn"), BinOp::GtEq, e_int(0))),
                        vec![
                            s_prop_assign(e_this(), "error", e_static_call("mysqli", "__elephcServerMessage", vec![e_call("elephc_pdo_errmsg", vec![e_this_prop("conn")])])),
                            s_prop_assign(e_this(), "errno", e_static_call("mysqli", "__elephcClientErrno", vec![e_call("elephc_pdo_errcode", vec![e_this_prop("conn")]), e_this_prop("error")])),
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
        .method(
            method("unreportedFailure")
                .private()
                .param("errno", TypeExpr::Int)
                .param("message", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "errno", e_var("errno")),
                    s_prop_assign(e_this(), "error", e_var("message")),
                    s_prop_assign(e_this(), "sqlstate", e_str("HY000")),
                    s_prop_assign(e_this(), "error_list", e_array(vec![e_array_assoc(vec![(e_str("errno"), e_var("errno")), (e_str("sqlstate"), e_str("HY000")), (e_str("error"), e_var("message"))])])),
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
        .build()
}
