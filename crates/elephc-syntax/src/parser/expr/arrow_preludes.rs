//! Purpose:
//! Collects arrow-function captures from parser-generated assignment preludes.
//!
//! Called from:
//! - `super::prefix_complex::collect_arrow_expr_captures`.
//!
//! Key details:
//! - Generated locals are filtered by the same capture predicate as ordinary expressions.
//! - Array writes read their receiver and therefore capture it even without an explicit read node.

use std::collections::HashSet;
use crate::parser::ast::{Stmt, StmtKind};
use super::prefix_complex::{collect_arrow_expr_captures, push_arrow_capture};

/// Visits the assignment statement shapes emitted in expression preludes.
pub(super) fn collect(
    statement: &Stmt, bound: &HashSet<String>, seen: &mut HashSet<String>, captures: &mut Vec<String>,
) {
    match &statement.kind {
        StmtKind::Synthetic(statements) => {
            for statement in statements { collect(statement, bound, seen, captures); }
        }
        StmtKind::ArrayPush { array, value } => {
            push_arrow_capture(array, bound, seen, captures);
            collect_arrow_expr_captures(value, bound, seen, captures);
        }
        StmtKind::ArrayAssign { array, index, value } => {
            push_arrow_capture(array, bound, seen, captures);
            collect_arrow_expr_captures(index, bound, seen, captures);
            collect_arrow_expr_captures(value, bound, seen, captures);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            collect_arrow_expr_captures(object, bound, seen, captures);
            collect_arrow_expr_captures(value, bound, seen, captures);
        }
        StmtKind::PropertyArrayAssign { object, index, value, .. } => {
            collect_arrow_expr_captures(object, bound, seen, captures);
            collect_arrow_expr_captures(index, bound, seen, captures);
            collect_arrow_expr_captures(value, bound, seen, captures);
        }
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            collect_arrow_expr_captures(index, bound, seen, captures);
            collect_arrow_expr_captures(value, bound, seen, captures);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            collect_arrow_expr_captures(target, bound, seen, captures);
            collect_arrow_expr_captures(value, bound, seen, captures);
        }
        StmtKind::Assign { value, .. }
        | StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. }
        | StmtKind::ExprStmt(value) => collect_arrow_expr_captures(value, bound, seen, captures),
        _ => {}
    }
}
