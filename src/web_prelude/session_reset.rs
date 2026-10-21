//! Purpose:
//! Build session_reset using fresh handler/file reads while preserving the held file lock.
//!
//! Called from:
//! - `super::build::web_declarations()` while injecting the web prelude.
//!
//! Key details:
//! - The public return stays true after an initially active session's read failure, as in PHP.
//! - Failed reads close the handler and make the session inactive; lazy snapshots refresh.

use crate::parser::ast::{BinOp, CType, Stmt, TypeExpr};
use crate::synthetic_class::{e_array, e_binop, e_bool, e_call, e_const, e_instance_of,
    e_int, e_method_call, e_static_prop, e_str, e_var, extern_fn, function, s_assign,
    s_expr, s_if, s_return, s_static_prop_assign};

/// Declares a reread operation that uses the already locked session descriptor.
pub(super) fn reread_declaration() -> Stmt {
    extern_fn("elephc_web_session_reread_bytes", "elephc_web").returns(CType::Ptr).build()
}

/// Emits the PHP read-failure warning, closes storage, and preserves the public true result.
fn failed_read(handler: &str, close: Stmt) -> Vec<Stmt> {
    vec![
        close,
        s_expr(e_call("elephc_web_session_set_status", vec![e_const("PHP_SESSION_NONE")])),
        s_expr(e_call("__elephc_web_trigger_error", vec![
            e_binop(e_binop(e_str(&format!("session_reset(): Failed to read session data: {handler} (path: ")),
                BinOp::Concat, e_var("save_path")), BinOp::Concat, e_str(")")),
            e_const("E_WARNING"),
        ])),
        s_return(e_bool(true)),
    ]
}

/// Reloads persisted data instead of restoring a potentially stale read-time snapshot.
pub(super) fn decl_fn_session_reset() -> Stmt {
    function("session_reset").returns(TypeExpr::Bool).body(vec![
        s_if(e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictNotEq,
            e_const("PHP_SESSION_ACTIVE")), vec![s_return(e_bool(false))], vec![], None),
        s_assign("handler", e_static_prop("__ElephcSessionState", "handler")),
        s_assign("id", e_call("elephc_web_session_get_id", vec![])),
        s_assign("save_path", e_call("elephc_web_session_get_save_path", vec![])),
        s_assign("_SESSION", e_array(vec![])),
        s_if(e_instance_of(e_var("handler"), "SessionHandlerInterface"), vec![
            s_assign("raw", e_method_call(e_var("handler"), "read", vec![e_var("id")])),
            s_if(e_binop(e_var("raw"), BinOp::StrictEq, e_bool(false)),
                failed_read("user", s_expr(e_method_call(e_var("handler"), "close", vec![]))), vec![], None),
            s_static_prop_assign("__ElephcSessionState", "snapshot", e_var("raw")),
            s_static_prop_assign("__ElephcSessionState", "snapshotValid", e_bool(true)),
        ], vec![], Some(vec![
            s_assign("pointer", e_call("elephc_web_session_reread_bytes", vec![])),
            s_if(e_binop(e_call("elephc_web_session_last_read_ok", vec![]), BinOp::StrictNotEq, e_int(1)),
                failed_read("files", s_expr(e_call("elephc_web_session_abort", vec![e_var("id"), e_var("save_path")]))), vec![], None),
            s_assign("raw", e_call("__elephc_session_copy_bytes", vec![
                e_var("pointer"), e_call("elephc_web_session_data_len", vec![]),
            ])),
        ])),
        s_if(e_binop(e_var("raw"), BinOp::StrictNotEq, e_str("")), vec![
            s_expr(e_call("__elephc_session_decode", vec![e_var("raw"), e_str("session_reset")])),
        ], vec![], None),
        s_return(e_bool(true)),
    ]).build()
}
