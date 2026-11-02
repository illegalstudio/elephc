//! Purpose:
//! Builds PHP's `strtok()` as a prelude declaration, for programs that name it.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program references `strtok` and declares no function of that name itself.
//!
//! Key details:
//! - The backend has no runtime symbol for it. The tokenizer state is two `static` locals (the
//!   string and the next byte offset), which is php's per-request global: it persists across calls
//!   and starts empty. Interpreted code has its own implementation and its own request-scoped
//!   state (`interpreter::builtins::string::strtok`); the two sides do not share a tokenizer.
//! - php-src semantics: leading delimiter bytes are skipped, the token runs to the next delimiter
//!   byte, that delimiter is consumed, `false` means nothing is left, and an empty delimiter list
//!   yields the rest of the string. The one-argument form (`$token === null`) treats `$string` as
//!   the delimiter list and continues the stored string. Byte-wise, so binary-safe.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_index, e_int, e_null, e_str, e_var, function,
    php_semantics_declarations, s_assign, s_if, s_return, s_static, s_while, t_nullable, t_union,
};

/// Returns the `strtok()` declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![decl_fn_strtok()])
}

/// `$pos < $len`.
fn in_bounds() -> Expr {
    e_binop(e_var("pos"), BinOp::Lt, e_var("len"))
}

/// `strpos($token, $str[$pos])`, compared to `false` with `op`.
fn delimiter_test(op: BinOp) -> Expr {
    e_binop(
        e_call("strpos", vec![e_var("token"), e_index(e_var("str"), e_var("pos"))]),
        op,
        e_bool(false),
    )
}

/// `$pos = $pos + 1;`.
fn advance() -> Stmt {
    s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(1)))
}

/// Builds the declaration.
fn decl_fn_strtok() -> Stmt {
    function("strtok")
        .param("string", TypeExpr::Str)
        .param_default("token", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_static("str", e_str("")),
            s_static("pos", e_int(0)),
            s_if(
                e_binop(e_var("token"), BinOp::StrictEq, e_null()),
                vec![s_assign("token", e_var("string"))],
                vec![],
                Some(vec![s_assign("str", e_var("string")), s_assign("pos", e_int(0))]),
            ),
            s_assign("len", e_call("strlen", vec![e_var("str")])),
            s_while(
                e_binop(in_bounds(), BinOp::And, delimiter_test(BinOp::StrictNotEq)),
                vec![advance()],
            ),
            s_if(
                e_binop(e_var("pos"), BinOp::GtEq, e_var("len")),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_assign("start", e_var("pos")),
            s_while(
                e_binop(in_bounds(), BinOp::And, delimiter_test(BinOp::StrictEq)),
                vec![advance()],
            ),
            s_assign(
                "out",
                e_call(
                    "substr",
                    vec![
                        e_var("str"),
                        e_var("start"),
                        e_binop(e_var("pos"), BinOp::Sub, e_var("start")),
                    ],
                ),
            ),
            s_if(in_bounds(), vec![advance()], vec![], None),
            s_return(e_var("out")),
        ])
        .build()
}
