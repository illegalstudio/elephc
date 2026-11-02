//! Purpose:
//! Builds the `array_pad()` fallback for arrays the native pad helper cannot copy, as prelude
//! declarations.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends it when the program references
//!   `array_pad`.
//! - `crate::ir_lower::expr::compat_preludes::lower_backend_gap_builtin_shape`, which redirects a
//!   call here unless its operand is a list the native helper copies and the pad value has that
//!   list's representation.
//!
//! Key details:
//! - The native helper copies 8-byte slots of an indexed array. A list of strings (16-byte slots),
//!   a hash, a boxed value, or a pad value of another type was refused at compile time. Laravel's
//!   `Kernel::parseMiddleware()` is the shape: `array_pad(explode(':', $m, 2), 2, [])`.
//! - php keeps string keys and renumbers integer ones; the padding goes before the elements for a
//!   negative length and after them otherwise, and nothing is added when `abs($length)` does not
//!   exceed the count.
//! - The declarations are BUILT, never parsed: elephc does not parse PHP source outside tests.

use crate::parser::ast::{BinOp, Program, Stmt};
use crate::synthetic_class::{
    e_binop, e_call, e_int, e_var, function, php_semantics_declarations, s_array_assign,
    s_array_push, s_assign, s_for, s_foreach, s_if, s_return, t_array, t_mixed,
};

/// Reserved helper the lowering redirects such `array_pad()` calls to.
pub(crate) const ARRAY_PAD_VALUES_NAME: &str = "__elephc_array_pad_values";

/// Returns the helper declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![pad()])
}

/// `for ($i = 0; $i < $missing; $i++) { $out[] = $value; }`
fn push_padding() -> Stmt {
    s_for(
        Some(s_assign("i", e_int(0))),
        Some(e_binop(e_var("i"), BinOp::Lt, e_var("missing"))),
        Some(s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1)))),
        vec![s_array_push("out", e_var("value"))],
    )
}

/// Builds `__elephc_array_pad_values(array $array, int $length, mixed $value): array`.
fn pad() -> Stmt {
    function(ARRAY_PAD_VALUES_NAME)
        .param("array", t_array())
        .param("length", crate::parser::ast::TypeExpr::Int)
        .param("value", t_mixed())
        .returns(t_array())
        .body(vec![
            s_assign(
                "missing",
                e_binop(
                    e_call("abs", vec![e_var("length")]),
                    BinOp::Sub,
                    e_call("count", vec![e_var("array")]),
                ),
            ),
            s_if(
                e_binop(e_var("missing"), BinOp::LtEq, e_int(0)),
                vec![s_return(e_var("array"))],
                Vec::new(),
                None,
            ),
            s_assign("out", crate::synthetic_class::e_array(Vec::new())),
            s_if(
                e_binop(e_var("length"), BinOp::Lt, e_int(0)),
                vec![push_padding()],
                Vec::new(),
                None,
            ),
            s_foreach(
                e_var("array"),
                Some("key"),
                "item",
                vec![s_if(
                    e_call("is_int", vec![e_var("key")]),
                    vec![s_array_push("out", e_var("item"))],
                    Vec::new(),
                    Some(vec![s_array_assign("out", e_var("key"), e_var("item"))]),
                )],
            ),
            s_if(
                e_binop(e_var("length"), BinOp::Gt, e_int(0)),
                vec![push_padding()],
                Vec::new(),
                None,
            ),
            s_return(e_var("out")),
        ])
        .build()
}
