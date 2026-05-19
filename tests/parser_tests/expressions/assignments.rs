//! Purpose:
//! Integration or regression tests for parser AST coverage of assignment expressions, including compound assignment missing ops parse, array compound assignment, and effectful array compound assignment uses synthetic temporary.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_compound_assignment_missing_ops_parse() {
    let cases = [
        ("<?php $x **= 3;", BinOp::Pow),
        ("<?php $x &= 3;", BinOp::BitAnd),
        ("<?php $x |= 3;", BinOp::BitOr),
        ("<?php $x ^= 3;", BinOp::BitXor),
        ("<?php $x <<= 3;", BinOp::ShiftLeft),
        ("<?php $x >>= 3;", BinOp::ShiftRight),
    ];

    for (src, expected_op) in cases {
        let stmts = parse_source(src);
        match &stmts[0].kind {
            StmtKind::Assign { name, value } => {
                assert_eq!(name, "x");
                match &value.kind {
                    ExprKind::BinaryOp { left, op, right } => {
                        assert_eq!(left.kind, ExprKind::Variable("x".into()));
                        assert_eq!(op, &expected_op);
                        assert_eq!(right.kind, ExprKind::IntLiteral(3));
                    }
                    other => panic!("expected BinaryOp, got {:?}", other),
                }
            }
            other => panic!("expected Assign, got {:?}", other),
        }
    }
}

#[test]
fn test_parse_array_compound_assignment() {
    let stmts = parse_source("<?php $items[0] += 3;");
    match &stmts[0].kind {
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            assert_eq!(array, "items");
            assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
            match &value.kind {
                ExprKind::BinaryOp { left, op, right } => {
                    assert_eq!(op, &BinOp::Add);
                    assert!(matches!(right.kind, ExprKind::IntLiteral(3)));
                    match &left.kind {
                        ExprKind::ArrayAccess { array, index } => {
                            assert!(matches!(array.kind, ExprKind::Variable(ref name) if name == "items"));
                            assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
                        }
                        other => panic!("Expected ArrayAccess lhs, got {:?}", other),
                    }
                }
                other => panic!("Expected BinaryOp value, got {:?}", other),
            }
        }
        other => panic!("Expected ArrayAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_effectful_array_compound_assignment_uses_synthetic_temporary() {
    let stmts = parse_source("<?php $items[idx()] += 3;");
    match &stmts[0].kind {
        StmtKind::Synthetic(stmts) => {
            assert_eq!(stmts.len(), 2);
            assert!(matches!(stmts[0].kind, StmtKind::Assign { .. }));
            assert!(matches!(stmts[1].kind, StmtKind::ArrayAssign { .. }));
        }
        other => panic!("Expected Synthetic lowering, got {:?}", other),
    }
}

#[test]
fn test_parse_nested_array_assignment_target() {
    let stmts = parse_source("<?php $data[\"a\"][0][\"b\"] = \"changed\";");
    match &stmts[0].kind {
        StmtKind::NestedArrayAssign { target, value } => {
            assert!(matches!(value.kind, ExprKind::StringLiteral(ref text) if text == "changed"));
            match &target.kind {
                ExprKind::ArrayAccess { array, index } => {
                    assert!(matches!(index.kind, ExprKind::StringLiteral(ref key) if key == "b"));
                    assert!(matches!(array.kind, ExprKind::ArrayAccess { .. }));
                }
                other => panic!("Expected nested ArrayAccess target, got {:?}", other),
            }
        }
        other => panic!("Expected NestedArrayAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_nullable_typed_assign() {
    let stmts = parse_source("<?php ?int $value = null;");
    match &stmts[0].kind {
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => {
            assert_eq!(name, "value");
            assert_eq!(type_expr, &TypeExpr::Nullable(Box::new(TypeExpr::Int)));
            assert_eq!(value.kind, ExprKind::Null);
        }
        other => panic!("Expected typed assign, got {:?}", other),
    }
}

#[test]
fn test_parse_union_typed_assign() {
    let stmts = parse_source("<?php int|string $value = 1;");
    match &stmts[0].kind {
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => {
            assert_eq!(name, "value");
            assert_eq!(type_expr, &TypeExpr::Union(vec![TypeExpr::Int, TypeExpr::Str]));
            assert_eq!(value.kind, ExprKind::IntLiteral(1));
        }
        other => panic!("Expected typed assign, got {:?}", other),
    }
}
