//! Purpose:
//! Visits all expression, statement, annotation, and attribute positions in an AST.
//!
//! Called from:
//! - Source syntax audits and pre-optimization semantic validation.
//!
//! Key details:
//! - Exhaustive node matches make newly introduced syntax require a traversal decision.
//! - Extension observations describe syntax; visitors decide whether it is permitted.

use crate::names::Name;
use crate::parser::ast::{
    AttributeGroup, CallableTarget, CatchClause, ClassConst, ClassMethod, ClassProperty,
    EnumCaseDecl, Expr, ExprKind, InstanceOfTarget, Program, Stmt, StmtKind, TypeExpr,
};
use crate::span::Span;

/// Receives source observations before each node's children are visited.
pub trait AstVisitor {
    /// Observes one expression in source traversal order.
    fn expression(&mut self, _expr: &Expr) {}
    /// Observes one statement, including generated nested statement lists.
    fn statement(&mut self, _stmt: &Stmt) {}
    /// Observes the end of a statement after all nested syntax has been visited.
    fn statement_end(&mut self, _stmt: &Stmt) {}
    /// Observes one type annotation with its enclosing source span.
    fn annotation(&mut self, _annotation: &TypeExpr, _span: Span) {}
    /// Observes a free-function declaration or call target.
    fn function_name(&mut self, _span: Span, _name: &Name) {}
    /// Observes syntax extending PHP, without deciding whether to reject it.
    fn extension(&mut self, _span: Span, _description: &str) {}
}

/// Visits a complete program without transforming or pruning any source nodes.
pub fn walk_program(program: &Program, visitor: &mut impl AstVisitor) {
    walk_stmts(program, visitor);
}

/// Forwards one extension observation to the policy-specific visitor.
fn extension(visitor: &mut impl AstVisitor, span: Span, description: &str) {
    visitor.extension(span, description);
}

mod statements;
mod expressions;
mod members;
use statements::*;
use expressions::*;
use members::*;

