//! Purpose:
//! Builds `__elephc_string_bitnot($s)`, php's `~` on a string: every byte complemented.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which appends it whenever the program spells a
//!   `~` at all; reachability drops it again when no string operand reached the lowering.
//! - `crate::ir_lower::expr` (the `BitNot` arm), which routes a `string`-typed operand to
//!   [`STRING_BITNOT_NAME`] instead of the integer `IBitNot`.
//!
//! Key details:
//! - brick/math's `BigInteger::fromBytes()` does `$bytes = ~$bytes;` for a negative two's
//!   complement input; the checker refused the whole file.
//! - `chr(255 - ord($c))` is `~$c` for one byte; checked against php's own `~` over all 256 bytes
//!   and random input before transcription.

use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Program, Stmt, TypeExpr};
use crate::span::Span;
use crate::synthetic_class::{
    e_binop, e_call, e_cast, e_index, e_int, e_new, e_post_inc, e_str, e_ternary, e_var, function,
    internal_declarations, s_assign, s_expr, s_for, s_if, s_return, s_throw, t_mixed,
};

/// The helper a `~` over a `string` operand is lowered to.
pub(crate) const STRING_BITNOT_NAME: &str = "__elephc_string_bitnot";

/// `__elephc_string_bitnot` — transcribed from the PHP form.
fn decl_fn_elephc_string_bitnot() -> Stmt {
    function("__elephc_string_bitnot")
        .param("s", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("out", e_str("")),
            s_assign("n", e_call("strlen", vec![e_var("s")])),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("n"))), Some(s_expr(e_post_inc("i"))), vec![
                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("chr", vec![e_binop(e_int(255), BinOp::Sub, e_call("ord", vec![e_index(e_var("s"), e_var("i"))]))]))),
            ]),
            s_return(e_var("out")),
        ])
        .build()
}

/// The helper a `~` over a `mixed` or union operand is lowered to.
pub(crate) const MIXED_BITNOT_NAME: &str = "__elephc_mixed_bitnot";

/// `~$v` of an integer-typed value.
fn int_bitnot(value: Expr) -> Expr {
    Expr::new(ExprKind::BitNot(Box::new(e_cast(CastType::Int, value))), Span::dummy())
}

/// `__elephc_mixed_bitnot` — php's `~` over a value known only at run time: a string is
/// complemented byte by byte, an int or float is complemented as an integer, and null, bool,
/// array and object operands throw php's `Cannot perform bitwise not on <type>` TypeError.
fn decl_fn_elephc_mixed_bitnot() -> Stmt {
    let value_name = e_ternary(
        e_call("is_bool", vec![e_var("v")]),
        e_ternary(e_var("v"), e_str("true"), e_str("false")),
        e_call("get_debug_type", vec![e_var("v")]),
    );
    function(MIXED_BITNOT_NAME)
        .param("v", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_call("is_string", vec![e_var("v")]),
                vec![s_return(e_call(STRING_BITNOT_NAME, vec![e_var("v")]))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("is_int", vec![e_var("v")]), BinOp::Or, e_call("is_float", vec![e_var("v")])),
                vec![s_return(int_bitnot(e_var("v")))],
                vec![],
                None,
            ),
            s_throw(e_new(
                "TypeError",
                vec![e_binop(e_str("Cannot perform bitwise not on "), BinOp::Concat, value_name)],
            )),
        ])
        .build()
}

/// Builds the helpers.
pub(crate) fn declarations() -> Program {
    internal_declarations(|| vec![decl_fn_elephc_string_bitnot(), decl_fn_elephc_mixed_bitnot()])
}

/// Whether `program` spells a `~` anywhere, the only way the helper can be reached.
pub(crate) fn program_uses_bitnot(program: &Program) -> bool {
    use crate::parser::ast::visit::{walk_nodes, Node};
    use crate::parser::ast::ExprKind;
    let mut found = false;
    walk_nodes(program, &mut |node| {
        if let Node::Expr(expr) = node {
            found |= matches!(expr.kind, ExprKind::BitNot(_));
        }
    });
    found
}
