//! Purpose:
//! Integration or regression tests for parser AST coverage of expression parsing, including string indexing uses array access AST, assoc array, and match.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;
use elephc::parser::ast::ArrayEntry;

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

/// Verifies a spread beside an explicit key parses to the ORDERED entry list, not to either
/// single-shape node.
///
/// `ArrayLiteral` has no place for a key and `ArrayLiteralAssoc` no place for a keyless spread,
/// so this shape used to lose whichever did not fit. The order of the entries is the part worth
/// asserting directly: a spread's elements take the next free integer key at the position the
/// spread occupies, so a parser that appended the spread at one end would still satisfy a test
/// that only counted them.
#[test]
fn test_parse_spread_beside_a_key_keeps_entry_order() {
    let stmts = parse_source("<?php $m = [...$src, \"a\" => 1, 7];");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Assign { value, .. } = &stmts[0].kind else {
        panic!("expected Assign");
    };
    let ExprKind::ArrayLiteralMixed(entries) = &value.kind else {
        panic!("expected ArrayLiteralMixed, got {:?}", value.kind);
    };
    assert_eq!(entries.len(), 3);
    match &entries[0] {
        ArrayEntry::Spread(source) => assert!(matches!(&source.kind, ExprKind::Spread(_))),
        other => panic!("expected a spread first, got {:?}", other),
    }
    match &entries[1] {
        ArrayEntry::Keyed(key, entry_value) => {
            assert_eq!(key.kind, ExprKind::StringLiteral("a".into()));
            assert_eq!(entry_value.kind, ExprKind::IntLiteral(1));
        }
        other => panic!("expected a keyed entry second, got {:?}", other),
    }
    match &entries[2] {
        // The bare element carries NO key: how many integer slots the spread consumed is a
        // runtime fact, so the parser must not number it.
        ArrayEntry::Value(entry_value) => assert_eq!(entry_value.kind, ExprKind::IntLiteral(7)),
        other => panic!("expected a bare value third, got {:?}", other),
    }
}

/// Verifies keys collected BEFORE the first spread move into the entry list with it.
///
/// The literal is built as pairs until a spread appears, and the entry list is the only thing
/// the node is made from once it is non-empty. Leaving the earlier pairs behind is what made
/// `["c" => 8, ...$v]` come out as just the spread.
#[test]
fn test_parse_key_before_spread_migrates_into_entries() {
    let stmts = parse_source("<?php $m = [\"c\" => 8, ...$v];");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Assign { value, .. } = &stmts[0].kind else {
        panic!("expected Assign");
    };
    let ExprKind::ArrayLiteralMixed(entries) = &value.kind else {
        panic!("expected ArrayLiteralMixed, got {:?}", value.kind);
    };
    assert_eq!(entries.len(), 2);
    assert!(matches!(&entries[0], ArrayEntry::Keyed(..)));
    assert!(matches!(&entries[1], ArrayEntry::Spread(_)));
}

/// Verifies a literal with no spread still parses to the plain associative node.
///
/// The entry list exists for the one shape the other two nodes cannot hold; everything else must
/// keep the node it had, or every pass that special-cases `ArrayLiteralAssoc` quietly stops
/// seeing the common case.
#[test]
fn test_parse_keys_without_a_spread_stay_associative() {
    let stmts = parse_source("<?php $m = [10, \"a\" => 1];");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Assign { value, .. } = &stmts[0].kind else {
        panic!("expected Assign");
    };
    assert!(matches!(&value.kind, ExprKind::ArrayLiteralAssoc(_)));
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
