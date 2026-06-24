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

// --- clone expression precedence ---

/// Verifies `clone` parses as a unary prefix expression wrapping its operand.
#[test]
fn test_clone_parses_unary() {
    let stmts = parse_source("<?php echo clone $a;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::Clone(Box::new(Expr::var("a"))));
}

/// Verifies `clone` binds tighter than `**` (pow), matching PHP: `clone $a ** 2`
/// parses as `(clone $a) ** 2`, so the cloned object is the pow left operand.
#[test]
fn test_clone_binds_tighter_than_pow() {
    let stmts = parse_source("<?php echo clone $a ** 2;");
    assert!(matches!(
        echoed_expr(&stmts),
        ExprKind::BinaryOp { op: BinOp::Pow, left, right }
            if matches!(left.kind, ExprKind::Clone(_))
                && matches!(right.kind, ExprKind::IntLiteral(2))
    ));
}

/// Verifies postfix property access binds tighter than `clone`, matching PHP:
/// `clone $a->n` parses as `clone ($a->n)`, so the cloned value is the property.
#[test]
fn test_clone_operand_takes_postfix_property() {
    let stmts = parse_source("<?php echo clone $a->n;");
    assert!(matches!(
        echoed_expr(&stmts),
        ExprKind::Clone(inner) if matches!(inner.kind, ExprKind::PropertyAccess { .. })
    ));
}

/// Verifies `clone new P()` parses as `clone (new P())` — the `new` expression is
/// the cloned operand, matching PHP's evaluation order.
#[test]
fn test_clone_new_object() {
    let stmts = parse_source("<?php echo clone new P();");
    assert!(matches!(
        echoed_expr(&stmts),
        ExprKind::Clone(inner) if matches!(inner.kind, ExprKind::NewObject { .. })
    ));
}

/// Verifies `new $arr['k']()` parses as a dynamic `new` whose class-name expression is the
/// array access, with the trailing `()` consumed as the (empty) constructor argument list
/// rather than as a call on the array element.
#[test]
fn test_new_dynamic_array_access_class_name() {
    let stmts = parse_source("<?php echo new $arr['k']();");
    assert!(matches!(
        echoed_expr(&stmts),
        ExprKind::NewDynamic { name_expr, args }
            if args.is_empty()
                && matches!(name_expr.kind, ExprKind::ArrayAccess { .. })
    ));
}

/// Verifies `new $obj->kind(7)` parses as a dynamic `new` whose class-name expression is the
/// property access, with `7` forwarded as a constructor argument — the `(7)` is the ctor
/// argument list, not a method call on the property.
#[test]
fn test_new_dynamic_property_class_name() {
    let stmts = parse_source("<?php echo new $obj->kind(7);");
    assert!(matches!(
        echoed_expr(&stmts),
        ExprKind::NewDynamic { name_expr, args }
            if args.len() == 1
                && matches!(name_expr.kind, ExprKind::PropertyAccess { ref property, .. } if property == "kind")
    ));
}

/// Verifies `new $cfg['cars']['sport']()` parses the full nested array-access chain as the
/// class-name expression (an `ArrayAccess` whose `array` is itself an `ArrayAccess`).
#[test]
fn test_new_dynamic_nested_array_access_class_name() {
    let stmts = parse_source("<?php echo new $cfg['cars']['sport']();");
    assert!(matches!(
        echoed_expr(&stmts),
        ExprKind::NewDynamic { name_expr, .. }
            if matches!(&name_expr.kind, ExprKind::ArrayAccess { array, .. }
                if matches!(array.kind, ExprKind::ArrayAccess { .. }))
    ));
}

/// Verifies the PHP 8.0 `new (expr)(args)` form parses the parenthesized expression as the
/// class-name expression of a dynamic `new`, with the following `()` as the ctor arguments.
#[test]
fn test_new_dynamic_parenthesized_expr_class_name() {
    let stmts = parse_source("<?php echo new (pick())(7);");
    assert!(matches!(
        echoed_expr(&stmts),
        ExprKind::NewDynamic { name_expr, args }
            if args.len() == 1
                && matches!(name_expr.kind, ExprKind::FunctionCall { .. })
    ));
}

// --- Null coalescing precedence ---
