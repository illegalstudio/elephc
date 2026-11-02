//! Purpose:
//! Builds `array_map()`'s multi-array form as prelude declarations.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends these declarations when the
//!   program references `array_map`, and `crate::ir_lower::expr::compat_preludes`, which routes a
//!   call with two or more arrays here.
//!
//! Key details:
//! - The native runtime maps ONE array. php's multi-array form walks the arrays in parallel by
//!   POSITION (keys are dropped), pads the shorter ones with `null`, and returns a list: of the
//!   callback's results, or of the tuples themselves when the callback is `null`.
//! - The declarations are BUILT, never parsed: elephc does not parse PHP source outside tests.
//! - `doctrine/orm`'s `MappingException` and DoctrineBundle's `MiddlewaresPass` write it, and
//!   refusing the arity kept those classes (and everything extending them) out of the compiled
//!   world.

use crate::parser::ast::{BinOp, Program, Stmt};
use crate::synthetic_class::{
    e_array, e_binop, e_call, e_closure_call, e_index, e_int, e_null, e_null_coalesce, e_post_inc, e_spread,
    e_var, function, php_semantics_declarations, s_array_push, s_assign, s_expr, s_for, s_foreach,
    s_array_assign, s_if, s_return, t_array,
};

/// Reserved helper used for `array_map()` calls with more than one array.
pub(crate) const ARRAY_MAP_MULTI_NAME: &str = "__elephc_array_map_multi";

/// Reserved helper used for single-array `array_map()` calls whose callback is only known at run
/// time (a `?Closure` property, say): the native runtime needs a statically bound callback.
pub(crate) const ARRAY_MAP_GRADUAL_NAME: &str = "__elephc_array_map_gradual";

/// Returns the multi-array `array_map()` declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![map_multi()])
}

/// Returns the gradual-callback `array_map()` declarations for injection.
pub(crate) fn gradual_declarations() -> Program {
    php_semantics_declarations(|| vec![map_gradual()])
}

/// Builds `__elephc_array_map_gradual($callback, array $array): array`.
///
/// php's single-array form keeps the source keys, and a `null` callback returns the array as it
/// is. `monolog/monolog`'s `LineFormatter::stacktracesParserCustom()` maps through
/// `$this->stacktracesParser`, a `?Closure` property.
fn map_gradual() -> Stmt {
    function(ARRAY_MAP_GRADUAL_NAME)
        .param_untyped("callback")
        .param("array", t_array())
        .returns(t_array())
        .body(vec![
            s_if(
                e_binop(e_var("callback"), BinOp::StrictEq, e_null()),
                vec![s_return(e_var("array"))],
                Vec::new(),
                None,
            ),
            s_assign("out", e_array(Vec::new())),
            s_foreach(
                e_var("array"),
                Some("key"),
                "value",
                vec![s_array_assign(
                    "out",
                    e_var("key"),
                    e_closure_call("callback", vec![e_var("value")]),
                )],
            ),
            s_return(e_var("out")),
        ])
        .build()
}

/// Builds `__elephc_array_map_multi($callback, array ...$arrays): array`.
fn map_multi() -> Stmt {
    function(ARRAY_MAP_MULTI_NAME)
        .param_untyped("callback")
        .variadic("arrays", Some(t_array()))
        .returns(t_array())
        .body(vec![
            s_assign("lists", e_array(Vec::new())),
            s_assign("longest", e_int(0)),
            s_foreach(
                e_var("arrays"),
                None,
                "array",
                vec![
                    s_assign("list", e_call("array_values", vec![e_var("array")])),
                    s_array_push("lists", e_var("list")),
                    s_assign("length", e_call("count", vec![e_var("list")])),
                    s_if(
                        e_binop(e_var("length"), BinOp::Gt, e_var("longest")),
                        vec![s_assign("longest", e_var("length"))],
                        Vec::new(),
                        None,
                    ),
                ],
            ),
            s_assign("out", e_array(Vec::new())),
            s_for(
                Some(s_assign("position", e_int(0))),
                Some(e_binop(e_var("position"), BinOp::Lt, e_var("longest"))),
                Some(s_expr(e_post_inc("position"))),
                vec![
                    s_assign("tuple", e_array(Vec::new())),
                    s_foreach(
                        e_var("lists"),
                        None,
                        "list",
                        vec![s_array_push(
                            "tuple",
                            e_null_coalesce(e_index(e_var("list"), e_var("position")), e_null()),
                        )],
                    ),
                    s_if(
                        e_binop(e_var("callback"), BinOp::StrictEq, e_null()),
                        vec![s_array_push("out", e_var("tuple"))],
                        Vec::new(),
                        Some(vec![s_array_push(
                            "out",
                            e_closure_call("callback", vec![e_spread(e_var("tuple"))]),
                        )]),
                    ),
                ],
            ),
            s_return(e_var("out")),
        ])
        .build()
}
