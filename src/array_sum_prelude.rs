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

use crate::parser::ast::{BinOp, Program, Stmt};
use crate::synthetic_class::{
    e_binop, e_int, e_var, function, php_semantics_declarations, s_assign, s_foreach, s_return,
    t_array, t_mixed,
};

/// Reserved helper the lowering redirects such `array_sum()` calls to.
pub(crate) const ARRAY_SUM_VALUES_NAME: &str = "__elephc_array_sum_values";
/// Reserved helper the lowering redirects such `array_product()` calls to.
pub(crate) const ARRAY_PRODUCT_VALUES_NAME: &str = "__elephc_array_product_values";

/// Returns both fold declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![
            fold(ARRAY_SUM_VALUES_NAME, 0, BinOp::Add),
            fold(ARRAY_PRODUCT_VALUES_NAME, 1, BinOp::Mul),
        ]
    })
}

/// Builds `<name>(array $array): mixed`, folding every value with `op` from `seed`.
fn fold(name: &str, seed: i64, op: BinOp) -> Stmt {
    function(name)
        .param("array", t_array())
        .returns(t_mixed())
        .body(vec![
            s_assign("result", e_int(seed)),
            s_foreach(
                e_var("array"),
                None,
                "value",
                vec![s_assign("result", e_binop(e_var("result"), op, e_var("value")))],
            ),
            s_return(e_var("result")),
        ])
        .build()
}
