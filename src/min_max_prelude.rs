//! Purpose:
//! Builds the single-argument `min()` / `max()` over a gradual (`mixed`) operand as a prelude
//! declaration.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program calls `min()` or `max()`.
//! - `crate::ir_lower::expr::compat_preludes`, which routes a one-argument call whose operand is
//!   `mixed` (or a union) to it.
//!
//! Key details:
//! - The native one-argument form reads a typed array. A `mixed` operand -- symfony/form's
//!   `min($options['years'])` -- was refused at compile time, which interpreted the file; the
//!   native lowering cannot take it either (it read the box as a scalar).
//! - php raises a `TypeError` for a non-array and a `ValueError` for an empty array, with the
//!   messages reproduced here; otherwise it keeps the first element that compares lower (`min`)
//!   or higher (`max`) than the current pick, which is the loop below.
//! - A spread call (`max(...$values)`) passes `$spread = true`: the elements are then the
//!   arguments, so an empty spread is an `ArgumentCountError` and a single element is the one
//!   argument (which must itself be an array).
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, Program, Stmt};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_concat_all, e_int, e_new, e_not, e_null, e_str, e_ternary, e_var,
    function, php_semantics_declarations, s_assign, s_foreach, s_if, s_return, s_throw,
};

/// Reserved helper a one-argument `min()` / `max()` over a gradual operand is routed to.
pub(crate) const MIN_MAX_MIXED_NAME: &str = "__elephc_min_max_mixed";

/// Returns the helper declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![decl_fn_min_max_mixed()])
}

/// Builds `__elephc_min_max_mixed($value, $want_max)`.
fn decl_fn_min_max_mixed() -> Stmt {
    let name = || e_ternary(e_var("want_max"), e_str("max"), e_str("min"));
    let body = vec![
        // `max(...$values)`: the spread's elements are the arguments. None is php's
        // `ArgumentCountError`; exactly one is that single argument, which must then be an array;
        // two or more compare as the elements of the array form do.
        s_if(
            e_var("spread"),
            vec![
                s_if(
                    e_binop(e_call("count", vec![e_var("value")]), BinOp::StrictEq, e_int(0)),
                    vec![s_throw(e_new(
                        "ArgumentCountError",
                        vec![e_concat_all(vec![
                            name(),
                            e_str("() expects at least 1 argument, 0 given"),
                        ])],
                    ))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("count", vec![e_var("value")]), BinOp::StrictEq, e_int(1)),
                    vec![
                        s_assign("only", e_null()),
                        s_foreach(e_var("value"), None, "item", vec![s_assign("only", e_var("item"))]),
                        s_assign("value", e_var("only")),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("is_array", vec![e_var("value")])),
            vec![s_throw(e_new(
                "TypeError",
                vec![e_concat_all(vec![
                    name(),
                    e_str("(): Argument #1 ($value) must be of type array, "),
                    e_call("get_debug_type", vec![e_var("value")]),
                    e_str(" given"),
                ])],
            ))],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_call("count", vec![e_var("value")]),
                BinOp::StrictEq,
                e_int(0),
            ),
            vec![s_throw(e_new(
                "ValueError",
                vec![e_concat_all(vec![
                    name(),
                    e_str("(): Argument #1 ($value) must contain at least one element"),
                ])],
            ))],
            vec![],
            None,
        ),
        s_assign("first", e_bool(true)),
        s_assign("best", e_null()),
        s_foreach(
            e_var("value"),
            None,
            "item",
            vec![s_if(
                e_binop(
                    e_var("first"),
                    BinOp::Or,
                    e_ternary(
                        e_var("want_max"),
                        e_binop(e_var("item"), BinOp::Gt, e_var("best")),
                        e_binop(e_var("item"), BinOp::Lt, e_var("best")),
                    ),
                ),
                vec![s_assign("best", e_var("item")), s_assign("first", e_bool(false))],
                vec![],
                None,
            )],
        ),
        s_return(e_var("best")),
    ];
    function(MIN_MAX_MIXED_NAME)
        .param_untyped("value")
        .param_untyped("want_max")
        .param_untyped_default("spread", e_bool(false))
        .body(body)
        .build()
}
