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

/// Verifies a simple `for` initializer keeps the statement-level assignment representation.
#[test]
fn test_for_simple_assignment_init_is_canonicalized() {
    let stmts = parse_source("<?php for ($base = 2; false; $base++) {}");
    let StmtKind::For { init, .. } = &stmts[0].kind else {
        panic!("expected For");
    };
    assert!(matches!(
        init.as_deref().map(|stmt| &stmt.kind),
        Some(StmtKind::Assign {
            name,
            value: Expr {
                kind: ExprKind::IntLiteral(2),
                ..
            },
        }) if name == "base"
    ));
}

/// Verifies a method call is accepted as a side-effecting `for` update expression.
#[test]
fn test_for_method_call_update_parses() {
    let stmts = parse_source("<?php for (; $iterator->valid(); $iterator->next()) {}");
    let StmtKind::For { update, .. } = &stmts[0].kind else {
        panic!("expected For");
    };
    assert!(matches!(
        update.as_deref().map(|stmt| &stmt.kind),
        Some(StmtKind::ExprStmt(Expr {
            kind: ExprKind::MethodCall { .. },
            ..
        }))
    ));
}

/// Verifies PHP 8.5's `(void)` discard form is accepted in `for` init and update clauses.
#[test]
fn test_for_void_discard_clauses_parse() {
    let stmts = parse_source(
        "<?php for ((void) strlen('init'); false; (void) strlen('update')) {}",
    );
    let StmtKind::For { init, update, .. } = &stmts[0].kind else {
        panic!("expected For");
    };
    for clause in [init, update] {
        assert!(matches!(
            clause.as_deref().map(|stmt| &stmt.kind),
            Some(StmtKind::ExprStmt(Expr {
                kind: ExprKind::Cast {
                    target: CastType::Void,
                    ..
                },
                ..
            }))
        ));
    }
}

/// Verifies `clone` can be used as a standalone expression statement.
#[test]
fn test_clone_expression_statement_parses() {
    let stmts = parse_source("<?php clone $object;");
    assert!(matches!(
        &stmts[0].kind,
        StmtKind::ExprStmt(Expr {
            kind: ExprKind::Clone(_),
            ..
        })
    ));
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

/// Verifies bracket destructuring in a foreach value target is lowered into the loop body.
#[test]
fn test_parse_foreach_value_destructuring() {
    let stmts = parse_source("<?php foreach ($pairs as [$left, $right]) { echo $left; }");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Foreach {
        value_var, body, ..
    } = &stmts[0].kind
    else {
        panic!("expected Foreach");
    };
    assert!(value_var.starts_with("__elephc_foreach_"));
    assert!(matches!(body[0].kind, StmtKind::ListUnpack { .. }));
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

/// Verifies `foreach ($m as [$a, $b])` desugars to a loop over a hidden value variable whose
/// body starts with the same unpack statement `[$a, $b] = $tmp;` produces.
#[test]
fn test_parse_foreach_value_destructuring_desugars_to_hidden_temp() {
    let stmts = parse_source("<?php foreach ($m as [$a, $b]) { echo $a; }");
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
    assert!(value_var.starts_with("__elephc_foreach_"));
    assert!(!value_by_ref);
    assert_eq!(body.len(), 2);
    let StmtKind::ListUnpack { vars, value } = &body[0].kind else {
        panic!("expected the unpack statement first in the body");
    };
    assert_eq!(vars, &vec!["a".to_string(), "b".to_string()]);
    assert_eq!(value.kind, ExprKind::Variable(value_var.clone()));
}

/// Verifies the `$key => [pattern]` form keeps the real key variable and only replaces the
/// value target with the hidden temporary.
#[test]
fn test_parse_foreach_key_with_value_destructuring() {
    let stmts = parse_source("<?php foreach ($m as $k => [$a, $b]) {}");
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
    assert!(value_var.starts_with("__elephc_foreach_"));
    assert_eq!(body.len(), 1);
    assert!(matches!(body[0].kind, StmtKind::ListUnpack { .. }));
}

/// Verifies a reference to a whole destructuring pattern is rejected: PHP allows `&` on the
/// targets inside the pattern, never on the pattern itself.
#[test]
fn test_parse_foreach_reference_to_pattern_is_rejected() {
    assert!(parse_fails("<?php foreach ($m as &[$a, $b]) {}"));
    assert!(parse_fails("<?php foreach ($m as $k => &[$a, $b]) {}"));
}

// --- Alternative control-structure syntax ---

/// Verifies `if (…): … endif;` produces exactly the same `StmtKind::If` shape as the brace form.
#[test]
fn test_alternative_if_parses_to_plain_if() {
    let alternative = parse_source("<?php if (1) { echo \"a\"; } ");
    let braces = parse_source("<?php if (1): echo \"a\"; endif;");
    assert_eq!(alternative.len(), 1);
    assert_eq!(braces.len(), 1);
    let (
        StmtKind::If {
            then_body: alt_body,
            elseif_clauses: alt_elseifs,
            else_body: alt_else,
            ..
        },
        StmtKind::If {
            then_body: brace_body,
            elseif_clauses: brace_elseifs,
            else_body: brace_else,
            ..
        },
    ) = (&alternative[0].kind, &braces[0].kind)
    else {
        panic!("expected both forms to parse to If");
    };
    assert_eq!(alt_body.len(), brace_body.len());
    assert_eq!(alt_elseifs.len(), brace_elseifs.len());
    assert_eq!(alt_else.is_some(), brace_else.is_some());
}

/// Verifies `elseif:` and `else:` segments populate the same clause list the brace form uses.
#[test]
fn test_alternative_if_elseif_else_parses() {
    let stmts =
        parse_source("<?php if (1): echo 1; elseif (2): echo 2; elseif (3): echo 3; else: echo 4; endif;");
    let StmtKind::If {
        elseif_clauses,
        else_body,
        then_body,
        ..
    } = &stmts[0].kind
    else {
        panic!("expected If");
    };
    assert_eq!(then_body.len(), 1);
    assert_eq!(elseif_clauses.len(), 2);
    assert_eq!(else_body.as_ref().map(Vec::len), Some(1));
}

/// Verifies each alternative loop form parses to its ordinary loop statement kind.
#[test]
fn test_alternative_loops_parse_to_plain_loops() {
    assert!(matches!(
        parse_source("<?php while (false): echo 1; endwhile;")[0].kind,
        StmtKind::While { .. }
    ));
    assert!(matches!(
        parse_source("<?php for ($i = 0; $i < 1; $i++): echo 1; endfor;")[0].kind,
        StmtKind::For { .. }
    ));
    assert!(matches!(
        parse_source("<?php foreach ([1] as $x): echo $x; endforeach;")[0].kind,
        StmtKind::Foreach { .. }
    ));
}

/// Verifies the alternative `switch` form collects cases and the default arm like the brace form.
#[test]
fn test_alternative_switch_parses_cases_and_default() {
    let stmts = parse_source(
        "<?php switch (1): case 1: echo 1; break; case 2: echo 2; break; default: echo 3; endswitch;",
    );
    let StmtKind::Switch { cases, default, .. } = &stmts[0].kind else {
        panic!("expected Switch");
    };
    assert_eq!(cases.len(), 2);
    assert!(default.is_some());
}

/// Verifies alternative bodies may be empty, matching PHP's `if (…): endif;`.
#[test]
fn test_alternative_bodies_may_be_empty() {
    let stmts = parse_source(
        "<?php if (false): endif; while (false): endwhile; foreach ([] as $x): endforeach; switch (1): endswitch;",
    );
    assert_eq!(stmts.len(), 4);
}

/// Verifies alternative and brace forms nest inside each other in both directions.
#[test]
fn test_alternative_and_brace_forms_nest() {
    let outer_alt = parse_source("<?php foreach ([1] as $a): if ($a) { echo 1; } endforeach;");
    let StmtKind::Foreach { body, .. } = &outer_alt[0].kind else {
        panic!("expected Foreach");
    };
    assert!(matches!(body[0].kind, StmtKind::If { .. }));

    let outer_brace = parse_source("<?php foreach ([1] as $a) { if ($a): echo 1; endif; }");
    let StmtKind::Foreach { body, .. } = &outer_brace[0].kind else {
        panic!("expected Foreach");
    };
    assert!(matches!(body[0].kind, StmtKind::If { .. }));
}

/// Verifies mixing the two `if` styles, an unterminated alternative block, a mismatched
/// terminator, and a stray terminator are all rejected, matching PHP.
#[test]
fn test_alternative_syntax_malformed_forms_are_rejected() {
    assert!(parse_fails("<?php if (true) { echo 1; } else: echo 2; endif;"));
    assert!(parse_fails("<?php if (true): echo 1; else { echo 2; } endif;"));
    assert!(parse_fails("<?php if (true): echo 1;"));
    assert!(parse_fails("<?php foreach ([1] as $x): echo 1; endwhile;"));
    assert!(parse_fails("<?php endif;"));
    assert!(parse_fails("<?php for ($i = 0; $i < 1; $i++): echo 1; endfor"));
}

// --- goto (unsupported) ---

/// Verifies `goto` and its target label are rejected at parse time rather than silently ignored.
#[test]
fn test_goto_and_labels_are_rejected() {
    assert!(parse_fails("<?php goto done; done: echo 1;"));
    assert!(parse_fails("<?php done: echo 1;"));
}
