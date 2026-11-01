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

use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_call, e_index, e_int, e_post_inc, e_str, e_var, function, internal_declarations,
    s_assign, s_expr, s_for, s_return,
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

/// Builds the helper.
pub(crate) fn declarations() -> Program {
    internal_declarations(|| vec![decl_fn_elephc_string_bitnot()])
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
