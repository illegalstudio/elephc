//! Purpose:
//! Builds the `array_sum()` / `array_product()` fallback for arrays the native integer and float
//! folds cannot sum, as prelude declarations.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends them when the program references
//!   `array_sum` or `array_product`.
//! - `crate::ir_lower::expr::compat_preludes::lower_backend_gap_builtin_shape`, which redirects a
//!   call here unless its operand is a plain list of ints.
//!
//! Key details:
//! - The boxed-`mixed` runtime folds (`__rt_array_sum_mixed`, `__rt_hash_sum_mixed`) only add
//!   integers, so a mixed list truncated every float and a hash summed to nothing:
//!
//!   ```text
//!       array_sum([1, 1.5])            php 2.5   elephc 2
//!       array_sum(["x" => 1, "y" => .5])  php 1.5   elephc 0
//!   ```
//!
//!   and an untyped argument (`function g($r) { return array_sum($r); }`) was refused at compile
//!   time.
//! - Folding with the compiler's own `+` / `*` gives php's int-or-float result and its numeric
//!   string handling.
//! - The declarations are BUILT, never parsed: elephc does not parse PHP source outside tests.

use crate::parser::ast::{BinOp, Expr, Program, Stmt};
use crate::synthetic_class::{
    e_binop, e_call, e_concat_all, e_int, e_str, e_var, function, php_semantics_declarations,
    s_assign, s_continue, s_expr, s_foreach, s_if, s_return, s_try, t_array, t_mixed,
};

/// Reserved helper the lowering redirects such `array_sum()` calls to.
pub(crate) const ARRAY_SUM_VALUES_NAME: &str = "__elephc_array_sum_values";
/// Reserved helper the lowering redirects such `array_product()` calls to.
pub(crate) const ARRAY_PRODUCT_VALUES_NAME: &str = "__elephc_array_product_values";

/// Returns both fold declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![
            fold(ARRAY_SUM_VALUES_NAME, "array_sum", "Addition", 0, BinOp::Add),
            fold(ARRAY_PRODUCT_VALUES_NAME, "array_product", "Multiplication", 1, BinOp::Mul),
        ]
    })
}

/// Builds `<name>(array $array): mixed`, folding every value with `op` from `seed`.
///
/// php's `php_array_binop()` does not fail where the plain operator would: an array or object
/// element warns `<function>(): <Operation> is not supported on type <type>` and is skipped, and
/// a non-numeric string warns the same way and counts as 0 (`array_sum([1, [2], 3])` is `int(4)`,
/// `array_product([2, "x", 3])` is `int(0)`). A leading-numeric string goes through the operator
/// and its own `A non-numeric value encountered` warning.
fn fold(name: &str, function_name: &str, operation: &str, seed: i64, op: BinOp) -> Stmt {
    let unsupported = |type_name: Expr| {
        s_expr(e_call(
            "__elephc_internal_warning",
            vec![e_concat_all(vec![
                e_str(&format!("Warning: {function_name}(): {operation} is not supported on type ")),
                type_name,
                e_str("\n"),
            ])],
        ))
    };
    function(name)
        .param("array", t_array())
        .returns(t_mixed())
        .body(vec![
            s_assign("result", e_int(seed)),
            s_foreach(
                e_var("array"),
                None,
                "value",
                vec![
                    s_if(
                        e_binop(
                            e_call("is_array", vec![e_var("value")]),
                            BinOp::Or,
                            e_call("is_object", vec![e_var("value")]),
                        ),
                        vec![
                            unsupported(e_call("get_debug_type", vec![e_var("value")])),
                            s_continue(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_try(
                        vec![s_assign("result", e_binop(e_var("result"), op.clone(), e_var("value")))],
                        vec![(
                            vec!["TypeError"],
                            None,
                            vec![
                                unsupported(e_str("string")),
                                s_assign("result", e_binop(e_var("result"), op, e_int(0))),
                            ],
                        )],
                        None,
                    ),
                ],
            ),
            s_return(e_var("result")),
        ])
        .build()
}
