//! Purpose:
//! Builds `array_filter()`'s callback form as prelude declarations, for calls whose callback is
//! only a value at run time.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends these declarations when the
//!   program references `array_filter`.
//!
//! Key details:
//! - The array runtimes take a native callback POINTER. The backend recovers one from a literal
//!   closure, a string name or a `callable` local, and refuses everything else with
//!   `array_filter callback with local callback operand that has no prior same-block store`.
//!   Calling a callback through a variable is ordinary PHP, so this helper does that instead.
//! - The declarations are BUILT, never parsed: elephc does not parse PHP source outside tests.
//! - Both writes go through an explicit key, so the result has ONE representation -- the
//!   key-preserving hash php's own `array_filter()` produces. `src/builtins/array/array_filter.rs`
//!   answers the same shape for the calls `compat_preludes` redirects here; the two must agree or
//!   the call site reads the helper's result as its own.
//! - `twig/twig`'s `CoreExtension::filter($env, $isSandboxed, $array, $arrow)` is the shape that
//!   needs it: `$arrow` is an untyped parameter, and the whole Symfony `--web` build stopped on it.

use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_closure_call, e_int, e_var, function, php_semantics_declarations,
    s_array_assign, s_assign, s_foreach, s_if, s_return, t_array,
};

/// Reserved helper used for `array_filter()` calls whose callback is only known at run time.
pub(crate) const ARRAY_FILTER_CALLBACK_NAME: &str = "__elephc_array_filter_callback";

/// php's `ARRAY_FILTER_USE_KEY`: the key is the callback's only argument.
const USE_KEY: i64 = 2;

/// php's `ARRAY_FILTER_USE_BOTH`: the callback takes the value and then the key.
const USE_BOTH: i64 = 1;

/// Returns the `array_filter()` callback declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![filter_with_callback()])
}

/// Builds `__elephc_array_filter_callback(array $array, $callback, int $mode): array`.
fn filter_with_callback() -> Stmt {
    function(ARRAY_FILTER_CALLBACK_NAME)
        .param("array", t_array())
        .param_untyped("callback")
        .param("mode", TypeExpr::Int)
        .returns(t_array())
        .body(vec![
            s_assign("out", e_array(Vec::new())),
            s_foreach(
                e_var("array"),
                Some("key"),
                "value",
                vec![
                    s_if(
                        e_binop(e_var("mode"), BinOp::StrictEq, e_int(USE_KEY)),
                        vec![s_assign(
                            "keep",
                            e_closure_call("callback", vec![e_var("key")]),
                        )],
                        vec![(
                            e_binop(e_var("mode"), BinOp::StrictEq, e_int(USE_BOTH)),
                            vec![s_assign(
                                "keep",
                                e_closure_call("callback", vec![e_var("value"), e_var("key")]),
                            )],
                        )],
                        Some(vec![s_assign(
                            "keep",
                            e_closure_call("callback", vec![e_var("value")]),
                        )]),
                    ),
                    s_if(
                        e_var("keep"),
                        vec![s_array_assign("out", e_var("key"), e_var("value"))],
                        Vec::new(),
                        None,
                    ),
                ],
            ),
            s_return(e_var("out")),
        ])
        .build()
}
