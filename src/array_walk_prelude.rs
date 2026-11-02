//! Purpose:
//! Builds `array_walk()` and `array_walk_recursive()` as prelude declarations, because the native
//! runtime helpers answer neither one for the shapes ordinary PHP uses.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends these declarations when the
//!   program references either builtin.
//!
//! Key details:
//! - The declarations are BUILT, never parsed: elephc does not parse PHP source outside tests.
//! - MEASURED against php 8.5.10. `__rt_array_walk` takes a native callback POINTER and a
//!   parameter list the callback cannot write through, and `__rt_array_walk_recursive` reads every
//!   slot as an integer:
//!
//!       $f = static function (&$v) { $v *= 2; };
//!       $a = [1, 2, 3];        array_walk($a, $f);
//!       php     2,4,6                elephc  refused: callback parameter $v must be passed a variable
//!
//!       $b = [1, [2, 3]];      array_walk_recursive($b, static function ($v) { echo $v; });
//!       php     123                  elephc  43036729604303673088    <- slot addresses
//!
//!       $b = [1, [2, 3]];      array_walk_recursive($b, static function (&$v) { $v *= 2; });
//!       php     [2,[4,6]]             elephc  [1,[2,3]]              <- no write-back at all
//!
//!   A wrong answer is worse than a slower one, and the two builtins are defined entirely in terms
//!   of a by-reference `foreach` and a callback call -- both of which the compiler already lowers
//!   correctly -- so they are expressed that way.
//! - `$hasArg` is a parameter rather than a `null` check on `$arg`, because php distinguishes an
//!   ABSENT third argument from an explicit `null` one: with `array_walk($a, $f, null)` the
//!   callback receives three arguments, and a callback declaring three is satisfied.
//! - Symfony's `UrlGenerator::doGenerate()` is the shape that needs it:
//!   `array_walk_recursive($extra, $caster = static function (&$v) use (&$caster, …) { … })`,
//!   whose inner `array_walk_recursive($vars, $caster)` passes the callback as a VARIABLE.

use crate::parser::ast::{Program, Stmt};
use crate::synthetic_class::{
    e_bool, e_call, e_closure_call, e_var, function, php_semantics_declarations, s_foreach_by_ref, s_if,
    s_return, t_array, t_mixed,
};

/// Reserved helper for `array_walk()`.
pub(crate) const ARRAY_WALK_NAME: &str = "__elephc_array_walk";

/// Reserved helper for `array_walk_recursive()`.
pub(crate) const ARRAY_WALK_RECURSIVE_NAME: &str = "__elephc_array_walk_recursive";

/// Returns the `array_walk()` family declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![array_walk(), array_walk_recursive()])
}

/// Builds `__elephc_array_walk(array &$array, $callback, bool $hasArg, mixed $arg): bool`.
///
/// The parameter is TYPED `array` on purpose. Untyped, the by-reference `foreach` converted the
/// caller's `array<int>` to boxed elements in place while the caller still read raw integers, and
/// `implode()` printed the box addresses. A declared `array` by-reference parameter goes through
/// `ref_place_args`, which widens the caller's local to `array<mixed>` before the call.
fn array_walk() -> Stmt {
    function(ARRAY_WALK_NAME)
        .param_by_ref("array", Some(t_array()))
        .param_untyped("callback")
        .param("hasArg", crate::parser::ast::TypeExpr::Bool)
        .param("arg", t_mixed())
        .returns(crate::parser::ast::TypeExpr::Bool)
        .body(vec![
            s_foreach_by_ref(
                e_var("array"),
                Some("key"),
                "value",
                vec![callback_call()],
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// Builds `__elephc_array_walk_recursive(&$array, $callback, bool $hasArg, mixed $arg): bool`.
///
/// An array-valued element recurses INSTEAD of reaching the callback, which is the whole
/// difference from `array_walk()`; php never hands a nested array to the callback.
fn array_walk_recursive() -> Stmt {
    function(ARRAY_WALK_RECURSIVE_NAME)
        .param_by_ref("array", Some(t_array()))
        .param_untyped("callback")
        .param("hasArg", crate::parser::ast::TypeExpr::Bool)
        .param("arg", t_mixed())
        .returns(crate::parser::ast::TypeExpr::Bool)
        .body(vec![
            s_foreach_by_ref(
                e_var("array"),
                Some("key"),
                "value",
                vec![s_if(
                    e_call("is_array", vec![e_var("value")]),
                    vec![crate::synthetic_class::s_expr(e_call(
                        ARRAY_WALK_RECURSIVE_NAME,
                        vec![
                            e_var("value"),
                            e_var("callback"),
                            e_var("hasArg"),
                            e_var("arg"),
                        ],
                    ))],
                    Vec::new(),
                    Some(vec![callback_call()]),
                )],
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `$callback($value, $key)`, or `$callback($value, $key, $arg)` when the caller supplied one.
fn callback_call() -> Stmt {
    s_if(
        e_var("hasArg"),
        vec![crate::synthetic_class::s_expr(e_closure_call(
            "callback",
            vec![e_var("value"), e_var("key"), e_var("arg")],
        ))],
        Vec::new(),
        Some(vec![crate::synthetic_class::s_expr(e_closure_call(
            "callback",
            vec![e_var("value"), e_var("key")],
        ))]),
    )
}
