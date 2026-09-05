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
//! - `mysqli_sql_exception extends RuntimeException` with a public `$sqlstate`,
//!   matching php-src's shape (there `$sqlstate` is protected with a getter; the
//!   public property is the documented elephc divergence).
//! - The process-wide report mode lives on `mysqli::$reportMode` (see
//!   `connection.rs`); its default is version-gated there (`ERROR|STRICT` = 3 for
//!   PHP >= 8.1, `OFF` = 0 for 8.0).

use crate::parser::ast::{TypeExpr, Stmt};
use crate::synthetic_class::{
    class,
    e_bool,
    e_int,
    e_null,
    e_str,
    e_this,
    e_this_prop,
    e_var,
    function,
    method,
    s_prop_assign,
    s_return,
    s_static_prop_assign,
    t_class,
    t_nullable,
};

/// `mysqli_sql_exception` — transcribed from the PHP form.
pub(super) fn decl_class_mysqli_sql_exception() -> Stmt {
    // php-src: class mysqli_sql_exception extends RuntimeException. The SQLSTATE is exposed as a
    // public property (php-src keeps it protected behind getSqlState(); documented divergence,
    // same bucket as writable mysqli properties).
    class("mysqli_sql_exception")
        .extends("RuntimeException")
        .prop("sqlstate", TypeExpr::Str, Some(e_str("00000")))
        // The previous exception in the chain. Same storage note as PDOException: the
        // compiler-owned base Throwable layout has no previous slot, so the class keeps its own
        // and dispatches getPrevious() below.
        .prop("previous", t_nullable(t_class("Throwable")), Some(e_null()))
        .method(
            method("__construct")
                .param_default("message", TypeExpr::Str, e_str(""))
                .param_default("code", TypeExpr::Int, e_int(0))
                .param_default("previous", t_nullable(t_class("Throwable")), e_null())
                .body(vec![
                    // The built-in Exception constructor is a checker-synthesized method with no
                    // linkable symbol, so `parent::__construct()` cannot be called; the public
                    // $message property is assigned directly (same pattern as PDOException in the
                    // PDO prelude).
                    s_prop_assign(e_this(), "message", e_var("message")),
                    s_prop_assign(e_this(), "code", e_var("code")),
                    s_prop_assign(e_this(), "previous", e_var("previous")),
                ]),
        )
        .method(
            method("getPrevious")
                .returns(t_nullable(t_class("Throwable")))
                .body(vec![
                    s_return(e_this_prop("previous")),
                ]),
        )
        // php 8.1+: the documented accessor for the SQLSTATE (the property itself is protected in
        // php-src; elephc exposes both — see the class note).
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
    // Stores the process-wide report mode consumed by every mysqli failure path. PHP 8.1+ default:
    // MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT; PHP 8.0: OFF (the default itself is baked into
    // mysqli::$reportMode per version, see connection.rs).
    function("mysqli_report")
        .param("flags", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_static_prop_assign("mysqli", "reportMode", e_var("flags")),
            s_return(e_bool(true)),
        ])
        .build()
}
