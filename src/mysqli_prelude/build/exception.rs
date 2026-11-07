//! Purpose:
//! Builds `mysqli_sql_exception` and the `mysqli_report()` flag store as AST. A
//! mysqli-only program must never see (or throw) `PDOException`; this exception
//! is the only one the surface raises.
//!
//! Called from:
//! - `crate::mysqli_prelude::build::mysqli_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from `mysqli_prelude::exception::SRC` (`synthetic_class::transcribe`);
//!   the oracle `built_declarations_match_the_php_for_every_version` compares each
//!   built node against that PHP.
//! - `final class mysqli_sql_exception extends RuntimeException` with php-src's
//!   protected `$sqlstate`; the private `__elephcCreate` factory sets it.

use crate::parser::ast::{TypeExpr, Stmt};
use crate::synthetic_class::{
    class,
    e_bool,
    e_int,
    e_new,
    e_null,
    e_str,
    e_this,
    e_this_prop,
    e_var,
    function,
    method,
    s_assign,
    s_prop_assign,
    s_return,
    s_static_prop_assign,
    t_class,
    t_nullable,
};

/// `mysqli_sql_exception` — transcribed from the PHP form.
pub(super) fn decl_class_mysqli_sql_exception() -> Stmt {
    class("mysqli_sql_exception")
        .final_()
        .extends("RuntimeException")
        .protected_prop("sqlstate", TypeExpr::Str, Some(e_str("00000")))
        .prop("previous", t_nullable(t_class("Throwable")), Some(e_null()))
        .method(
            method("__construct")
                .param_default("message", TypeExpr::Str, e_str(""))
                .param_default("code", TypeExpr::Int, e_int(0))
                .param_default("previous", t_nullable(t_class("Throwable")), e_null())
                .body(vec![
                    s_prop_assign(e_this(), "message", e_var("message")),
                    s_prop_assign(e_this(), "code", e_var("code")),
                    s_prop_assign(e_this(), "previous", e_var("previous")),
                ]),
        )
        .method(
            method("__elephcCreate")
                .private()
                .static_()
                .param("message", TypeExpr::Str)
                .param("code", TypeExpr::Int)
                .param("sqlstate", TypeExpr::Str)
                .returns(t_class("mysqli_sql_exception"))
                .body(vec![
                    s_assign("_e", e_new("mysqli_sql_exception", vec![e_var("message"), e_var("code")])),
                    s_prop_assign(e_var("_e"), "sqlstate", e_var("sqlstate")),
                    s_return(e_var("_e")),
                ]),
        )
        .method(
            method("getSqlState")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_this_prop("sqlstate")),
                ]),
        )
        .build()
}

/// `mysqli_report` — transcribed from the PHP form.
pub(super) fn decl_fn_mysqli_report() -> Stmt {
    function("mysqli_report")
        .param("flags", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_static_prop_assign("mysqli", "reportMode", e_var("flags")),
            s_return(e_bool(true)),
        ])
        .build()
}
