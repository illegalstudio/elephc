use elephc::lexer::tokenize;
use elephc::parser::ast::{BinOp, Expr, Stmt, StmtKind};
use elephc::parser::parse;

fn parse_source(src: &str) -> Vec<Stmt> {
    let tokens = tokenize(src).unwrap();
    parse(&tokens).unwrap()
}

fn parse_fails(src: &str) -> bool {
    let tokens = match tokenize(src) {
        Ok(t) => t,
        Err(_) => return true,
    };
    parse(&tokens).is_err()
}

// --- Echo ---

#[test]
fn test_echo_string_literal() {
    let stmts = parse_source("<?php echo \"hello\";");
    assert_eq!(stmts, vec![Stmt::echo(Expr::string_lit("hello"))]);
}

#[test]
fn test_echo_integer() {
    let stmts = parse_source("<?php echo 42;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::int_lit(42))]);
}

// --- Assignment ---

#[test]
fn test_variable_assignment() {
    let stmts = parse_source("<?php $x = 10;");
    assert_eq!(stmts, vec![Stmt::assign("x", Expr::int_lit(10))]);
}

#[test]
fn test_echo_variable() {
    let stmts = parse_source("<?php $x = 5; echo $x;");
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[1], Stmt::echo(Expr::var("x")));
}

// --- Unary ---

#[test]
fn test_negative_integer() {
    let stmts = parse_source("<?php echo -7;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::negate(Expr::int_lit(7)))]);
}

// --- Operator precedence ---

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

#[test]
fn test_parenthesized_expr() {
    let stmts = parse_source("<?php echo (2 + 3) * 4;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(2), BinOp::Add, Expr::int_lit(3)),
        BinOp::Mul,
        Expr::int_lit(4),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_left_associativity() {
    let stmts = parse_source("<?php echo 1 - 2 - 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Sub, Expr::int_lit(2)),
        BinOp::Sub,
        Expr::int_lit(3),
    ));
    assert_eq!(stmts, vec![expected]);
}

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

#[test]
fn test_concat_lower_than_comparison() {
    // "x" . 1 < 2 should parse as "x" . (1 < 2)
    let stmts = parse_source("<?php echo \"x\" . 1 < 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::string_lit("x"),
        BinOp::Concat,
        Expr::binop(Expr::int_lit(1), BinOp::Lt, Expr::int_lit(2)),
    ));
    assert_eq!(stmts, vec![expected]);
}

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

#[test]
fn test_if_parses() {
    let stmts = parse_source("<?php if (1 == 1) { echo \"yes\"; }");
    assert_eq!(stmts.len(), 1);
    assert!(matches!(&stmts[0].kind, StmtKind::If { .. }));
}

#[test]
fn test_if_else_parses() {
    let stmts = parse_source("<?php if (1) { echo \"a\"; } else { echo \"b\"; }");
    if let StmtKind::If { else_body, .. } = &stmts[0].kind {
        assert!(else_body.is_some());
    } else {
        panic!("expected If");
    }
}

#[test]
fn test_if_elseif_else_parses() {
    let stmts = parse_source(
        "<?php if (1) { echo \"a\"; } elseif (2) { echo \"b\"; } else { echo \"c\"; }",
    );
    if let StmtKind::If { elseif_clauses, else_body, .. } = &stmts[0].kind {
        assert_eq!(elseif_clauses.len(), 1);
        assert!(else_body.is_some());
    } else {
        panic!("expected If");
    }
}

#[test]
fn test_while_parses() {
    let stmts = parse_source("<?php while (1) { echo \"loop\"; }");
    assert!(matches!(&stmts[0].kind, StmtKind::While { .. }));
}

#[test]
fn test_for_parses() {
    let stmts = parse_source("<?php for ($i = 0; $i < 10; $i++) { echo $i; }");
    assert!(matches!(&stmts[0].kind, StmtKind::For { .. }));
}

#[test]
fn test_break_parses() {
    let stmts = parse_source("<?php while (1) { break; }");
    if let StmtKind::While { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Break));
    }
}

#[test]
fn test_continue_parses() {
    let stmts = parse_source("<?php while (1) { continue; }");
    if let StmtKind::While { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Continue));
    }
}

// --- Functions ---

#[test]
fn test_function_declaration_parses() {
    let stmts = parse_source("<?php function foo($a, $b) { return $a; }");
    if let StmtKind::FunctionDecl { name, params, body } = &stmts[0].kind {
        assert_eq!(name, "foo");
        assert_eq!(params, &["a".to_string(), "b".to_string()]);
        assert_eq!(body.len(), 1);
    } else {
        panic!("expected FunctionDecl");
    }
}

#[test]
fn test_function_no_params() {
    let stmts = parse_source("<?php function noop() { return; }");
    if let StmtKind::FunctionDecl { params, .. } = &stmts[0].kind {
        assert!(params.is_empty());
    }
}

#[test]
fn test_return_value_parses() {
    let stmts = parse_source("<?php function f() { return 42; }");
    if let StmtKind::FunctionDecl { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Return(Some(_))));
    }
}

#[test]
fn test_return_void_parses() {
    let stmts = parse_source("<?php function f() { return; }");
    if let StmtKind::FunctionDecl { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Return(None)));
    }
}

#[test]
fn test_multiple_statements() {
    let stmts = parse_source("<?php $a = 1; $b = 2; echo $a;");
    assert_eq!(stmts.len(), 3);
}

// --- Parse errors ---

#[test]
fn test_missing_semicolon() {
    assert!(parse_fails("<?php echo \"hi\""));
}

#[test]
fn test_missing_closing_brace() {
    assert!(parse_fails("<?php if (1) { echo \"a\";"));
}

#[test]
fn test_missing_condition_parens() {
    assert!(parse_fails("<?php if 1 { echo \"a\"; }"));
}

// --- Strict comparison ---

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

#[test]
fn test_include_parses() {
    let stmts = parse_source("<?php include 'file.php';");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Include { path, once, required } = &stmts[0].kind {
        assert_eq!(path, "file.php");
        assert!(!once);
        assert!(!required);
    } else {
        panic!("expected Include");
    }
}

#[test]
fn test_require_parses() {
    let stmts = parse_source("<?php require 'file.php';");
    if let StmtKind::Include { path, once, required } = &stmts[0].kind {
        assert_eq!(path, "file.php");
        assert!(!once);
        assert!(required);
    } else {
        panic!("expected Include (require)");
    }
}

#[test]
fn test_include_once_parses() {
    let stmts = parse_source("<?php include_once 'file.php';");
    if let StmtKind::Include { once, required, .. } = &stmts[0].kind {
        assert!(once);
        assert!(!required);
    } else {
        panic!("expected Include (include_once)");
    }
}

#[test]
fn test_require_once_parses() {
    let stmts = parse_source("<?php require_once 'file.php';");
    if let StmtKind::Include { once, required, .. } = &stmts[0].kind {
        assert!(once);
        assert!(required);
    } else {
        panic!("expected Include (require_once)");
    }
}

#[test]
fn test_include_with_parens_parses() {
    let stmts = parse_source("<?php include('file.php');");
    if let StmtKind::Include { path, .. } = &stmts[0].kind {
        assert_eq!(path, "file.php");
    } else {
        panic!("expected Include");
    }
}

// --- Float ---

#[test]
fn test_float_literal() {
    let stmts = parse_source("<?php echo 3.14;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::float_lit(3.14))]);
}

#[test]
fn test_negative_float() {
    let stmts = parse_source("<?php echo -3.14;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::negate(Expr::float_lit(3.14)))]);
}
