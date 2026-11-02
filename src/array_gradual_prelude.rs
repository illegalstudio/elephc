//! Purpose:
//! Declares the AST-built helpers behind the array builtins whose operands or flags php settles
//! when the call runs: `sort()`'s `$flags`, `array_unique()`'s `$flags`, `array_pad()`,
//! `array_reverse($gradual, true)`, `array_diff_assoc()`, `array_intersect_assoc()`,
//! `array_merge_recursive()` over a spread, and `array_map()`, `array_intersect()` and `range()`
//! over a spread of unknown length.
//!
//! Called from:
//! - `crate::pipeline::compile()` in the compat-prelude phases and the codegen test harness.
//! - `crate::ir_lower::expr::gradual_array_calls`, which redirects the calls to these names.
//! - `crate::ast_usage`, which records a helper's name for exactly the call shapes the lowering
//!   redirects, so the helper is injected (and survives pruning) only in a program that needs it.
//!
//! Key details:
//! - The native array runtimes walk one statically known layout, and the checker used to refuse
//!   every other operand at compile time (`sort() takes exactly 1 argument`, `array_pad() first
//!   argument must be array`, ...). php accepts them and raises its own `TypeError`/`ValueError`
//!   when the value is wrong, so the helpers are plain php over the language's dynamic paths and
//!   check their operands the way php words it.
//! - A comparison `$flags` selects (`SORT_REGULAR`, `SORT_NUMERIC`, `SORT_STRING`,
//!   `SORT_NATURAL`, `SORT_LOCALE_STRING` in the C locale, with `SORT_FLAG_CASE`) lives in ONE
//!   function, `__elephc_sort_flags_compare()`; `sort()` and its siblings reach `usort()` /
//!   `uasort()` through a comparator closure over it and `array_unique()` compares with it.
//! - The helpers declare `array` returns where the checker filed `php_array()`, and the
//!   lowering redirects on the FILED type, so the two agree about which calls a helper answers.
//! - The interpreter has its own implementations of the builtins themselves
//!   (`elephc_magician::interpreter::builtins::array`), because a compiler prelude is never
//!   reachable from interpreted code.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::synthetic_class::{
    closure,
    e_array,
    e_binop,
    e_bool,
    e_call,
    e_cast,
    e_closure_call,
    e_float,
    e_index,
    e_int,
    e_neg,
    e_new,
    e_not,
    e_null,
    e_post_inc,
    e_spread,
    e_str,
    e_ternary,
    e_var,
    function,
    internal_declarations,
    s_array_assign,
    s_array_push,
    s_assign,
    s_break,
    s_expr,
    s_for,
    s_foreach,
    s_if,
    s_return,
    s_throw,
    t_array,
};


/// Prelude inventory group holding the declarations.
pub const GROUP: &str = "array_gradual";

/// Comparator factory behind `sort($a, $flags)` and its siblings.
pub(crate) const SORT_COMPARATOR_NAME: &str = "__elephc_sort_flags_comparator";
/// The comparison itself, shared by the comparator and `array_unique()`.
pub(crate) const SORT_COMPARE_NAME: &str = "__elephc_sort_flags_compare";
/// `array_unique($a, $flags)` for any comparison but `SORT_STRING`.
pub(crate) const ARRAY_UNIQUE_NAME: &str = "__elephc_array_unique_flags";
/// `array_diff_assoc()` over operands the hash runtime cannot compare.
pub(crate) const ARRAY_DIFF_ASSOC_NAME: &str = "__elephc_array_diff_assoc_gradual";
/// `array_intersect_assoc()` over operands the hash runtime cannot compare.
pub(crate) const ARRAY_INTERSECT_ASSOC_NAME: &str = "__elephc_array_intersect_assoc_gradual";
/// `array_reverse($gradual, $preserve_keys)`.
pub(crate) const ARRAY_REVERSE_PRESERVE_NAME: &str = "__elephc_array_reverse_preserve_gradual";
/// `array_pad()` over a gradual array.
pub(crate) const ARRAY_PAD_NAME: &str = "__elephc_array_pad_gradual";
/// `array_merge_recursive(...$arrays)`.
pub(crate) const ARRAY_MERGE_RECURSIVE_SPREAD_NAME: &str =
    "__elephc_array_merge_recursive_spread";
/// `array_intersect(...$arrays)`.
pub(crate) const ARRAY_INTERSECT_SPREAD_NAME: &str = "__elephc_array_intersect_spread";
/// `array_map(...$arguments)`.
pub(crate) const ARRAY_MAP_SPREAD_NAME: &str = "__elephc_array_map_spread";
/// `range(...$arguments)`.
pub(crate) const RANGE_SPREAD_NAME: &str = "__elephc_range_spread";
/// `range()` over operands whose type is only known at run time.
pub(crate) const RANGE_GRADUAL_NAME: &str = "__elephc_range_gradual";
/// `iterator_to_array()` over a scalar: the `TypeError` php raises.
pub(crate) const ITERATOR_SOURCE_ERROR_NAME: &str = "__elephc_iterator_source_error";
/// `pcntl_sigprocmask()` and the wait functions over a signal set known only at run time.
pub(crate) const SIGNAL_SET_NAME: &str = "__elephc_signal_set";
/// The recursive combination step of `array_merge_recursive()`.
const MERGE_RECURSIVE_INTO_NAME: &str = "__elephc_array_merge_recursive_into";
/// The double walk behind `range()` over floats.
const RANGE_FLOATS_NAME: &str = "__elephc_range_floats";
/// The operand check every helper shares.
const REQUIRE_ARRAY_NAME: &str = "__elephc_require_array";

/// Returns the helper that answers `builtin(...$arguments)` when the spread's length is only
/// known at run time, for the builtins the checker answers with their declared type instead of
/// refusing the call.
pub(crate) fn spread_helper(builtin: &str) -> Option<&'static str> {
    match builtin {
        "array_map" => Some(ARRAY_MAP_SPREAD_NAME),
        "array_intersect" => Some(ARRAY_INTERSECT_SPREAD_NAME),
        "range" => Some(RANGE_SPREAD_NAME),
        "array_merge_recursive" => Some(ARRAY_MERGE_RECURSIVE_SPREAD_NAME),
        _ => None,
    }
}

/// Returns whether a `$flags` argument is the literal `SORT_STRING`, the comparison the native
/// runtime implements. A named `flags:` argument is looked through.
pub(crate) fn flags_are_sort_string(flags: &Expr) -> bool {
    let value = match &flags.kind {
        ExprKind::NamedArg { value, .. } => value.as_ref(),
        _ => flags,
    };
    matches!(value.kind, ExprKind::IntLiteral(2))
}

/// Returns whether any argument is a spread of something other than a written-out array
/// literal, which php expands when the call runs.
pub(crate) fn has_unknown_length_spread(args: &[Expr]) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::Spread(inner) => !matches!(
            &inner.kind,
            ExprKind::ArrayLiteral(elements)
                if !elements.iter().any(|element| matches!(element.kind, ExprKind::Spread(_)))
        ),
        _ => false,
    })
}

/// Records the helper names the lowering redirects a call to, for the call shapes it redirects.
///
/// The record is a superset of what the lowering does (the lowering also reads operand types),
/// which only means a helper is injected without being called; pruning removes it.
pub(crate) fn record_call_shape(
    record: &mut impl FnMut(&str),
    builtin: &str,
    args: &[Expr],
) {
    let positional = args.len();
    match builtin {
        // The sort becomes `usort()` / `uasort()` over the comparator, so those (and their
        // gradual-operand helpers) are referenced too.
        "sort" | "rsort" if positional == 2 => {
            record(SORT_COMPARATOR_NAME);
            record("usort");
        }
        "asort" | "arsort" if positional == 2 => {
            record(SORT_COMPARATOR_NAME);
            record("uasort");
        }
        "array_unique" if positional == 2 => record(ARRAY_UNIQUE_NAME),
        "array_merge_recursive" => record(ARRAY_MERGE_RECURSIVE_SPREAD_NAME),
        "array_diff_assoc" => record(ARRAY_DIFF_ASSOC_NAME),
        "array_intersect_assoc" => record(ARRAY_INTERSECT_ASSOC_NAME),
        "array_pad" => record(ARRAY_PAD_NAME),
        "array_reverse" if positional == 2 => record(ARRAY_REVERSE_PRESERVE_NAME),
        "iterator_to_array" => record(ITERATOR_SOURCE_ERROR_NAME),
        "pcntl_sigprocmask" | "pcntl_sigwaitinfo" | "pcntl_sigtimedwait" => {
            record(SIGNAL_SET_NAME);
        }
        _ => {}
    }
    if let Some(helper) = spread_helper(builtin) {
        if has_unknown_length_spread(args) {
            record(helper);
        }
    }
}

/// The helpers each entry point needs besides itself.
fn dependencies(helper: &str) -> &'static [&'static str] {
    match helper {
        SORT_COMPARATOR_NAME => &[SORT_COMPARE_NAME],
        ARRAY_UNIQUE_NAME => &[REQUIRE_ARRAY_NAME, SORT_COMPARE_NAME],
        ARRAY_MERGE_RECURSIVE_SPREAD_NAME => &[REQUIRE_ARRAY_NAME, MERGE_RECURSIVE_INTO_NAME],
        ARRAY_DIFF_ASSOC_NAME
        | ARRAY_INTERSECT_ASSOC_NAME
        | ARRAY_REVERSE_PRESERVE_NAME
        | ARRAY_PAD_NAME
        | ARRAY_INTERSECT_SPREAD_NAME
        | ARRAY_MAP_SPREAD_NAME
        | SIGNAL_SET_NAME => &[REQUIRE_ARRAY_NAME],
        RANGE_SPREAD_NAME => &[RANGE_GRADUAL_NAME],
        RANGE_GRADUAL_NAME => &[RANGE_FLOATS_NAME],
        _ => &[],
    }
}

/// Every function this prelude declares, entry points first.
pub(crate) const FUNCTIONS: &[&str] = &[
    SORT_COMPARATOR_NAME,
    SORT_COMPARE_NAME,
    ARRAY_UNIQUE_NAME,
    ARRAY_DIFF_ASSOC_NAME,
    ARRAY_INTERSECT_ASSOC_NAME,
    ARRAY_REVERSE_PRESERVE_NAME,
    ARRAY_PAD_NAME,
    ARRAY_MERGE_RECURSIVE_SPREAD_NAME,
    MERGE_RECURSIVE_INTO_NAME,
    ARRAY_INTERSECT_SPREAD_NAME,
    ARRAY_MAP_SPREAD_NAME,
    RANGE_SPREAD_NAME,
    RANGE_GRADUAL_NAME,
    RANGE_FLOATS_NAME,
    ITERATOR_SOURCE_ERROR_NAME,
    SIGNAL_SET_NAME,
    REQUIRE_ARRAY_NAME,
];

/// Returns every declaration, in the order `FUNCTIONS` lists the names.
pub(crate) fn declarations() -> Program {
    crate::synthetic_class::php_semantics_declarations(helper_declarations)
}

/// Returns the name a function declaration statement declares.
fn declared_name(stmt: &Stmt) -> Option<&str> {
    match &stmt.kind {
        StmtKind::FunctionDecl { name, .. } => Some(name),
        _ => None,
    }
}

/// Returns whether a top-level, namespace-less, guarded or synthetic statement declares the
/// function `name`.
fn program_declares_function(program: &[Stmt], name: &str) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { name: declared, .. } => declared.eq_ignore_ascii_case(name),
        StmtKind::NamespaceBlock { name: None, body }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => program_declares_function(body, name),
        _ => false,
    })
}

/// Prepends the helpers the program's call shapes need, each with the helpers it calls.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let usage = crate::ast_usage::collect(&program);
    let mut wanted: Vec<&str> = Vec::new();
    for name in FUNCTIONS {
        if usage.references(name) && !wanted.contains(name) {
            wanted.push(name);
        }
    }
    if wanted.is_empty() {
        return program;
    }
    let mut index = 0;
    while index < wanted.len() {
        for dependency in dependencies(wanted[index]) {
            if !wanted.contains(dependency) {
                wanted.push(dependency);
            }
        }
        index += 1;
    }
    // A program that declares one of the names itself keeps its own definition.
    if wanted
        .iter()
        .any(|name| program_declares_function(&program, name))
    {
        return program;
    }
    let mut combined: Program = declarations()
        .into_iter()
        .filter(|stmt| declared_name(stmt).is_some_and(|name| wanted.contains(&name)))
        .collect();
    inventory.record_program(GROUP, &combined);
    combined.extend(program);
    combined
}

/// `__elephc_require_array` — transcribed from the PHP form.
fn decl_fn_elephc_require_array() -> Stmt {
    function("__elephc_require_array")
        .param_untyped("value")
        .param_untyped("function")
        .param_untyped("position")
        .param_untyped("name")
        .body(vec![
            s_if(
                e_not(e_call("is_array", vec![e_var("value")])),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("function"), BinOp::Concat, e_str("(): Argument #")), BinOp::Concat, e_var("position")), BinOp::Concat, e_ternary(e_binop(e_var("name"), BinOp::StrictNotEq, e_str("")), e_binop(e_binop(e_str(" ($"), BinOp::Concat, e_var("name")), BinOp::Concat, e_str(")")), e_str(""))), BinOp::Concat, e_str(" must be of type array, ")), BinOp::Concat, e_call("get_debug_type", vec![e_var("value")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
        ])
        .build()
}

/// `__elephc_sort_flags_compare` — transcribed from the PHP form.
fn decl_fn_elephc_sort_flags_compare() -> Stmt {
    function("__elephc_sort_flags_compare")
        .param_untyped("a")
        .param_untyped("b")
        .param_untyped("flags")
        .body(vec![
            s_assign("case", e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(8)), BinOp::StrictNotEq, e_int(0))),
            s_assign("mode", e_binop(e_var("flags"), BinOp::BitAnd, e_int(7))),
            s_if(
                e_binop(e_var("mode"), BinOp::StrictEq, e_int(1)),
                vec![
                    s_return(e_binop(e_cast(CastType::Float, e_var("a")), BinOp::Spaceship, e_cast(CastType::Float, e_var("b")))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("mode"), BinOp::StrictEq, e_int(2)), BinOp::Or, e_binop(e_var("mode"), BinOp::StrictEq, e_int(5))),
                vec![
                    s_if(
                        e_var("case"),
                        vec![
                            s_return(e_binop(e_call("strcasecmp", vec![e_cast(CastType::String, e_var("a")), e_cast(CastType::String, e_var("b"))]), BinOp::Spaceship, e_int(0))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_call("strcmp", vec![e_cast(CastType::String, e_var("a")), e_cast(CastType::String, e_var("b"))]), BinOp::Spaceship, e_int(0))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("mode"), BinOp::StrictEq, e_int(6)),
                vec![
                    s_if(
                        e_var("case"),
                        vec![
                            s_return(e_call("strnatcasecmp", vec![e_cast(CastType::String, e_var("a")), e_cast(CastType::String, e_var("b"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("strnatcmp", vec![e_cast(CastType::String, e_var("a")), e_cast(CastType::String, e_var("b"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_var("a"), BinOp::Spaceship, e_var("b"))),
        ])
        .build()
}

/// `__elephc_sort_flags_comparator` — transcribed from the PHP form.
fn decl_fn_elephc_sort_flags_comparator() -> Stmt {
    function("__elephc_sort_flags_comparator")
        .param_untyped("flags")
        .param_untyped("reverse")
        .keep_unread_params()
        .body(vec![
            s_if(
                e_var("reverse"),
                vec![
                    s_return(closure()
                        .param_untyped("a")
                        .param_untyped("b")
                        .captures("flags")
                        .body(vec![
                            s_return(e_call("__elephc_sort_flags_compare", vec![e_var("b"), e_var("a"), e_var("flags")])),
                        ])
                        .build()),
                ],
                vec![],
                None,
            ),
            s_return(closure()
                .param_untyped("a")
                .param_untyped("b")
                .captures("flags")
                .body(vec![
                    s_return(e_call("__elephc_sort_flags_compare", vec![e_var("a"), e_var("b"), e_var("flags")])),
                ])
                .build()),
        ])
        .build()
}

/// `__elephc_array_unique_flags` — transcribed from the PHP form.
fn decl_fn_elephc_array_unique_flags() -> Stmt {
    function("__elephc_array_unique_flags")
        .param_untyped("array")
        .param_untyped("flags")
        .returns(t_array())
        .body(vec![
            s_expr(e_call("__elephc_require_array", vec![e_var("array"), e_str("array_unique"), e_int(1), e_str("array")])),
            s_assign("result", e_array(vec![])),
            s_if(
                e_binop(e_var("flags"), BinOp::StrictEq, e_int(2)),
                vec![
                    s_assign("seen", e_array(vec![])),
                    s_foreach(e_var("array"), Some("key"), "value", vec![
                        s_assign("text", e_cast(CastType::String, e_var("value"))),
                        s_if(
                            e_not(e_call("isset", vec![e_index(e_var("seen"), e_var("text"))])),
                            vec![
                                s_array_assign("seen", e_var("text"), e_bool(true)),
                                s_array_assign("result", e_var("key"), e_var("value")),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_var("result")),
                ],
                vec![],
                None,
            ),
            s_assign("kept", e_array(vec![])),
            s_foreach(e_var("array"), Some("key"), "value", vec![
                s_assign("duplicate", e_bool(false)),
                s_foreach(e_var("kept"), None, "other", vec![
                    s_if(
                        e_binop(e_call("__elephc_sort_flags_compare", vec![e_var("value"), e_var("other"), e_var("flags")]), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_assign("duplicate", e_bool(true)),
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                s_if(
                    e_not(e_var("duplicate")),
                    vec![
                        s_array_push("kept", e_var("value")),
                        s_array_assign("result", e_var("key"), e_var("value")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_var("result")),
        ])
        .build()
}

/// `__elephc_array_diff_assoc_gradual` — transcribed from the PHP form.
fn decl_fn_elephc_array_diff_assoc_gradual() -> Stmt {
    function("__elephc_array_diff_assoc_gradual")
        .param_untyped("left")
        .param_untyped("right")
        .returns(t_array())
        .body(vec![
            s_expr(e_call("__elephc_require_array", vec![e_var("left"), e_str("array_diff_assoc"), e_int(1), e_str("array")])),
            s_expr(e_call("__elephc_require_array", vec![e_var("right"), e_str("array_diff_assoc"), e_int(2), e_str("")])),
            s_assign("result", e_array(vec![])),
            s_foreach(e_var("left"), Some("key"), "value", vec![
                s_if(
                    e_binop(e_not(e_call("array_key_exists", vec![e_var("key"), e_var("right")])), BinOp::Or, e_binop(e_cast(CastType::String, e_index(e_var("right"), e_var("key"))), BinOp::StrictNotEq, e_cast(CastType::String, e_var("value")))),
                    vec![
                        s_array_assign("result", e_var("key"), e_var("value")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_var("result")),
        ])
        .build()
}

/// `__elephc_array_intersect_assoc_gradual` — transcribed from the PHP form.
fn decl_fn_elephc_array_intersect_assoc_gradual() -> Stmt {
    function("__elephc_array_intersect_assoc_gradual")
        .param_untyped("left")
        .param_untyped("right")
        .returns(t_array())
        .body(vec![
            s_expr(e_call("__elephc_require_array", vec![e_var("left"), e_str("array_intersect_assoc"), e_int(1), e_str("array")])),
            s_expr(e_call("__elephc_require_array", vec![e_var("right"), e_str("array_intersect_assoc"), e_int(2), e_str("")])),
            s_assign("result", e_array(vec![])),
            s_foreach(e_var("left"), Some("key"), "value", vec![
                s_if(
                    e_binop(e_call("array_key_exists", vec![e_var("key"), e_var("right")]), BinOp::And, e_binop(e_cast(CastType::String, e_index(e_var("right"), e_var("key"))), BinOp::StrictEq, e_cast(CastType::String, e_var("value")))),
                    vec![
                        s_array_assign("result", e_var("key"), e_var("value")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_var("result")),
        ])
        .build()
}

/// `__elephc_array_reverse_preserve_gradual` — transcribed from the PHP form.
fn decl_fn_elephc_array_reverse_preserve_gradual() -> Stmt {
    function("__elephc_array_reverse_preserve_gradual")
        .param_untyped("array")
        .param_untyped("preserve")
        .returns(t_array())
        .body(vec![
            s_expr(e_call("__elephc_require_array", vec![e_var("array"), e_str("array_reverse"), e_int(1), e_str("array")])),
            s_if(
                e_not(e_var("preserve")),
                vec![
                    s_return(e_call("array_reverse", vec![e_var("array")])),
                ],
                vec![],
                None,
            ),
            s_assign("keys", e_call("array_keys", vec![e_var("array")])),
            s_assign("result", e_array(vec![])),
            s_for(Some(s_assign("index", e_binop(e_call("count", vec![e_var("keys")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("index"), BinOp::GtEq, e_int(0))), Some(s_assign("index", e_binop(e_var("index"), BinOp::Sub, e_int(1)))), vec![
                s_array_assign("result", e_index(e_var("keys"), e_var("index")), e_index(e_var("array"), e_index(e_var("keys"), e_var("index")))),
            ]),
            s_return(e_var("result")),
        ])
        .build()
}

/// `__elephc_array_pad_gradual` — transcribed from the PHP form.
fn decl_fn_elephc_array_pad_gradual() -> Stmt {
    function("__elephc_array_pad_gradual")
        .param_untyped("array")
        .param_untyped("length")
        .param_untyped("value")
        .returns(t_array())
        .body(vec![
            s_expr(e_call("__elephc_require_array", vec![e_var("array"), e_str("array_pad"), e_int(1), e_str("array")])),
            s_assign("size", e_call("count", vec![e_var("array")])),
            s_assign("target", e_ternary(e_binop(e_var("length"), BinOp::Lt, e_int(0)), e_neg(e_var("length")), e_var("length"))),
            s_if(
                e_binop(e_var("target"), BinOp::LtEq, e_var("size")),
                vec![
                    s_return(e_var("array")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("target"), BinOp::Sub, e_var("size")), BinOp::Gt, e_int(1048576)),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("array_pad(): Argument #2 ($length) must be less than or equal to 1048576")])),
                ],
                vec![],
                None,
            ),
            s_assign("result", e_array(vec![])),
            s_if(
                e_binop(e_var("length"), BinOp::Lt, e_int(0)),
                vec![
                    s_for(Some(s_assign("index", e_var("size"))), Some(e_binop(e_var("index"), BinOp::Lt, e_var("target"))), Some(s_expr(e_post_inc("index"))), vec![
                        s_array_push("result", e_var("value")),
                    ]),
                ],
                vec![],
                None,
            ),
            s_foreach(e_var("array"), Some("key"), "item", vec![
                s_if(
                    e_call("is_int", vec![e_var("key")]),
                    vec![
                        s_array_push("result", e_var("item")),
                    ],
                    vec![],
                    Some(vec![
                    s_array_assign("result", e_var("key"), e_var("item")),
                ]),
                ),
            ]),
            s_if(
                e_binop(e_var("length"), BinOp::Gt, e_int(0)),
                vec![
                    s_for(Some(s_assign("index", e_var("size"))), Some(e_binop(e_var("index"), BinOp::Lt, e_var("target"))), Some(s_expr(e_post_inc("index"))), vec![
                        s_array_push("result", e_var("value")),
                    ]),
                ],
                vec![],
                None,
            ),
            s_return(e_var("result")),
        ])
        .build()
}

/// `__elephc_array_merge_recursive_spread` — transcribed from the PHP form.
fn decl_fn_elephc_array_merge_recursive_spread() -> Stmt {
    // function __elephc_array_merge_recursive_spread($arrays): array {
    //     $result = []; $position = 0;
    //     foreach ($arrays as $array) {
    //         $position++;
    //         __elephc_require_array($array, 'array_merge_recursive', $position, '');
    //         if ($position === 1) {
    //             foreach ($array as $key => $value) {
    //                 if (is_string($key)) { $result[$key] = $value; } else { $result[] = $value; }
    //             }
    //         } else {
    //             $result = __elephc_array_merge_recursive_into($result, $array);
    //         }
    //     }
    //     return $result;
    // }
    function("__elephc_array_merge_recursive_spread")
        .param_untyped("arrays")
        .returns(t_array())
        .body(vec![
            s_assign("result", e_array(vec![])),
            s_assign("position", e_int(0)),
            s_foreach(e_var("arrays"), None, "array", vec![
                s_expr(e_post_inc("position")),
                s_expr(e_call("__elephc_require_array", vec![e_var("array"), e_str("array_merge_recursive"), e_var("position"), e_str("")])),
                s_if(
                    e_binop(e_var("position"), BinOp::StrictEq, e_int(1)),
                    vec![
                        s_foreach(e_var("array"), Some("key"), "value", vec![
                            s_if(
                                e_call("is_string", vec![e_var("key")]),
                                vec![s_array_assign("result", e_var("key"), e_var("value"))],
                                vec![],
                                Some(vec![s_array_push("result", e_var("value"))]),
                            ),
                        ]),
                    ],
                    vec![],
                    Some(vec![s_assign("result", e_call(MERGE_RECURSIVE_INTO_NAME, vec![e_var("result"), e_var("array")]))]),
                ),
            ]),
            s_return(e_var("result")),
        ])
        .build()
}

/// `__elephc_array_merge_recursive_into` -- php's `php_array_merge_recursive()`: string keys that
/// collide combine into a list (an array value merges recursively), integer keys append.
fn decl_fn_elephc_array_merge_recursive_into() -> Stmt {
    // function __elephc_array_merge_recursive_into($dest, $src) {
    //     foreach ($src as $key => $value) {
    //         if (is_string($key)) {
    //             if (array_key_exists($key, $dest)) {
    //                 $current = $dest[$key];
    //                 if (!is_array($current)) { $current = [$current]; }
    //                 if (is_array($value)) { $current = self($current, $value); } else { $current[] = $value; }
    //                 $dest[$key] = $current;
    //             } else { $dest[$key] = $value; }
    //         } else { $dest[] = $value; }
    //     }
    //     return $dest;
    // }
    function(MERGE_RECURSIVE_INTO_NAME)
        .param_untyped("dest")
        .param_untyped("src")
        .body(vec![
            s_foreach(e_var("src"), Some("key"), "value", vec![
                s_if(
                    e_call("is_string", vec![e_var("key")]),
                    vec![
                        s_if(
                            e_call("array_key_exists", vec![e_var("key"), e_var("dest")]),
                            vec![
                                s_assign("current", e_index(e_var("dest"), e_var("key"))),
                                s_if(
                                    e_not(e_call("is_array", vec![e_var("current")])),
                                    vec![s_assign("current", e_array(vec![e_var("current")]))],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_call("is_array", vec![e_var("value")]),
                                    vec![s_assign("current", e_call(MERGE_RECURSIVE_INTO_NAME, vec![e_var("current"), e_var("value")]))],
                                    vec![],
                                    Some(vec![s_array_push("current", e_var("value"))]),
                                ),
                                s_array_assign("dest", e_var("key"), e_var("current")),
                            ],
                            vec![],
                            Some(vec![s_array_assign("dest", e_var("key"), e_var("value"))]),
                        ),
                    ],
                    vec![],
                    Some(vec![s_array_push("dest", e_var("value"))]),
                ),
            ]),
            s_return(e_var("dest")),
        ])
        .build()
}

/// `__elephc_array_intersect_spread` — transcribed from the PHP form.
fn decl_fn_elephc_array_intersect_spread() -> Stmt {
    function("__elephc_array_intersect_spread")
        .param_untyped("arrays")
        .returns(t_array())
        .body(vec![
            s_assign("count", e_call("count", vec![e_var("arrays")])),
            s_if(
                e_binop(e_var("count"), BinOp::Lt, e_int(1)),
                vec![
                    s_throw(e_new("ArgumentCountError", vec![e_str("array_intersect() expects at least 1 argument, 0 given")])),
                ],
                vec![],
                None,
            ),
            s_assign("position", e_int(0)),
            s_assign("result", e_array(vec![])),
            s_foreach(e_var("arrays"), None, "array", vec![
                s_expr(e_post_inc("position")),
                s_expr(e_call("__elephc_require_array", vec![e_var("array"), e_str("array_intersect"), e_var("position"), e_ternary(e_binop(e_var("position"), BinOp::StrictEq, e_int(1)), e_str("array"), e_str(""))])),
                s_if(
                    e_binop(e_var("position"), BinOp::StrictEq, e_int(1)),
                    vec![
                        s_assign("result", e_var("array")),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("result", e_call("array_intersect", vec![e_var("result"), e_var("array")])),
                ]),
                ),
            ]),
            s_return(e_var("result")),
        ])
        .build()
}

/// `__elephc_array_map_spread` — transcribed from the PHP form.
fn decl_fn_elephc_array_map_spread() -> Stmt {
    function("__elephc_array_map_spread")
        .param_untyped("args")
        .returns(t_array())
        .body(vec![
            s_assign("count", e_call("count", vec![e_var("args")])),
            s_if(
                e_binop(e_var("count"), BinOp::Lt, e_int(2)),
                vec![
                    s_throw(e_new("ArgumentCountError", vec![e_binop(e_binop(e_str("array_map() expects at least 2 arguments, "), BinOp::Concat, e_var("count")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_assign("callback", e_index(e_var("args"), e_int(0))),
            s_expr(e_call("__elephc_require_array", vec![e_index(e_var("args"), e_int(1)), e_str("array_map"), e_int(2), e_str("array")])),
            s_if(
                e_binop(e_var("count"), BinOp::StrictEq, e_int(2)),
                vec![
                    s_return(e_call("array_map", vec![e_var("callback"), e_index(e_var("args"), e_int(1))])),
                ],
                vec![],
                None,
            ),
            s_assign("columns", e_array(vec![])),
            s_assign("longest", e_int(0)),
            s_for(Some(s_assign("position", e_int(1))), Some(e_binop(e_var("position"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("position"))), vec![
                s_expr(e_call("__elephc_require_array", vec![e_index(e_var("args"), e_var("position")), e_str("array_map"), e_binop(e_var("position"), BinOp::Add, e_int(1)), e_ternary(e_binop(e_var("position"), BinOp::StrictEq, e_int(1)), e_str("array"), e_str(""))])),
                s_assign("values", e_call("array_values", vec![e_index(e_var("args"), e_var("position"))])),
                s_array_push("columns", e_var("values")),
                s_if(
                    e_binop(e_call("count", vec![e_var("values")]), BinOp::Gt, e_var("longest")),
                    vec![
                        s_assign("longest", e_call("count", vec![e_var("values")])),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_assign("result", e_array(vec![])),
            s_for(Some(s_assign("index", e_int(0))), Some(e_binop(e_var("index"), BinOp::Lt, e_var("longest"))), Some(s_expr(e_post_inc("index"))), vec![
                s_assign("row", e_array(vec![])),
                s_foreach(e_var("columns"), None, "values", vec![
                    s_array_push("row", e_ternary(e_binop(e_var("index"), BinOp::Lt, e_call("count", vec![e_var("values")])), e_index(e_var("values"), e_var("index")), e_null())),
                ]),
                s_if(
                    e_binop(e_var("callback"), BinOp::StrictEq, e_null()),
                    vec![
                        s_array_push("result", e_var("row")),
                    ],
                    vec![],
                    Some(vec![
                    s_array_push("result", e_closure_call("callback", vec![e_spread(e_var("row"))])),
                ]),
                ),
            ]),
            s_return(e_var("result")),
        ])
        .build()
}

/// `__elephc_range_gradual` — transcribed from the PHP form.
fn decl_fn_elephc_range_gradual() -> Stmt {
    function("__elephc_range_gradual")
        .param_untyped("start")
        .param_untyped("end")
        .param_untyped("step")
        .returns(t_array())
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("step"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("step"), BinOp::StrictEq, e_float(0.0))),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("range(): Argument #3 ($step) cannot be 0")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_call("is_string", vec![e_var("start")]), BinOp::And, e_call("is_string", vec![e_var("end")])), BinOp::And, e_binop(e_var("start"), BinOp::StrictNotEq, e_str(""))), BinOp::And, e_binop(e_var("end"), BinOp::StrictNotEq, e_str(""))),
                vec![
                    s_assign("bytes", e_binop(e_binop(e_call("strlen", vec![e_var("start")]), BinOp::StrictEq, e_int(1)), BinOp::And, e_binop(e_call("strlen", vec![e_var("end")]), BinOp::StrictEq, e_int(1)))),
                    s_if(
                        e_binop(e_binop(e_not(e_var("bytes")), BinOp::And, e_call("is_numeric", vec![e_var("start")])), BinOp::And, e_call("is_numeric", vec![e_var("end")])),
                        vec![
                            s_assign("start", e_binop(e_var("start"), BinOp::Add, e_int(0))),
                            s_assign("end", e_binop(e_var("end"), BinOp::Add, e_int(0))),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_call("is_float", vec![e_var("step")]), BinOp::Or, e_binop(e_binop(e_call("is_string", vec![e_var("step")]), BinOp::And, e_call("is_numeric", vec![e_var("step")])), BinOp::And, e_call("is_float", vec![e_binop(e_var("step"), BinOp::Add, e_int(0))]))),
                            vec![
                                s_assign("start", e_int(0)),
                                s_assign("end", e_int(0)),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("low", e_call("ord", vec![e_index(e_var("start"), e_int(0))])),
                            s_assign("high", e_call("ord", vec![e_index(e_var("end"), e_int(0))])),
                            s_assign("stride", e_cast(CastType::Int, e_var("step"))),
                            s_if(
                                e_binop(e_var("stride"), BinOp::Lt, e_int(0)),
                                vec![
                                    s_assign("stride", e_neg(e_var("stride"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("result", e_array(vec![])),
                            s_if(
                                e_binop(e_var("low"), BinOp::StrictEq, e_var("high")),
                                vec![
                                    s_array_push("result", e_call("chr", vec![e_var("low")])),
                                    s_return(e_var("result")),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("span", e_ternary(e_binop(e_var("low"), BinOp::Gt, e_var("high")), e_binop(e_var("low"), BinOp::Sub, e_var("high")), e_binop(e_var("high"), BinOp::Sub, e_var("low")))),
                            s_if(
                                e_binop(e_var("stride"), BinOp::Gt, e_var("span")),
                                vec![
                                    s_throw(e_new("ValueError", vec![e_str("range(): Argument #3 ($step) must be less than the range spanned by argument #1 ($start) and argument #2 ($end)")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("low"), BinOp::Gt, e_var("high")),
                                vec![
                                    s_for(Some(s_assign("code", e_var("low"))), Some(e_binop(e_var("code"), BinOp::GtEq, e_var("high"))), Some(s_assign("code", e_binop(e_var("code"), BinOp::Sub, e_var("stride")))), vec![
                                        s_array_push("result", e_call("chr", vec![e_var("code")])),
                                    ]),
                                ],
                                vec![],
                                Some(vec![
                                s_for(Some(s_assign("code", e_var("low"))), Some(e_binop(e_var("code"), BinOp::LtEq, e_var("high"))), Some(s_assign("code", e_binop(e_var("code"), BinOp::Add, e_var("stride")))), vec![
                                    s_array_push("result", e_call("chr", vec![e_var("code")])),
                                ]),
                            ]),
                            ),
                            s_return(e_var("result")),
                        ]),
                        ),
                    ]),
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("is_string", vec![e_var("step")]), BinOp::And, e_call("is_numeric", vec![e_var("step")])),
                vec![
                    s_assign("step", e_binop(e_var("step"), BinOp::Add, e_int(0))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("is_string", vec![e_var("start")]), BinOp::And, e_call("is_numeric", vec![e_var("start")])),
                vec![
                    s_assign("start", e_binop(e_var("start"), BinOp::Add, e_int(0))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("is_string", vec![e_var("end")]), BinOp::And, e_call("is_numeric", vec![e_var("end")])),
                vec![
                    s_assign("end", e_binop(e_var("end"), BinOp::Add, e_int(0))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_call("is_float", vec![e_var("start")]), BinOp::Or, e_call("is_float", vec![e_var("end")])), BinOp::Or, e_call("is_float", vec![e_var("step")])),
                vec![
                    s_return(e_call(RANGE_FLOATS_NAME, vec![e_cast(CastType::Float, e_var("start")), e_cast(CastType::Float, e_var("end")), e_cast(CastType::Float, e_var("step"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("range", vec![e_cast(CastType::Int, e_var("start")), e_cast(CastType::Int, e_var("end")), e_cast(CastType::Int, e_var("step"))])),
        ])
        .build()
}

/// `__elephc_range_floats` -- php's double range: element `i` is `start +/- i * step`, for as long
/// as it stays inside the bounds. The native `range()` only walks integers and literal bytes.
fn decl_fn_elephc_range_floats() -> Stmt {
    // function __elephc_range_floats(float $start, float $end, float $step): array {
    //     if ($step < 0.0) { $step = -$step; }
    //     $span = $start <= $end ? $end - $start : $start - $end;
    //     if ($span > 0.0 && $step > $span) { throw new ValueError('range(): Argument #3 ($step) must be less than the range spanned by argument #1 ($start) and argument #2 ($end)'); }
    //     $result = [];
    //     $size = (int) ($span / $step + 1.0);
    //     if ($start <= $end) {
    //         for ($i = 0; $i < $size; $i++) { $element = $start + $i * $step; if ($element > $end) { break; } $result[] = $element; }
    //     } else {
    //         for ($i = 0; $i < $size; $i++) { $element = $start - $i * $step; if ($element < $end) { break; } $result[] = $element; }
    //     }
    //     return $result;
    // }
    function(RANGE_FLOATS_NAME)
        .param_untyped("start")
        .param_untyped("end")
        .param_untyped("step")
        .returns(t_array())
        .body(vec![
            s_if(
                e_binop(e_var("step"), BinOp::Lt, e_float(0.0)),
                vec![s_assign("step", e_neg(e_var("step")))],
                vec![],
                None,
            ),
            s_assign("span", e_ternary(
                e_binop(e_var("start"), BinOp::LtEq, e_var("end")),
                e_binop(e_var("end"), BinOp::Sub, e_var("start")),
                e_binop(e_var("start"), BinOp::Sub, e_var("end")),
            )),
            s_if(
                e_binop(e_binop(e_var("span"), BinOp::Gt, e_float(0.0)), BinOp::And, e_binop(e_var("step"), BinOp::Gt, e_var("span"))),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("range(): Argument #3 ($step) must be less than the range spanned by argument #1 ($start) and argument #2 ($end)")])),
                ],
                vec![],
                None,
            ),
            s_assign("result", e_array(vec![])),
            s_assign("size", e_cast(CastType::Int, e_binop(e_binop(e_var("span"), BinOp::Div, e_var("step")), BinOp::Add, e_float(1.0)))),
            s_if(
                e_binop(e_var("start"), BinOp::LtEq, e_var("end")),
                vec![
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("size"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_assign("element", e_binop(e_var("start"), BinOp::Add, e_binop(e_var("i"), BinOp::Mul, e_var("step")))),
                        s_if(e_binop(e_var("element"), BinOp::Gt, e_var("end")), vec![s_break(1)], vec![], None),
                        s_array_push("result", e_var("element")),
                    ]),
                ],
                vec![],
                Some(vec![
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("size"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_assign("element", e_binop(e_var("start"), BinOp::Sub, e_binop(e_var("i"), BinOp::Mul, e_var("step")))),
                        s_if(e_binop(e_var("element"), BinOp::Lt, e_var("end")), vec![s_break(1)], vec![], None),
                        s_array_push("result", e_var("element")),
                    ]),
                ]),
            ),
            s_return(e_var("result")),
        ])
        .build()
}

/// `__elephc_range_spread` — transcribed from the PHP form.
fn decl_fn_elephc_range_spread() -> Stmt {
    function("__elephc_range_spread")
        .param_untyped("args")
        .returns(t_array())
        .body(vec![
            s_assign("count", e_call("count", vec![e_var("args")])),
            s_if(
                e_binop(e_var("count"), BinOp::Lt, e_int(2)),
                vec![
                    s_throw(e_new("ArgumentCountError", vec![e_binop(e_binop(e_str("range() expects at least 2 arguments, "), BinOp::Concat, e_var("count")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("count"), BinOp::StrictEq, e_int(2)),
                vec![
                    s_return(e_call("__elephc_range_gradual", vec![e_index(e_var("args"), e_int(0)), e_index(e_var("args"), e_int(1)), e_int(1)])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("count"), BinOp::Gt, e_int(3)),
                vec![
                    s_throw(e_new("ArgumentCountError", vec![e_binop(e_binop(e_str("range() expects at most 3 arguments, "), BinOp::Concat, e_var("count")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_range_gradual", vec![e_index(e_var("args"), e_int(0)), e_index(e_var("args"), e_int(1)), e_index(e_var("args"), e_int(2))])),
        ])
        .build()
}

/// `__elephc_iterator_source_error` — transcribed from the PHP form.
fn decl_fn_elephc_iterator_source_error() -> Stmt {
    function("__elephc_iterator_source_error")
        .param_untyped("value")
        .returns(t_array())
        .body(vec![
            s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, "), BinOp::Concat, e_call("get_debug_type", vec![e_var("value")])), BinOp::Concat, e_str(" given"))])),
        ])
        .build()
}

/// `__elephc_signal_set` — transcribed from the PHP form.
fn decl_fn_elephc_signal_set() -> Stmt {
    function("__elephc_signal_set")
        .param_untyped("value")
        .param_untyped("function")
        .param_untyped("position")
        .body(vec![
            s_expr(e_call("__elephc_require_array", vec![e_var("value"), e_var("function"), e_var("position"), e_str("signals")])),
            s_return(e_call("array_values", vec![e_var("value")])),
        ])
        .build()
}

/// Builds the whole surface, one declaration per helper above.
fn helper_declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_fn_elephc_require_array(),
            decl_fn_elephc_sort_flags_compare(),
            decl_fn_elephc_sort_flags_comparator(),
            decl_fn_elephc_array_unique_flags(),
            decl_fn_elephc_array_diff_assoc_gradual(),
            decl_fn_elephc_array_intersect_assoc_gradual(),
            decl_fn_elephc_array_reverse_preserve_gradual(),
            decl_fn_elephc_array_pad_gradual(),
            decl_fn_elephc_array_merge_recursive_into(),
            decl_fn_elephc_array_merge_recursive_spread(),
            decl_fn_elephc_array_intersect_spread(),
            decl_fn_elephc_array_map_spread(),
            decl_fn_elephc_range_floats(),
            decl_fn_elephc_range_gradual(),
            decl_fn_elephc_range_spread(),
            decl_fn_elephc_iterator_source_error(),
            decl_fn_elephc_signal_set(),
        ]
    })
}
