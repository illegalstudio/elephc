//! Purpose:
//! Integration or regression tests for parser AST coverage of statements, including echo string literal, echo integer, and variable assignment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets cover successful AST shapes plus malformed syntax that must fail during parsing.

use super::*;

/// Verifies that `<?php echo "hello";` parses to a single `Echo` stmt containing a `StringLiteral`.
#[test]
fn test_echo_string_literal() {
    let stmts = parse_source("<?php echo \"hello\";");
    assert_eq!(stmts, vec![Stmt::echo(Expr::string_lit("hello"))]);
}

/// Verifies that `<?php echo 42;` parses to a single `Echo` stmt containing an `IntLiteral(42)`.
#[test]
fn test_echo_integer() {
    let stmts = parse_source("<?php echo 42;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::int_lit(42))]);
}

/// Verifies that `<?php $x = 10;` parses to a simple `Assign` stmt with variable name "x"
/// and integer literal value 10.
#[test]
fn test_variable_assignment() {
    let stmts = parse_source("<?php $x = 10;");
    assert_eq!(stmts, vec![Stmt::assign("x", Expr::int_lit(10))]);
}

/// Verifies that `<?php $x = 5; echo $x;` parses to two stmts: assign and echo.
/// Asserts the echoed expression is a `Variable("x")`.
#[test]
fn test_echo_variable() {
    let stmts = parse_source("<?php $x = 5; echo $x;");
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[1], Stmt::echo(Expr::var("x")));
}

// --- Unary ---

/// Verifies that `<?php $a = 1; $b = 2; echo $a;` parses to three stmts in order.
#[test]
fn test_multiple_statements() {
    let stmts = parse_source("<?php $a = 1; $b = 2; echo $a;");
    assert_eq!(stmts.len(), 3);
}

// --- Parse errors ---

/// Verifies that `<?php echo "hi"` (missing semicolon) fails during parsing.
#[test]
fn test_missing_semicolon() {
    assert!(parse_fails("<?php echo \"hi\""));
}

/// Verifies that `<?php if (1) { echo "a";` (missing closing brace) fails during parsing.
#[test]
fn test_missing_closing_brace() {
    assert!(parse_fails("<?php if (1) { echo \"a\";"));
}

/// Verifies that `<?php if 1 { echo "a"; }` (missing parentheses around condition) fails parsing.
#[test]
fn test_missing_condition_parens() {
    assert!(parse_fails("<?php if 1 { echo \"a\"; }"));
}

/// Verifies that `<?php print "hello";` parses as an `ExprStmt` wrapping `Expr::print(...)`.
/// PHP's `print` is an expression construct (returns 1), distinct from `echo`.
#[test]
fn test_print_parses_as_expression_statement() {
    let stmts = parse_source("<?php print \"hello\";");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::ExprStmt(Expr::print(Expr::string_lit("hello"))),
            elephc::span::Span::dummy(),
        )]
    );
}

/// Verifies parenthesized expressions are accepted as standalone expression statements.
#[test]
fn test_parenthesized_expression_statement() {
    let stmts = parse_source("<?php (1 + 2);");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::ExprStmt(Expr::binop(Expr::int_lit(1), BinOp::Add, Expr::int_lit(2))),
            elephc::span::Span::dummy(),
        )]
    );
}

/// Standalone arrays, including spreads and nested arrays, are expressions rather than patterns.
#[test]
fn test_array_expression_statements_are_not_destructuring() {
    for source in ["<?php [];", "<?php [...$items];", "<?php [[1], [...$items]];"] {
        let statements = parse_source(source);
        assert_eq!(statements.len(), 1, "{source}");
        assert!(matches!(&statements[0].kind, StmtKind::ExprStmt(Expr { kind: ExprKind::ArrayLiteral(_), .. })), "{source}: {statements:?}");
    }
    let statements = parse_source("<?php [1, 2][0];");
    assert!(matches!(&statements[0].kind, StmtKind::ExprStmt(Expr { kind: ExprKind::ArrayAccess { .. }, .. })));
}

/// Matching the outer bracket retains positional, nested and keyed destructuring assignments.
#[test]
fn test_bracket_assignment_still_selects_destructuring() {
    for source in ["<?php [$a] = $items;", "<?php [[$a], $b] = $items;", "<?php ['key' => $a] = $items;"] {
        let statements = parse_source(source);
        assert!(!statements.is_empty(), "{source}");
        assert!(!matches!(&statements[0].kind, StmtKind::ExprStmt(_)), "{source}");
    }
    for source in ["<?php [...$a] = $items;", "<?php [1] = $items;", "<?php [...$items;"] {
        assert!(parse_fails(source), "{source}");
    }
}

/// Verifies `$this->n++;` parses to the same read-modify-write statement as `$this->n += 1;`.
/// Regression: the `$this` statement parser used to reject the trailing `++`.
#[test]
fn test_this_property_postfix_increment_parses_as_compound_assignment() {
    assert_eq!(
        parse_source("<?php $this->n++;"),
        parse_source("<?php $this->n += 1;")
    );
    assert_eq!(
        parse_source("<?php $this->n--;"),
        parse_source("<?php $this->n -= 1;")
    );
}

/// Property updates retain their lowering while array updates preserve PHP incdec semantics.
#[test]
fn test_prefix_increment_on_complex_targets_retains_array_incdec() {
    assert_eq!(
        parse_source("<?php ++$this->n;"),
        parse_source("<?php $this->n += 1;")
    );
    assert_eq!(
        parse_source("<?php ++$obj->n;"),
        parse_source("<?php $obj->n += 1;")
    );
    let statements = parse_source("<?php --$a[0];");
    let StmtKind::ArrayAssign { array, index, value } = &statements[0].kind else {
        panic!("Expected array writeback, got {:?}", statements[0].kind);
    };
    assert_eq!(array, "a");
    assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
    let ExprKind::Assignment { value, prelude, .. } = &value.kind else {
        panic!("Expected captured decrement, got {:?}", value.kind);
    };
    let ExprKind::PreDecrement(old_name) = &value.kind else {
        panic!("Expected PHP decrement, got {:?}", value.kind);
    };
    assert_eq!(prelude.len(), 1);
    assert!(matches!(&prelude[0].kind, StmtKind::Assign { name, value }
        if name == old_name && matches!(value.kind, ExprKind::ArrayAccess { .. })));
}

/// Property array elements capture their value and retain PHP increment semantics.
#[test]
fn test_this_property_element_increment_retains_php_incdec() {
    let statements = parse_source("<?php $this->arr[0]++;");
    let StmtKind::PropertyArrayAssign { object, property, index, value } = &statements[0].kind else {
        panic!("Expected property array writeback, got {:?}", statements[0].kind);
    };
    assert!(matches!(object.kind, ExprKind::This));
    assert_eq!(property, "arr");
    assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
    let ExprKind::Assignment { value, prelude, .. } = &value.kind else {
        panic!("Expected captured increment, got {:?}", value.kind);
    };
    let ExprKind::PreIncrement(old_name) = &value.kind else {
        panic!("Expected PHP increment, got {:?}", value.kind);
    };
    assert_eq!(prelude.len(), 1);
    let StmtKind::Assign { name, value } = &prelude[0].kind else {
        panic!("Expected captured property element, got {:?}", prelude[0].kind);
    };
    assert_eq!(name, old_name);
    let ExprKind::ArrayAccess { array, index } = &value.kind else {
        panic!("Expected property element read, got {:?}", value.kind);
    };
    assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
    assert!(matches!(&array.kind, ExprKind::PropertyAccess { object, property }
        if matches!(object.kind, ExprKind::This) && property == "arr"));
}
