//! Purpose:
//! Builds the long forms of `fputcsv()` and `fgetcsv()` -- the ones that name the `$escape` (and,
//! for `fputcsv()`, `$eol`; for `fgetcsv()`, `$enclosure`) parameters -- as prelude declarations.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declarations when the
//!   program calls either builtin with the longer argument list.
//! - `crate::ir_lower::expr::compat_preludes`, which routes such a call to them.
//!
//! Key details:
//! - The native runtimes write and read the default dialect only (`\` escape, `"` enclosure, `\n`
//!   terminator). php 8.1+ takes `$escape` and `$eol` in `fputcsv()` and an enclosure and escape in
//!   `fgetcsv()`, and `$escape = ''` (no escape character) is the form new code is told to use.
//! - `fputcsv()` follows php-src's `php_fputcsv`: a field is enclosed when it holds the
//!   separator, the enclosure, the escape character (when one is set), a newline, a carriage
//!   return, a tab or a space; inside an enclosed field the enclosure is doubled unless the
//!   character before it was the escape character. The answer is `fwrite()`'s byte count.
//! - `fgetcsv()` reads a physical line and keeps reading while a quoted field is still open
//!   (a field may span lines), drops one trailing line ending, and parses the record with the
//!   `str_getcsv()` helper. A blank line is `[null]`, end of input is `false`.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_cast, e_concat, e_index, e_int, e_not, e_null, e_str, e_ternary,
    e_var, function, php_semantics_declarations, s_assign, s_foreach, s_if, s_return, s_while,
};

/// Reserved helper a five- or six-argument `fputcsv()` call is routed to.
pub(crate) const FPUTCSV_FULL_NAME: &str = "__elephc_fputcsv_full";

/// Reserved helper a four- or five-argument `fgetcsv()` call is routed to.
pub(crate) const FGETCSV_FULL_NAME: &str = "__elephc_fgetcsv_full";

/// Reserved helper telling `fgetcsv()` whether a quoted field is still open at the end of a line.
pub(crate) const CSV_OPEN_QUOTE_NAME: &str = "__elephc_csv_open_quote";

/// Returns the helper declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![decl_fn_open_quote(), decl_fn_fputcsv_full(), decl_fn_fgetcsv_full()]
    })
}

/// `$haystack` contains `$needle` (a non-empty needle).
fn contains(haystack: &str, needle: crate::parser::ast::Expr) -> crate::parser::ast::Expr {
    e_binop(
        e_call("strpos", vec![e_var(haystack), needle]),
        BinOp::StrictNotEq,
        e_bool(false),
    )
}

/// Builds `__elephc_fputcsv_full($stream, $fields, $separator, $enclosure, $escape, $eol)`.
fn decl_fn_fputcsv_full() -> Stmt {
    let needs_enclosure = e_binop(
        e_binop(
            e_binop(
                contains("text", e_var("separator")),
                BinOp::Or,
                contains("text", e_var("enclosure")),
            ),
            BinOp::Or,
            e_binop(
                e_binop(e_var("escape"), BinOp::StrictNotEq, e_str("")),
                BinOp::And,
                contains("text", e_var("escape")),
            ),
        ),
        BinOp::Or,
        e_binop(
            e_binop(
                contains("text", e_str("\n")),
                BinOp::Or,
                contains("text", e_str("\r")),
            ),
            BinOp::Or,
            e_binop(contains("text", e_str("\t")), BinOp::Or, contains("text", e_str(" "))),
        ),
    );
    let enclosed_field = vec![
        s_assign("quoted", e_var("enclosure")),
        s_assign("escaped", e_bool(false)),
        s_assign("index", e_int(0)),
        s_assign("length", e_call("strlen", vec![e_var("text")])),
        s_while(
            e_binop(e_var("index"), BinOp::Lt, e_var("length")),
            vec![
                s_assign("character", e_index(e_var("text"), e_var("index"))),
                s_if(
                    e_binop(
                        e_binop(e_var("escape"), BinOp::StrictNotEq, e_str("")),
                        BinOp::And,
                        e_binop(e_var("character"), BinOp::StrictEq, e_var("escape")),
                    ),
                    vec![s_assign("escaped", e_bool(true))],
                    vec![],
                    Some(vec![s_if(
                        e_binop(
                            e_not(e_var("escaped")),
                            BinOp::And,
                            e_binop(e_var("character"), BinOp::StrictEq, e_var("enclosure")),
                        ),
                        vec![s_assign("quoted", e_concat(e_var("quoted"), e_var("enclosure")))],
                        vec![],
                        Some(vec![s_assign("escaped", e_bool(false))]),
                    )]),
                ),
                s_assign("quoted", e_concat(e_var("quoted"), e_var("character"))),
                s_assign("index", e_binop(e_var("index"), BinOp::Add, e_int(1))),
            ],
        ),
        s_assign("quoted", e_concat(e_var("quoted"), e_var("enclosure"))),
        s_assign("line", e_concat(e_var("line"), e_var("quoted"))),
    ];
    function(FPUTCSV_FULL_NAME)
        .param_untyped("stream")
        .param("fields", crate::synthetic_class::t_array())
        .param_default("separator", TypeExpr::Str, e_str(","))
        .param_default("enclosure", TypeExpr::Str, e_str("\""))
        .param_default("escape", TypeExpr::Str, e_str("\\"))
        .param_default("eol", TypeExpr::Str, e_str("\n"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("line", e_str("")),
            s_assign("first", e_bool(true)),
            s_foreach(
                e_var("fields"),
                None,
                "field",
                vec![
                    s_if(
                        e_not(e_var("first")),
                        vec![s_assign("line", e_concat(e_var("line"), e_var("separator")))],
                        vec![],
                        None,
                    ),
                    s_assign("first", e_bool(false)),
                    s_assign("text", e_cast(CastType::String, e_var("field"))),
                    s_if(
                        needs_enclosure,
                        enclosed_field,
                        vec![],
                        Some(vec![s_assign("line", e_concat(e_var("line"), e_var("text")))]),
                    ),
                ],
            ),
            s_assign("line", e_concat(e_var("line"), e_var("eol"))),
            s_assign("written", e_call("fwrite", vec![e_var("stream"), e_var("line")])),
            s_return(e_ternary(
                e_binop(e_var("written"), BinOp::StrictEq, e_bool(false)),
                e_int(0),
                e_var("written"),
            )),
        ])
        .build()
}

/// Builds `__elephc_csv_open_quote(string $line, string $separator, string $enclosure,
/// string $escape): bool`, the quote state `str_getcsv()` would be in at the end of `$line`.
fn decl_fn_open_quote() -> Stmt {
    function(CSV_OPEN_QUOTE_NAME)
        .param("line", TypeExpr::Str)
        .param("separator", TypeExpr::Str)
        .param("enclosure", TypeExpr::Str)
        .param("escape", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("quoted", e_bool(false)),
            s_assign("start", e_bool(true)),
            s_assign("index", e_int(0)),
            s_assign("length", e_call("strlen", vec![e_var("line")])),
            s_while(
                e_binop(e_var("index"), BinOp::Lt, e_var("length")),
                vec![
                    s_assign("character", e_index(e_var("line"), e_var("index"))),
                    s_if(
                        e_var("quoted"),
                        vec![s_if(
                            e_binop(
                                e_binop(
                                    e_binop(e_var("escape"), BinOp::StrictNotEq, e_str("")),
                                    BinOp::And,
                                    e_binop(e_var("character"), BinOp::StrictEq, e_var("escape")),
                                ),
                                BinOp::And,
                                e_binop(
                                    e_binop(e_var("index"), BinOp::Add, e_int(1)),
                                    BinOp::Lt,
                                    e_var("length"),
                                ),
                            ),
                            vec![s_assign("index", e_binop(e_var("index"), BinOp::Add, e_int(1)))],
                            vec![],
                            Some(vec![s_if(
                                e_binop(e_var("character"), BinOp::StrictEq, e_var("enclosure")),
                                vec![s_if(
                                    e_binop(
                                        e_binop(
                                            e_binop(e_var("index"), BinOp::Add, e_int(1)),
                                            BinOp::Lt,
                                            e_var("length"),
                                        ),
                                        BinOp::And,
                                        e_binop(
                                            e_index(
                                                e_var("line"),
                                                e_binop(e_var("index"), BinOp::Add, e_int(1)),
                                            ),
                                            BinOp::StrictEq,
                                            e_var("enclosure"),
                                        ),
                                    ),
                                    vec![s_assign(
                                        "index",
                                        e_binop(e_var("index"), BinOp::Add, e_int(1)),
                                    )],
                                    vec![],
                                    Some(vec![s_assign("quoted", e_bool(false))]),
                                )],
                                vec![],
                                None,
                            )]),
                        )],
                        vec![],
                        Some(vec![s_if(
                            e_binop(e_var("character"), BinOp::StrictEq, e_var("separator")),
                            vec![s_assign("start", e_bool(true))],
                            vec![],
                            Some(vec![s_if(
                                e_binop(
                                    e_binop(e_var("character"), BinOp::StrictEq, e_var("enclosure")),
                                    BinOp::And,
                                    e_var("start"),
                                ),
                                vec![
                                    s_assign("quoted", e_bool(true)),
                                    s_assign("start", e_bool(false)),
                                ],
                                vec![],
                                Some(vec![s_assign("start", e_bool(false))]),
                            )]),
                        )]),
                    ),
                    s_assign("index", e_binop(e_var("index"), BinOp::Add, e_int(1))),
                ],
            ),
            s_return(e_var("quoted")),
        ])
        .build()
}

/// Builds `__elephc_fgetcsv_full($stream, $length, $separator, $enclosure, $escape)`.
fn decl_fn_fgetcsv_full() -> Stmt {
    let ends_with = |suffix: &str| {
        e_binop(
            e_call("substr", vec![e_var("line"), e_int(-(suffix.len() as i64))]),
            BinOp::StrictEq,
            e_str(suffix),
        )
    };
    function(FGETCSV_FULL_NAME)
        .param_untyped("stream")
        .param_untyped_default("length", e_null())
        .param_default("separator", TypeExpr::Str, e_str(","))
        .param_default("enclosure", TypeExpr::Str, e_str("\""))
        .param_default("escape", TypeExpr::Str, e_str("\\"))
        .body(vec![
            s_assign("line", e_call("fgets", vec![e_var("stream")])),
            s_if(
                e_binop(e_var("line"), BinOp::StrictEq, e_bool(false)),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_assign(
                "open",
                e_call(
                    CSV_OPEN_QUOTE_NAME,
                    vec![
                        e_var("line"),
                        e_var("separator"),
                        e_var("enclosure"),
                        e_var("escape"),
                    ],
                ),
            ),
            s_while(
                e_var("open"),
                vec![
                    s_assign("more", e_call("fgets", vec![e_var("stream")])),
                    s_if(
                        e_binop(e_var("more"), BinOp::StrictEq, e_bool(false)),
                        vec![s_assign("open", e_bool(false))],
                        vec![],
                        Some(vec![
                            s_assign("line", e_concat(e_var("line"), e_var("more"))),
                            s_assign(
                                "open",
                                e_call(
                                    CSV_OPEN_QUOTE_NAME,
                                    vec![
                                        e_var("line"),
                                        e_var("separator"),
                                        e_var("enclosure"),
                                        e_var("escape"),
                                    ],
                                ),
                            ),
                        ]),
                    ),
                ],
            ),
            s_if(
                ends_with("\r\n"),
                vec![s_assign("line", e_call("substr", vec![e_var("line"), e_int(0), e_int(-2)]))],
                vec![(
                    e_binop(ends_with("\n"), BinOp::Or, ends_with("\r")),
                    vec![s_assign("line", e_call("substr", vec![e_var("line"), e_int(0), e_int(-1)]))],
                )],
                None,
            ),
            s_return(e_call(
                "__elephc_str_getcsv",
                vec![
                    e_var("line"),
                    e_var("separator"),
                    e_var("enclosure"),
                    e_var("escape"),
                ],
            )),
        ])
        .build()
}
