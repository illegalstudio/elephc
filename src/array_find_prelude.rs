//! Purpose:
//! Builds `array_any()`, `array_all()` and `array_find()` as prelude declarations that iterate
//! any array the way php does.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declarations when the
//!   program references either builtin.
//! - `crate::ir_lower::expr::compat_preludes`, which routes positional calls here.
//!
//! Key details:
//! - php calls the predicate with the value AND the key (`callback(mixed $value, mixed $key)`),
//!   and takes any array: a keyed one, or a `mixed` value that holds one. The native predicate
//!   runtime walks a dense list and passes only the value, so a keyed array was refused at compile
//!   time and a two-parameter predicate as well. A plain `foreach` answers both.
//! - The declarations are BUILT, never parsed: elephc does not parse PHP source outside tests.

use crate::parser::ast::{Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_bool, e_closure_call, e_not, e_null, e_var, function, php_semantics_declarations, s_foreach, s_if,
    s_return, t_array, t_mixed,
};

/// Reserved helper backing `array_any()`.
pub(crate) const ARRAY_ANY_NAME: &str = "__elephc_array_any";

/// Reserved helper backing `array_all()`.
pub(crate) const ARRAY_ALL_NAME: &str = "__elephc_array_all";

/// Reserved helper backing `array_find()`.
pub(crate) const ARRAY_FIND_NAME: &str = "__elephc_array_find";

/// Returns the `array_any()` / `array_all()` / `array_find()` declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![
            predicate_walk(ARRAY_ANY_NAME, true),
            predicate_walk(ARRAY_ALL_NAME, false),
            find_walk(),
        ]
    })
}

/// Builds the `array_find()` walk: the first value the callback accepts, or null.
fn find_walk() -> Stmt {
    function(ARRAY_FIND_NAME)
        .param("array", t_array())
        .param_untyped("callback")
        .returns(t_mixed())
        .body(vec![
            s_foreach(
                e_var("array"),
                Some("key"),
                "value",
                vec![s_if(
                    e_closure_call("callback", vec![e_var("value"), e_var("key")]),
                    vec![s_return(e_var("value"))],
                    Vec::new(),
                    None,
                )],
            ),
            s_return(e_null()),
        ])
        .build()
}

/// Builds one predicate walk: `array_any` returns true at the first element the callback accepts,
/// `array_all` returns false at the first one it rejects; each answers the opposite when the walk
/// ends.
fn predicate_walk(name: &str, stop_on_true: bool) -> Stmt {
    let verdict = e_closure_call("callback", vec![e_var("value"), e_var("key")]);
    let stop_when = if stop_on_true { verdict } else { e_not(verdict) };
    function(name)
        .param("array", t_array())
        .param_untyped("callback")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_foreach(
                e_var("array"),
                Some("key"),
                "value",
                vec![s_if(
                    stop_when,
                    vec![s_return(e_bool(stop_on_true))],
                    Vec::new(),
                    None,
                )],
            ),
            s_return(e_bool(!stop_on_true)),
        ])
        .build()
}
