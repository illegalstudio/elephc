//! Purpose:
//! Integration or regression tests for parser AST coverage of expression operators, including arithmetic precedence, concat operator, and comparison lower than arithmetic.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Verifies that `<?php echo 2 + 3 * 4;` parses as `2 + (3 * 4)` — multiplication has higher
/// precedence than addition, matching PHP's arithmetic precedence.
#[test]
fn test_arithmetic_precedence() {
    let stmts = parse_source("<?php echo 2 + 3 * 4;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(2),
        BinOp::Add,
        Expr::binop(Expr::int_lit(3), BinOp::Mul, Expr::int_lit(4)),
    ));
    assert_eq!(stmts, vec![expected]);
}

/// Verifies that `<?php echo "a" . "b";` parses as a binary concat operation.
/// The `.` operator concatenates two string literals.
#[test]
fn test_concat_operator() {
    let stmts = parse_source("<?php echo \"a\" . \"b\";");
    let expected = Stmt::echo(Expr::binop(
        Expr::string_lit("a"),
        BinOp::Concat,
        Expr::string_lit("b"),
    ));
    assert_eq!(stmts, vec![expected]);
}

/// Verifies that `<?php echo 1 + 2 == 3;` parses as `(1 + 2) == 3` — addition has higher
/// precedence than equality, matching PHP's precedence rules.
#[test]
fn test_comparison_lower_than_arithmetic() {
    // 1 + 2 == 3 should parse as (1 + 2) == 3
    let stmts = parse_source("<?php echo 1 + 2 == 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Add, Expr::int_lit(2)),
        BinOp::Eq,
        Expr::int_lit(3),
    ));
    assert_eq!(stmts, vec![expected]);
}

/// Verifies that `<>` parses to the same `BinOp::NotEq` node as `!=`, so the alias is
/// indistinguishable from `!=` after parsing.
#[test]
fn test_angle_not_equal_is_alias_of_not_equal() {
    let angle = parse_source("<?php echo 1 <> 2;");
    let bang = parse_source("<?php echo 1 != 2;");
    assert_eq!(angle, bang);
    assert_eq!(
        angle,
        vec![Stmt::echo(Expr::binop(
            Expr::int_lit(1),
            BinOp::NotEq,
            Expr::int_lit(2),
        ))]
    );
}

/// Verifies the Pratt binding power of `<>` matches `!=` exactly: it binds looser than
/// `+` and looser than `<`, and it is left-associative like the other equality operators.
#[test]
fn test_angle_not_equal_binding_power_matches_not_equal() {
    // Arithmetic (bp 29) binds tighter than `<>` (bp 21): 1 + 2 <> 3 is (1 + 2) <> 3.
    let stmts = parse_source("<?php echo 1 + 2 <> 3;");
    assert_eq!(
        stmts,
        vec![Stmt::echo(Expr::binop(
            Expr::binop(Expr::int_lit(1), BinOp::Add, Expr::int_lit(2)),
            BinOp::NotEq,
            Expr::int_lit(3),
        ))]
    );

    // Relational (bp 23) binds tighter than `<>` (bp 21): 1 <> 2 < 3 is 1 <> (2 < 3).
    let stmts = parse_source("<?php echo 1 <> 2 < 3;");
    assert_eq!(
        stmts,
        vec![Stmt::echo(Expr::binop(
            Expr::int_lit(1),
            BinOp::NotEq,
            Expr::binop(Expr::int_lit(2), BinOp::Lt, Expr::int_lit(3)),
        ))]
    );

    // `<>` is left-associative and shares its level with `==`: 1 <> 2 == 3 is (1 <> 2) == 3.
    let stmts = parse_source("<?php echo 1 <> 2 == 3;");
    assert_eq!(
        stmts,
        vec![Stmt::echo(Expr::binop(
            Expr::binop(Expr::int_lit(1), BinOp::NotEq, Expr::int_lit(2)),
            BinOp::Eq,
            Expr::int_lit(3),
        ))]
    );

    // `&&` (bp 13) binds looser than `<>`: 1 <> 2 && 3 is (1 <> 2) && 3.
    let stmts = parse_source("<?php echo 1 <> 2 && 3;");
    assert_eq!(
        stmts,
        vec![Stmt::echo(Expr::binop(
            Expr::binop(Expr::int_lit(1), BinOp::NotEq, Expr::int_lit(2)),
            BinOp::And,
            Expr::int_lit(3),
        ))]
    );
}

/// Verifies that `<?php echo "x" . 1 < 2;` parses as `("x" . 1) < 2` — concatenation has higher
/// precedence than comparison, matching PHP precedence.
#[test]
fn test_concat_higher_than_comparison() {
    // "x" . 1 < 2 should parse as ("x" . 1) < 2 — PHP precedence
    let stmts = parse_source("<?php echo \"x\" . 1 < 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::string_lit("x"), BinOp::Concat, Expr::int_lit(1)),
        BinOp::Lt,
        Expr::int_lit(2),
    ));
    assert_eq!(stmts, vec![expected]);
}

/// Verifies that `<?php echo 10 % 3 * 2;` parses as `(10 % 3) * 2` — modulo and multiplication
/// have the same precedence and are left-associative.
#[test]
fn test_modulo_same_as_multiply() {
    // 10 % 3 * 2 should parse as (10 % 3) * 2
    let stmts = parse_source("<?php echo 10 % 3 * 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(10), BinOp::Mod, Expr::int_lit(3)),
        BinOp::Mul,
        Expr::int_lit(2),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Control flow ---

/// Verifies that `<?php echo 1 === 1;` parses as a strict equality binary operation.
/// The `===` operator checks type-strict equality in PHP.
#[test]
fn test_strict_equal_parses() {
    let stmts = parse_source("<?php echo 1 === 1;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(1),
        BinOp::StrictEq,
        Expr::int_lit(1),
    ));
    assert_eq!(stmts, vec![expected]);
}

/// Verifies that `<?php echo 1 !== 2;` parses as a strict inequality binary operation.
/// The `!==` operator checks type-strict inequality in PHP.
#[test]
fn test_strict_not_equal_parses() {
    let stmts = parse_source("<?php echo 1 !== 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(1),
        BinOp::StrictNotEq,
        Expr::int_lit(2),
    ));
    assert_eq!(stmts, vec![expected]);
}

/// Verifies that `<?php echo 1 + 2 === 3;` parses as `(1 + 2) === 3` — arithmetic has higher
/// precedence than strict equality, consistent with PHP's precedence table.
#[test]
fn test_strict_equal_same_precedence_as_loose() {
    // 1 + 2 === 3 should parse as (1 + 2) === 3
    let stmts = parse_source("<?php echo 1 + 2 === 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Add, Expr::int_lit(2)),
        BinOp::StrictEq,
        Expr::int_lit(3),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Include/Require ---

/// Verifies that `<?php echo 2 ** 3;` parses as an exponentiation binary operation.
/// The `**` operator computes the power of left operand raised to the right operand.
#[test]
fn test_pow_operator_parses() {
    let stmts = parse_source("<?php echo 2 ** 3;");
    let expected = Stmt::echo(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(3)));
    assert_eq!(stmts, vec![expected]);
}

/// Verifies that `<?php echo 2 ** 3 ** 2;` parses as `2 ** (3 ** 2)` — exponentiation is
/// right-associative in PHP, so the rightmost `**` groups first.
#[test]
fn test_pow_right_associative_parse() {
    // 2 ** 3 ** 2 should parse as 2 ** (3 ** 2)
    let stmts = parse_source("<?php echo 2 ** 3 ** 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(2),
        BinOp::Pow,
        Expr::binop(Expr::int_lit(3), BinOp::Pow, Expr::int_lit(2)),
    ));
    assert_eq!(stmts, vec![expected]);
}

/// Verifies that `<?php echo 3 * 2 ** 3;` parses as `3 * (2 ** 3)` — exponentiation has
/// higher precedence than multiplication, matching PHP precedence rules.
#[test]
fn test_pow_higher_than_mul_parse() {
    // 3 * 2 ** 3 should parse as 3 * (2 ** 3)
    let stmts = parse_source("<?php echo 3 * 2 ** 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(3),
        BinOp::Mul,
        Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(3)),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Type casting ---

/// Verifies that `<?php echo 1 == 1 & 0;` parses as `(1 == 1) & 0` — bitwise AND has lower
/// precedence than loose equality, matching PHP precedence table.
#[test]
fn test_bitwise_and_lower_than_equality() {
    // 1 == 1 & 0 should parse as (1 == 1) & 0 — PHP precedence
    let stmts = parse_source("<?php echo 1 == 1 & 0;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Eq, Expr::int_lit(1)),
        BinOp::BitAnd,
        Expr::int_lit(0),
    ));
    assert_eq!(stmts, vec![expected]);
}

/// Verifies that `<?php echo 1 << 2 < 10;` parses as `(1 << 2) < 10` — shift operators have
/// higher precedence than comparison, matching PHP precedence.
#[test]
fn test_shift_higher_than_comparison() {
    // 1 << 2 < 10 should parse as (1 << 2) < 10 — PHP precedence
    let stmts = parse_source("<?php echo 1 << 2 < 10;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::ShiftLeft, Expr::int_lit(2)),
        BinOp::Lt,
        Expr::int_lit(10),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Unary operators ---

/// Verifies that `<?php echo ++$i;`, `<?php echo $i++;`, `<?php echo --$i;`, and
/// `<?php echo $i--;` parse correctly as pre/post increment/decrement expressions.
/// Tests all four variants to ensure the parser distinguishes prefix vs postfix forms.
#[test]
fn test_increment_decrement_parses() {
    let pre_inc = parse_source("<?php echo ++$i;");
    assert_eq!(echoed_expr(&pre_inc), &ExprKind::PreIncrement("i".into()));
    let post_inc = parse_source("<?php echo $i++;");
    assert_eq!(echoed_expr(&post_inc), &ExprKind::PostIncrement("i".into()));
    let pre_dec = parse_source("<?php echo --$i;");
    assert_eq!(echoed_expr(&pre_dec), &ExprKind::PreDecrement("i".into()));
    let post_dec = parse_source("<?php echo $i--;");
    assert_eq!(echoed_expr(&post_dec), &ExprKind::PostDecrement("i".into()));
}

/// Verifies the prefix `++`/`--` fast path declines a variable that a place suffix continues.
///
/// `++$o->n` starts with `Token::Variable("o")`. Taking the bare-variable node on that alone
/// consumed the name and left `->n` unparsed, so the increment landed on the OBJECT (issue
/// #682). The four suffix tokens are `->`, `?->`, `::` and `[`; seeing any of them has to send
/// the parse down the l-value desugar, which shows up as an `Assignment` carrying a prelude.
#[test]
fn test_prefix_incdec_declines_a_variable_with_a_place_suffix() {
    for source in ["<?php echo ++$o->n;", "<?php echo ++$a[0];"] {
        let stmts = parse_source(source);
        let parsed = echoed_expr(&stmts);
        assert!(
            matches!(parsed, ExprKind::Assignment { prelude, .. } if !prelude.is_empty()),
            "{source} must desugar the place, got {parsed:?}"
        );
    }

    // `?->` and `$o::$n` are places the desugar does not support either, so they end at the
    // fallback error. What matters is the same thing: NEITHER silently becomes `++$o`.
    for source in ["<?php echo ++$o?->n;", "<?php echo ++$o::$n;"] {
        assert!(parse_fails(source), "{source} must not parse as ++$o");
    }

    // The bare variable still takes the dedicated node: the guard narrows the fast path, it
    // does not remove it.
    let bare = parse_source("<?php echo ++$i;");
    assert_eq!(echoed_expr(&bare), &ExprKind::PreIncrement("i".into()));
}

/// Verifies the l-value desugar evaluates a computed index exactly ONCE.
///
/// `$b[ix()]++` reads the element, writes it back, and answers the old value, so a desugar that
/// spelled the index out at each use would call `ix()` three times. The index is stabilized into
/// a prelude temporary first, and every later mention names that temporary.
#[test]
fn test_postfix_incdec_desugar_evaluates_a_computed_index_once() {
    let stmts = parse_source("<?php echo $b[ix()]++;");
    let ExprKind::Assignment { prelude, .. } = echoed_expr(&stmts) else {
        panic!("expected a desugared assignment, got {:?}", echoed_expr(&stmts));
    };

    let rendered = format!("{prelude:?}");
    assert_eq!(
        rendered.matches("FunctionCall").count(),
        1,
        "the index must be evaluated once: {prelude:?}"
    );
    assert!(
        rendered.contains("__elephc_assign_expr"),
        "the index must be stabilized into a temporary: {prelude:?}"
    );
}

/// Verifies the statement-level postfix scan declines when a top-level assignment comes first.
///
/// `$t += $b[0]++;` increments `$b[0]`; it does not increment `$t += $b[0]`. The scan used to
/// claim the whole line, parse that as its target, and reject it with `Invalid assignment
/// target` (issue #682). Declining hands the statement to the assignment parsers, which lower
/// the increment inside the value expression.
#[test]
fn test_statement_postfix_scan_declines_behind_a_top_level_assignment() {
    let stmts = parse_source("<?php $t += $b[0]++;");
    assert!(
        matches!(&stmts[0].kind, StmtKind::Assign { name, .. } if name == "t"),
        "expected a compound assignment to $t, got {:?}",
        stmts[0].kind
    );

    // The same scan still claims the statement when nothing precedes the `++`.
    let plain = parse_source("<?php $b[0]++;");
    assert!(
        matches!(&plain[0].kind, StmtKind::ArrayAssign { array, .. } if array == "b"),
        "expected the element increment to be claimed, got {:?}",
        plain[0].kind
    );
}

/// Verifies a ternary at statement position is not claimed by the postfix scan.
///
/// `$c ? $a[0]++ : $b;` has a top-level `?`, so the `++` sits in a BRANCH. The scan used to
/// claim the statement, truncate it at the `++`, and report `Expected ':' in ternary operator`
/// from the middle of a fragment it had cut itself. Since #827 the whole statement parses as
/// one ternary expression statement.
#[test]
fn test_ternary_statement_with_an_element_increment_is_not_an_increment_statement() {
    let stmts = parse_source("<?php $c ? $a[0]++ : $b;");
    assert!(
        matches!(&stmts[0].kind, StmtKind::ExprStmt(expr) if matches!(expr.kind, ExprKind::Ternary { .. })),
        "expected one ternary expression statement, got {:?}",
        stmts[0].kind
    );
}

/// Verifies variable-led expressions that are not assignments parse as expression statements
/// (#827, #841): a ternary, a `||`/`&&` guard, `instanceof`, and a bare `and`/`or` chain. A
/// missing `=` still reports as one.
#[test]
fn test_variable_led_expression_statements_parse() {
    for source in [
        "<?php $flag ? left() : right();",
        "<?php $ok || throw new RuntimeException('x');",
        "<?php $ok && $v = 5;",
        "<?php $o instanceof Foo;",
        "<?php $ok and go();",
        "<?php $n ?? fallback();",
    ] {
        let stmts = parse_source(source);
        assert!(
            matches!(&stmts[0].kind, StmtKind::ExprStmt(_)),
            "{source}: expected an expression statement, got {:?}",
            stmts[0].kind
        );
    }
    assert!(parse_fails("<?php $x \"hi\";"));
}

/// Verifies that `<?php echo ~$x;` parses as a bitwise NOT unary operation.
/// The `~` operator inverts bits of its operand.
#[test]
fn test_bitwise_not_parses() {
    let stmts = parse_source("<?php echo ~$x;");
    assert_eq!(
        echoed_expr(&stmts),
        &ExprKind::BitNot(Box::new(Expr::var("x")))
    );
}

/// Verifies that `<?php echo $arr[0]();` parses as an ExprCall node whose callee is an
/// ArrayAccess. This exercises callable expressions where the callee is a subscript result.
#[test]
fn test_expr_call_parses() {
    // `$arr[0]()` calls the result of an array access — an ExprCall node.
    let stmts = parse_source("<?php echo $arr[0]();");
    match echoed_expr(&stmts) {
        ExprKind::ExprCall { callee, args } => {
            assert!(matches!(callee.kind, ExprKind::ArrayAccess { .. }));
            assert!(args.is_empty());
        }
        other => panic!("expected ExprCall, got {:?}", other),
    }
}

// --- Null coalescing precedence ---
