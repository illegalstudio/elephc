//! Purpose:
//! Builds `array_keys()`'s search form as prelude declarations.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends these declarations when the
//!   program calls `array_keys` with a search value, and `crate::ir_lower::expr::compat_preludes`,
//!   which routes such a call here.
//!
//! Key details:
//! - The native runtime lists every key. php's two- and three-argument forms list only the keys
//!   whose value equals `$filter_value`, loosely or (with `$strict`) identically.
//! - The declarations are BUILT, never parsed: elephc does not parse PHP source outside tests.
//! - `doctrine/orm`'s `BasicEntityPersister::getSelectConditionStatementSQL()` writes
//!   `array_keys($value, null, true)`; refusing the arity kept the entity persister -- which every
//!   query goes through -- out of the compiled world.

use crate::parser::ast::{BinOp, Program, Stmt};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_var, function, php_semantics_declarations, s_array_push, s_assign,
    s_foreach, s_if, s_return, t_array, t_mixed,
};

/// Reserved helper used for `array_keys()` calls with a search value.
pub(crate) const ARRAY_KEYS_SEARCH_NAME: &str = "__elephc_array_keys_search";

/// Returns the search-form `array_keys()` declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![keys_search()])
}

/// Builds `__elephc_array_keys_search(array $array, mixed $search, $strict = false): array`.
fn keys_search() -> Stmt {
    let push_key = || vec![s_array_push("out", e_var("key"))];
    function(ARRAY_KEYS_SEARCH_NAME)
        .param("array", t_array())
        .param("search", t_mixed())
        .param_untyped_default("strict", e_bool(false))
        .returns(t_array())
        .body(vec![
            s_assign("out", e_array(Vec::new())),
            s_foreach(
                e_var("array"),
                Some("key"),
                "value",
                vec![s_if(
                    e_var("strict"),
                    vec![s_if(
                        e_binop(e_var("value"), BinOp::StrictEq, e_var("search")),
                        push_key(),
                        Vec::new(),
                        None,
                    )],
                    Vec::new(),
                    Some(vec![s_if(
                        e_binop(e_var("value"), BinOp::Eq, e_var("search")),
                        push_key(),
                        Vec::new(),
                        None,
                    )]),
                )],
            ),
            s_return(e_var("out")),
        ])
        .build()
}
