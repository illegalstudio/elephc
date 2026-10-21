//! Purpose:
//! Build the web session decoder and propagate invalid-data cancellation through EIR.
//!
//! Called from:
//! - `super::build::web_declarations()` while injecting the web prelude.
//!
//! Key details:
//! - A whole session is unserialized once, preserving references across entries.
//! - Framing failures destroy the active session, clear its values and return false.

use crate::parser::ast::{BinOp, CType, ExprKind, Stmt, TypeExpr};
use crate::synthetic_class::{e_array, e_binop, e_bool, e_call, e_const, e_int,
    e_null, e_str, e_ternary, e_var, extern_fn, function, s_array_assign, s_assign,
    s_expr, s_foreach, s_if, s_return, t_mixed};

/// Declares the complete-buffer converter and the shared cancellation helper.
pub(super) fn declarations() -> Vec<Stmt> {
    vec![
        function("__elephc_session_empty_root").returns(t_mixed())
            .body(vec![s_return(e_array(vec![]))]).build(),
        extern_fn("elephc_web_session_clear_id", "elephc_web")
            .returns(CType::Void).build(),
        extern_fn("elephc_web_session_decode_array_bytes", "elephc_web")
            .param("data", CType::Ptr).param("len", CType::Int).param("binary", CType::Int)
            .returns(CType::Ptr).build(),
        function("__elephc_session_decode_failed")
            .param("context", TypeExpr::Str).returns(TypeExpr::Bool)
            .body(vec![
                s_expr(e_call("session_destroy", vec![])),
                s_assign("_SESSION", e_array(vec![])),
                s_expr(e_call("__elephc_web_trigger_error", vec![
                    e_binop(e_var("context"), BinOp::Concat,
                        e_str("(): Failed to decode session object. Session has been destroyed")),
                    e_const("E_WARNING"),
                ])),
                s_return(e_bool(false)),
            ]).build(),
    ]
}

/// Implements the public active-session check and preserves the decoder's failure result.
pub(super) fn decl_fn_session_decode() -> Stmt {
    function("session_decode").param("data", TypeExpr::Str).returns(TypeExpr::Bool)
        .body(vec![
            s_if(e_binop(e_call("elephc_web_session_get_status", vec![]),
                BinOp::StrictNotEq, e_const("PHP_SESSION_ACTIVE")), vec![
                s_expr(e_call("__elephc_web_trigger_error", vec![
                    e_str("session_decode(): Session data cannot be decoded when there is no active session"),
                    e_const("E_WARNING"),
                ])),
                s_return(e_bool(false)),
            ], vec![], None),
            s_return(e_call("__elephc_session_decode", vec![e_var("data"), e_str("session_decode")])),
        ]).build()
}

/// Decodes one shared serialization graph and cancels the session on malformed data.
pub(super) fn decl_fn_elephc_session_decode() -> Stmt {
    let mut unserialize = e_call("unserialize", vec![e_var("serialized")]);
    unserialize.kind = ExprKind::ErrorSuppress(Box::new(unserialize.clone()));
    function("__elephc_session_decode")
        .param("raw", TypeExpr::Str)
        .param_default("context", TypeExpr::Str, e_str("session_start"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("handler", e_call("elephc_web_session_get_serialize_handler", vec![])),
            s_if(e_binop(e_var("handler"), BinOp::StrictEq, e_str("php_serialize")), vec![
                s_assign("serialized", e_var("raw")),
                s_if(e_binop(e_var("raw"), BinOp::StrictEq, e_str("")), vec![
                    s_assign("_SESSION", e_array(vec![])),
                    s_return(e_bool(true)),
                ], vec![], None),
            ], vec![], Some(vec![
                s_assign("length", e_call("strlen", vec![e_var("raw")])),
                s_assign("input", e_call("__elephc_session_stage_bytes", vec![e_var("raw")])),
                s_assign("output", e_call("elephc_web_session_decode_array_bytes", vec![
                    e_var("input"), e_var("length"),
                    e_ternary(e_binop(e_var("handler"), BinOp::StrictEq, e_str("php_binary")), e_int(1), e_int(0)),
                ])),
                s_assign("length", e_call("elephc_web_session_data_len", vec![])),
                s_if(e_binop(e_var("length"), BinOp::StrictEq, e_int(0)), vec![
                    s_return(e_call("__elephc_session_decode_failed", vec![e_var("context")])),
                ], vec![], None),
                s_assign("serialized", e_call("__elephc_session_copy_bytes", vec![e_var("output"), e_var("length")])),
            ])),
            s_assign("decoded", unserialize),
            s_if(e_binop(e_binop(e_var("decoded"), BinOp::StrictEq, e_bool(false)), BinOp::And,
                e_binop(e_call("substr", vec![e_var("serialized"), e_int(0), e_int(4)]),
                    BinOp::StrictNotEq, e_str("b:0;"))), vec![
                s_return(e_call("__elephc_session_decode_failed", vec![e_var("context")])),
            ], vec![], None),
            s_if(e_binop(e_var("handler"), BinOp::StrictEq, e_str("php_serialize")), vec![
                // php-src normalizes a serialized null root to an empty session array.
                s_if(e_binop(e_var("decoded"), BinOp::StrictEq, e_null()), vec![
                    s_assign("_SESSION", e_array(vec![])),
                ], vec![], Some(vec![s_assign("_SESSION", e_var("decoded"))])),
            ], vec![], Some(vec![
                s_if(e_call("is_array", vec![e_var("decoded")]), vec![
                    s_if(e_call("is_array", vec![e_var("_SESSION")]), vec![
                        s_if(e_binop(e_call("count", vec![e_var("_SESSION")]), BinOp::Gt, e_int(0)), vec![
                            s_foreach(e_var("decoded"), Some("key"), "value", vec![
                                s_array_assign("_SESSION", e_var("key"), e_var("value")),
                            ]),
                        ], vec![], Some(vec![s_assign("_SESSION", e_var("decoded"))])),
                    ], vec![], Some(vec![s_assign("_SESSION", e_var("decoded"))])),
                ], vec![], Some(vec![
                    s_return(e_call("__elephc_session_decode_failed", vec![e_var("context")])),
                ])),
            ])),
            s_return(e_bool(true)),
        ]).build()
}
