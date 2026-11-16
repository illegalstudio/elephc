//! Purpose:
//! Builds the `mysqli_driver` and `mysqli_warning` classes as AST.
//!
//! Called from:
//! - `crate::mysqli_prelude::build::mysqli_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from `mysqli_prelude::driver::SRC` (`synthetic_class::transcribe`);
//!   the oracle compares each built class against that PHP.
//! - `mysqli_driver`'s properties are virtual property hooks (`.hooked(...)` plus
//!   the `__propget_*` / `__propset_*` accessor methods the parser produces).

use crate::parser::ast::{BinOp, CastType, TypeExpr, Stmt};
use crate::synthetic_class::{
    class,
    e_array,
    e_binop,
    e_bool,
    e_call,
    e_cast,
    e_index,
    e_int,
    e_method_call,
    e_new,
    e_static_prop,
    e_str,
    e_this,
    e_this_prop,
    e_var,
    method,
    s_assign,
    s_expr,
    s_if,
    s_prop_assign,
    s_return,
    s_static_prop_assign,
    t_array,
    t_class,
};

/// `mysqli_driver` — transcribed from the PHP form.
pub(super) fn decl_class_mysqli_driver() -> Stmt {
    class("mysqli_driver")
        .final_()
        .prop("client_info", TypeExpr::Str, None)
        .hooked(true, false, false)
        .prop("client_version", TypeExpr::Int, None)
        .hooked(true, false, false)
        .prop("driver_version", TypeExpr::Int, None)
        .hooked(true, false, false)
        .prop("report_mode", TypeExpr::Int, None)
        .hooked(true, true, false)
        .method(
            method("__propget_client_info")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_call("elephc_pdo_mysql_client_info", vec![])),
                ]),
        )
        .method(
            method("__propget_client_version")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("mysqli_get_client_version", vec![])),
                ]),
        )
        .method(
            method("__propget_driver_version")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_int(101009)),
                ]),
        )
        .method(
            method("__propget_report_mode")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_static_prop("mysqli", "reportMode")),
                ]),
        )
        .method(
            method("__propset_report_mode")
                .param("value", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_static_prop_assign("mysqli", "reportMode", e_var("value")),
                ]),
        )
        .build()
}

/// `mysqli_warning` — transcribed from the PHP form.
pub(super) fn decl_class_mysqli_warning() -> Stmt {
    class("mysqli_warning")
        .final_()
        .prop("message", TypeExpr::Str, Some(e_str("")))
        .prop("sqlstate", TypeExpr::Str, Some(e_str("HY000")))
        .prop("errno", TypeExpr::Int, Some(e_int(0)))
        .private_prop("codes", t_array(), Some(e_array(vec![])))
        .private_prop("messages", t_array(), Some(e_array(vec![])))
        .private_prop("position", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .private(),
        )
        .method(
            method("__elephcFromRows")
                .private()
                .static_()
                .param("codes", t_array())
                .param("messages", t_array())
                .returns(t_class("mysqli_warning"))
                .body(vec![
                    s_assign("_warning", e_new("mysqli_warning", vec![])),
                    s_prop_assign(e_var("_warning"), "codes", e_var("codes")),
                    s_prop_assign(e_var("_warning"), "messages", e_var("messages")),
                    s_expr(e_method_call(e_var("_warning"), "load", vec![e_int(0)])),
                    s_return(e_var("_warning")),
                ]),
        )
        .method(
            method("next")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_this_prop("position"), BinOp::Add, e_int(1)), BinOp::GtEq, e_call("count", vec![e_this_prop("codes")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "load", vec![e_binop(e_this_prop("position"), BinOp::Add, e_int(1))])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("load")
                .private()
                .param("index", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "position", e_var("index")),
                    s_prop_assign(e_this(), "errno", e_cast(CastType::Int, e_index(e_this_prop("codes"), e_var("index")))),
                    s_prop_assign(e_this(), "message", e_cast(CastType::String, e_index(e_this_prop("messages"), e_var("index")))),
                    s_prop_assign(e_this(), "sqlstate", e_str("HY000")),
                ]),
        )
        .build()
}
