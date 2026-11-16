//! Purpose:
//! Injects target-independent elephc-PHP `array_merge()` helpers for gradual or associative
//! operands that the indexed-slot runtime helpers cannot represent.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness before type checking.
//!
//! Key details:
//! - The native lowering copies 8- or 16-byte slots out of INDEXED arrays. None of the six
//!   `__rt_array_merge*` helpers walks a hash, so none of them can honour PHP's key rules.
//! - Those rules are what makes the obvious shortcut wrong: `array_merge` renumbers integer keys
//!   but PRESERVES string keys, later winning. Reusing the gradual `array_chunk` trick
//!   (`array_merge(array_values($a), array_values($b))`) would silently drop every string key.
//! - Writing the rule in elephc-PHP needs no new runtime helper: `foreach` over a gradual value,
//!   `is_int` on the key, then an append or a keyed store. Fixed-arity wrappers cover common
//!   three-, five-, and six-array calls. Spread wrappers receive the outer array as an
//!   ordinary argument and flatten it themselves, avoiding the compiler's currently lossy
//!   variadic-parameter materialization. Concrete compatible packed arrays keep the native path.
//! - Built as AST, never parsed from PHP source.
//! - Verified against `php -n` over 196 array pairs (empty, list, sparse integer keys, string keys,
//!   colliding keys, numeric-string keys, nested arrays) in both spellings of the integer arm.

use crate::optimize::reachability::PreludeInventory;
use crate::parser::ast::{Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_call, e_concat_all, e_int, e_new, e_not, e_post_inc, e_str, e_var, function,
    php_semantics_declarations, s_array_assign, s_array_push, s_assign, s_expr, s_foreach, s_if,
    s_return, s_throw, t_array, t_mixed,
};

/// The operand check every helper runs before merging.
const REQUIRE_ARRAY_NAME: &str = "__elephc_array_merge_require_array";

/// Reserved function name used for gradual `array_merge()` calls.
pub(crate) const GRADUAL_ARRAY_MERGE_NAME: &str = "__elephc_array_merge_gradual";

/// Reserved helper name for gradual three-argument `array_merge()` calls.
pub(crate) const GRADUAL_ARRAY_MERGE_THREE_NAME: &str = "__elephc_array_merge_gradual_three";

/// Reserved helper name for gradual five-argument `array_merge()` calls.
pub(crate) const GRADUAL_ARRAY_MERGE_FIVE_NAME: &str = "__elephc_array_merge_gradual_five";

/// Reserved helper name for gradual six-argument `array_merge()` calls.
pub(crate) const GRADUAL_ARRAY_MERGE_SIX_NAME: &str = "__elephc_array_merge_gradual_six";

/// Reserved helper name for `array_merge($first, ...$rest)` calls.
pub(crate) const GRADUAL_ARRAY_MERGE_SPREAD_NAME: &str =
    "__elephc_array_merge_gradual_spread";

/// Reserved helper name for `array_merge(...$arrays)` calls.
pub(crate) const GRADUAL_ARRAY_MERGE_ONLY_SPREAD_NAME: &str =
    "__elephc_array_merge_gradual_only_spread";

/// Names the fixed-arity helpers give their parameters, in argument order.
const POSITION_NAMES: [&str; 6] = ["first", "second", "third", "fourth", "fifth", "sixth"];

/// Returns the gradual `array_merge()` helper declarations, built as AST.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![
            require_array(),
            fixed_arity(GRADUAL_ARRAY_MERGE_NAME, 2),
            fixed_arity(GRADUAL_ARRAY_MERGE_THREE_NAME, 3),
            fixed_arity(GRADUAL_ARRAY_MERGE_FIVE_NAME, 5),
            fixed_arity(GRADUAL_ARRAY_MERGE_SIX_NAME, 6),
            first_and_spread(),
            only_spread(),
        ]
    })
}

/// `__elephc_array_merge_require_array(mixed $value, int $position): void`: php's `TypeError`
/// for a non-array operand, naming its position and its debug type.
fn require_array() -> Stmt {
    function(REQUIRE_ARRAY_NAME)
        .param("value", t_mixed())
        .param("position", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![s_if(
            e_not(e_call("is_array", vec![e_var("value")])),
            vec![s_throw(e_new(
                "TypeError",
                vec![e_concat_all(vec![
                    e_str("array_merge(): Argument #"),
                    e_var("position"),
                    e_str(" must be of type array, "),
                    e_call("get_debug_type", vec![e_var("value")]),
                    e_str(" given"),
                ])],
            ))],
            vec![],
            None,
        )])
        .build()
}

/// `__elephc_array_merge_require_array($<operand>, <position>);`
fn check_operand(operand: Expr, position: Expr) -> Stmt {
    s_expr(e_call(REQUIRE_ARRAY_NAME, vec![operand, position]))
}

/// `foreach ($<source> as $key => $value) { if (is_int($key)) { $merged[] = $value; } else {
/// $merged[$key] = $value; } }`: php's merge rule, renumbering integer keys and keeping string
/// keys (a later one wins).
fn merge_into(source: Expr) -> Stmt {
    s_foreach(
        source,
        Some("key"),
        "value",
        vec![s_if(
            e_call("is_int", vec![e_var("key")]),
            vec![s_array_push("merged", e_var("value"))],
            vec![],
            Some(vec![s_array_assign("merged", e_var("key"), e_var("value"))]),
        )],
    )
}

/// `<name>(mixed $first, ..., mixed $<arity-th>): array`: checks every operand, then merges them
/// in order.
fn fixed_arity(name: &str, arity: usize) -> Stmt {
    let operands = &POSITION_NAMES[..arity];
    let mut body: Vec<Stmt> = operands
        .iter()
        .zip(1..)
        .map(|(operand, position)| check_operand(e_var(operand), e_int(position)))
        .collect();
    body.push(s_assign("merged", e_array(vec![])));
    body.extend(operands.iter().map(|operand| merge_into(e_var(operand))));
    body.push(s_return(e_var("merged")));
    operands
        .iter()
        .fold(function(name), |builder, operand| builder.param(operand, t_mixed()))
        .returns(t_array())
        .body(body)
        .build()
}

/// The loop over a spread operand's arrays: each is checked at its 1-based position, then merged.
fn merge_each_spread_array(spread: &str) -> Stmt {
    s_foreach(
        e_var(spread),
        None,
        "array",
        vec![
            s_expr(e_post_inc("position")),
            check_operand(e_var("array"), e_var("position")),
            merge_into(e_var("array")),
        ],
    )
}

/// `__elephc_array_merge_gradual_spread(mixed $first, mixed $rest): array`, for
/// `array_merge($first, ...$rest)`.
fn first_and_spread() -> Stmt {
    function(GRADUAL_ARRAY_MERGE_SPREAD_NAME)
        .param("first", t_mixed())
        .param("rest", t_mixed())
        .returns(t_array())
        .body(vec![
            check_operand(e_var("first"), e_int(1)),
            s_assign("position", e_int(1)),
            s_assign("merged", e_array(vec![])),
            merge_into(e_var("first")),
            merge_each_spread_array("rest"),
            s_return(e_var("merged")),
        ])
        .build()
}

/// `__elephc_array_merge_gradual_only_spread(mixed $arrays): array`, for
/// `array_merge(...$arrays)`.
fn only_spread() -> Stmt {
    function(GRADUAL_ARRAY_MERGE_ONLY_SPREAD_NAME)
        .param("arrays", t_mixed())
        .returns(t_array())
        .body(vec![
            s_assign("position", e_int(0)),
            s_assign("merged", e_array(vec![])),
            merge_each_spread_array("arrays"),
            s_return(e_var("merged")),
        ])
        .build()
}

/// Prepends the gradual `array_merge()` prelude when the program references `array_merge`.
///
/// The reference test is the PHP name rather than the helper's, because the decision to route a
/// call here is made from inferred operand types during EIR lowering — long after injection.
/// `crate::filter_var_prelude` is injected on the same principle.
pub fn inject_if_used(program: Program, inventory: &mut PreludeInventory) -> Program {
    if !crate::ast_usage::collect(&program).references("array_merge") {
        return program;
    }
    let mut combined = declarations();
    inventory.record_program("array_merge", &combined);
    combined.extend(program);
    combined
}
