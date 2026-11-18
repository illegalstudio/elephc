//! Purpose:
//! Package P2 of the DOM migration: libxml_* functions, LibXMLError and the shared error state (Part::Libxml).
//!
//! Called from:
//! - `super::extend::PACKAGES`, through `contribute`.
//!
//! Key details:
//! - The state lives in the static properties of `__ElephcDomState`: the internal-errors flag,
//!   the entity-loader flag, the buffered error list and the last error. libxml keeps the last
//!   error apart from the list, so `libxml_use_internal_errors(false)` (which empties the list)
//!   and `libxml_clear_errors()` (which empties both) differ exactly as in php 8.5.
//! - `__elephc_libxml_record` is the single entry point a parser reports a structured error
//!   through; nothing in the native bridge reports one yet, so the list stays empty until it
//!   does.
//! - This file registers classes and functions only through the `Contributions` API documented
//!   at the top of `extend.rs`.

use crate::parser::ast::*;
use crate::synthetic_class::*;

use super::extend::{Contributions, Part};

fn state_class() -> Stmt {
    class("__ElephcDomState")
        .final_()
        .static_prop("internalErrors", TypeExpr::Bool, Some(e_bool(false)))
        .static_prop("entityLoaderDisabled", TypeExpr::Bool, Some(e_bool(false)))
        .static_prop("errors", t_array(), Some(e_array(vec![])))
        .static_prop("lastError", t_nullable(t_class("LibXMLError")), Some(e_null()))
        .build()
}

fn error_class() -> Stmt {
    class("LibXMLError")
        .prop("level", TypeExpr::Int, None)
        .prop("code", TypeExpr::Int, None)
        .prop("column", TypeExpr::Int, None)
        .prop("message", TypeExpr::Str, None)
        .prop("file", TypeExpr::Str, None)
        .prop("line", TypeExpr::Int, None)
        .build()
}

/// `libxml_use_internal_errors(?bool $use_errors = null): bool` — switching it off drops the buffered list.
fn use_internal_errors() -> Stmt {
    function("libxml_use_internal_errors")
        .param_default("use_errors", t_nullable(TypeExpr::Bool), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("previous", e_static_prop("__ElephcDomState", "internalErrors")),
            s_if(
                e_binop(e_var("use_errors"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_static_prop_assign("__ElephcDomState", "internalErrors", e_var("use_errors")),
                    s_if(
                        e_binop(e_var("use_errors"), BinOp::StrictEq, e_bool(false)),
                        vec![s_static_prop_assign("__ElephcDomState", "errors", e_array(vec![]))],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_return(e_var("previous")),
        ])
        .build()
}

fn clear_errors() -> Stmt {
    function("libxml_clear_errors")
        .returns(TypeExpr::Void)
        .body(vec![
            s_static_prop_assign("__ElephcDomState", "errors", e_array(vec![])),
            s_static_prop_assign("__ElephcDomState", "lastError", e_null()),
        ])
        .build()
}

fn get_errors() -> Stmt {
    function("libxml_get_errors")
        .returns(t_array())
        .body(vec![s_return(e_static_prop("__ElephcDomState", "errors"))])
        .build()
}

fn get_last_error() -> Stmt {
    function("libxml_get_last_error")
        .returns(t_union(vec![t_class("LibXMLError"), TypeExpr::False]))
        .body(vec![
            s_assign("last", e_static_prop("__ElephcDomState", "lastError")),
            s_if(
                e_instance_of(e_var("last"), "LibXMLError"),
                vec![s_return(e_var("last"))],
                vec![],
                None,
            ),
            s_return(e_bool(false)),
        ])
        .build()
}

fn disable_entity_loader() -> Stmt {
    function("libxml_disable_entity_loader")
        .param_default("disable", TypeExpr::Bool, e_bool(true))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("previous", e_static_prop("__ElephcDomState", "entityLoaderDisabled")),
            s_static_prop_assign("__ElephcDomState", "entityLoaderDisabled", e_var("disable")),
            s_return(e_var("previous")),
        ])
        .build()
}

/// Records one structured libxml error: always as the last error, in the list only while internal errors are on.
fn record() -> Stmt {
    function("__elephc_libxml_record")
        .param("level", TypeExpr::Int)
        .param("code", TypeExpr::Int)
        .param("column", TypeExpr::Int)
        .param("message", TypeExpr::Str)
        .param("file", TypeExpr::Str)
        .param("line", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("error", e_new("LibXMLError", vec![])),
            s_prop_assign(e_var("error"), "level", e_var("level")),
            s_prop_assign(e_var("error"), "code", e_var("code")),
            s_prop_assign(e_var("error"), "column", e_var("column")),
            s_prop_assign(e_var("error"), "message", e_var("message")),
            s_prop_assign(e_var("error"), "file", e_var("file")),
            s_prop_assign(e_var("error"), "line", e_var("line")),
            s_static_prop_assign("__ElephcDomState", "lastError", e_var("error")),
            s_if(
                e_static_prop("__ElephcDomState", "internalErrors"),
                vec![s_static_prop_array_push("__ElephcDomState", "errors", e_var("error"))],
                vec![],
                None,
            ),
        ])
        .build()
}

/// Registers the libxml state, `LibXMLError` and the five `libxml_*` functions.
pub(super) fn contribute(c: &mut Contributions) {
    c.add_class(Part::Libxml, state_class());
    c.add_class(Part::Libxml, error_class());
    for function in [
        use_internal_errors(), clear_errors(), get_errors(), get_last_error(),
        disable_entity_loader(), record(),
    ] {
        c.add_function(Part::Libxml, function);
    }
}
