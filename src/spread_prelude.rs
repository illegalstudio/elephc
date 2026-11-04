//! Purpose:
//! Builds the helper that turns an array-literal spread source not proven to be an array into
//! one, as a prelude declaration.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program spreads into an array literal.
//! - `crate::ir_lower::expr::compat_preludes`, which routes such a source to it.
//!
//! Key details:
//! - The native spread reads array storage, so a `Traversable` in a boxed value threw
//!   `Only arrays and Traversables can be unpacked`: symfony/ux-icons'
//!   `[...$svgElement->attributes]` over a `DOMNamedNodeMap`, which failed every Symfony Demo page.
//! - Array unpacking is not `iterator_to_array()`: a string key is kept and an integer key is
//!   renumbered, so a generator repeating key 0 keeps every value. Any other key is php's Error.
//! - An array comes back untouched: the literal's own spread then renumbers it as before.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{Program, Stmt};
use crate::synthetic_class::{
    e_array, e_call, e_concat_all, e_new, e_not, e_str, e_var, function,
    php_semantics_declarations, s_array_assign, s_array_push, s_assign, s_foreach, s_if, s_return,
    s_throw, t_mixed,
};

/// Reserved helper an array-literal spread source not proven to be an array is routed to.
pub(crate) const SPREAD_SOURCE_NAME: &str = "__elephc_spread_source";

/// Returns the helper declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![decl_fn_spread_source()])
}

/// Builds `__elephc_spread_source($source)`.
fn decl_fn_spread_source() -> Stmt {
    let body = vec![
        s_if(e_call("is_array", vec![e_var("source")]), vec![s_return(e_var("source"))], vec![], None),
        s_if(
            e_not(e_call("is_iterable", vec![e_var("source")])),
            vec![
                s_assign(
                    "message",
                    e_concat_all(vec![
                        e_str("Only arrays and Traversables can be unpacked, "),
                        e_call("get_debug_type", vec![e_var("source")]),
                        e_str(" given"),
                    ]),
                ),
                // php's two classes for one message: an object is a `TypeError`.
                s_if(
                    e_call("is_object", vec![e_var("source")]),
                    vec![s_throw(e_new("TypeError", vec![e_var("message")]))],
                    vec![],
                    None,
                ),
                s_throw(e_new("Error", vec![e_var("message")])),
            ],
            vec![],
            None,
        ),
        s_assign("items", e_array(vec![])),
        s_foreach(
            e_var("source"),
            Some("key"),
            "value",
            vec![s_if(
                e_call("is_string", vec![e_var("key")]),
                vec![s_array_assign("items", e_var("key"), e_var("value"))],
                vec![],
                Some(vec![s_if(
                    e_call("is_int", vec![e_var("key")]),
                    vec![s_array_push("items", e_var("value"))],
                    vec![],
                    Some(vec![s_throw(e_new(
                        "Error",
                        vec![e_str("Keys must be of type int|string during array unpacking")],
                    ))]),
                )]),
            )],
        ),
        s_return(e_var("items")),
    ];
    // `mixed` on both sides: the lowering hands the result to the same boxed-spread path a `mixed`
    // source takes, and a call site must not specialize the parameter.
    function(SPREAD_SOURCE_NAME).param("source", t_mixed()).returns(t_mixed()).body(body).build()
}
