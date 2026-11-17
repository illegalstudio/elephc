//! Purpose:
//! Builds `__elephc_strtr_pairs($string, $pairs)`, the two-argument `strtr()` over replacement
//! values that are not already strings, as an AST-built prelude declaration.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program references `strtr`.
//! - `crate::ir_lower::expr::compat_preludes`, which routes a two-argument call whose statically
//!   typed pair array does not hold strings to [`STRTR_PAIRS_NAME`].
//!
//! Key details:
//! - The native `__rt_strtr_hash` reads a typed replacement slot as a string pointer and length,
//!   so an `int`, `float`, `bool` or object value was read as garbage (`strtr("a", ["a" => 5])`
//!   answered `""`), and for a boxed `mixed` value it coerced the cell once while measuring the
//!   result and again while writing it, running a `__toString()` twice per match.
//! - php-src (`php_strtr_array`) coerces every replacement exactly once, up front. This helper
//!   does the same: it spells each value with `(string)` into a fresh string-valued array and
//!   hands that to the native fast path, whose typed-string branch coerces nothing.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_call, e_cast, e_var, function, php_semantics_declarations, s_array_assign,
    s_assign, s_foreach, s_return, t_array,
};

/// Reserved helper backing a two-argument `strtr()` over non-string replacement values.
pub(crate) const STRTR_PAIRS_NAME: &str = "__elephc_strtr_pairs";

/// Returns the `strtr()` replacement-coercion declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![strtr_pairs_fn()])
}

/// Builds `__elephc_strtr_pairs(string $string, array $pairs): string`.
fn strtr_pairs_fn() -> Stmt {
    function(STRTR_PAIRS_NAME)
        .param("string", TypeExpr::Str)
        .param("pairs", t_array())
        .returns(TypeExpr::Str)
        .body(vec![
            // Copy-on-write from the caller's pairs, so the copy keeps their list or hash shape.
            s_assign("spelled", e_var("pairs")),
            s_foreach(
                e_var("pairs"),
                Some("key"),
                "value",
                vec![s_array_assign(
                    "spelled",
                    e_var("key"),
                    e_cast(CastType::String, e_var("value")),
                )],
            ),
            s_return(e_call("strtr", vec![e_var("string"), e_var("spelled")])),
        ])
        .build()
}
