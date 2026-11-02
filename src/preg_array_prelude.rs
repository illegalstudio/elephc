//! Purpose:
//! Builds the array and flag forms of `preg_replace()` / `preg_replace_callback()`, and the whole
//! of `preg_filter()` and `preg_replace_callback_array()`, as prelude declarations on top of the
//! native one-pattern, one-subject calls.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declarations when the
//!   program references any of the four functions.
//! - `crate::ir_lower::expr::compat_preludes`, which redirects a `preg_replace()` or
//!   `preg_replace_callback()` call here when an operand may be an array at run time, or when
//!   `$flags` is passed.
//!
//! Key details:
//! - The declarations are BUILT, never parsed: elephc does not parse PHP source outside tests.
//! - The native runtime replaces ONE pattern in ONE string. php's array forms compose that:
//!   patterns apply in iteration order to the evolving subject; an array replacement is taken
//!   POSITIONALLY in iteration order (its keys are ignored) and runs out as `''`; an array
//!   subject keeps its keys and drops every element whose replacement failed.
//! - `$count` totals every replacement made, including those of a subject whose later pattern
//!   failed: php counts them before it discovers the failure. MEASURED on php 8.5:
//!   `preg_replace(['/a/', '/(/'], 'x', ['k' => 'ab', 'j' => 'cd'], -1, $c)` is `[]` with `$c`
//!   `=== 1`.
//! - `preg_filter()` is `preg_replace()` that also drops a subject no pattern matched, and answers
//!   null for a scalar one.
//! - `$flags` (`PREG_OFFSET_CAPTURE`, `PREG_UNMATCHED_AS_NULL`) reach the callback through the
//!   `preg_match_all()` flag helper in SET order, whose rows have exactly the shape php hands the
//!   callback; the replacement string is then stitched from the reported offsets.
//! - Every parameter that takes any value is DECLARED `mixed`. The checker never sees a call the
//!   lowering redirects here, so an untyped parameter would be specialized from the prelude's own
//!   call sites alone: `preg_replace_callback_array()` passes `(string) $regex`, and the body was
//!   then compiled for a string pattern that a redirected call hands an array.
//! - A malformed pattern warns under the native call's name (`preg_replace()`), where php names
//!   the function the program called (`preg_filter()`, `preg_replace_callback_array()`).

use crate::parser::ast::{BinOp, CastType, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_closure_call, e_index, e_int, e_new, e_not,
    e_null, e_null_coalesce, e_str, e_ternary, e_var, function, php_semantics_declarations,
    s_array_assign, s_assign, s_break, s_continue, s_foreach, s_if, s_return, s_throw, t_mixed,
};

/// Reserved helper for `preg_replace()` calls whose pattern, replacement or subject may be an
/// array, and the body of `preg_filter()`.
pub(crate) const PREG_REPLACE_FULL_NAME: &str = "__elephc_preg_replace_full";

/// Reserved helper for `preg_replace_callback()` calls whose pattern or subject may be an array,
/// or that pass `$flags`.
pub(crate) const PREG_REPLACE_CALLBACK_FULL_NAME: &str = "__elephc_preg_replace_callback_full";

/// Applies every pattern to ONE string subject.
const REPLACE_SUBJECT_NAME: &str = "__elephc_preg_replace_subject";

/// Applies every pattern's callback to ONE string subject.
const CALLBACK_SUBJECT_NAME: &str = "__elephc_preg_replace_callback_subject";

/// Applies one pattern's callback to one string subject with `$flags`.
const CALLBACK_FLAGGED_NAME: &str = "__elephc_preg_replace_callback_flagged";

/// Reduces one `[text, offset]` match row to its texts, keeping the group keys.
const CAPTURE_TEXTS_NAME: &str = "__elephc_preg_capture_texts";

const PREG_SET_ORDER: i64 = 2;
const PREG_OFFSET_CAPTURE: i64 = 256;
const PREG_UNMATCHED_AS_NULL: i64 = 512;

/// Returns the declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![
            replace_subject(),
            replace_full(),
            capture_texts(),
            callback_flagged(),
            callback_subject(),
            callback_full(),
            preg_filter(),
            preg_replace_callback_array(),
        ]
    })
}

/// `$name === null`.
fn is_null(name: &str) -> Expr {
    e_binop(e_var(name), BinOp::StrictEq, e_null())
}

/// `$target += $name`, spelled as an assignment.
fn add_to(target: &str, name: &str) -> Stmt {
    s_assign(target, e_binop(e_var(target), BinOp::Add, e_var(name)))
}

/// `$target .= value`, spelled as an assignment.
fn append(target: &str, value: Expr) -> Stmt {
    s_assign(target, e_binop(e_var(target), BinOp::Concat, value))
}

/// `(string) value`.
fn to_string(value: Expr) -> Expr {
    e_cast(CastType::String, value)
}

/// `is_array($name)`.
fn is_array(name: &str) -> Expr {
    e_call("is_array", vec![e_var(name)])
}

/// Walks an array subject through `helper`, keeping each element's key; a scalar subject is the
/// one-element case. `extra` are the helper arguments after `$limit`'s by-reference counter, and
/// `drop_unmatched` is the `preg_filter()` rule.
fn over_subject(helper: &str, second: &str, extra: Vec<Expr>, drop_unmatched: Option<Expr>) -> Vec<Stmt> {
    let call = |subject: Expr| {
        let mut args = vec![e_var("pattern"), e_var(second), to_string(subject), e_var("limit"), e_var("n")];
        args.extend(extra.iter().cloned());
        e_call(helper, args)
    };
    let unmatched = e_binop(e_var("n"), BinOp::StrictEq, e_int(0));
    let (element_dropped, scalar_dropped) = match &drop_unmatched {
        Some(filter) => (
            e_binop(is_null("result"), BinOp::Or, e_binop(filter.clone(), BinOp::And, unmatched.clone())),
            Some(e_binop(filter.clone(), BinOp::And, unmatched)),
        ),
        None => (is_null("result"), None),
    };
    let mut scalar = vec![
        s_assign("n", e_int(0)),
        s_assign("result", call(e_var("subject"))),
        s_assign("count", e_var("n")),
    ];
    if let Some(dropped) = scalar_dropped {
        scalar.push(s_if(dropped, vec![s_return(e_null())], Vec::new(), None));
    }
    scalar.push(s_return(e_var("result")));
    let mut body = vec![
        s_assign("count", e_int(0)),
        s_if(
            is_array("subject"),
            vec![
                s_assign("out", e_array(Vec::new())),
                s_foreach(
                    e_var("subject"),
                    Some("key"),
                    "value",
                    vec![
                        s_assign("n", e_int(0)),
                        s_assign("result", call(e_var("value"))),
                        add_to("count", "n"),
                        s_if(element_dropped, vec![s_continue(1)], Vec::new(), None),
                        s_array_assign("out", e_var("key"), e_var("result")),
                    ],
                ),
                s_return(e_var("out")),
            ],
            Vec::new(),
            None,
        ),
    ];
    body.extend(scalar);
    body
}

/// Builds `__elephc_preg_replace_subject($pattern, $replacement, string $subject, int $limit, &$count)`.
fn replace_subject() -> Stmt {
    function(REPLACE_SUBJECT_NAME)
        .param("pattern", t_mixed())
        .param("replacement", t_mixed())
        .param("subject", TypeExpr::Str)
        .param("limit", TypeExpr::Int)
        .param_by_ref("count", Some(t_mixed()))
        .body(vec![
            s_assign("count", e_int(0)),
            s_if(
                e_not(is_array("pattern")),
                vec![s_return(e_call(
                    "preg_replace",
                    vec![
                        to_string(e_var("pattern")),
                        to_string(e_var("replacement")),
                        e_var("subject"),
                        e_var("limit"),
                        e_var("count"),
                    ],
                ))],
                Vec::new(),
                None,
            ),
            s_assign(
                "replacements",
                e_ternary(
                    is_array("replacement"),
                    e_call("array_values", vec![e_var("replacement")]),
                    e_null(),
                ),
            ),
            s_assign("index", e_int(0)),
            s_assign("result", e_var("subject")),
            s_foreach(
                e_var("pattern"),
                None,
                "regex",
                vec![
                    s_assign(
                        "current",
                        e_ternary(
                            is_null("replacements"),
                            to_string(e_var("replacement")),
                            to_string(e_null_coalesce(
                                e_index(e_var("replacements"), e_var("index")),
                                e_str(""),
                            )),
                        ),
                    ),
                    s_assign("index", e_binop(e_var("index"), BinOp::Add, e_int(1))),
                    s_assign("n", e_int(0)),
                    s_assign(
                        "result",
                        e_call(
                            "preg_replace",
                            vec![
                                to_string(e_var("regex")),
                                e_var("current"),
                                to_string(e_var("result")),
                                e_var("limit"),
                                e_var("n"),
                            ],
                        ),
                    ),
                    add_to("count", "n"),
                    s_if(is_null("result"), vec![s_return(e_null())], Vec::new(), None),
                ],
            ),
            s_return(e_var("result")),
        ])
        .build()
}

/// Builds `__elephc_preg_replace_full($pattern, $replacement, $subject, $limit, &$count, $filter,
/// $function)`.
fn replace_full() -> Stmt {
    let mut body = vec![s_if(
        e_binop(is_array("replacement"), BinOp::And, e_not(is_array("pattern"))),
        vec![s_throw(e_new(
            "TypeError",
            vec![e_binop(
                e_var("function"),
                BinOp::Concat,
                e_str(
                    "(): Argument #1 ($pattern) must be of type array when argument #2 \
                     ($replacement) is an array, string given",
                ),
            )],
        ))],
        Vec::new(),
        None,
    )];
    body.extend(over_subject(
        REPLACE_SUBJECT_NAME,
        "replacement",
        Vec::new(),
        Some(e_var("filter")),
    ));
    function(PREG_REPLACE_FULL_NAME)
        .param("pattern", t_mixed())
        .param("replacement", t_mixed())
        .param("subject", t_mixed())
        .param_default("limit", TypeExpr::Int, e_int(-1))
        .param_by_ref_default("count", Some(t_mixed()), e_null())
        .param_default("filter", TypeExpr::Bool, e_bool(false))
        .param_default("function", TypeExpr::Str, e_str("preg_replace"))
        .body(body)
        .build()
}

/// Builds `__elephc_preg_capture_texts(array $set): array`.
fn capture_texts() -> Stmt {
    function(CAPTURE_TEXTS_NAME)
        .param("set", crate::synthetic_class::t_array())
        .returns(crate::synthetic_class::t_array())
        .body(vec![
            s_assign("texts", e_array(Vec::new())),
            s_foreach(
                e_var("set"),
                Some("group"),
                "capture",
                vec![s_array_assign(
                    "texts",
                    e_var("group"),
                    e_index(e_var("capture"), e_int(0)),
                )],
            ),
            s_return(e_var("texts")),
        ])
        .build()
}

/// Builds `__elephc_preg_replace_callback_flagged(string $pattern, $callback, string $subject,
/// int $limit, &$count, int $flags)`.
fn callback_flagged() -> Stmt {
    let has_offsets = e_binop(
        e_binop(e_var("flags"), BinOp::BitAnd, e_int(PREG_OFFSET_CAPTURE)),
        BinOp::StrictNotEq,
        e_int(0),
    );
    let match_flags = e_binop(
        e_int(PREG_SET_ORDER | PREG_OFFSET_CAPTURE),
        BinOp::BitOr,
        e_binop(e_var("flags"), BinOp::BitAnd, e_int(PREG_UNMATCHED_AS_NULL)),
    );
    function(CALLBACK_FLAGGED_NAME)
        .param("pattern", TypeExpr::Str)
        .param("callback", t_mixed())
        .param("subject", TypeExpr::Str)
        .param("limit", TypeExpr::Int)
        .param_by_ref("count", Some(t_mixed()))
        .param("flags", TypeExpr::Int)
        .body(vec![
            s_assign("count", e_int(0)),
            s_assign("sets", e_array(Vec::new())),
            s_assign(
                "found",
                e_call(
                    "preg_match_all",
                    vec![e_var("pattern"), e_var("subject"), e_var("sets"), match_flags],
                ),
            ),
            s_if(
                e_binop(e_var("found"), BinOp::StrictEq, e_bool(false)),
                vec![s_return(e_null())],
                Vec::new(),
                None,
            ),
            s_assign("out", e_str("")),
            s_assign("position", e_int(0)),
            s_foreach(
                e_var("sets"),
                None,
                "set",
                vec![
                    // A negative limit is no limit; zero replaces nothing.
                    s_if(
                        e_binop(
                            e_binop(e_var("limit"), BinOp::GtEq, e_int(0)),
                            BinOp::And,
                            e_binop(e_var("count"), BinOp::GtEq, e_var("limit")),
                        ),
                        vec![s_break(1)],
                        Vec::new(),
                        None,
                    ),
                    s_assign("whole", e_index(e_var("set"), e_int(0))),
                    s_assign("start", e_index(e_var("whole"), e_int(1))),
                    append(
                        "out",
                        e_call(
                            "substr",
                            vec![
                                e_var("subject"),
                                e_var("position"),
                                e_binop(e_var("start"), BinOp::Sub, e_var("position")),
                            ],
                        ),
                    ),
                    s_if(
                        has_offsets,
                        vec![s_assign("argument", e_var("set"))],
                        Vec::new(),
                        // Built by its own helper: element writes into this local, which the
                        // other branch assigns a `mixed` row, leaked the row they built.
                        Some(vec![s_assign(
                            "argument",
                            e_call(CAPTURE_TEXTS_NAME, vec![e_var("set")]),
                        )]),
                    ),
                    // `$argument + []`, not the bare variable: php hands the callback its matches
                    // BY VALUE, and a bare local reaches an untyped callback parameter through an
                    // invoker reference cell whose converted copy is never released -- eight
                    // blocks per match. A union is an expression, with no cell to lend, and keeps
                    // the group keys; an `(array)` cast of a local already typed `array` folds
                    // back to the local.
                    append(
                        "out",
                        to_string(e_closure_call(
                            "callback",
                            vec![e_binop(e_var("argument"), BinOp::Add, e_array(Vec::new()))],
                        )),
                    ),
                    s_assign(
                        "position",
                        e_binop(
                            e_var("start"),
                            BinOp::Add,
                            e_call("strlen", vec![to_string(e_index(e_var("whole"), e_int(0)))]),
                        ),
                    ),
                    s_assign("count", e_binop(e_var("count"), BinOp::Add, e_int(1))),
                ],
            ),
            append("out", e_call("substr", vec![e_var("subject"), e_var("position")])),
            s_return(e_var("out")),
        ])
        .build()
}

/// Builds `__elephc_preg_replace_callback_subject($pattern, $callback, string $subject,
/// int $limit, &$count, int $flags)`.
fn callback_subject() -> Stmt {
    function(CALLBACK_SUBJECT_NAME)
        .param("pattern", t_mixed())
        .param("callback", t_mixed())
        .param("subject", TypeExpr::Str)
        .param("limit", TypeExpr::Int)
        .param_by_ref("count", Some(t_mixed()))
        .param("flags", TypeExpr::Int)
        .body(vec![
            s_assign("count", e_int(0)),
            s_assign(
                "patterns",
                e_ternary(is_array("pattern"), e_var("pattern"), e_array(vec![e_var("pattern")])),
            ),
            s_assign("result", e_var("subject")),
            s_foreach(
                e_var("patterns"),
                None,
                "regex",
                vec![
                    s_assign("n", e_int(0)),
                    s_if(
                        e_binop(e_var("flags"), BinOp::StrictEq, e_int(0)),
                        vec![s_assign(
                            "result",
                            e_call(
                                "preg_replace_callback",
                                vec![
                                    to_string(e_var("regex")),
                                    e_var("callback"),
                                    to_string(e_var("result")),
                                    e_var("limit"),
                                    e_var("n"),
                                ],
                            ),
                        )],
                        Vec::new(),
                        Some(vec![s_assign(
                            "result",
                            e_call(
                                CALLBACK_FLAGGED_NAME,
                                vec![
                                    to_string(e_var("regex")),
                                    e_var("callback"),
                                    to_string(e_var("result")),
                                    e_var("limit"),
                                    e_var("n"),
                                    e_var("flags"),
                                ],
                            ),
                        )]),
                    ),
                    add_to("count", "n"),
                    s_if(is_null("result"), vec![s_return(e_null())], Vec::new(), None),
                ],
            ),
            s_return(e_var("result")),
        ])
        .build()
}

/// Builds `__elephc_preg_replace_callback_full($pattern, $callback, $subject, $limit, &$count,
/// $flags)`.
fn callback_full() -> Stmt {
    function(PREG_REPLACE_CALLBACK_FULL_NAME)
        .param("pattern", t_mixed())
        .param("callback", t_mixed())
        .param("subject", t_mixed())
        .param_default("limit", TypeExpr::Int, e_int(-1))
        .param_by_ref_default("count", Some(t_mixed()), e_null())
        .param_default("flags", TypeExpr::Int, e_int(0))
        .body(over_subject(CALLBACK_SUBJECT_NAME, "callback", vec![e_var("flags")], None))
        .build()
}

/// Builds php's `preg_filter($pattern, $replacement, $subject, $limit = -1, &$count = null)`.
fn preg_filter() -> Stmt {
    function("preg_filter")
        .param("pattern", t_mixed())
        .param("replacement", t_mixed())
        .param("subject", t_mixed())
        .param_default("limit", TypeExpr::Int, e_int(-1))
        .param_by_ref_default("count", Some(t_mixed()), e_null())
        .body(vec![s_return(e_call(
            PREG_REPLACE_FULL_NAME,
            vec![
                e_var("pattern"),
                e_var("replacement"),
                e_var("subject"),
                e_var("limit"),
                e_var("count"),
                e_bool(true),
                e_str("preg_filter"),
            ],
        ))])
        .build()
}

/// Builds php's `preg_replace_callback_array(array $pattern, $subject, $limit = -1,
/// &$count = null, $flags = 0)`.
fn preg_replace_callback_array() -> Stmt {
    function("preg_replace_callback_array")
        .param("pattern", crate::synthetic_class::t_array())
        .param("subject", t_mixed())
        .param_default("limit", TypeExpr::Int, e_int(-1))
        .param_by_ref_default("count", Some(t_mixed()), e_null())
        .param_default("flags", TypeExpr::Int, e_int(0))
        .body(vec![
            // php validates every callback before it runs any pattern.
            s_foreach(
                e_var("pattern"),
                None,
                "callback",
                vec![s_if(
                    e_not(e_call("is_callable", vec![e_var("callback")])),
                    vec![s_throw(e_new(
                        "TypeError",
                        vec![e_str(
                            "preg_replace_callback_array(): Argument #1 ($pattern) must contain \
                             only valid callbacks",
                        )],
                    ))],
                    Vec::new(),
                    None,
                )],
            ),
            s_assign("count", e_int(0)),
            s_foreach(
                e_var("pattern"),
                Some("regex"),
                "callback",
                vec![
                    s_assign("n", e_int(0)),
                    s_assign(
                        "subject",
                        e_call(
                            PREG_REPLACE_CALLBACK_FULL_NAME,
                            vec![
                                to_string(e_var("regex")),
                                e_var("callback"),
                                e_var("subject"),
                                e_var("limit"),
                                e_var("n"),
                                e_var("flags"),
                            ],
                        ),
                    ),
                    add_to("count", "n"),
                    s_if(is_null("subject"), vec![s_return(e_null())], Vec::new(), None),
                ],
            ),
            s_return(e_var("subject")),
        ])
        .build()
}
