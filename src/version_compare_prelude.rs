//! Purpose:
//! Builds PHP's `version_compare()` as prelude declarations, for programs that name it.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declarations when the
//!   program references `version_compare`.
//!
//! Key details:
//! - A transcription of php-src 8.5's `ext/standard/versioning.c`: `php_canonicalize_version`
//!   (`-`, `_` and `+` become `.`, a `.` is inserted at every digit/non-digit boundary, any other
//!   non-alphanumeric byte becomes `.`, and -- new in 8.5.10 -- a trailing `.` is dropped), the
//!   `dev < alpha = a < beta = b < RC = rc < # < pl = p` prefix table, numeric segments compared
//!   as `strtol` does, and the `#N#` sentinel standing in for "a number" when one side runs out.
//!   php's C string semantics stop at the first NUL byte, so each argument is cut there.
//! - With `$operator` the answer is a bool for exactly `<` `lt` `<=` `le` `>` `gt` `>=` `ge` `==`
//!   `=` `eq` `!=` `<>` `ne`; any other operator is php's ValueError.
//! - The interpreter has its own home (`interpreter::builtins::string::version_compare`):
//!   interpreted code cannot reach a compiler prelude, and Symfony's password hasher factory
//!   asks `version_compare()` from interpreted code on every login.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, CastType, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_cast, e_index, e_int, e_new_fq, e_not, e_null, e_str, e_ternary,
    e_var, function, php_semantics_declarations, s_assign, s_break, s_if, s_return, s_throw, s_while,
    t_nullable, t_union,
};

/// Canonicalizing helper.
const CANONICALIZE_NAME: &str = "__elephc_version_canonicalize";
/// Special-form order helper.
const SPECIAL_NAME: &str = "__elephc_version_special";
/// "Is byte `$i` of `$s` an ASCII digit" helper.
const DIGIT_AT_NAME: &str = "__elephc_version_digit_at";
/// Three-way comparison helper (`php_version_compare`).
const COMPARE_NAME: &str = "__elephc_version_compare";

/// php's special version forms, in the order its prefix table is searched.
const SPECIAL_FORMS: &[(&str, i64)] = &[
    ("dev", 0),
    ("alpha", 1),
    ("a", 1),
    ("beta", 2),
    ("b", 2),
    ("RC", 3),
    ("rc", 3),
    ("#", 4),
    ("pl", 5),
    ("p", 5),
];

/// Returns the `version_compare()` declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![
            decl_fn_canonicalize(),
            decl_fn_special(),
            decl_fn_digit_at(),
            decl_fn_compare(),
            decl_fn_version_compare(),
        ]
    })
}

/// `$left <op> $right` for two expressions.
fn bin(left: Expr, op: BinOp, right: Expr) -> Expr {
    e_binop(left, op, right)
}

/// `$name === value`.
fn is(name: &str, value: Expr) -> Expr {
    bin(e_var(name), BinOp::StrictEq, value)
}

/// `$low <= $name && $name <= $high` over integer byte values.
fn byte_between(name: &str, low: u8, high: u8) -> Expr {
    bin(
        bin(e_var(name), BinOp::GtEq, e_int(i64::from(low))),
        BinOp::And,
        bin(e_var(name), BinOp::LtEq, e_int(i64::from(high))),
    )
}

/// `$name[strlen($name) - 1]`, the last byte of a non-empty string.
fn last_byte(name: &str) -> Expr {
    e_index(
        e_var(name),
        bin(e_call("strlen", vec![e_var(name)]), BinOp::Sub, e_int(1)),
    )
}

/// `if ($last !== '.') { $out .= '.'; }`, php's "insert one separator" step.
fn separate() -> Stmt {
    s_if(
        bin(e_var("last"), BinOp::StrictNotEq, e_str(".")),
        vec![append_out(e_str("."))],
        vec![],
        None,
    )
}

/// `$out .= value`.
fn append_out(value: Expr) -> Stmt {
    s_assign("out", bin(e_var("out"), BinOp::Concat, value))
}

/// `__elephc_version_canonicalize(string $version): string` -- `php_canonicalize_version`.
fn decl_fn_canonicalize() -> Stmt {
    let byte = e_index(e_var("version"), e_var("i"));
    let lp_non_digit = bin(e_not(e_var("lpDigit")), BinOp::And, e_not(is("lp", e_int(46))));
    let c_non_digit = bin(e_not(e_var("cDigit")), BinOp::And, e_not(is("c", e_int(46))));
    let boundary = bin(
        bin(lp_non_digit, BinOp::And, e_var("cDigit")),
        BinOp::Or,
        bin(e_var("lpDigit"), BinOp::And, c_non_digit),
    );
    let special = bin(
        bin(is("c", e_int(45)), BinOp::Or, is("c", e_int(95))),
        BinOp::Or,
        is("c", e_int(43)),
    );
    let loop_body = vec![
        s_assign("c", e_call("ord", vec![byte.clone()])),
        s_assign("last", last_byte("out")),
        s_assign("cDigit", byte_between("c", b'0', b'9')),
        s_assign(
            "cAlpha",
            bin(byte_between("c", b'A', b'Z'), BinOp::Or, byte_between("c", b'a', b'z')),
        ),
        s_assign("lpDigit", byte_between("lp", b'0', b'9')),
        s_if(
            special,
            vec![separate()],
            vec![
                (boundary, vec![separate(), append_out(byte.clone())]),
                (
                    bin(e_not(e_var("cDigit")), BinOp::And, e_not(e_var("cAlpha"))),
                    vec![separate()],
                ),
            ],
            Some(vec![append_out(byte)]),
        ),
        s_assign("lp", e_var("c")),
        s_assign("i", bin(e_var("i"), BinOp::Add, e_int(1))),
    ];
    let body = vec![
        s_assign("len", e_call("strlen", vec![e_var("version")])),
        s_if(is("len", e_int(0)), vec![s_return(e_str(""))], vec![], None),
        s_assign("out", e_index(e_var("version"), e_int(0))),
        s_assign("lp", e_call("ord", vec![e_index(e_var("version"), e_int(0))])),
        s_assign("i", e_int(1)),
        s_while(bin(e_var("i"), BinOp::Lt, e_var("len")), loop_body),
        // php 8.5.10: an empty last component (a trailing `.`) is dropped.
        s_if(
            bin(last_byte("out"), BinOp::StrictEq, e_str(".")),
            vec![s_assign("out", e_call("substr", vec![e_var("out"), e_int(0), e_int(-1)]))],
            vec![],
            None,
        ),
        s_return(e_var("out")),
    ];
    function(CANONICALIZE_NAME)
        .param("version", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(body)
        .build()
}

/// `__elephc_version_special(string $form): int` -- the order of the first matching prefix.
fn decl_fn_special() -> Stmt {
    let mut body: Vec<Stmt> = SPECIAL_FORMS
        .iter()
        .map(|(name, order)| {
            s_if(
                bin(
                    e_call(
                        "substr",
                        vec![e_var("form"), e_int(0), e_int(name.len() as i64)],
                    ),
                    BinOp::StrictEq,
                    e_str(name),
                ),
                vec![s_return(e_int(*order))],
                vec![],
                None,
            )
        })
        .collect();
    body.push(s_return(e_int(-1)));
    function(SPECIAL_NAME)
        .param("form", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(body)
        .build()
}

/// `__elephc_version_digit_at(string $s, int $i): bool`.
fn decl_fn_digit_at() -> Stmt {
    function(DIGIT_AT_NAME)
        .param("s", TypeExpr::Str)
        .param("i", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                bin(e_var("i"), BinOp::GtEq, e_call("strlen", vec![e_var("s")])),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_assign("c", e_call("ord", vec![e_index(e_var("s"), e_var("i"))])),
            s_return(byte_between("c", b'0', b'9')),
        ])
        .build()
}

/// `__elephc_version_special($a) <=> __elephc_version_special($b)`.
fn special_cmp(left: Expr, right: Expr) -> Expr {
    bin(
        e_call(SPECIAL_NAME, vec![left]),
        BinOp::Spaceship,
        e_call(SPECIAL_NAME, vec![right]),
    )
}

/// `$n = ($f = strpos($v, '.', $p)) === false ? -1 : $f` for one side.
fn next_dot(side: &str) -> Vec<Stmt> {
    let found = format!("f{side}");
    vec![
        s_assign(
            &found,
            e_call("strpos", vec![e_var(&format!("v{side}")), e_str("."), e_var(&format!("p{side}"))]),
        ),
        s_assign(
            &format!("n{side}"),
            e_ternary(
                bin(e_var(&found), BinOp::StrictEq, e_bool(false)),
                e_int(-1),
                e_var(&found),
            ),
        ),
    ]
}

/// `$s = $n >= 0 ? substr($v, $p, $n - $p) : substr($v, $p)` for one side.
fn segment(side: &str) -> Stmt {
    let v = || e_var(&format!("v{side}"));
    let p = || e_var(&format!("p{side}"));
    let n = || e_var(&format!("n{side}"));
    s_assign(
        &format!("s{side}"),
        e_ternary(
            bin(n(), BinOp::GtEq, e_int(0)),
            e_call("substr", vec![v(), p(), bin(n(), BinOp::Sub, p())]),
            e_call("substr", vec![v(), p()]),
        ),
    )
}

/// `$v = $x[0] === '#' ? $x : __elephc_version_canonicalize($x)` for one side.
fn canonical_side(side: &str, original: &str) -> Stmt {
    s_assign(
        &format!("v{side}"),
        e_ternary(
            bin(e_index(e_var(original), e_int(0)), BinOp::StrictEq, e_str("#")),
            e_var(original),
            e_call(CANONICALIZE_NAME, vec![e_var(original)]),
        ),
    )
}

/// `__elephc_version_compare(string $a, string $b): int` -- `php_version_compare`.
fn decl_fn_compare() -> Stmt {
    let empty_a = is("a", e_str(""));
    let empty_b = is("b", e_str(""));
    let early = s_if(
        bin(empty_a.clone(), BinOp::Or, empty_b.clone()),
        vec![
            s_if(bin(empty_a, BinOp::And, empty_b), vec![s_return(e_int(0))], vec![], None),
            s_return(e_ternary(
                bin(e_var("a"), BinOp::StrictNotEq, e_str("")),
                e_int(1),
                e_int(-1),
            )),
        ],
        vec![],
        None,
    );
    let mut loop_body = next_dot("1");
    loop_body.extend(next_dot("2"));
    loop_body.push(segment("1"));
    loop_body.push(segment("2"));
    loop_body.push(s_assign("d1", e_call(DIGIT_AT_NAME, vec![e_var("s1"), e_int(0)])));
    loop_body.push(s_assign("d2", e_call(DIGIT_AT_NAME, vec![e_var("s2"), e_int(0)])));
    loop_body.push(s_if(
        bin(e_var("d1"), BinOp::And, e_var("d2")),
        vec![s_assign(
            "compare",
            bin(
                e_cast(CastType::Int, e_var("s1")),
                BinOp::Spaceship,
                e_cast(CastType::Int, e_var("s2")),
            ),
        )],
        vec![
            (
                bin(e_not(e_var("d1")), BinOp::And, e_not(e_var("d2"))),
                vec![s_assign("compare", special_cmp(e_var("s1"), e_var("s2")))],
            ),
            (
                e_var("d1"),
                vec![s_assign("compare", special_cmp(e_str("#N#"), e_var("s2")))],
            ),
        ],
        Some(vec![s_assign("compare", special_cmp(e_var("s1"), e_str("#N#")))]),
    ));
    loop_body.push(s_if(
        bin(e_var("compare"), BinOp::StrictNotEq, e_int(0)),
        vec![s_break(1)],
        vec![],
        None,
    ));
    for side in ["1", "2"] {
        let n = format!("n{side}");
        loop_body.push(s_if(
            bin(e_var(&n), BinOp::GtEq, e_int(0)),
            vec![s_assign(&format!("p{side}"), bin(e_var(&n), BinOp::Add, e_int(1)))],
            vec![],
            None,
        ));
    }
    let condition = bin(
        bin(
            bin(e_var("p1"), BinOp::Lt, e_var("l1")),
            BinOp::And,
            bin(e_var("p2"), BinOp::Lt, e_var("l2")),
        ),
        BinOp::And,
        bin(
            bin(e_var("n1"), BinOp::GtEq, e_int(0)),
            BinOp::And,
            bin(e_var("n2"), BinOp::GtEq, e_int(0)),
        ),
    );
    let rest = |side: &str| e_call("substr", vec![e_var(&format!("v{side}")), e_var(&format!("p{side}"))]);
    let tail = s_if(
        is("compare", e_int(0)),
        vec![s_if(
            bin(e_var("n1"), BinOp::GtEq, e_int(0)),
            vec![s_assign(
                "compare",
                e_ternary(
                    e_call(DIGIT_AT_NAME, vec![e_var("v1"), e_var("p1")]),
                    e_int(1),
                    e_call(COMPARE_NAME, vec![rest("1"), e_str("#N#")]),
                ),
            )],
            vec![(
                bin(e_var("n2"), BinOp::GtEq, e_int(0)),
                vec![s_assign(
                    "compare",
                    e_ternary(
                        e_call(DIGIT_AT_NAME, vec![e_var("v2"), e_var("p2")]),
                        e_int(-1),
                        e_call(COMPARE_NAME, vec![e_str("#N#"), rest("2")]),
                    ),
                )],
            )],
            None,
        )],
        vec![],
        None,
    );
    let body = vec![
        early,
        canonical_side("1", "a"),
        canonical_side("2", "b"),
        s_assign("l1", e_call("strlen", vec![e_var("v1")])),
        s_assign("l2", e_call("strlen", vec![e_var("v2")])),
        s_assign("p1", e_int(0)),
        s_assign("p2", e_int(0)),
        s_assign("n1", e_int(0)),
        s_assign("n2", e_int(0)),
        s_assign("compare", e_int(0)),
        s_while(condition, loop_body),
        tail,
        s_return(e_var("compare")),
    ];
    function(COMPARE_NAME)
        .param("a", TypeExpr::Str)
        .param("b", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(body)
        .build()
}

/// `$name = substr($name, 0, $nul)` when `$name` holds a NUL byte (C string semantics).
fn cut_at_nul(name: &str) -> Vec<Stmt> {
    let nul = format!("{name}Nul");
    vec![
        s_assign(&nul, e_call("strpos", vec![e_var(name), e_str("\0")])),
        s_if(
            bin(e_var(&nul), BinOp::StrictNotEq, e_bool(false)),
            vec![s_assign(name, e_call("substr", vec![e_var(name), e_int(0), e_var(&nul)]))],
            vec![],
            None,
        ),
    ]
}

/// `$operator === a || $operator === b ...`.
fn operator_is(spellings: &[&str]) -> Expr {
    let mut spellings = spellings.iter();
    let first = is("operator", e_str(spellings.next().expect("at least one spelling")));
    spellings.fold(first, |acc, spelling| bin(acc, BinOp::Or, is("operator", e_str(spelling))))
}

/// The PHP-visible `version_compare(string $version1, string $version2,
/// ?string $operator = null): int|bool`.
fn decl_fn_version_compare() -> Stmt {
    let mut body = cut_at_nul("version1");
    body.extend(cut_at_nul("version2"));
    body.push(s_assign(
        "compare",
        e_call(COMPARE_NAME, vec![e_var("version1"), e_var("version2")]),
    ));
    body.push(s_if(is("operator", e_null()), vec![s_return(e_var("compare"))], vec![], None));
    let arms: [(&[&str], Expr); 6] = [
        (&["<", "lt"], is("compare", e_int(-1))),
        (&["<=", "le"], bin(e_var("compare"), BinOp::StrictNotEq, e_int(1))),
        (&[">", "gt"], is("compare", e_int(1))),
        (&[">=", "ge"], bin(e_var("compare"), BinOp::StrictNotEq, e_int(-1))),
        (&["==", "=", "eq"], is("compare", e_int(0))),
        (&["!=", "<>", "ne"], bin(e_var("compare"), BinOp::StrictNotEq, e_int(0))),
    ];
    for (spellings, answer) in arms {
        body.push(s_if(operator_is(spellings), vec![s_return(answer)], vec![], None));
    }
    body.push(s_throw(e_new_fq(
        "ValueError",
        vec![e_str(
            "version_compare(): Argument #3 ($operator) must be a valid comparison operator",
        )],
    )));
    function("version_compare")
        .param("version1", TypeExpr::Str)
        .param("version2", TypeExpr::Str)
        .param_default("operator", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::Bool]))
        .body(body)
        .build()
}
