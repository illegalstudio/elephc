//! Purpose:
//! Integration or regression tests for parser AST coverage of expression modern PHP operators assignment, including parenthesized word logical assignment rhs, assignment expression binds tighter than word and, and assignment expression is right associative.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

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

#[test]
fn test_non_local_assignment_expression_parses_property_target() {
    let stmts = parse_source("<?php echo ($box->value += 2);");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment { target, value, .. } => {
                assert!(matches!(target.kind, ExprKind::PropertyAccess { .. }));
                assert!(matches!(value.kind, ExprKind::BinaryOp { op: BinOp::Add, .. }));
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

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
