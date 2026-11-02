//! Purpose:
//! Builds the four-argument `str_replace()` -- the by-reference `$count` form -- as a prelude
//! declaration.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program references `str_replace`.
//! - `crate::ir_lower::expr::compat_preludes`, which routes a four-argument call to it.
//!
//! Key details:
//! - The native helper replaces without counting. php counts what it replaces search by search,
//!   each later search running over the text the earlier ones produced, which is exactly
//!   `substr_count()` of a needle over the current text followed by its own replacement:
//!   both scan left to right without overlap. An empty search is skipped, as php skips it.
//! - An array `$search` takes its replacements from an array `$replace` in ORDER, not by key,
//!   and an exhausted `$replace` replaces with `""`; a string `$replace` serves every search.
//! - An array `$subject` is replaced entry by entry with its keys kept, the counts summed.
//! - Symfony's `cache:clear` rewrites every warmed cache file with
//!   `str_replace($search, $replace, $content, $count)` and writes only when `$count` is set;
//!   without this form the command could not be compiled.
//! - A THREE-argument call whose search or replace is gradual (a `mixed` value, a hash, an array
//!   of non-string elements) comes here too: the native helper only walks `array<string>`, and
//!   refused Twig's `str_replace($this->openingBrackets, $this->closingBrackets, $expect)` over
//!   untyped array properties. `$count` is optional for that reason.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, CastType, Expr, Program, Stmt};
use crate::synthetic_class::{
    e_array, e_binop, e_call, e_cast, e_index, e_int, e_null, e_null_coalesce, e_post_inc, e_str,
    e_ternary, e_var, function, php_semantics_declarations, s_array_assign, s_assign, s_continue,
    s_expr, s_foreach, s_if, s_return,
};

/// Reserved helper the four-argument `str_replace()` call is routed to.
pub(crate) const STR_REPLACE_COUNT_NAME: &str = "__elephc_str_replace_count";

/// Returns the helper declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![decl_fn_str_replace_count()])
}

/// `(string) $name`.
fn as_string(name: &str) -> Expr {
    e_cast(CastType::String, e_var(name))
}

/// `$count += substr_count($text, $needle); $text = str_replace($needle, <with>, $text);`
fn count_then_replace(with: Expr) -> Vec<Stmt> {
    vec![
        s_assign(
            "count",
            e_binop(
                e_var("count"),
                BinOp::Add,
                e_call("substr_count", vec![e_var("text"), e_var("needle")]),
            ),
        ),
        s_assign(
            "text",
            e_call("str_replace", vec![e_var("needle"), with, e_var("text")]),
        ),
    ]
}

/// Builds the declaration.
fn decl_fn_str_replace_count() -> Stmt {
    let subject_array = s_if(
        e_call("is_array", vec![e_var("subject")]),
        vec![
            s_assign("out", e_array(vec![])),
            s_foreach(
                e_var("subject"),
                Some("key"),
                "item",
                vec![
                    s_assign("entry_count", e_int(0)),
                    s_array_assign(
                        "out",
                        e_var("key"),
                        e_call(
                            STR_REPLACE_COUNT_NAME,
                            vec![
                                e_var("search"),
                                e_var("replace"),
                                e_var("item"),
                                e_var("entry_count"),
                            ],
                        ),
                    ),
                    s_assign(
                        "count",
                        e_binop(e_var("count"), BinOp::Add, e_var("entry_count")),
                    ),
                ],
            ),
            s_return(e_var("out")),
        ],
        vec![],
        None,
    );
    let mut single_search = vec![s_assign("needle", as_string("search"))];
    single_search.push(s_if(
        e_binop(e_var("needle"), BinOp::StrictEq, e_str("")),
        vec![s_return(e_var("text"))],
        vec![],
        None,
    ));
    single_search.extend(count_then_replace(as_string("replace")));
    single_search.push(s_return(e_var("text")));
    let mut each_search = vec![
        s_assign("needle", as_string("candidate")),
        s_assign(
            "with",
            e_ternary(
                e_binop(e_var("replacements"), BinOp::StrictEq, e_null()),
                as_string("replace"),
                e_cast(
                    CastType::String,
                    e_null_coalesce(
                        e_index(e_var("replacements"), e_var("position")),
                        e_str(""),
                    ),
                ),
            ),
        ),
        s_expr(e_post_inc("position")),
        s_if(
            e_binop(e_var("needle"), BinOp::StrictEq, e_str("")),
            vec![s_continue(1)],
            vec![],
            None,
        ),
    ];
    each_search.extend(count_then_replace(e_var("with")));
    let body = vec![
        s_assign("count", e_int(0)),
        subject_array,
        s_assign("text", as_string("subject")),
        s_if(
            e_binop(
                e_call("is_array", vec![e_var("search")]),
                BinOp::StrictEq,
                crate::synthetic_class::e_bool(false),
            ),
            single_search,
            vec![],
            None,
        ),
        s_assign(
            "replacements",
            e_ternary(
                e_call("is_array", vec![e_var("replace")]),
                e_call("array_values", vec![e_var("replace")]),
                e_null(),
            ),
        ),
        s_assign("position", e_int(0)),
        s_foreach(e_var("search"), None, "candidate", each_search),
        s_return(e_var("text")),
    ];
    function(STR_REPLACE_COUNT_NAME)
        .param_untyped("search")
        .param_untyped("replace")
        .param_untyped("subject")
        .param_by_ref_default("count", None, e_null())
        .body(body)
        .build()
}
