//! Purpose:
//! Parser coverage for append l-values beyond a trailing `$var[] = $v` (issue #845): an append
//! in the middle of a nested write, an append used as an assignment expression, `$this[]`,
//! and an append onto a call result.
//!
//! Called from:
//! - `cargo test --test parser_tests append_lvalues` through Rust's test harness.
//!
//! Key details:
//! - Every shape desugars at parse time into existing statements; these tests pin the desugar
//!   so the checker and IR lowering keep seeing only the statement kinds they already handle.

use super::*;

/// Collects every statement in `stmts`, flattening nested `Synthetic` groups in order.
fn flatten(stmts: &[Stmt]) -> Vec<&Stmt> {
    let mut out = Vec::new();
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Synthetic(inner) => out.extend(flatten(inner)),
            _ => out.push(stmt),
        }
    }
    out
}

/// Verifies `$body['p'][]['to'][]['email'] = 'ok'` appends the fully built fresh element
/// `['to' => [['email' => 'ok']]]` to `$body['p']` through the nested-append group: every `[]`
/// creates an empty element, so a later `[]` on it lands at key 0 and a key creates that key.
#[test]
fn test_parse_mid_chain_append_appends_a_nested_literal() {
    let stmts = parse_source("<?php $body['p'][]['to'][]['email'] = 'ok';");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Synthetic(body) = &stmts[0].kind else {
        panic!("expected the nested-append group, got {:?}", stmts[0].kind);
    };
    let flat = flatten(body);
    assert_eq!(flat.len(), 3, "{:?}", flat);
    let StmtKind::ArrayPush { value, .. } = &flat[1].kind else {
        panic!("expected the bucket push, got {:?}", flat[1].kind);
    };
    let ExprKind::ArrayLiteralAssoc(outer) = &value.kind else {
        panic!("expected ['to' => ...], got {:?}", value.kind);
    };
    assert!(matches!(&outer[0].0.kind, ExprKind::StringLiteral(key) if key == "to"));
    let ExprKind::ArrayLiteral(list) = &outer[0].1.kind else {
        panic!("expected [[...]] for the inner append, got {:?}", outer[0].1.kind);
    };
    let ExprKind::ArrayLiteralAssoc(inner) = &list[0].kind else {
        panic!("expected ['email' => 'ok'], got {:?}", list[0].kind);
    };
    assert!(matches!(&inner[0].0.kind, ExprKind::StringLiteral(key) if key == "email"));
    assert!(matches!(&inner[0].1.kind, ExprKind::StringLiteral(v) if v == "ok"));
    match &flat[2].kind {
        StmtKind::ArrayAssign { array, index, .. } => {
            assert_eq!(array, "body");
            assert!(matches!(&index.kind, ExprKind::StringLiteral(key) if key == "p"));
        }
        other => panic!("expected the write-back into $body['p'], got {:?}", other),
    }
}

/// Verifies `$m[][] = 1` pushes the fresh element `[1]` onto `$m`.
#[test]
fn test_parse_double_append() {
    let stmts = parse_source("<?php $m[][] = 1;");
    match &stmts[0].kind {
        StmtKind::ArrayPush { array, value } => {
            assert_eq!(array, "m");
            assert!(matches!(
                &value.kind,
                ExprKind::ArrayLiteral(items)
                    if items.len() == 1 && matches!(items[0].kind, ExprKind::IntLiteral(1))
            ));
        }
        other => panic!("expected ArrayPush of [1], got {:?}", other),
    }
}

/// Verifies an append in value position becomes an `Assignment` expression whose prelude
/// performs the push, so `$x = ($a[] = 5)` binds the assigned value.
#[test]
fn test_parse_append_assignment_expression() {
    let stmts = parse_source("<?php $x = ($a[] = 5);");
    let StmtKind::Assign { name, value } = &stmts[0].kind else {
        panic!("expected Assign, got {:?}", stmts[0].kind);
    };
    assert_eq!(name, "x");
    let ExprKind::Assignment { prelude, value, .. } = &value.kind else {
        panic!("expected an Assignment expression, got {:?}", value.kind);
    };
    let flat = flatten(prelude);
    let StmtKind::Assign { name: temp, value: rhs } = &flat[0].kind else {
        panic!("expected the value temporary first, got {:?}", flat[0].kind);
    };
    assert!(matches!(rhs.kind, ExprKind::IntLiteral(5)));
    assert!(matches!(
        &flat[1].kind,
        StmtKind::ArrayPush { array, value }
            if array == "a" && matches!(&value.kind, ExprKind::Variable(v) if v == temp)
    ));
    assert!(matches!(&value.kind, ExprKind::Variable(v) if v == temp));
}

/// Verifies chained appends (`$y = $a[] = $b[] = 9`) parse right-associatively.
#[test]
fn test_parse_chained_append_assignments() {
    let stmts = parse_source("<?php $y = $a[] = $b[] = 9;");
    assert!(matches!(&stmts[0].kind, StmtKind::Assign { name, .. } if name == "y"));
    let stmts = parse_source("<?php $a[] = $b[] = 9;");
    assert!(matches!(
        &stmts[0].kind,
        StmtKind::ArrayPush { array, value }
            if array == "a" && matches!(value.kind, ExprKind::Assignment { .. })
    ));
}

/// Verifies `$this[] = $v` binds `$this` to a temporary and appends through it, which is the
/// variable append path that dispatches `offsetSet(null, $v)` on an `ArrayAccess` object.
#[test]
fn test_parse_this_append_goes_through_a_temporary() {
    let stmts = parse_source("<?php class C { function f($v) { $this[] = $v; } }");
    let StmtKind::ClassDecl { methods, .. } = &stmts[0].kind else {
        panic!("expected ClassDecl, got {:?}", stmts[0].kind);
    };
    let flat = flatten(&methods[0].body);
    assert_eq!(flat.len(), 2, "{:?}", flat);
    let StmtKind::Assign { name: holder, value } = &flat[0].kind else {
        panic!("expected $this bound to a temporary, got {:?}", flat[0].kind);
    };
    assert!(matches!(value.kind, ExprKind::This));
    assert!(matches!(&flat[1].kind, StmtKind::ArrayPush { array, .. } if array == holder));
}

/// Verifies `values()[] = 2` evaluates the call once into a temporary and appends to it.
#[test]
fn test_parse_call_result_append() {
    let stmts = parse_source("<?php values()[] = 2;");
    let flat = flatten(&stmts);
    assert_eq!(flat.len(), 2, "{:?}", flat);
    let StmtKind::Assign { name: holder, value } = &flat[0].kind else {
        panic!("expected the call bound to a temporary, got {:?}", flat[0].kind);
    };
    assert!(matches!(value.kind, ExprKind::FunctionCall { .. }));
    assert!(matches!(&flat[1].kind, StmtKind::ArrayPush { array, .. } if array == holder));
}

/// Verifies the shapes PHP rejects stay rejected: an append read, a compound append, an
/// append onto a temporary expression, and a by-reference append.
#[test]
fn test_parse_invalid_append_shapes_fail() {
    assert!(parse_fails("<?php echo $a[];"));
    assert!(parse_fails("<?php $a[]['k'] .= 'x';"));
    assert!(parse_fails("<?php $x = ($a[] += 1);"));
    assert!(parse_fails("<?php $x = ((1 + 2)[] = 3);"));
    assert!(parse_fails("<?php $b[] = &$y;"));
    assert!(parse_fails("<?php $b['k'][][] = &$y;"));
}
