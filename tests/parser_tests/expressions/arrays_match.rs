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

// --- Long-form `array(...)` literal ---

/// Verifies that the long-form `array(1, 2, 3)` parses to the same `ArrayLiteral` node as the
/// short `[...]` form — it is the array-literal language construct, not a function call.
#[test]
fn test_parse_long_array_indexed() {
    let stmts = parse_source("<?php $a = array(1, 2, 3);");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        match &value.kind {
            ExprKind::ArrayLiteral(items) => assert_eq!(items.len(), 3),
            other => panic!("expected ArrayLiteral, got {:?}", other),
        }
    } else {
        panic!("expected Assign");
    }
}

/// Verifies that `array("a" => 1)` parses to an `ArrayLiteralAssoc`, matching the short keyed form.
#[test]
fn test_parse_long_array_assoc() {
    let stmts = parse_source("<?php $m = array(\"a\" => 1, \"b\" => 2);");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        assert!(matches!(&value.kind, ExprKind::ArrayLiteralAssoc(_)));
    } else {
        panic!("expected Assign");
    }
}

/// Verifies that an empty long-form `array()` parses to an empty `ArrayLiteral`.
#[test]
fn test_parse_long_array_empty() {
    let stmts = parse_source("<?php $a = array();");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        match &value.kind {
            ExprKind::ArrayLiteral(items) => assert!(items.is_empty()),
            other => panic!("expected empty ArrayLiteral, got {:?}", other),
        }
    } else {
        panic!("expected Assign");
    }
}

/// Verifies that the long form nests like the short form: `array("x" => array(1, 2))` yields an
/// `ArrayLiteralAssoc` whose value is itself an `ArrayLiteral`.
#[test]
fn test_parse_long_array_nested() {
    let stmts = parse_source("<?php $a = array(\"x\" => array(1, 2));");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        match &value.kind {
            ExprKind::ArrayLiteralAssoc(items) => {
                assert_eq!(items.len(), 1);
                assert!(matches!(items[0].1.kind, ExprKind::ArrayLiteral(_)));
            }
            other => panic!("expected ArrayLiteralAssoc, got {:?}", other),
        }
    } else {
        panic!("expected Assign");
    }
}

// --- Foreach with key => value ---
