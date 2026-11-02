//! Purpose:
//! Declares the helpers the long forms of a few stream and filesystem builtins are lowered to: the
//! native runtimes implement only the short form.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, when `crate::ast_usage` recorded a long-form
//!   call; `crate::ir_lower::expr::compat_preludes` routes the call to the helper.
//!
//! Key details:
//! - `fgets($stream, $length)` reads at most `$length - 1` bytes, stopping after a newline, and
//!   answers `false` when it read nothing (php's `fgets($h, 1)`); a non-positive length is php's
//!   `ValueError`. psysh's readline and many line readers pass a length.
//! - `fwrite($stream, $data, $length)` writes the first `$length` bytes (none for a negative one).
//! - The `$context` of `rmdir()`, `unlink()`, `rename()`, `opendir()` and `file()` is evaluated and
//!   not consulted: the native calls take the default context, which is php's answer for a context
//!   without wrapper options (psysh's Hoa `File*` classes pass their stream context through).
//! - The declarations are BUILT, never parsed.

use crate::parser::ast::{BinOp, Program, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_concat, e_int, e_new_fq, e_null, e_str, e_var, function,
    php_semantics_declarations, s_assign, s_if, s_return, s_throw, s_while, t_mixed, t_nullable,
};

/// `fgets($stream, $length)`.
pub(crate) const FGETS_LENGTH_NAME: &str = "__elephc_fgets_length";
/// `fwrite($stream, $data, $length)` (and its `fputs` alias).
pub(crate) const FWRITE_LENGTH_NAME: &str = "__elephc_fwrite_length";
/// `rmdir($directory, $context)`.
pub(crate) const RMDIR_CONTEXT_NAME: &str = "__elephc_rmdir_context";
/// `unlink($filename, $context)`.
pub(crate) const UNLINK_CONTEXT_NAME: &str = "__elephc_unlink_context";
/// `rename($from, $to, $context)`.
pub(crate) const RENAME_CONTEXT_NAME: &str = "__elephc_rename_context";
/// `opendir($directory, $context)`.
pub(crate) const OPENDIR_CONTEXT_NAME: &str = "__elephc_opendir_context";
/// `file($filename, $flags, $context)`.
pub(crate) const FILE_CONTEXT_NAME: &str = "__elephc_file_context";

/// Every helper, for the usage scan.
pub(crate) const HELPERS: &[&str] = &[
    FGETS_LENGTH_NAME,
    FWRITE_LENGTH_NAME,
    RMDIR_CONTEXT_NAME,
    UNLINK_CONTEXT_NAME,
    RENAME_CONTEXT_NAME,
    OPENDIR_CONTEXT_NAME,
    FILE_CONTEXT_NAME,
];

/// The helper a long-form call of `builtin` with `argc` arguments is lowered to, if any.
pub(crate) fn helper_for(builtin: &str, argc: usize) -> Option<&'static str> {
    Some(match (builtin, argc) {
        ("fgets", 2) => FGETS_LENGTH_NAME,
        ("fwrite" | "fputs", 3) => FWRITE_LENGTH_NAME,
        ("rmdir", 2) => RMDIR_CONTEXT_NAME,
        ("unlink", 2) => UNLINK_CONTEXT_NAME,
        ("rename", 3) => RENAME_CONTEXT_NAME,
        ("opendir", 2) => OPENDIR_CONTEXT_NAME,
        ("file", 3) => FILE_CONTEXT_NAME,
        _ => return None,
    })
}

/// `__elephc_fgets_length(mixed $stream, ?int $length): mixed`.
fn fgets_length() -> crate::parser::ast::Stmt {
    function(FGETS_LENGTH_NAME)
        .param("stream", t_mixed())
        .param("length", t_nullable(TypeExpr::Int))
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_var("length"), BinOp::StrictEq, e_null()),
                vec![s_return(e_call("fgets", vec![e_var("stream")]))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("length"), BinOp::LtEq, e_int(0)),
                vec![s_throw(e_new_fq(
                    "ValueError",
                    vec![e_str("fgets(): Argument #2 ($length) must be greater than 0")],
                ))],
                vec![],
                None,
            ),
            s_assign("line", e_str("")),
            s_while(
                e_binop(
                    e_call("strlen", vec![e_var("line")]),
                    BinOp::Lt,
                    e_binop(e_var("length"), BinOp::Sub, e_int(1)),
                ),
                vec![
                    s_assign("byte", e_call("fgetc", vec![e_var("stream")])),
                    s_if(
                        e_binop(e_var("byte"), BinOp::StrictEq, e_bool(false)),
                        vec![crate::synthetic_class::s_break(1)],
                        vec![],
                        None,
                    ),
                    s_assign("line", e_concat(e_var("line"), e_var("byte"))),
                    s_if(
                        e_binop(e_var("byte"), BinOp::StrictEq, e_str("\n")),
                        vec![crate::synthetic_class::s_break(1)],
                        vec![],
                        None,
                    ),
                ],
            ),
            s_if(
                e_binop(e_var("line"), BinOp::StrictEq, e_str("")),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_return(e_var("line")),
        ])
        .build()
}

/// `__elephc_fwrite_length(mixed $stream, string $data, ?int $length): mixed`.
fn fwrite_length() -> crate::parser::ast::Stmt {
    function(FWRITE_LENGTH_NAME)
        .param("stream", t_mixed())
        .param("data", TypeExpr::Str)
        .param("length", t_nullable(TypeExpr::Int))
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_var("length"), BinOp::StrictNotEq, e_null()),
                vec![s_assign(
                    "data",
                    e_call(
                        "substr",
                        vec![e_var("data"), e_int(0), e_call("max", vec![e_int(0), e_var("length")])],
                    ),
                )],
                vec![],
                None,
            ),
            s_return(e_call("fwrite", vec![e_var("stream"), e_var("data")])),
        ])
        .build()
}

/// A helper that drops its trailing `$context` and calls `builtin` with the leading parameters.
fn without_context(name: &str, builtin: &str, params: &[(&str, TypeExpr)]) -> crate::parser::ast::Stmt {
    let mut builder = function(name);
    for (param, ty) in params {
        builder = builder.param(param, ty.clone());
    }
    builder
        .param("context", t_mixed())
        .returns(t_mixed())
        .body(vec![s_return(e_call(
            builtin,
            params.iter().map(|(param, _)| e_var(param)).collect(),
        ))])
        .build()
}

/// Returns every helper declaration.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![
            fgets_length(),
            fwrite_length(),
            without_context(RMDIR_CONTEXT_NAME, "rmdir", &[("directory", TypeExpr::Str)]),
            without_context(UNLINK_CONTEXT_NAME, "unlink", &[("filename", TypeExpr::Str)]),
            without_context(
                RENAME_CONTEXT_NAME,
                "rename",
                &[("from", TypeExpr::Str), ("to", TypeExpr::Str)],
            ),
            without_context(OPENDIR_CONTEXT_NAME, "opendir", &[("directory", TypeExpr::Str)]),
            without_context(
                FILE_CONTEXT_NAME,
                "file",
                &[("filename", TypeExpr::Str), ("flags", TypeExpr::Int)],
            ),
        ]
    })
}
