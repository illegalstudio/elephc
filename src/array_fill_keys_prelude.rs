//! Purpose:
//! Builds `array_fill_keys()` over keys whose type is only known at run time as a prelude
//! declaration.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program calls `array_fill_keys()`.
//! - `crate::ir_lower::expr::compat_preludes`, which routes a call whose keys are gradual to it.
//!
//! Key details:
//! - The native lowering reads int or string keys; gradual keys -- Doctrine's
//!   `ObjectHydrator::onClear()` fills `array_keys($this->identifierMap)` -- were refused at code
//!   generation ("array_fill_keys key element PHP type Mixed"), which interpreted the hydrator.
//! - php keeps an int key and converts every other key to a string (`1.5` is `"1.5"`, `true` is
//!   `"1"`, `null` is `""`); the string then follows the ordinary array-key rules, so `"7"`
//!   becomes the int key 7. Measured with `php -n` 8.5.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{CastType, Program, Stmt};
use crate::synthetic_class::{
    e_array, e_call, e_cast, e_ternary, e_var, function, php_semantics_declarations,
    s_array_assign, s_assign, s_foreach, s_return,
};

/// Reserved helper a gradual-key `array_fill_keys()` call is routed to.
pub(crate) const ARRAY_FILL_KEYS_MIXED_NAME: &str = "__elephc_array_fill_keys_mixed";

/// Returns the helper declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![decl_fn_array_fill_keys_mixed()])
}

/// Builds `__elephc_array_fill_keys_mixed($keys, $value)`.
fn decl_fn_array_fill_keys_mixed() -> Stmt {
    let body = vec![
        s_assign("result", e_array(vec![])),
        s_foreach(
            e_var("keys"),
            None,
            "key",
            vec![s_array_assign(
                "result",
                e_ternary(
                    e_call("is_int", vec![e_var("key")]),
                    e_var("key"),
                    e_cast(CastType::String, e_var("key")),
                ),
                e_var("value"),
            )],
        ),
        s_return(e_var("result")),
    ];
    function(ARRAY_FILL_KEYS_MIXED_NAME)
        .param_untyped("keys")
        .param_untyped("value")
        .body(body)
        .build()
}
