//! Purpose:
//! Integration or regression tests for parser AST coverage of control, including if parses, if else parses, and if elseif else parses.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Verifies that `<?php if (1 == 1) { echo "yes"; }` parses to an `If` statement.
#[test]
fn test_if_parses() {
    let stmts = parse_source("<?php if (1 == 1) { echo \"yes\"; }");
    assert_eq!(stmts.len(), 1);
    assert!(matches!(&stmts[0].kind, StmtKind::If { .. }));
}

/// Verifies that `<?php if (1) { echo "a"; } else { echo "b"; }` parses to an `If` with `else_body` present.
#[test]
fn test_if_else_parses() {
    let stmts = parse_source("<?php if (1) { echo \"a\"; } else { echo \"b\"; }");
    if let StmtKind::If { else_body, .. } = &stmts[0].kind {
        assert!(else_body.is_some());
    } else {
        panic!("expected If");
    }
}

/// Verifies that `<?php if (1) { echo "a"; } elseif (2) { echo "b"; } else { echo "c"; }`
/// parses to an `If` with one `elseif_clause` and an `else_body`.
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

/// Verifies that `<?php while (1) { echo "loop"; }` parses to a `While` statement.
#[test]
fn test_while_parses() {
    let stmts = parse_source("<?php while (1) { echo \"loop\"; }");
    assert!(matches!(&stmts[0].kind, StmtKind::While { .. }));
}

/// Verifies that `<?php do { echo "loop"; } while (1);` parses to a `DoWhile` statement.
#[test]
fn test_do_while_parses() {
    let stmts = parse_source("<?php do { echo \"loop\"; } while (1);");
    assert!(matches!(&stmts[0].kind, StmtKind::DoWhile { .. }));
}

/// Verifies that `<?php for ($i = 0; $i < 10; $i++) { echo $i; }` parses to a `For` statement.
#[test]
fn test_for_parses() {
    let stmts = parse_source("<?php for ($i = 0; $i < 10; $i++) { echo $i; }");
    assert!(matches!(&stmts[0].kind, StmtKind::For { .. }));
}

/// Verifies that `<?php while (1) { break; }` parses with the `Break(1)` statement nested
/// inside `While`. The argument 1 means break one level.
#[test]
fn test_break_parses() {
    let stmts = parse_source("<?php while (1) { break; }");
    if let StmtKind::While { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Break(1)));
    }
}

/// Verifies that `<?php while (1) { while (1) { break 2; } }` parses with `Break(2)` at depth 2.
/// The numeric argument must be preserved correctly across nesting levels.
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

/// Verifies that `<?php while (1) { continue; }` parses with `Continue(1)` inside `While`.
#[test]
fn test_continue_parses() {
    let stmts = parse_source("<?php while (1) { continue; }");
    if let StmtKind::While { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Continue(1)));
    }
}

/// Verifies that `<?php while (1) { while (1) { continue (2); } }` parses with `Continue(2)`
/// at depth 2. The parenthesized form of the level argument must be accepted.
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

/// Verifies that `<?php switch ($x) { case 1: echo "one"; break; default: echo "other"; }`
/// parses to a `Switch` statement with a default case.
#[test]
fn test_parse_switch() {
    let stmts =
        parse_source("<?php switch ($x) { case 1: echo \"one\"; break; default: echo \"other\"; }");
    assert_eq!(stmts.len(), 1);
    assert!(matches!(&stmts[0].kind, StmtKind::Switch { .. }));
}

// --- Match ---

/// Verifies that `<?php foreach ($a as $k => $v) {}` parses with `key_var = Some("k")`,
/// `value_var = "v"`, and `value_by_ref = false`.
#[test]
fn test_parse_foreach_key_value() {
    let stmts = parse_source("<?php foreach ($a as $k => $v) {}");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Foreach {
        key_var,
        value_var,
        value_by_ref,
        ..
    } = &stmts[0].kind
    {
        assert_eq!(key_var, &Some("k".to_string()));
        assert_eq!(value_var, "v");
        assert!(!value_by_ref);
    } else {
        panic!("expected Foreach");
    }
}

/// Verifies that `<?php foreach ($a as $value) {}` parses with no key variable,
/// `value_var = "value"`, and `value_by_ref = false`.
#[test]
fn test_parse_foreach_value_only() {
    let stmts = parse_source("<?php foreach ($a as $value) {}");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Foreach {
        key_var,
        value_var,
        value_by_ref,
        ..
    } = &stmts[0].kind
    {
        assert_eq!(key_var, &None);
        assert_eq!(value_var, "value");
        assert!(!value_by_ref);
    } else {
        panic!("expected Foreach");
    }
}

/// Verifies that `<?php foreach ($a as &$value) {}` parses with no key variable,
/// `value_var = "value"`, and `value_by_ref = true`.
#[test]
fn test_parse_foreach_value_by_ref() {
    let stmts = parse_source("<?php foreach ($a as &$value) {}");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Foreach {
        key_var,
        value_var,
        value_by_ref,
        ..
    } = &stmts[0].kind
    {
        assert_eq!(key_var, &None);
        assert_eq!(value_var, "value");
        assert!(value_by_ref);
    } else {
        panic!("expected Foreach");
    }
}

/// Verifies that `<?php foreach ($a as $key => &$value) {}` parses with key_var = Some("key"),
/// `value_var = "value"`, and `value_by_ref = true`.
#[test]
fn test_parse_foreach_key_value_by_ref() {
    let stmts = parse_source("<?php foreach ($a as $key => &$value) {}");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Foreach {
        key_var,
        value_var,
        value_by_ref,
        ..
    } = &stmts[0].kind
    {
        assert_eq!(key_var, &Some("key".to_string()));
        assert_eq!(value_var, "value");
        assert!(value_by_ref);
    } else {
        panic!("expected Foreach");
    }
}

/// Verifies `foreach ($a as [$x, $y]) {}` desugars to a `Foreach` whose synthetic
/// `value_var` is bound and whose body starts with a `ListUnpack` of `[$x, $y]`.
#[test]
fn test_parse_foreach_destructure_positional() {
    let stmts = parse_source("<?php foreach ($a as [$x, $y]) {}");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Foreach {
        key_var,
        value_var,
        value_by_ref,
        body,
        ..
    } = &stmts[0].kind
    else {
        panic!("expected Foreach");
    };
    assert_eq!(key_var, &None);
    assert!(value_var.starts_with("__elephc_foreach_destructure_"));
    assert!(!value_by_ref);
    assert!(matches!(
        body.first().map(|s| &s.kind),
        Some(StmtKind::ListUnpack { vars, .. }) if vars.len() == 2
    ));
}

/// Verifies `foreach ($a as $k => [$x, $y]) {}` keeps the key and desugars the value
/// pattern into a leading `ListUnpack`.
#[test]
fn test_parse_foreach_destructure_key_value() {
    let stmts = parse_source("<?php foreach ($a as $k => [$x, $y]) {}");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Foreach {
        key_var,
        value_var,
        body,
        ..
    } = &stmts[0].kind
    else {
        panic!("expected Foreach");
    };
    assert_eq!(key_var, &Some("k".to_string()));
    assert!(value_var.starts_with("__elephc_foreach_destructure_"));
    assert!(matches!(
        body.first().map(|s| &s.kind),
        Some(StmtKind::ListUnpack { vars, .. }) if vars.len() == 2
    ));
}

/// Verifies a keyed foreach destructure pattern lowers to a `Synthetic` body prefix
/// (keyed entries cannot use the simple `ListUnpack` form).
#[test]
fn test_parse_foreach_destructure_keyed_pattern() {
    let stmts = parse_source("<?php foreach ($a as [\"id\" => $id]) {}");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Foreach { body, .. } = &stmts[0].kind else {
        panic!("expected Foreach");
    };
    assert!(matches!(
        body.first().map(|s| &s.kind),
        Some(StmtKind::Synthetic(stmts)) if !stmts.is_empty()
    ));
}

/// Verifies `goto target;` parses to a `Goto` statement carrying the label name.
#[test]
fn test_goto_parses() {
    let stmts = parse_source("<?php goto target;");
    assert!(matches!(&stmts[0].kind, StmtKind::Goto(name) if name == "target"));
}

/// Verifies a bare `name:` at statement position parses to a `Label` statement, distinct from a
/// constant-expression statement or a static `::` reference.
#[test]
fn test_label_parses() {
    let stmts = parse_source("<?php target: echo 1;");
    assert!(matches!(&stmts[0].kind, StmtKind::Label(name) if name == "target"));
    assert!(matches!(&stmts[1].kind, StmtKind::Echo(_)));
}

/// Verifies an `Identifier ::` reference is not misparsed as a label: `Foo::BAR;` stays an
/// expression statement because `::` lexes as one `DoubleColon` token, not `Identifier` + `Colon`.
#[test]
fn test_static_ref_is_not_label() {
    let stmts = parse_source("<?php Foo::BAR;");
    assert!(!matches!(&stmts[0].kind, StmtKind::Label(_)));
}

/// Verifies a `static $x;` with no initializer parses to a `StaticVar` whose init defaults to null.
#[test]
fn test_static_var_no_initializer_parses() {
    let stmts = parse_source("<?php static $x;");
    let StmtKind::StaticVar { name, init } = &stmts[0].kind else {
        panic!("expected StaticVar");
    };
    assert_eq!(name, "x");
    assert!(matches!(init.kind, ExprKind::Null));
}

/// Verifies a comma-separated `static $a = 1, $b;` declaration parses to a `Synthetic` block holding
/// one `StaticVar` per variable, preserving each initializer (the second defaults to null).
#[test]
fn test_static_var_comma_list_parses() {
    let stmts = parse_source("<?php static $a = 1, $b;");
    let StmtKind::Synthetic(decls) = &stmts[0].kind else {
        panic!("expected Synthetic block for multiple static vars");
    };
    assert_eq!(decls.len(), 2);
    assert!(matches!(&decls[0].kind, StmtKind::StaticVar { name, init }
        if name == "a" && matches!(init.kind, ExprKind::IntLiteral(1))));
    assert!(matches!(&decls[1].kind, StmtKind::StaticVar { name, init }
        if name == "b" && matches!(init.kind, ExprKind::Null)));
}
