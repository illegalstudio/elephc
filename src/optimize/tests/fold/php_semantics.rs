//! Purpose:
//! Regression tests pinning constant folding to PHP 8.4's observable results for comparisons,
//! integer arithmetic overflow, array-key normalization, and string casts.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness.
//!
//! Key details:
//! - Every expectation in this file was produced by running the equivalent snippet under
//!   `php -r` on PHP 8.4.20; the table in `test_fold_comparisons_match_php` is generated from
//!   a `<=> / == / < / > / <= / >=` sweep over the same operand set.
//! - `fold_constants` is driven end-to-end rather than the private helpers, so the tests cover
//!   the operand-order rules relational folding depends on.

use super::*;

/// Builds the literal expression named by the comparison table's operand keys.
///
/// The names mirror the PHP fixture that generated the expected results, so a row can be read
/// back against `php -r` verbatim.
fn comparison_operand(name: &str) -> Expr {
    match name {
        "null" => Expr::new(ExprKind::Null, Span::dummy()),
        "false" => Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
        "true" => Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
        "i0" => Expr::int_lit(0),
        "i1" => Expr::int_lit(1),
        "i2" => Expr::int_lit(2),
        "im1" => Expr::int_lit(-1),
        "intmax" => Expr::int_lit(i64::MAX),
        "intmax_m1" => Expr::int_lit(i64::MAX - 1),
        "intmin" => Expr::int_lit(i64::MIN),
        "f0" => Expr::float_lit(0.0),
        "fneg0" => Expr::float_lit(-0.0),
        "f1" => Expr::float_lit(1.0),
        "f1_5" => Expr::float_lit(1.5),
        "fbig" => Expr::float_lit(9.2233720368547758e18),
        "s_empty" => Expr::string_lit(""),
        "s_0" => Expr::string_lit("0"),
        "s_1" => Expr::string_lit("1"),
        "s_01" => Expr::string_lit("01"),
        "s_sp1" => Expr::string_lit(" 1"),
        "s_1sp" => Expr::string_lit("1 "),
        "s_1e1" => Expr::string_lit("1e1"),
        "s_10" => Expr::string_lit("10"),
        "s_0e1" => Expr::string_lit("0e1"),
        "s_0e2" => Expr::string_lit("0e2"),
        "s_abc" => Expr::string_lit("abc"),
        "s_1abc" => Expr::string_lit("1abc"),
        "s_spaces" => Expr::string_lit("  "),
        "s_intmax_m1" => Expr::string_lit("9223372036854775806"),
        "s_intmax" => Expr::string_lit("9223372036854775807"),
        "s_over1" => Expr::string_lit("9223372036854775808"),
        "s_over2" => Expr::string_lit("9223372036854775809"),
        "s_intmin" => Expr::string_lit("-9223372036854775808"),
        "s_1_5" => Expr::string_lit("1.5"),
        "s_a" => Expr::string_lit("a"),
        "s_b" => Expr::string_lit("b"),
        "s_A" => Expr::string_lit("A"),
        other => panic!("unknown comparison operand {other}"),
    }
}

/// Folds `left op right` and returns the resulting expression kind.
fn fold_binop(left: Expr, op: BinOp, right: Expr) -> ExprKind {
    let folded = fold_constants(vec![Stmt::echo(Expr::binop(left, op, right))]);
    let StmtKind::Echo(expr) = &folded[0].kind else {
        panic!("expected echo statement");
    };
    expr.kind.clone()
}

/// Folds `left op right` where both operands are looked up from the comparison table names.
fn fold_named_binop(left: &str, op: BinOp, right: &str) -> ExprKind {
    fold_binop(comparison_operand(left), op, comparison_operand(right))
}

/// PHP 8.4 results for `<=>`, `==`, `<`, `>`, `<=` and `>=` over operand pairs that exercise
/// integer precision, numeric-string classification, and the null/bool fallbacks.
#[rustfmt::skip]
const PHP_COMPARISONS: &[(&str, &str, i64, bool, bool, bool, bool, bool)] = &[
    ("intmax_m1", "intmax", -1, false, true, false, true, false),
    ("intmax", "intmax_m1", 1, false, false, true, false, true),
    ("intmax", "intmax", 0, true, false, false, true, true),
    ("intmax_m1", "s_intmax", -1, false, true, false, true, false),
    ("s_intmax_m1", "s_intmax", -1, false, true, false, true, false),
    ("s_intmax_m1", "intmax", -1, false, true, false, true, false),
    ("intmax", "fbig", 0, true, false, false, true, true),
    ("intmax_m1", "fbig", 0, true, false, false, true, true),
    ("fbig", "intmax", 0, true, false, false, true, true),
    ("intmax_m1", "s_over1", 0, true, false, false, true, true),
    ("s_over1", "s_over2", -1, false, true, false, true, false),
    ("intmin", "s_intmin", 0, true, false, false, true, true),
    ("s_0e1", "s_0e2", 0, true, false, false, true, true),
    ("s_1e1", "s_10", 0, true, false, false, true, true),
    ("s_10", "s_1e1", 0, true, false, false, true, true),
    ("s_1", "s_01", 0, true, false, false, true, true),
    ("s_01", "s_1", 0, true, false, false, true, true),
    ("s_sp1", "i1", 0, true, false, false, true, true),
    ("s_1sp", "i1", 0, true, false, false, true, true),
    ("s_1", "i1", 0, true, false, false, true, true),
    ("s_abc", "i0", 1, false, false, true, false, true),
    ("i0", "s_abc", -1, false, true, false, true, false),
    ("s_1abc", "i1", 1, false, false, true, false, true),
    ("s_empty", "i0", -1, false, true, false, true, false),
    ("i0", "s_empty", 1, false, false, true, false, true),
    ("s_spaces", "i0", -1, false, true, false, true, false),
    ("null", "s_empty", 0, true, false, false, true, true),
    ("null", "s_0", -1, false, true, false, true, false),
    ("null", "i0", 0, true, false, false, true, true),
    ("null", "false", 0, true, false, false, true, true),
    ("null", "true", -1, false, true, false, true, false),
    ("false", "s_0", 0, true, false, false, true, true),
    ("true", "i1", 0, true, false, false, true, true),
    ("true", "s_abc", 0, true, false, false, true, true),
    ("false", "s_empty", 0, true, false, false, true, true),
    ("i0", "null", 0, true, false, false, true, true),
    ("s_0", "false", 0, true, false, false, true, true),
    ("f0", "fneg0", 0, true, false, false, true, true),
    ("fneg0", "f0", 0, true, false, false, true, true),
    ("f1_5", "s_1_5", 0, true, false, false, true, true),
    ("s_1_5", "f1_5", 0, true, false, false, true, true),
    ("f1", "i1", 0, true, false, false, true, true),
    ("f1", "s_1", 0, true, false, false, true, true),
    ("s_a", "s_b", -1, false, true, false, true, false),
    ("s_b", "s_a", 1, false, false, true, false, true),
    ("s_A", "s_a", -1, false, true, false, true, false),
    ("s_abc", "s_abc", 0, true, false, false, true, true),
    ("im1", "true", 0, true, false, false, true, true),
    ("i2", "true", 0, true, false, false, true, true),
    ("s_a", "true", 0, true, false, false, true, true),
    ("i0", "false", 0, true, false, false, true, true),
];

/// Verifies that every folded comparison matches PHP 8.4.
///
/// The regression this pins is B5: `PHP_INT_MAX - 1 <=> PHP_INT_MAX` used to fold through
/// `f64` and answer `0`, and `PHP_INT_MAX - 1 == "9223372036854775807"` used to answer `true`.
#[test]
fn test_fold_comparisons_match_php() {
    for &(left, right, spaceship, eq, lt, gt, le, ge) in PHP_COMPARISONS {
        assert_eq!(
            fold_named_binop(left, BinOp::Spaceship, right),
            ExprKind::IntLiteral(spaceship),
            "{left} <=> {right}"
        );
        for (op, expected, label) in [
            (BinOp::Eq, eq, "=="),
            (BinOp::NotEq, !eq, "!="),
            (BinOp::Lt, lt, "<"),
            (BinOp::Gt, gt, ">"),
            (BinOp::LtEq, le, "<="),
            (BinOp::GtEq, ge, ">="),
        ] {
            assert_eq!(
                fold_named_binop(left, op, right),
                ExprKind::BoolLiteral(expected),
                "{left} {label} {right}"
            );
        }
    }
}

/// Operand set for the string-versus-string `<=>` matrix below.
#[rustfmt::skip]
const NUMERIC_STRING_OPERANDS: &[&str] = &[
    "", "0", "1", "01", " 1", "1 ", "1e1", "10", "0e1", "abc", "1abc", "1.5", ".5",
    "5.", "1e400", "-1e400", "9223372036854775807", "9223372036854775808",
    "-9223372036854775808", "-9223372036854775809", "  ", "0x1A", "1_000", "007", "+1",
];

/// PHP 8.4 `$a <=> $b` for every pair of `NUMERIC_STRING_OPERANDS`, row-major, encoded as
/// `<` / `=` / `>`. Generated with a `foreach` sweep under `php -r`.
///
/// This is `zendi_smart_strcmp` coverage: two integer strings compare as integers, two
/// same-side overflowed integers or two infinities fall back to a byte comparison, and any
/// non-numeric operand makes the whole comparison byte-wise.
#[rustfmt::skip]
const PHP_STRING_ORDERINGS: &[&str] = &[
    "=<<<<<<<<<<<<<<<<<<<<<<<<",
    ">=<<<<<<=<<<<<<><<>>><<<<",
    ">>====<<><<<><<><<>>>><<=",
    ">>====<<><<<><<><<>>><<<=",
    ">>====<<><<<><<><<>>><<<=",
    ">>====<<><<<><<><<>>>><<=",
    ">>>>>>==><>>>><><<>>>>>>>",
    ">>>>>>==><<>>><><<>>>><>>",
    ">=<<<<<<=<<<<<<><<>>><<<<",
    ">>>>>>>>>=>>>>>>>>>>>>>>>",
    ">>>>>><>><=>><<><<>>>>>>>",
    ">>>>>><<><<=><<><<>>>><<>",
    ">><<<<<<><<<=<<><<>>><<<<",
    ">>>>>><<><>>>=<><<>>>>><>",
    ">>>>>>>>><>>>>=>>>>>>>>>>",
    "><<<<<<<<<<<<<<=<<<<><<<<",
    ">>>>>>>>><>>>><>=<>>>>>>>",
    ">>>>>>>>><>>>><>>=>>>>>>>",
    "><<<<<<<<<<<<<<><<=>><<<<",
    "><<<<<<<<<<<<<<><<<=><<<<",
    "><<<<<<<<<<<<<<<<<<<=<<<<",
    ">><>><<<><<<><<><<>>>=<>>",
    ">>>>>><>><<>><<><<>>>>=>>",
    ">>>>>><<><<>>><><<>>><<=>",
    ">>====<<><<<><<><<>>><<<=",
];

/// Verifies string-versus-string `<=>` folding reproduces PHP's smart string comparison.
///
/// Before the fix both operands were parsed to `f64`, so `"9223372036854775807"` and
/// `"9223372036854775808"` compared equal.
#[test]
fn test_fold_string_orderings_match_php() {
    assert_eq!(NUMERIC_STRING_OPERANDS.len(), PHP_STRING_ORDERINGS.len());
    for (left, row) in NUMERIC_STRING_OPERANDS.iter().zip(PHP_STRING_ORDERINGS) {
        assert_eq!(row.len(), NUMERIC_STRING_OPERANDS.len());
        for (right, expected) in NUMERIC_STRING_OPERANDS.iter().zip(row.chars()) {
            let expected = match expected {
                '<' => -1,
                '=' => 0,
                '>' => 1,
                other => panic!("bad expectation {other}"),
            };
            assert_eq!(
                fold_binop(
                    Expr::string_lit(*left),
                    BinOp::Spaceship,
                    Expr::string_lit(*right)
                ),
                ExprKind::IntLiteral(expected),
                "{left:?} <=> {right:?}"
            );
        }
    }
}

/// Verifies NAN keeps PHP's asymmetric comparison behavior.
///
/// PHP answers `false` for `NAN < 1`, `NAN > 1`, `NAN <= 1` and `NAN >= 1` alike, but `1 <=>
/// NAN` and `NAN <=> NAN` are both `1`, because `zend_compare` returns `1` for any NAN pair
/// and the relational operators are spelled through it in a fixed argument order.
#[test]
fn test_fold_nan_comparisons_match_php() {
    let nan = || Expr::float_lit(f64::NAN);
    for op in [BinOp::Lt, BinOp::Gt, BinOp::LtEq, BinOp::GtEq] {
        assert_eq!(
            fold_binop(nan(), op.clone(), Expr::int_lit(1)),
            ExprKind::BoolLiteral(false),
            "NAN {op:?} 1"
        );
        assert_eq!(
            fold_binop(Expr::int_lit(1), op.clone(), nan()),
            ExprKind::BoolLiteral(false),
            "1 {op:?} NAN"
        );
    }
    assert_eq!(
        fold_binop(Expr::int_lit(1), BinOp::Spaceship, nan()),
        ExprKind::IntLiteral(1)
    );
    assert_eq!(
        fold_binop(nan(), BinOp::Spaceship, nan()),
        ExprKind::IntLiteral(1)
    );
    assert_eq!(
        fold_binop(nan(), BinOp::Eq, nan()),
        ExprKind::BoolLiteral(false)
    );
}

/// Verifies a float against a non-numeric string is left for the runtime.
///
/// PHP stringifies the float through `zend_double_to_str` and compares bytes, so `INF ==
/// "INF"` is `true`; the fold refuses to guess that formatting and keeps the operation.
#[test]
fn test_float_versus_non_numeric_string_declines_fold() {
    let expr = Expr::binop(Expr::float_lit(1.5), BinOp::Eq, Expr::string_lit("abc"));
    let folded = fold_constants(vec![Stmt::echo(expr.clone())]);
    assert_eq!(folded, vec![Stmt::echo(expr)]);
}

/// Verifies integer arithmetic folds match PHP, including every overflow boundary.
///
/// `PHP_INT_MIN % -1` used to panic the compiler with "attempt to calculate the remainder
/// with overflow"; `6 / 3` and `2 ** 3` used to fold to floats; `1 << 64` and `-PHP_INT_MIN`
/// used to decline and reach a wrapping runtime.
#[test]
fn test_fold_integer_arithmetic_matches_php() {
    // php -r 'var_dump(PHP_INT_MIN % -1, 7 % 3, -7 % 3, 7 % -3);'
    assert_eq!(
        fold_binop(Expr::int_lit(i64::MIN), BinOp::Mod, Expr::int_lit(-1)),
        ExprKind::IntLiteral(0)
    );
    assert_eq!(
        fold_binop(Expr::int_lit(-7), BinOp::Mod, Expr::int_lit(3)),
        ExprKind::IntLiteral(-1)
    );

    // php -r 'var_dump(6 / 3, 7 / 2, PHP_INT_MIN / -1);'
    assert_eq!(
        fold_binop(Expr::int_lit(6), BinOp::Div, Expr::int_lit(3)),
        ExprKind::IntLiteral(2)
    );
    assert_eq!(
        fold_binop(Expr::int_lit(7), BinOp::Div, Expr::int_lit(2)),
        ExprKind::FloatLiteral(3.5)
    );
    assert_eq!(
        fold_binop(Expr::int_lit(i64::MIN), BinOp::Div, Expr::int_lit(-1)),
        ExprKind::FloatLiteral(9.223372036854776e18)
    );

    // php -r 'var_dump(2 ** 3, (-2) ** 3, 2 ** 0, 2 ** -1);'
    assert_eq!(
        fold_binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(3)),
        ExprKind::IntLiteral(8)
    );
    assert_eq!(
        fold_binop(Expr::int_lit(-2), BinOp::Pow, Expr::int_lit(3)),
        ExprKind::IntLiteral(-8)
    );
    assert_eq!(
        fold_binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(0)),
        ExprKind::IntLiteral(1)
    );
    assert_eq!(
        fold_binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(-1)),
        ExprKind::FloatLiteral(0.5)
    );

    // Overflowing `**` follows PHP's square-and-multiply loop, not a single `pow()` call: the
    // two differ in the last ULP for most inputs.
    // php -r 'printf("%.17g %.17g %.17g", 2 ** 64, 654 ** 32, (-133) ** 101);'
    assert_eq!(
        fold_binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(64)),
        ExprKind::FloatLiteral(1.8446744073709552e19)
    );
    assert_eq!(
        fold_binop(Expr::int_lit(654), BinOp::Pow, Expr::int_lit(32)),
        ExprKind::FloatLiteral(1.2545499179770422e90)
    );
    assert_eq!(
        fold_binop(Expr::int_lit(-133), BinOp::Pow, Expr::int_lit(101)),
        ExprKind::FloatLiteral(-3.2286111158631344e214)
    );

    // php -r 'var_dump(1 << 63, 1 << 64, -1 >> 64, 8 >> 64, -1 >> 63);'
    assert_eq!(
        fold_binop(Expr::int_lit(1), BinOp::ShiftLeft, Expr::int_lit(63)),
        ExprKind::IntLiteral(i64::MIN)
    );
    assert_eq!(
        fold_binop(Expr::int_lit(1), BinOp::ShiftLeft, Expr::int_lit(64)),
        ExprKind::IntLiteral(0)
    );
    assert_eq!(
        fold_binop(Expr::int_lit(-1), BinOp::ShiftRight, Expr::int_lit(64)),
        ExprKind::IntLiteral(-1)
    );
    assert_eq!(
        fold_binop(Expr::int_lit(8), BinOp::ShiftRight, Expr::int_lit(64)),
        ExprKind::IntLiteral(0)
    );

    // php -r 'var_dump(PHP_INT_MAX + 1, PHP_INT_MIN - 1, PHP_INT_MAX * 2);'
    assert_eq!(
        fold_binop(Expr::int_lit(i64::MAX), BinOp::Add, Expr::int_lit(1)),
        ExprKind::FloatLiteral(9.223372036854776e18)
    );
}

/// Verifies a negative shift count is not folded so the runtime raises `ArithmeticError`.
#[test]
fn test_negative_shift_declines_fold() {
    for op in [BinOp::ShiftLeft, BinOp::ShiftRight] {
        let expr = Expr::binop(Expr::int_lit(1), op, Expr::int_lit(-1));
        let folded = fold_constants(vec![Stmt::echo(expr.clone())]);
        assert_eq!(folded, vec![Stmt::echo(expr)]);
    }
}

/// Verifies `-PHP_INT_MIN` folds to the float PHP produces instead of wrapping to `PHP_INT_MIN`.
///
/// php -r 'var_dump(-PHP_INT_MIN);' prints `float(9.2233720368548E+18)`.
#[test]
fn test_fold_negate_int_min_promotes_to_float() {
    let folded = fold_constants(vec![Stmt::echo(Expr::new(
        ExprKind::Negate(Box::new(Expr::int_lit(i64::MIN))),
        Span::dummy(),
    ))]);
    let StmtKind::Echo(expr) = &folded[0].kind else {
        panic!("expected echo statement");
    };
    assert_eq!(expr.kind, ExprKind::FloatLiteral(9.223372036854776e18));
}

/// Builds an associative array literal access `[key => value, ...][index]`.
fn assoc_access(entries: Vec<(Expr, Expr)>, index: Expr) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(
                ExprKind::ArrayLiteralAssoc(entries),
                Span::dummy(),
            )),
            index: Box::new(index),
        },
        Span::dummy(),
    )
}

/// Verifies associative array-literal access normalizes keys the way PHP's hash table does.
///
/// Before the fix the fold compared raw scalar variants, so `[0 => "a", false => "b"][0]`
/// folded to `"a"` while PHP prints `"b"` — the two keys are the same slot and the literal is
/// built last-wins.
#[test]
fn test_fold_assoc_access_normalizes_php_array_keys() {
    let null = || Expr::new(ExprKind::Null, Span::dummy());
    let bool_lit = |value| Expr::new(ExprKind::BoolLiteral(value), Span::dummy());

    // php -r 'var_dump([0 => "a", false => "b"][0]);' → "b"
    let cases: Vec<(Vec<(Expr, Expr)>, Expr, &str)> = vec![
        (
            vec![
                (Expr::int_lit(0), Expr::string_lit("a")),
                (bool_lit(false), Expr::string_lit("b")),
            ],
            Expr::int_lit(0),
            "b",
        ),
        (
            vec![
                (Expr::string_lit("1"), Expr::string_lit("a")),
                (Expr::int_lit(1), Expr::string_lit("b")),
            ],
            Expr::string_lit("1"),
            "b",
        ),
        (
            vec![
                (null(), Expr::string_lit("a")),
                (Expr::string_lit(""), Expr::string_lit("b")),
            ],
            null(),
            "b",
        ),
        (
            vec![
                (bool_lit(true), Expr::string_lit("a")),
                (Expr::int_lit(1), Expr::string_lit("b")),
            ],
            bool_lit(true),
            "b",
        ),
        // php -r 'var_dump(["01" => "a", 1 => "b"]["01"]);' → "a": "01" is not an integer key.
        (
            vec![
                (Expr::string_lit("01"), Expr::string_lit("a")),
                (Expr::int_lit(1), Expr::string_lit("b")),
            ],
            Expr::string_lit("01"),
            "a",
        ),
        // php -r 'var_dump([" 1" => "a", 1 => "b"][" 1"]);' → "a": leading space keeps a string key.
        (
            vec![
                (Expr::string_lit(" 1"), Expr::string_lit("a")),
                (Expr::int_lit(1), Expr::string_lit("b")),
            ],
            Expr::string_lit(" 1"),
            "a",
        ),
        // php -r 'var_dump([2.0 => "a", 2 => "b"][2]);' → "b": integral floats truncate silently.
        (
            vec![
                (Expr::float_lit(2.0), Expr::string_lit("a")),
                (Expr::int_lit(2), Expr::string_lit("b")),
            ],
            Expr::int_lit(2),
            "b",
        ),
    ];

    for (entries, index, expected) in cases {
        let folded = fold_constants(vec![Stmt::echo(assoc_access(entries, index))]);
        assert_eq!(folded, vec![Stmt::echo(Expr::string_lit(expected))]);
    }
}

/// Verifies a lossy float array key is not folded so the runtime keeps PHP's deprecation.
///
/// php -r 'var_dump([1.7 => "a"][1]);' emits "Implicit conversion from float 1.7 to int loses
/// precision" before printing `"a"`.
#[test]
fn test_lossy_float_array_key_declines_fold() {
    let expr = assoc_access(
        vec![(Expr::float_lit(1.7), Expr::string_lit("a"))],
        Expr::int_lit(1),
    );
    let folded = fold_constants(vec![Stmt::echo(expr.clone())]);
    assert_eq!(folded, vec![Stmt::echo(expr)]);
}

/// Verifies indexed array-literal access normalizes the index like PHP.
///
/// php -r 'var_dump(["a", "b"][true], ["a", "b"]["1"]);' prints `"b"` twice.
#[test]
fn test_fold_indexed_access_normalizes_index() {
    let items = || {
        Expr::new(
            ExprKind::ArrayLiteral(vec![Expr::string_lit("a"), Expr::string_lit("b")]),
            Span::dummy(),
        )
    };
    for index in [
        Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
        Expr::string_lit("1"),
        Expr::int_lit(1),
    ] {
        let folded = fold_constants(vec![Stmt::echo(Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(items()),
                index: Box::new(index),
            },
            Span::dummy(),
        ))]);
        assert_eq!(folded, vec![Stmt::echo(Expr::string_lit("b"))]);
    }
}

/// Folds `(target) expr` and returns the resulting expression kind.
fn fold_cast(target: CastType, expr: Expr) -> ExprKind {
    let folded = fold_constants(vec![Stmt::echo(Expr::new(
        ExprKind::Cast {
            target,
            expr: Box::new(expr),
        },
        Span::dummy(),
    ))]);
    let StmtKind::Echo(expr) = &folded[0].kind else {
        panic!("expected echo statement");
    };
    expr.kind.clone()
}

/// PHP 8.4 results for `(float)` and `(int)` casts of string literals.
///
/// Produced by `php -r 'printf("%s %s", var_export((float) $s, true), var_export((int) $s, true));'`
/// for each subject.
#[rustfmt::skip]
const PHP_STRING_CASTS: &[(&str, f64, i64)] = &[
    // Rust's `f64` parser accepts these; PHP's numeric grammar does not.
    ("INF", 0.0, 0),
    ("inf", 0.0, 0),
    ("nan", 0.0, 0),
    ("NaN", 0.0, 0),
    ("infinity", 0.0, 0),
    ("-INF", 0.0, 0),
    // Prefix parsing.
    ("1e3", 1000.0, 1000),
    (" 12", 12.0, 12),
    ("12 ", 12.0, 12),
    ("\n12", 12.0, 12),
    ("12abc", 12.0, 12),
    ("  -12xyz", -12.0, -12),
    ("0x1A", 0.0, 0),
    ("0b101", 0.0, 0),
    ("1_000", 1.0, 1),
    (".5", 0.5, 0),
    ("5.", 5.0, 5),
    ("+.5e-2", 0.005, 0),
    ("1.2.3", 1.2, 1),
    ("1e", 1.0, 1),
    ("1e+", 1.0, 1),
    ("007", 7.0, 7),
    // No numeric prefix at all.
    ("abc", 0.0, 0),
    ("", 0.0, 0),
    ("-", 0.0, 0),
    ("- 1", 0.0, 0),
    (".", 0.0, 0),
    ("--1", 0.0, 0),
    // Saturation and range.
    ("9223372036854775807", 9.223372036854776e18, i64::MAX),
    ("9223372036854775808", 9.223372036854776e18, i64::MAX),
    ("-9223372036854775808", -9.223372036854776e18, i64::MIN),
    ("-9223372036854775809", -9.223372036854776e18, i64::MIN),
    ("1e-400", 0.0, 0),
];

/// Verifies `(float)` and `(int)` string casts fold to PHP's results.
///
/// The regression this pins is B14: the fold used Rust's `str::parse::<f64>()`, so
/// `(float) "INF"` folded to infinity and `(float) "nan"` to NAN, where PHP produces `0`.
#[test]
fn test_fold_string_casts_match_php() {
    for &(subject, expected_float, expected_int) in PHP_STRING_CASTS {
        assert_eq!(
            fold_cast(CastType::Float, Expr::string_lit(subject)),
            ExprKind::FloatLiteral(expected_float),
            "(float) {subject:?}"
        );
        assert_eq!(
            fold_cast(CastType::Int, Expr::string_lit(subject)),
            ExprKind::IntLiteral(expected_int),
            "(int) {subject:?}"
        );
    }
    // php -r 'var_dump((float) "1e400");' → float(INF)
    let ExprKind::FloatLiteral(value) = fold_cast(CastType::Float, Expr::string_lit("1e400"))
    else {
        panic!("expected folded float literal");
    };
    assert!(value.is_infinite() && value.is_sign_positive());
    // php -r 'var_dump((int) "1e400");' → int(0): `zend_dval_to_lval_cap` zeroes non-finite input.
    assert_eq!(
        fold_cast(CastType::Int, Expr::string_lit("1e400")),
        ExprKind::IntLiteral(0)
    );
}

/// Verifies a ternary whose arms are `0.0` and `-0.0` is not collapsed to one constant.
///
/// PHP prints `-0` for `echo -0.0`, so the sign is observable: propagating `0.0` into a use of
/// a variable that may hold `-0.0` changes the program's output.
#[test]
fn test_signed_zero_ternary_arms_do_not_merge() {
    let program = vec![
        Stmt::assign(
            "x",
            Expr::new(
                ExprKind::Ternary {
                    condition: Box::new(Expr::var("flag")),
                    then_expr: Box::new(Expr::float_lit(0.0)),
                    else_expr: Box::new(Expr::float_lit(-0.0)),
                },
                Span::dummy(),
            ),
        ),
        Stmt::echo(Expr::var("x")),
    ];

    let propagated = propagate_constants(program);

    let StmtKind::Echo(expr) = &propagated[1].kind else {
        panic!("expected echo statement");
    };
    assert_eq!(expr.kind, ExprKind::Variable("x".to_string()));
}

/// Verifies an `if`/`else` that assigns `0.0` on one path and `-0.0` on the other does not
/// merge into a single propagated constant.
///
/// php -r 'if ($argc > 1000) { $x = 0.0; } else { $x = -0.0; } echo $x;' prints `-0`.
#[test]
fn test_signed_zero_branch_assignments_do_not_merge() {
    let program = vec![
        Stmt::new(
            StmtKind::If {
                condition: Expr::var("flag"),
                then_body: vec![Stmt::assign("x", Expr::float_lit(0.0))],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::assign("x", Expr::float_lit(-0.0))]),
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::var("x")),
    ];

    let propagated = propagate_constants(program);

    let StmtKind::Echo(expr) = &propagated[1].kind else {
        panic!("expected echo statement");
    };
    assert_eq!(expr.kind, ExprKind::Variable("x".to_string()));
}

/// Verifies an `if`/`else` that assigns the same float on both paths still merges.
#[test]
fn test_identical_float_branch_assignments_still_merge() {
    let program = vec![
        Stmt::new(
            StmtKind::If {
                condition: Expr::var("flag"),
                then_body: vec![Stmt::assign("x", Expr::float_lit(2.5))],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::assign("x", Expr::float_lit(2.5))]),
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::var("x")),
    ];

    let propagated = propagate_constants(program);

    let StmtKind::Echo(expr) = &propagated[1].kind else {
        panic!("expected echo statement");
    };
    assert_eq!(expr.kind, ExprKind::FloatLiteral(2.5));
}

/// Verifies a ternary whose arms are the same float constant still merges.
///
/// Guards the fix above against over-correcting: only the signed-zero (and NAN payload) cases
/// must stay distinct.
#[test]
fn test_identical_float_ternary_arms_still_merge() {
    let program = vec![
        Stmt::assign(
            "x",
            Expr::new(
                ExprKind::Ternary {
                    condition: Box::new(Expr::var("flag")),
                    then_expr: Box::new(Expr::float_lit(2.5)),
                    else_expr: Box::new(Expr::float_lit(2.5)),
                },
                Span::dummy(),
            ),
        ),
        Stmt::echo(Expr::var("x")),
    ];

    let propagated = propagate_constants(program);

    let StmtKind::Echo(expr) = &propagated[1].kind else {
        panic!("expected echo statement");
    };
    assert_eq!(expr.kind, ExprKind::FloatLiteral(2.5));
}

/// Verifies an associative literal access does not fold through a spread entry.
///
/// PHP merges the spread source in source order, so `['a' => 1, ...$extra]['a']` is whatever
/// `$extra` last wrote for key `a`, not the literal `1` the pair carries. The spread is stored as
/// a pair whose key IS the spread, so the fold has to recognize that shape and decline.
#[test]
fn test_fold_declines_assoc_array_access_through_spread() {
    let spread = crate::parser::ast::assoc_spread_entry(Expr::new(
        ExprKind::Spread(Box::new(Expr::var("extra"))),
        Span::dummy(),
    ));
    let program = vec![Stmt::echo(Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(
                ExprKind::ArrayLiteralAssoc(vec![
                    (Expr::string_lit("a"), Expr::int_lit(1)),
                    spread,
                ]),
                Span::dummy(),
            )),
            index: Box::new(Expr::string_lit("a")),
        },
        Span::dummy(),
    ))];

    let folded = fold_constants(program);

    let StmtKind::Echo(expr) = &folded[0].kind else {
        panic!("expected echo statement");
    };
    assert!(
        matches!(expr.kind, ExprKind::ArrayAccess { .. }),
        "{expr:?}",
    );
}

/// Verifies the same access without a spread still folds, so the guard is not over-broad.
#[test]
fn test_fold_assoc_array_access_without_spread_still_folds() {
    let program = vec![Stmt::echo(Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(
                ExprKind::ArrayLiteralAssoc(vec![(Expr::string_lit("a"), Expr::int_lit(1))]),
                Span::dummy(),
            )),
            index: Box::new(Expr::string_lit("a")),
        },
        Span::dummy(),
    ))];

    let folded = fold_constants(program);

    let StmtKind::Echo(expr) = &folded[0].kind else {
        panic!("expected echo statement");
    };
    assert_eq!(expr.kind, ExprKind::IntLiteral(1));
}
