//! Purpose:
//! Integration or regression tests for parser AST coverage of expression parsing, including string indexing uses array access AST, assoc array, and match.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Verifies that `<?php echo $name[1];` parses as an `ArrayAccess` expression with an integer
/// index. String indexing in PHP uses the same `ArrayAccess` AST node as array indexing.
#[test]
fn test_parse_string_indexing_uses_array_access_ast() {
    let stmts = parse_source("<?php echo $name[1];");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::ArrayAccess { array, index } => {
                assert_eq!(array.kind, ExprKind::Variable("name".into()));
                assert_eq!(index.kind, ExprKind::IntLiteral(1));
            }
            other => panic!("expected array access, got {:?}", other),
        },
        other => panic!("expected echo, got {:?}", other),
    }
}

/// Verifies that `<?php $m = ["a" => 1];` parses to an `Assign` with an `ArrayLiteralAssoc` value.
#[test]
fn test_parse_assoc_array() {
    let stmts = parse_source("<?php $m = [\"a\" => 1];");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        assert!(matches!(&value.kind, ExprKind::ArrayLiteralAssoc(_)));
    } else {
        panic!("expected Assign");
    }
}

/// Verifies that leading positional elements are preserved when a later array
/// entry uses an explicit key.
#[test]
fn test_parse_mixed_array_preserves_leading_positional_element() {
    let stmts = parse_source("<?php $m = [10, \"a\" => 1];");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Assign { value, .. } = &stmts[0].kind else {
        panic!("expected Assign");
    };
    let ExprKind::ArrayLiteralAssoc(items) = &value.kind else {
        panic!("expected ArrayLiteralAssoc");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].0.kind, ExprKind::IntLiteral(0));
    assert_eq!(items[0].1.kind, ExprKind::IntLiteral(10));
}

/// Verifies a spread inside an associative literal parses to the sentinel pair shape every
/// semantic consumer detects with `parser::ast::assoc_spread_source`: the pair's key IS the
/// spread and its value is an inert `null` placeholder that carries no meaning of its own.
#[test]
fn test_parse_assoc_array_spread_uses_the_null_placeholder_pair_shape() {
    let stmts = parse_source("<?php $m = [\"a\" => 1, ...$extra];");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Assign { value, .. } = &stmts[0].kind else {
        panic!("expected Assign");
    };
    let ExprKind::ArrayLiteralAssoc(items) = &value.kind else {
        panic!("expected ArrayLiteralAssoc");
    };
    assert_eq!(items.len(), 2);
    assert!(elephc::parser::ast::assoc_spread_source(&items[0].0, &items[0].1).is_none());
    let source = elephc::parser::ast::assoc_spread_source(&items[1].0, &items[1].1)
        .expect("the second entry is a spread");
    assert_eq!(source.kind, ExprKind::Variable("extra".into()));
    assert_eq!(items[1].1.kind, ExprKind::Null);
}

// --- Switch ---

/// Verifies that `<?php $x = match(1) { 1 => "a" };` parses to an `Assign` with a `Match`
/// expression. The `match` arm subject and single arm are preserved in the AST.
#[test]
fn test_parse_match() {
    let stmts = parse_source("<?php $x = match(1) { 1 => \"a\" };");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        assert!(matches!(&value.kind, ExprKind::Match { .. }));
    } else {
        panic!("expected Assign containing Match");
    }
}

/// Verifies that standalone `match` expressions parse as expression statements.
#[test]
fn test_parse_standalone_match_expression_statement() {
    let stmts = parse_source("<?php match (1) { 1 => 2 }; echo 3;");
    assert_eq!(stmts.len(), 2);
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => assert!(matches!(&expr.kind, ExprKind::Match { .. })),
        other => panic!("expected ExprStmt containing Match, got {:?}", other),
    }
}

// --- Foreach with key => value ---
