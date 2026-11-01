//! Purpose:
//! Builds PHP's `addcslashes()` as a prelude declaration, for programs that name it.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program references `addcslashes`.
//!
//! Key details:
//! - The backend has no runtime symbol for it; the interpreter has its own home
//!   (`interpreter::builtins::string::addcslashes`), and both transcribe php-src's
//!   `php_addcslashes_str` over `php_charmask`: `a..z` ranges in the character list, the seven
//!   control bytes 7..13 as `\a\b\t\n\v\f\r`, other bytes outside 32..126 as three-digit
//!   octal, and everything else as a backslash plus the byte itself.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_index, e_int, e_str, e_var, function, php_semantics_declarations,
    s_assign, s_array_assign, s_if, s_return, s_while,
};

/// Returns the `addcslashes()` declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![decl_fn_addcslashes()])
}

/// `ord($characters[$index])`.
fn ord_at(string: &str, index: Expr) -> Expr {
    e_call("ord", vec![e_index(e_var(string), index)])
}

/// `$name + offset`.
fn plus(name: &str, offset: i64) -> Expr {
    e_binop(e_var(name), BinOp::Add, e_int(offset))
}

/// `$target .= value` spelled as an assignment.
fn append(target: &str, value: Expr) -> Stmt {
    s_assign(target, e_binop(e_var(target), BinOp::Concat, value))
}

/// Builds the declaration.
fn decl_fn_addcslashes() -> Stmt {
    let range = e_binop(
        e_binop(
            e_binop(plus("i", 3), BinOp::Lt, e_var("n")),
            BinOp::And,
            e_binop(e_index(e_var("characters"), plus("i", 1)), BinOp::StrictEq, e_str(".")),
        ),
        BinOp::And,
        e_binop(
            e_binop(e_index(e_var("characters"), plus("i", 2)), BinOp::StrictEq, e_str(".")),
            BinOp::And,
            e_binop(ord_at("characters", plus("i", 3)), BinOp::GtEq, e_var("c")),
        ),
    );
    let build_mask = vec![
        s_assign("mask", crate::synthetic_class::e_array(vec![])),
        s_assign("n", e_call("strlen", vec![e_var("characters")])),
        s_assign("i", e_int(0)),
        s_while(
            e_binop(e_var("i"), BinOp::Lt, e_var("n")),
            vec![
                s_assign("c", ord_at("characters", e_var("i"))),
                s_if(
                    range,
                    vec![
                        s_assign("last", ord_at("characters", plus("i", 3))),
                        s_while(
                            e_binop(e_var("c"), BinOp::LtEq, e_var("last")),
                            vec![
                                s_array_assign("mask", e_var("c"), e_bool(true)),
                                s_assign("c", plus("c", 1)),
                            ],
                        ),
                        s_assign("i", plus("i", 4)),
                    ],
                    vec![],
                    Some(vec![
                        s_array_assign("mask", e_var("c"), e_bool(true)),
                        s_assign("i", plus("i", 1)),
                    ]),
                ),
            ],
        ),
    ];
    let escape = s_if(
        e_binop(
            e_binop(e_var("o"), BinOp::GtEq, e_int(7)),
            BinOp::And,
            e_binop(e_var("o"), BinOp::LtEq, e_int(13)),
        ),
        vec![append(
            "out",
            e_binop(e_str("\\"), BinOp::Concat, e_index(e_str("abtnvfr"), e_binop(e_var("o"), BinOp::Sub, e_int(7)))),
        )],
        vec![(
            e_binop(
                e_binop(e_var("o"), BinOp::Lt, e_int(32)),
                BinOp::Or,
                e_binop(e_var("o"), BinOp::Gt, e_int(126)),
            ),
            vec![append(
                "out",
                e_binop(e_str("\\"), BinOp::Concat, e_call("sprintf", vec![e_str("%03o"), e_var("o")])),
            )],
        )],
        Some(vec![append("out", e_binop(e_str("\\"), BinOp::Concat, e_var("ch")))]),
    );
    let mut body = build_mask;
    body.extend(vec![
        s_assign("out", e_str("")),
        s_assign("len", e_call("strlen", vec![e_var("string")])),
        s_assign("j", e_int(0)),
        s_while(
            e_binop(e_var("j"), BinOp::Lt, e_var("len")),
            vec![
                s_assign("ch", e_index(e_var("string"), e_var("j"))),
                s_assign("o", e_call("ord", vec![e_var("ch")])),
                s_if(
                    e_call("isset", vec![e_index(e_var("mask"), e_var("o"))]),
                    vec![escape],
                    vec![],
                    Some(vec![append("out", e_var("ch"))]),
                ),
                s_assign("j", plus("j", 1)),
            ],
        ),
        s_return(e_var("out")),
    ]);
    function("addcslashes")
        .param("string", TypeExpr::Str)
        .param("characters", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(body)
        .build()
}
