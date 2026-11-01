//! Purpose:
//! Compiles php's `highlight_string()` and `highlight_file()` natively: the calls are routed to
//! two prelude functions that render through the `elephc_highlight` library, so a program that
//! highlights source (Symfony's HTML error renderer does) links no interpreter.
//!
//! Called from:
//! - `crate::literal_format_split`, which rewrites the calls and prepends [`highlight_declarations`]
//!   from `crate::func_args::desugar`, ahead of the checker.
//!
//! Key details:
//! - The renderer is the crate the interpreter also uses, so both backends answer byte-identical
//!   HTML. The source travels as bytes plus an explicit length.
//! - `highlight_file()`'s failure path is php's: two `E_WARNING`s (the stream failure with its
//!   reason, then "Failed opening ... for highlighting") and `false`, raised through
//!   `__elephc_internal_warning` so `@`, the user error handler and `error_get_last()` see them.
//! - The reason is decided before the read, with php's two common spellings; the read itself
//!   runs only on a readable file and so raises nothing of its own.

use crate::parser::ast::{BinOp, CType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_concat_all, e_not, e_str, e_var, extern_fn, function,
    internal_declarations, s_assign, s_echo, s_expr, s_if, s_return, t_union,
};

/// The library the render entry binds to.
const HIGHLIGHT_BRIDGE: &str = "elephc_highlight";

/// The prelude function `highlight_string()` calls are routed to.
pub(crate) const HIGHLIGHT_STRING_HELPER: &str = "__elephc_highlight_string";
/// The prelude function `highlight_file()` calls are routed to.
pub(crate) const HIGHLIGHT_FILE_HELPER: &str = "__elephc_highlight_file";

/// The helper a call to php's `name` is routed to, when `name` is one of the two builtins.
pub(crate) fn helper_for(name: &str) -> Option<&'static str> {
    if name.eq_ignore_ascii_case("highlight_string") {
        Some(HIGHLIGHT_STRING_HELPER)
    } else if name.eq_ignore_ascii_case("highlight_file") {
        Some(HIGHLIGHT_FILE_HELPER)
    } else {
        None
    }
}

/// `elephc_highlight_render()` plus the two helpers, php's signatures:
/// `highlight_string(string $string, bool $return = false): string|bool` and
/// `highlight_file(string $filename, bool $return = false): string|bool`.
pub(crate) fn highlight_declarations() -> Program {
    internal_declarations(|| {
        vec![
            extern_fn("elephc_highlight_render", HIGHLIGHT_BRIDGE)
                .param("source", CType::Str)
                .param("length", CType::Int)
                .returns(CType::Str)
                .build(),
            highlight_string_helper(),
            highlight_file_helper(),
        ]
    })
}

/// `string|bool`, both helpers' return type.
fn string_or_bool() -> TypeExpr {
    t_union(vec![TypeExpr::Str, TypeExpr::Bool])
}

/// Renders `$string`, then answers the HTML (`$return`) or echoes it and answers `true`.
fn highlight_string_helper() -> Stmt {
    function(HIGHLIGHT_STRING_HELPER)
        .param("string", TypeExpr::Str)
        .param_default("return", TypeExpr::Bool, e_bool(false))
        .returns(string_or_bool())
        .body(vec![
            s_assign(
                "html",
                e_call(
                    "elephc_highlight_render",
                    vec![e_var("string"), e_call("strlen", vec![e_var("string")])],
                ),
            ),
            s_if(e_var("return"), vec![s_return(e_var("html"))], vec![], None),
            s_echo(e_var("html")),
            s_return(e_bool(true)),
        ])
        .build()
}

/// Reads `$filename` and highlights it, or warns twice and answers `false` when it cannot.
fn highlight_file_helper() -> Stmt {
    let warn = |parts: Vec<crate::parser::ast::Expr>| {
        s_expr(e_call("__elephc_internal_warning", vec![e_concat_all(parts)]))
    };
    function(HIGHLIGHT_FILE_HELPER)
        .param("filename", TypeExpr::Str)
        .param_default("return", TypeExpr::Bool, e_bool(false))
        .returns(string_or_bool())
        .body(vec![
            s_if(
                e_binop(
                    e_not(e_call("is_file", vec![e_var("filename")])),
                    BinOp::Or,
                    e_not(e_call("is_readable", vec![e_var("filename")])),
                ),
                vec![
                    s_assign("reason", e_str("No such file or directory")),
                    s_if(
                        e_call("file_exists", vec![e_var("filename")]),
                        vec![s_assign("reason", e_str("Permission denied"))],
                        vec![],
                        None,
                    ),
                    warn(vec![
                        e_str("Warning: highlight_file("),
                        e_var("filename"),
                        e_str("): Failed to open stream: "),
                        e_var("reason"),
                        e_str("\n"),
                    ]),
                    warn(vec![
                        e_str("Warning: highlight_file(): Failed opening '"),
                        e_var("filename"),
                        e_str("' for highlighting\n"),
                    ]),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call(
                HIGHLIGHT_STRING_HELPER,
                vec![
                    e_call("file_get_contents", vec![e_var("filename")]),
                    e_var("return"),
                ],
            )),
        ])
        .build()
}
