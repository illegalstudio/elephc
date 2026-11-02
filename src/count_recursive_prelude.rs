//! Purpose:
//! Builds `count($value, $mode)` over a receiver that may hold nested arrays as a prelude
//! declaration.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program calls `count()` (or its alias `sizeof()`) with an explicit `$mode`.
//! - `crate::ir_lower::expr::compat_preludes`, which routes such a call to it.
//!
//! Key details:
//! - elephc's indexed arrays store their payload untagged, so the native `COUNT_RECURSIVE`
//!   cannot tell an `array<int>` slot from an `array<array<int>>` slot and refuses a receiver
//!   that can nest. Illuminate's `MessageBag::count()` --
//!   `count($this->messages, COUNT_RECURSIVE) - count($this->messages)` -- was refused at code
//!   generation and interpreted the class.
//! - php adds the size of every nested ARRAY (objects are not descended into) and ignores
//!   `$mode` for a `Countable` object. A `$mode` other than 0 or 1 raises the `ValueError` the
//!   native guard raises.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_call, e_int, e_new, e_str, e_var, function, php_semantics_declarations, s_assign,
    s_foreach, s_if, s_return, s_throw,
};

/// Reserved helper a nested-capable `count($value, $mode)` call is routed to.
pub(crate) const COUNT_RECURSIVE_NAME: &str = "__elephc_count_recursive";

/// Returns the helper declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![decl_fn_count_recursive()])
}

/// Builds `__elephc_count_recursive($value, int $mode): int`.
fn decl_fn_count_recursive() -> Stmt {
    let body = vec![
        s_if(
            e_binop(
                e_binop(e_var("mode"), BinOp::StrictNotEq, e_int(0)),
                BinOp::And,
                e_binop(e_var("mode"), BinOp::StrictNotEq, e_int(1)),
            ),
            vec![s_throw(e_new(
                "ValueError",
                vec![e_str(
                    "count(): Argument #2 ($mode) must be either COUNT_NORMAL or COUNT_RECURSIVE",
                )],
            ))],
            vec![],
            None,
        ),
        s_if(
            e_call("is_array", vec![e_var("value")]),
            vec![
                s_assign("total", e_call("count", vec![e_var("value")])),
                s_if(
                    e_binop(e_var("mode"), BinOp::StrictEq, e_int(1)),
                    vec![s_foreach(
                        e_var("value"),
                        None,
                        "item",
                        vec![s_if(
                            e_call("is_array", vec![e_var("item")]),
                            vec![s_assign(
                                "total",
                                e_binop(
                                    e_var("total"),
                                    BinOp::Add,
                                    e_call(
                                        COUNT_RECURSIVE_NAME,
                                        vec![e_var("item"), e_int(1)],
                                    ),
                                ),
                            )],
                            vec![],
                            None,
                        )],
                    )],
                    vec![],
                    None,
                ),
                s_return(e_var("total")),
            ],
            vec![],
            None,
        ),
        s_return(e_call("count", vec![e_var("value")])),
    ];
    function(COUNT_RECURSIVE_NAME)
        .param_untyped("value")
        .param("mode", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(body)
        .build()
}
