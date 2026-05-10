//! Purpose:
//! Integration or regression tests for parser AST coverage of expression modern PHP operators logical and instanceof, including word logical and lower than oror, word logical or lower than andand, and word logical xor precedence.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_word_logical_and_lower_than_oror() {
    // $a || $b and $c should parse as ($a || $b) and $c — PHP precedence
    let stmts = parse_source("<?php echo $a || $b and $c;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::var("a"), BinOp::Or, Expr::var("b")),
        BinOp::And,
        Expr::var("c"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_word_logical_or_lower_than_andand() {
    // $a && $b or $c should parse as ($a && $b) or $c — PHP precedence
    let stmts = parse_source("<?php echo $a && $b or $c;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b")),
        BinOp::Or,
        Expr::var("c"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_word_logical_xor_precedence() {
    // $a xor $b and $c should parse as $a xor ($b and $c)
    let stmts = parse_source("<?php echo $a xor $b and $c;");
    let expected = Stmt::echo(Expr::binop(
        Expr::var("a"),
        BinOp::Xor,
        Expr::binop(Expr::var("b"), BinOp::And, Expr::var("c")),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_word_logical_xor_higher_than_or() {
    // $a or $b xor $c should parse as $a or ($b xor $c)
    let stmts = parse_source("<?php echo $a or $b xor $c;");
    let expected = Stmt::echo(Expr::binop(
        Expr::var("a"),
        BinOp::Or,
        Expr::binop(Expr::var("b"), BinOp::Xor, Expr::var("c")),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_print_expression_binds_tighter_than_word_and() {
    let stmts = parse_source("<?php echo print $a and $b;");
    let expected = Stmt::echo(Expr::binop(
        Expr::print(Expr::var("a")),
        BinOp::And,
        Expr::var("b"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_print_expression_operand_accepts_short_ternary() {
    let stmts = parse_source("<?php echo print $a ?: $b;");
    let expected = Stmt::echo(Expr::print(Expr::new(
        ExprKind::ShortTernary {
            value: Box::new(Expr::var("a")),
            default: Box::new(Expr::var("b")),
        },
        elephc::span::Span::dummy(),
    )));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_parse_instanceof_expression() {
    let stmts = parse_source("<?php echo $a instanceof Foo;");
    let expected = Stmt::echo(Expr::instance_of(
        Expr::var("a"),
        Name::unqualified("Foo"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_parse_dynamic_instanceof_variable_target() {
    let stmts = parse_source("<?php echo $a instanceof $className;");
    let expected = Stmt::echo(Expr::dynamic_instance_of(
        Expr::var("a"),
        Expr::var("className"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_parse_dynamic_instanceof_property_and_array_targets() {
    let stmts = parse_source("<?php echo $a instanceof $holder->className; echo $a instanceof $names[0];");
    let property_target = Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::var("holder")),
            property: "className".to_string(),
        },
        elephc::span::Span::dummy(),
    );
    let array_target = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::var("names")),
            index: Box::new(Expr::int_lit(0)),
        },
        elephc::span::Span::dummy(),
    );
    assert_eq!(
        stmts,
        vec![
            Stmt::echo(Expr::dynamic_instance_of(Expr::var("a"), property_target)),
            Stmt::echo(Expr::dynamic_instance_of(Expr::var("a"), array_target)),
        ]
    );
}

#[test]
fn test_parse_parenthesized_dynamic_instanceof_expression_target() {
    let stmts = parse_source("<?php echo $a instanceof ($prefix . $suffix);");
    let target = Expr::binop(Expr::var("prefix"), BinOp::Concat, Expr::var("suffix"));
    let expected = Stmt::echo(Expr::dynamic_instance_of(Expr::var("a"), target));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_instanceof_binds_tighter_than_not() {
    let stmts = parse_source("<?php echo !$a instanceof Foo;");
    let expected = Stmt::echo(Expr::new(
        ExprKind::Not(Box::new(Expr::instance_of(
            Expr::var("a"),
            Name::unqualified("Foo"),
        ))),
        elephc::span::Span::dummy(),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_instanceof_binds_tighter_than_addition() {
    let stmts = parse_source("<?php echo 1 + $a instanceof Foo;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(1),
        BinOp::Add,
        Expr::instance_of(Expr::var("a"), Name::unqualified("Foo")),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_dynamic_instanceof_binds_tighter_than_concat() {
    let stmts = parse_source("<?php echo $a instanceof $className . \"!\";");
    let expected = Stmt::echo(Expr::binop(
        Expr::dynamic_instance_of(Expr::var("a"), Expr::var("className")),
        BinOp::Concat,
        Expr::string_lit("!"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_instanceof_chains_left_to_right() {
    let stmts = parse_source("<?php echo $a instanceof Foo instanceof Bar;");
    let expected = Stmt::echo(Expr::instance_of(
        Expr::instance_of(Expr::var("a"), Name::unqualified("Foo")),
        Name::unqualified("Bar"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_dynamic_instanceof_chains_left_to_right() {
    let stmts = parse_source("<?php echo $a instanceof $className instanceof Foo;");
    let expected = Stmt::echo(Expr::instance_of(
        Expr::dynamic_instance_of(Expr::var("a"), Expr::var("className")),
        Name::unqualified("Foo"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_instanceof_accepts_special_class_targets() {
    let stmts = parse_source("<?php echo $a instanceof self; echo $a instanceof parent; echo $a instanceof static;");
    assert_eq!(
        stmts,
        vec![
            Stmt::echo(Expr::instance_of(Expr::var("a"), Name::unqualified("self"))),
            Stmt::echo(Expr::instance_of(Expr::var("a"), Name::unqualified("parent"))),
            Stmt::echo(Expr::instance_of(Expr::var("a"), Name::unqualified("static"))),
        ]
    );
}

