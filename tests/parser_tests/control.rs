//! Purpose:
//! Integration or regression tests for parser AST coverage of control, including if parses, if else parses, and if elseif else parses.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

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
    if let StmtKind::If {
        elseif_clauses,
        else_body,
        ..
    } = &stmts[0].kind
    {
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
fn test_do_while_parses() {
    let stmts = parse_source("<?php do { echo \"loop\"; } while (1);");
    assert!(matches!(&stmts[0].kind, StmtKind::DoWhile { .. }));
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
        assert!(matches!(&body[0].kind, StmtKind::Break(1)));
    }
}

#[test]
fn test_multilevel_break_parses() {
    let stmts = parse_source("<?php while (1) { while (1) { break 2; } }");
    if let StmtKind::While { body, .. } = &stmts[0].kind {
        if let StmtKind::While { body, .. } = &body[0].kind {
            assert!(matches!(&body[0].kind, StmtKind::Break(2)));
        } else {
            panic!("expected nested While");
        }
    } else {
        panic!("expected While");
    }
}

#[test]
fn test_continue_parses() {
    let stmts = parse_source("<?php while (1) { continue; }");
    if let StmtKind::While { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Continue(1)));
    }
}

#[test]
fn test_multilevel_continue_parses() {
    let stmts = parse_source("<?php while (1) { while (1) { continue (2); } }");
    if let StmtKind::While { body, .. } = &stmts[0].kind {
        if let StmtKind::While { body, .. } = &body[0].kind {
            assert!(matches!(&body[0].kind, StmtKind::Continue(2)));
        } else {
            panic!("expected nested While");
        }
    } else {
        panic!("expected While");
    }
}

// --- Functions ---

#[test]
fn test_parse_switch() {
    let stmts =
        parse_source("<?php switch ($x) { case 1: echo \"one\"; break; default: echo \"other\"; }");
    assert_eq!(stmts.len(), 1);
    assert!(matches!(&stmts[0].kind, StmtKind::Switch { .. }));
}

// --- Match ---

#[test]
fn test_parse_foreach_key_value() {
    let stmts = parse_source("<?php foreach ($a as $k => $v) {}");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Foreach {
        key_var, value_var, ..
    } = &stmts[0].kind
    {
        assert_eq!(key_var, &Some("k".to_string()));
        assert_eq!(value_var, "v");
    } else {
        panic!("expected Foreach");
    }
}

#[test]
fn test_parse_foreach_value_only() {
    let stmts = parse_source("<?php foreach ($a as $value) {}");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Foreach {
        key_var, value_var, ..
    } = &stmts[0].kind
    {
        assert_eq!(key_var, &None);
        assert_eq!(value_var, "value");
    } else {
        panic!("expected Foreach");
    }
}
