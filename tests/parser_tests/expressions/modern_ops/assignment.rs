//! Purpose:
//! Integration or regression tests for parser AST coverage of expression modern PHP operators assignment, including parenthesized word logical assignment rhs, assignment expression binds tighter than word and, and assignment expression is right associative.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Verifies that a parenthesized expression on the RHS of a word logical operator
/// (`and`) parses as a `BinaryOp(And)` within the assignment's value field, not as
/// some other expression wrapper. Regression check for precedence handling when
/// parentheses wrap the RHS of a word logical operator inside an assignment.
#[test]
fn test_parenthesized_word_logical_assignment_rhs() {
    let stmts = parse_source("<?php $x = (true and false);");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::BinaryOp { op, .. } => assert_eq!(op, &BinOp::And),
            other => panic!("expected BinaryOp, got {:?}", other),
        },
        other => panic!("expected Assign, got {:?}", other),
    }
}

/// Verifies that an assignment expression (`$x = true`) binds tighter than the word
/// logical operator `and`. The source `$x = true and false` must parse as
/// `BinaryOp(And, Assignment(...), BoolLiteral(false))` — i.e. the assignment is
/// the left operand of `and`. Regression check for precedence between assignment
/// and word logical operators.
#[test]
fn test_assignment_expression_binds_tighter_than_word_and() {
    let stmts = parse_source("<?php $x = true and false;");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::BinaryOp { left, op, right } => {
                assert_eq!(op, &BinOp::And);
                assert!(matches!(right.kind, ExprKind::BoolLiteral(false)));
                match &left.kind {
                    ExprKind::Assignment { target, value, .. } => {
                        assert!(matches!(target.kind, ExprKind::Variable(ref name) if name == "x"));
                        assert!(matches!(value.kind, ExprKind::BoolLiteral(true)));
                    }
                    other => panic!("expected assignment expression, got {:?}", other),
                }
            }
            other => panic!("expected BinaryOp, got {:?}", other),
        },
        other => panic!("expected ExprStmt, got {:?}", other),
    }
}

/// Verifies that chained assignment expressions are right associative: `$x = $y = 1`
/// parses as `$x = ($y = 1)`, where the outer `Assign` has `$y = 1` as its value.
/// Regression check that nested assignment expressions nest to the right.
#[test]
fn test_assignment_expression_is_right_associative() {
    let stmts = parse_source("<?php $x = $y = 1;");
    match &stmts[0].kind {
        StmtKind::Assign { name, value } => {
            assert_eq!(name, "x");
            match &value.kind {
                ExprKind::Assignment { target, value, .. } => {
                    assert!(matches!(target.kind, ExprKind::Variable(ref name) if name == "y"));
                    assert!(matches!(value.kind, ExprKind::IntLiteral(1)));
                }
                other => panic!("expected nested assignment expression, got {:?}", other),
            }
        }
        other => panic!("expected Assign, got {:?}", other),
    }
}

/// Verifies that a bare array access (`$items[$i]`) parses as the `target` of an
/// assignment expression used in a non-local context (e.g. inside `echo`). The
/// `prelude` must be empty when the RHS is a literal (no container snapshot needed).
/// Regression check for array target parsing in expression contexts.
#[test]
fn test_non_local_assignment_expression_parses_array_target() {
    let stmts = parse_source("<?php echo ($items[$i] = 2);");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment { target, value, prelude, .. } => {
                // Literal RHS is replayable, so no prelude bind is emitted
                // and the value field keeps the literal directly.
                assert!(prelude.is_empty());
                assert!(matches!(value.kind, ExprKind::IntLiteral(2)));
                match &target.kind {
                    ExprKind::ArrayAccess { array, index } => {
                        assert!(matches!(array.kind, ExprKind::Variable(ref name) if name == "items"));
                        assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                    }
                    other => panic!("expected array target, got {:?}", other),
                }
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

/// Verifies that when the RHS of an assignment is a variable identical to the array
/// being accessed (`$items[0] = $items`), the prelude contains exactly one entry
/// — a snapshot of the RHS container before the index is evaluated. Ensures the
/// parser records the correct container capture for COW semantics.
#[test]
fn test_non_local_assignment_expression_snapshots_rhs_container() {
    let stmts = parse_source("<?php echo ($items[0] = $items);");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment { value, prelude, .. } => {
                assert_eq!(prelude.len(), 1);
                assert!(matches!(value.kind, ExprKind::Variable(_)));
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

/// Verifies that an object property access (`$box->value += 2`) parses as the
/// `target` of an assignment expression in a non-local context. The prelude must
/// have exactly one entry (the object temp) and `result_target` must be `Some`.
/// Regression check for property target parsing with compound assignment.
#[test]
fn test_non_local_assignment_expression_parses_property_target() {
    let stmts = parse_source("<?php echo ($box->value += 2);");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                assert!(matches!(target.kind, ExprKind::PropertyAccess { .. }));
                assert_eq!(prelude.len(), 1);
                assert!(matches!(value.kind, ExprKind::Variable(_)));
                assert!(result_target.is_some());
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

/// Verifies that a static property access (`Registry::$count ??= 1`) parses as the
/// `target` of a null-coalesce assignment expression. The RHS is a `NullCoalesce`
/// node. Regression check for static property target in compound assignment.
#[test]
fn test_non_local_assignment_expression_parses_static_property_target() {
    let stmts = parse_source("<?php echo (Registry::$count ??= 1);");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment { target, value, .. } => {
                assert!(matches!(target.kind, ExprKind::StaticPropertyAccess { .. }));
                assert!(matches!(value.kind, ExprKind::NullCoalesce { .. }));
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

/// Verifies that when the array index of the assignment target is itself an
/// effectful expression (`idx()`), the prelude has exactly 2 entries (object temp
/// plus index temp) and `result_target` is `Some`. This forces stabilization so
/// the effectful index expression is evaluated once and its result reused.
/// Regression check for effectful index stabilization in assignment targets.
#[test]
fn test_non_local_assignment_expression_stabilizes_effectful_index() {
    let stmts = parse_source("<?php echo ($items[idx()] = value());");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                assert_eq!(prelude.len(), 2);
                assert!(result_target.is_some());
                assert!(matches!(value.kind, ExprKind::Variable(_)));
                match &target.kind {
                    ExprKind::ArrayAccess { index, .. } => {
                        assert!(matches!(index.kind, ExprKind::Variable(_)));
                    }
                    other => panic!("expected stabilized array target, got {:?}", other),
                }
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

/// Verifies that when the array index is a simple variable (no effects), the
/// parser records only one prelude entry — the index variable itself. No
/// stabilization overhead is needed for trivial indices. Regression check for
/// simple variable index handling in non-local assignment expressions.
#[test]
fn test_non_local_assignment_expression_delays_simple_variable_index() {
    let stmts = parse_source("<?php echo ($items[$i] = ($i = 1));");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                assert_eq!(prelude.len(), 1);
                assert!(result_target.is_some());
                assert!(matches!(value.kind, ExprKind::Variable(_)));
                match &target.kind {
                    ExprKind::ArrayAccess { index, .. } => {
                        assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                    }
                    other => panic!("expected array target, got {:?}", other),
                }
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

/// Verifies that a null-coalesce assignment with a parenthesized assignment as
/// the RHS (`$items[$i] ??= ($i = 1)`) emits a `conditional_value_temp` slot.
/// The prelude is empty (no index stabilization), `result_target` is `Some`,
/// and `conditional_value_temp` is `Some`. Regression check for the null-coalesce
/// conditional-value temp allocation.
#[test]
fn test_null_coalesce_assignment_expression_uses_conditional_value_temp() {
    let stmts = parse_source("<?php echo ($items[$i] ??= ($i = 1));");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                conditional_value_temp,
            } => {
                assert!(prelude.is_empty());
                assert!(result_target.is_some());
                assert!(conditional_value_temp.is_some());
                assert!(matches!(value.kind, ExprKind::NullCoalesce { .. }));
                match &target.kind {
                    ExprKind::ArrayAccess { index, .. } => {
                        assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                    }
                    other => panic!("expected array target, got {:?}", other),
                }
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

/// Verifies that a null-coalesce assignment with a computed index (`$items[$i + 0]`
/// as the target) records one prelude entry (index stabilization) and a
/// `conditional_value_temp` slot. Regression check for computed index handling
/// in null-coalesce assignment expressions.
#[test]
fn test_null_coalesce_assignment_expression_stabilizes_computed_mutated_index() {
    let stmts = parse_source("<?php echo ($items[$i + 0] ??= ($i = 1));");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                conditional_value_temp,
            } => {
                assert_eq!(prelude.len(), 1);
                assert!(result_target.is_some());
                assert!(conditional_value_temp.is_some());
                assert!(matches!(value.kind, ExprKind::NullCoalesce { .. }));
                match &target.kind {
                    ExprKind::ArrayAccess { index, .. } => {
                        assert!(matches!(index.kind, ExprKind::Variable(_)));
                    }
                    other => panic!("expected stabilized array target, got {:?}", other),
                }
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

/// Verifies that `false !== $x = 3` binds the `=` to the adjacent lvalue, parsing as
/// `false !== ($x = 3)` — the assignment is the right operand of the `!==` comparison, not the
/// comparison being assigned to. This is the PHP idiom `if (false !== $pos = strrpos(...))`.
#[test]
fn test_assignment_binds_to_lvalue_inside_comparison() {
    let stmts = parse_source("<?php $r = false !== $x = 3;");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::BinaryOp { op, right, .. } => {
                assert_eq!(op, &BinOp::StrictNotEq);
                assert!(matches!(right.kind, ExprKind::Assignment { .. }));
            }
            other => panic!("expected BinaryOp(StrictNotEq), got {:?}", other),
        },
        other => panic!("expected Assign, got {:?}", other),
    }
}

/// Verifies that `1 + $b = 5` parses as `1 + ($b = 5)` — the assignment binds to the adjacent
/// lvalue even under the higher-precedence `+`, matching PHP.
#[test]
fn test_assignment_binds_to_lvalue_inside_arithmetic() {
    let stmts = parse_source("<?php $r = 1 + $b = 5;");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::BinaryOp { op, right, .. } => {
                assert_eq!(op, &BinOp::Add);
                assert!(matches!(right.kind, ExprKind::Assignment { .. }));
            }
            other => panic!("expected BinaryOp(Add), got {:?}", other),
        },
        other => panic!("expected Assign, got {:?}", other),
    }
}

/// Verifies that `!$b = 7` parses as `!($b = 7)` — the prefix `!` wraps the whole assignment,
/// since `=` binds to the adjacent lvalue rather than to `!$b`.
#[test]
fn test_assignment_binds_to_lvalue_under_prefix_not() {
    let stmts = parse_source("<?php $x = !$b = 7;");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::Not(inner) => {
                assert!(matches!(inner.kind, ExprKind::Assignment { .. }));
            }
            other => panic!("expected Not wrapping an assignment, got {:?}", other),
        },
        other => panic!("expected Assign, got {:?}", other),
    }
}
