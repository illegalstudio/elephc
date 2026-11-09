//! Purpose:
//! The `ext/libxml` error surface of the xml prelude: the `LibXMLError` class and the
//! `libxml_use_internal_errors()` / `libxml_get_errors()` / `libxml_get_last_error()` /
//! `libxml_clear_errors()` functions over the bridge's libxml error state
//! (`crates/elephc-xml/src/libxml_errors.rs`).
//!
//! Called from:
//! - `crate::xml_prelude::build::xml_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from the PHP form kept in `crate::xml_prelude::fragments`; the oracle test
//!   compares both node by node.
//! - The bridge mirrors php-src `ext/libxml/libxml.c`: the list exists only while internal
//!   errors are enabled, and `libxml_get_last_error()` falls back to libxml2's last error.
//! - `LibXMLError` declares php-src's properties in stub order (level, code, column, message,
//!   file, line), which `var_dump()` follows.

use crate::parser::ast::{BinOp, CType, Stmt, TypeExpr};
use crate::synthetic_class::{
    class, e_array, e_binop, e_bool, e_call, e_int, e_neg, e_new, e_null, e_post_inc,
    e_ternary, e_var, extern_fn, function, s_array_push, s_assign, s_expr, s_for, s_if,
    s_prop_assign, s_return, t_array, t_class, t_nullable, t_union,
};

/// `elephc_xml_libxml_use_internal_errors` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_libxml_use_internal_errors() -> Stmt {
    extern_fn("elephc_xml_libxml_use_internal_errors", "elephc_xml")
        .param("mode", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_libxml_error_count` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_libxml_error_count() -> Stmt {
    extern_fn("elephc_xml_libxml_error_count", "elephc_xml")
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_libxml_error_select` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_libxml_error_select() -> Stmt {
    extern_fn("elephc_xml_libxml_error_select", "elephc_xml")
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_libxml_error_int` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_libxml_error_int() -> Stmt {
    extern_fn("elephc_xml_libxml_error_int", "elephc_xml")
        .param("field", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_libxml_error_string` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_libxml_error_string() -> Stmt {
    extern_fn("elephc_xml_libxml_error_string", "elephc_xml")
        .param("field", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_libxml_clear_errors` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_libxml_clear_errors() -> Stmt {
    extern_fn("elephc_xml_libxml_clear_errors", "elephc_xml")
        .returns(CType::Void)
        .build()
}

/// `LibXMLError` — transcribed from the PHP form.
pub(super) fn decl_class_libxmlerror() -> Stmt {
    class("LibXMLError")
        .prop("level", TypeExpr::Int, None)
        .prop("code", TypeExpr::Int, None)
        .prop("column", TypeExpr::Int, None)
        .prop("message", TypeExpr::Str, None)
        .prop("file", TypeExpr::Str, None)
        .prop("line", TypeExpr::Int, None)
        .build()
}

/// The statements that copy the bridge's selected error into a new `$error` object; the PHP
/// form spells them out in both `libxml_get_errors()` and `libxml_get_last_error()`.
fn selected_error_into_var() -> Vec<Stmt> {
    let field = |kind: &str, index: i64| e_call(kind, vec![e_int(index)]);
    vec![
        s_assign("error", e_new("LibXMLError", vec![])),
        s_prop_assign(e_var("error"), "level", field("elephc_xml_libxml_error_int", 0)),
        s_prop_assign(e_var("error"), "code", field("elephc_xml_libxml_error_int", 1)),
        s_prop_assign(e_var("error"), "column", field("elephc_xml_libxml_error_int", 2)),
        s_prop_assign(e_var("error"), "message", field("elephc_xml_libxml_error_string", 0)),
        s_prop_assign(e_var("error"), "file", field("elephc_xml_libxml_error_string", 1)),
        s_prop_assign(e_var("error"), "line", field("elephc_xml_libxml_error_int", 3)),
    ]
}

/// `libxml_use_internal_errors` — transcribed from the PHP form.
pub(super) fn decl_fn_libxml_use_internal_errors() -> Stmt {
    let switch = |mode| {
        e_binop(
            e_call("elephc_xml_libxml_use_internal_errors", vec![mode]),
            BinOp::StrictNotEq,
            e_int(0),
        )
    };
    function("libxml_use_internal_errors")
        .param_default("use_errors", t_nullable(TypeExpr::Bool), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("use_errors"), BinOp::StrictEq, e_null()),
                vec![s_return(switch(e_neg(e_int(1))))],
                vec![],
                None,
            ),
            s_return(switch(e_ternary(e_var("use_errors"), e_int(1), e_int(0)))),
        ])
        .build()
}

/// `libxml_get_errors` — transcribed from the PHP form.
pub(super) fn decl_fn_libxml_get_errors() -> Stmt {
    function("libxml_get_errors")
        .returns(t_array())
        .body(vec![
            s_assign("errors", e_array(vec![])),
            s_assign("count", e_call("elephc_xml_libxml_error_count", vec![])),
            s_for(
                Some(s_assign("index", e_int(0))),
                Some(e_binop(e_var("index"), BinOp::Lt, e_var("count"))),
                Some(s_expr(e_post_inc("index"))),
                [
                    vec![s_expr(e_call("elephc_xml_libxml_error_select", vec![e_var("index")]))],
                    selected_error_into_var(),
                    vec![s_array_push("errors", e_var("error"))],
                ]
                .concat(),
            ),
            s_return(e_var("errors")),
        ])
        .build()
}

/// `libxml_get_last_error` — transcribed from the PHP form.
pub(super) fn decl_fn_libxml_get_last_error() -> Stmt {
    function("libxml_get_last_error")
        .returns(t_union(vec![t_class("LibXMLError"), TypeExpr::False]))
        .body(
            [
                vec![s_if(
                    e_binop(
                        e_call("elephc_xml_libxml_error_select", vec![e_neg(e_int(1))]),
                        BinOp::StrictEq,
                        e_int(0),
                    ),
                    vec![s_return(e_bool(false))],
                    vec![],
                    None,
                )],
                selected_error_into_var(),
                vec![s_return(e_var("error"))],
            ]
            .concat(),
        )
        .build()
}

/// `libxml_clear_errors` — transcribed from the PHP form.
pub(super) fn decl_fn_libxml_clear_errors() -> Stmt {
    function("libxml_clear_errors")
        .returns(TypeExpr::Void)
        .body(vec![s_expr(e_call("elephc_xml_libxml_clear_errors", vec![]))])
        .build()
}
