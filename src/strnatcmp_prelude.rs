//! Purpose:
//! Builds PHP's natural-order string comparison — `strnatcmp()` and `strnatcasecmp()` — as
//! prelude declarations, for programs that name either one.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends these declarations when the
//!   program references either function.
//!
//! Key details:
//! - The backend has NO natural-order comparator to expose. `natsort()`/`natcasesort()` are
//!   emitted as a tail-jump to the ascending integer sort, which is only correct for the
//!   integer payloads they were written for, so there is nothing for a `strnatcmp()` lowering
//!   to call. Writing the rule once here gives the compiled and the interpreted side the same
//!   answer, and costs no new runtime symbol on either architecture.
//! - The declarations are BUILT, never parsed: elephc does not parse PHP source outside tests.
//! - This is a transcription of php-src's `string_natural_compare_func`, not of the natural-order
//!   algorithm "in spirit". The three places where the obvious reading is wrong, each caught by
//!   differential testing against `php -n` 8.5.10:
//!   * LEADING ZEROS are skipped only at the very START of each string (`leading` is cleared
//!     after the first iteration), so `strnatcmp('a02', 'a2')` is -1, not 0.
//!   * A digit run where EITHER side starts with `'0'` is "fractional" and compares
//!     LEFT-aligned — first differing digit wins outright — while any other run compares
//!     RIGHT-aligned, where the longer run wins and a difference is only remembered as a bias.
//!   * Whitespace is skipped before EVERY comparison, not only a leading one, which is what
//!     makes `strnatcmp('pic 5', 'pic05')` compare `'5'` against `'0'` and answer 1.
//! - Reading past either end behaves like php-src reading the NUL terminator: the helpers below
//!   yield the empty string, which is neither a digit nor a space and sorts below every byte.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_assign, e_binop, e_bool, e_call, e_index, e_int, e_not, e_post_inc, e_str, e_ternary, e_var,
    function, php_semantics_declarations, s_assign, s_break, s_continue, s_expr, s_if, s_return, s_while,
};

/// Shared helper both PHP-visible wrappers call.
const STRNATCMP_NAME: &str = "__elephc_strnatcmp";

/// Single-byte digit predicate shared by the scan and the run comparison.
const IS_DIGIT_NAME: &str = "__elephc_strnat_is_digit";

/// Single-byte whitespace predicate matching C's `isspace`.
const IS_SPACE_NAME: &str = "__elephc_strnat_is_space";

/// Returns the natural-order comparison declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![
            decl_fn_is_digit(),
            decl_fn_is_space(),
            decl_fn_elephc_strnatcmp(),
            decl_fn_strnatcmp(),
            decl_fn_strnatcasecmp(),
        ]
    })
}

/// Builds `$cursor < $length`.
fn in_bounds(cursor: &str, length: &str) -> Expr {
    e_binop(e_var(cursor), BinOp::Lt, e_var(length))
}

/// Builds `$cursor >= $length`.
fn past_end(cursor: &str, length: &str) -> Expr {
    e_binop(e_var(cursor), BinOp::GtEq, e_var(length))
}

/// Builds the byte at `$string[$cursor]`, or `''` once the cursor is past the end.
///
/// php-src reads the NUL terminator there and keeps going; the empty string is the faithful
/// stand-in, because it is neither a digit nor a space and compares below every real byte.
fn byte_or_end(string: &str, cursor: &str, length: &str) -> Expr {
    e_ternary(
        in_bounds(cursor, length),
        e_index(e_var(string), e_var(cursor)),
        e_str(""),
    )
}

/// Builds `__elephc_strnat_is_digit($value)`.
fn is_digit(value: Expr) -> Expr {
    e_call(IS_DIGIT_NAME, vec![value])
}

/// Builds `__elephc_strnat_is_space($value)`.
fn is_space(value: Expr) -> Expr {
    e_call(IS_SPACE_NAME, vec![value])
}

/// Builds `return $left < $right ? -1 : 1;`, PHP's normalized ordering answer.
fn ordered_return(left: Expr, right: Expr) -> Stmt {
    s_return(e_ternary(
        e_binop(left, BinOp::Lt, right),
        e_int(-1),
        e_int(1),
    ))
}

/// Returns -1/0/1 for whichever side still has characters left, the check php-src repeats after
/// every comparison that did not decide the order.
fn end_of_input_returns() -> Vec<Stmt> {
    vec![
        s_if(
            e_binop(past_end("i", "lenA"), BinOp::And, past_end("j", "lenB")),
            vec![s_return(e_int(0))],
            vec![],
            None,
        ),
        s_if(
            past_end("i", "lenA"),
            vec![s_return(e_int(-1))],
            vec![],
            None,
        ),
        s_if(
            past_end("j", "lenB"),
            vec![s_return(e_int(1))],
            vec![],
            None,
        ),
    ]
}

/// Skips one side's leading zeros, but only while a digit follows and only on the first pass.
fn skip_leading_zeros(string: &str, cursor: &str, length: &str, current: &str) -> Stmt {
    let next = e_binop(e_var(cursor), BinOp::Add, e_int(1));
    let followed_by_digit = e_binop(
        e_binop(next.clone(), BinOp::Lt, e_var(length)),
        BinOp::And,
        is_digit(e_index(e_var(string), next)),
    );
    s_while(
        e_binop(
            e_binop(
                e_var("leading"),
                BinOp::And,
                e_binop(e_var(current), BinOp::StrictEq, e_str("0")),
            ),
            BinOp::And,
            followed_by_digit,
        ),
        vec![
            s_expr(e_post_inc(cursor)),
            s_assign(current, e_index(e_var(string), e_var(cursor))),
        ],
    )
}

/// Skips one side's whitespace run, refreshing the byte it holds.
fn skip_spaces(string: &str, cursor: &str, length: &str, current: &str) -> Stmt {
    s_while(
        is_space(e_var(current)),
        vec![
            s_expr(e_post_inc(cursor)),
            s_assign(current, byte_or_end(string, cursor, length)),
        ],
    )
}

/// Builds the digit-run comparison: php-src's `compare_left` and `compare_right` in one loop.
///
/// `$fractional` selects between them because they differ in exactly two places — whether a
/// digit difference returns immediately or is only remembered as `$bias`, and what an exhausted
/// pair of runs answers.
fn compare_digit_runs() -> Vec<Stmt> {
    vec![
        s_assign(
            "fractional",
            e_binop(
                e_binop(e_var("ca"), BinOp::StrictEq, e_str("0")),
                BinOp::Or,
                e_binop(e_var("cb"), BinOp::StrictEq, e_str("0")),
            ),
        ),
        s_assign("bias", e_int(0)),
        s_assign("result", e_int(0)),
        s_while(
            e_bool(true),
            vec![
                s_assign("da", byte_or_end("a", "i", "lenA")),
                s_assign("db", byte_or_end("b", "j", "lenB")),
                s_assign("digitA", is_digit(e_var("da"))),
                s_assign("digitB", is_digit(e_var("db"))),
                // Both runs ended together: right-aligned comparison answers with the bias it
                // accumulated, left-aligned comparison answers "equal".
                s_if(
                    e_binop(e_not(e_var("digitA")), BinOp::And, e_not(e_var("digitB"))),
                    vec![
                        s_assign(
                            "result",
                            e_ternary(e_var("fractional"), e_int(0), e_var("bias")),
                        ),
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                // One run ended first, which makes the other the longer number.
                s_if(
                    e_not(e_var("digitA")),
                    vec![s_assign("result", e_int(-1)), s_break(1)],
                    vec![],
                    None,
                ),
                s_if(
                    e_not(e_var("digitB")),
                    vec![s_assign("result", e_int(1)), s_break(1)],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("da"), BinOp::StrictNotEq, e_var("db")),
                    vec![
                        s_if(
                            e_var("fractional"),
                            vec![
                                s_assign(
                                    "result",
                                    e_ternary(
                                        e_binop(e_var("da"), BinOp::Lt, e_var("db")),
                                        e_int(-1),
                                        e_int(1),
                                    ),
                                ),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("bias"), BinOp::StrictEq, e_int(0)),
                            vec![s_assign(
                                "bias",
                                e_ternary(
                                    e_binop(e_var("da"), BinOp::Lt, e_var("db")),
                                    e_int(-1),
                                    e_int(1),
                                ),
                            )],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
                s_expr(e_post_inc("i")),
                s_expr(e_post_inc("j")),
            ],
        ),
        s_if(
            e_binop(e_var("result"), BinOp::StrictNotEq, e_int(0)),
            vec![s_return(e_var("result"))],
            vec![],
            None,
        ),
    ]
}

/// Builds one iteration of the outer scan.
fn compare_step() -> Vec<Stmt> {
    let mut body = vec![
        s_assign("ca", byte_or_end("a", "i", "lenA")),
        s_assign("cb", byte_or_end("b", "j", "lenB")),
        skip_leading_zeros("a", "i", "lenA", "ca"),
        skip_leading_zeros("b", "j", "lenB", "cb"),
        s_assign("leading", e_bool(false)),
        skip_spaces("a", "i", "lenA", "ca"),
        skip_spaces("b", "j", "lenB", "cb"),
    ];

    let mut digit_branch = compare_digit_runs();
    digit_branch.extend(end_of_input_returns());
    // Both cursors already sit past their runs, so the scan resumes without the byte-step
    // advance the ordinary branch performs.
    digit_branch.push(s_continue(1));
    body.push(s_if(
        e_binop(is_digit(e_var("ca")), BinOp::And, is_digit(e_var("cb"))),
        digit_branch,
        vec![],
        None,
    ));

    body.push(s_if(
        e_var("foldCase"),
        vec![
            s_expr(e_assign(
                e_var("ca"),
                e_call("strtoupper", vec![e_var("ca")]),
            )),
            s_expr(e_assign(
                e_var("cb"),
                e_call("strtoupper", vec![e_var("cb")]),
            )),
        ],
        vec![],
        None,
    ));
    body.push(s_if(
        e_binop(e_var("ca"), BinOp::StrictNotEq, e_var("cb")),
        vec![ordered_return(e_var("ca"), e_var("cb"))],
        vec![],
        None,
    ));
    body.push(s_expr(e_post_inc("i")));
    body.push(s_expr(e_post_inc("j")));
    body.extend(end_of_input_returns());
    body
}

/// `__elephc_strnat_is_digit($value)` — C's `isdigit` over a single byte.
fn decl_fn_is_digit() -> Stmt {
    function(IS_DIGIT_NAME)
        .param("value", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .returning(e_binop(
            e_binop(
                e_call("strlen", vec![e_var("value")]),
                BinOp::StrictEq,
                e_int(1),
            ),
            BinOp::And,
            e_binop(
                e_binop(e_var("value"), BinOp::GtEq, e_str("0")),
                BinOp::And,
                e_binop(e_var("value"), BinOp::LtEq, e_str("9")),
            ),
        ))
        .build()
}

/// `__elephc_strnat_is_space($value)` — C's `isspace` over a single byte.
fn decl_fn_is_space() -> Stmt {
    let spaces = [" ", "\t", "\n", "\u{b}", "\u{c}", "\r"];
    let mut test: Option<Expr> = None;
    for candidate in spaces {
        let comparison = e_binop(e_var("value"), BinOp::StrictEq, e_str(candidate));
        test = Some(match test {
            Some(previous) => e_binop(previous, BinOp::Or, comparison),
            None => comparison,
        });
    }
    function(IS_SPACE_NAME)
        .param("value", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .returning(test.expect("the whitespace set is not empty"))
        .build()
}

/// `__elephc_strnatcmp($a, $b, $foldCase)` — the shared natural-order comparison.
fn decl_fn_elephc_strnatcmp() -> Stmt {
    function(STRNATCMP_NAME)
        .param("a", TypeExpr::Str)
        .param("b", TypeExpr::Str)
        .param("foldCase", TypeExpr::Bool)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("lenA", e_call("strlen", vec![e_var("a")])),
            s_assign("lenB", e_call("strlen", vec![e_var("b")])),
            // php-src answers an empty operand by length alone, before the scan starts.
            s_if(
                e_binop(
                    e_binop(e_var("lenA"), BinOp::StrictEq, e_int(0)),
                    BinOp::Or,
                    e_binop(e_var("lenB"), BinOp::StrictEq, e_int(0)),
                ),
                vec![
                    s_if(
                        e_binop(e_var("lenA"), BinOp::StrictEq, e_var("lenB")),
                        vec![s_return(e_int(0))],
                        vec![],
                        None,
                    ),
                    ordered_return(e_var("lenA"), e_var("lenB")),
                ],
                vec![],
                None,
            ),
            s_assign("i", e_int(0)),
            s_assign("j", e_int(0)),
            s_assign("leading", e_bool(true)),
            s_while(e_bool(true), compare_step()),
        ])
        .build()
}

/// PHP-visible `strnatcmp()`.
fn decl_fn_strnatcmp() -> Stmt {
    function("strnatcmp")
        .param("string1", TypeExpr::Str)
        .param("string2", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .returning(e_call(
            STRNATCMP_NAME,
            vec![e_var("string1"), e_var("string2"), e_bool(false)],
        ))
        .build()
}

/// PHP-visible `strnatcasecmp()`.
fn decl_fn_strnatcasecmp() -> Stmt {
    function("strnatcasecmp")
        .param("string1", TypeExpr::Str)
        .param("string2", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .returning(e_call(
            STRNATCMP_NAME,
            vec![e_var("string1"), e_var("string2"), e_bool(true)],
        ))
        .build()
}
