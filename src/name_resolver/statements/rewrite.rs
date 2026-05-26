//! Purpose:
//! Resolves ordinary non-declaration statements under an active namespace/import context.
//! Rewrites child expressions, nested bodies, catch clauses, and control-flow statements.
//!
//! Called from:
//! - `crate::name_resolver::statements::list::resolve_stmt_list()`.
//!
//! Key details:
//! - Statement structure is preserved while contained names are canonicalized for downstream passes.

use crate::errors::CompileError;
use crate::parser::ast::{Stmt, StmtKind};

use super::context::ResolveContext;

/// Rewrites an ordinary non-declaration statement under the active namespace/import context.
///
/// Recursively applies expression rewriting (`ctx.expr`), statement-list rewriting
/// (`ctx.stmt_list`), type-expression rewriting (`ctx.type_expr`), and catch-clause
/// rewriting (`ctx.catch_clause`) to all child nodes. Statement structure and
/// identifiers (variable names, property names, labels) are preserved unchanged.
/// Unrecognized `StmtKind` variants are returned as-is.
///
/// # Arguments
/// * `stmt` - the statement to rewrite
/// * `ctx` - the resolution context carrying namespace/use state
///
/// # Returns
/// A rewritten `Stmt` with all contained names canonicalized, or a `CompileError`
/// if any child expression or statement rewriting fails.
pub(super) fn resolve_regular_stmt(
    stmt: &Stmt,
    ctx: ResolveContext<'_>,
) -> Result<Stmt, CompileError> {
    let span = stmt.span;
    let kind = match &stmt.kind {
        StmtKind::Synthetic(stmts) => StmtKind::Synthetic(ctx.stmt_list(stmts)?),
        StmtKind::IncludeOnceMark { label } => StmtKind::IncludeOnceMark {
            label: label.clone(),
        },
        StmtKind::FunctionVariantMark { name, variant } => StmtKind::FunctionVariantMark {
            name: name.clone(),
            variant: variant.clone(),
        },
        StmtKind::IncludeOnceGuard { label, body } => StmtKind::IncludeOnceGuard {
            label: label.clone(),
            body: ctx.stmt_list(body)?,
        },
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition: ctx.expr(condition),
            then_body: ctx.stmt_list(then_body)?,
            elseif_clauses: elseif_clauses
                .iter()
                .map(|(cond, body)| Ok((ctx.expr(cond), ctx.stmt_list(body)?)))
                .collect::<Result<Vec<_>, CompileError>>()?,
            else_body: else_body.as_ref().map(|body| ctx.stmt_list(body)).transpose()?,
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition: ctx.expr(condition),
            body: ctx.stmt_list(body)?,
        },
        StmtKind::DoWhile { body, condition } => StmtKind::DoWhile {
            body: ctx.stmt_list(body)?,
            condition: ctx.expr(condition),
        },
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => StmtKind::For {
            init: init
                .as_ref()
                .map(|stmt| ctx.one_stmt(stmt))
                .transpose()?
                .map(Box::new),
            condition: condition.as_ref().map(|expr| ctx.expr(expr)),
            update: update
                .as_ref()
                .map(|stmt| ctx.one_stmt(stmt))
                .transpose()?
                .map(Box::new),
            body: ctx.stmt_list(body)?,
        },
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => StmtKind::Foreach {
            array: ctx.expr(array),
            key_var: key_var.clone(),
            value_var: value_var.clone(),
            value_by_ref: *value_by_ref,
            body: ctx.stmt_list(body)?,
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject: ctx.expr(subject),
            cases: cases
                .iter()
                .map(|(values, body)| {
                    Ok((
                        values.iter().map(|value| ctx.expr(value)).collect(),
                        ctx.stmt_list(body)?,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
            default: default.as_ref().map(|body| ctx.stmt_list(body)).transpose()?,
        },
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => StmtKind::Try {
            try_body: ctx.stmt_list(try_body)?,
            catches: catches
                .iter()
                .map(|catch_clause| ctx.catch_clause(catch_clause))
                .collect::<Result<Vec<_>, CompileError>>()?,
            finally_body: finally_body
                .as_ref()
                .map(|body| ctx.stmt_list(body))
                .transpose()?,
        },
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name: name.clone(),
            value: ctx.expr(value),
        },
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => StmtKind::TypedAssign {
            type_expr: ctx.type_expr(type_expr),
            name: name.clone(),
            value: ctx.expr(value),
        },
        StmtKind::Echo(expr) => StmtKind::Echo(ctx.expr(expr)),
        StmtKind::Throw(expr) => StmtKind::Throw(ctx.expr(expr)),
        StmtKind::ExprStmt(expr) => StmtKind::ExprStmt(ctx.expr(expr)),
        StmtKind::Return(expr) => StmtKind::Return(expr.as_ref().map(|expr| ctx.expr(expr))),
        StmtKind::ListUnpack { vars, value } => StmtKind::ListUnpack {
            vars: vars.clone(),
            value: ctx.expr(value),
        },
        StmtKind::ArrayAssign { array, index, value } => StmtKind::ArrayAssign {
            array: array.clone(),
            index: ctx.expr(index),
            value: ctx.expr(value),
        },
        StmtKind::NestedArrayAssign { target, value } => StmtKind::NestedArrayAssign {
            target: ctx.expr(target),
            value: ctx.expr(value),
        },
        StmtKind::ArrayPush { array, value } => StmtKind::ArrayPush {
            array: array.clone(),
            value: ctx.expr(value),
        },
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => StmtKind::PropertyAssign {
            object: Box::new(ctx.expr(object)),
            property: property.clone(),
            value: ctx.expr(value),
        },
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyAssign {
            receiver: ctx.static_receiver(receiver),
            property: property.clone(),
            value: ctx.expr(value),
        },
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyArrayPush {
            receiver: ctx.static_receiver(receiver),
            property: property.clone(),
            value: ctx.expr(value),
        },
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => StmtKind::StaticPropertyArrayAssign {
            receiver: ctx.static_receiver(receiver),
            property: property.clone(),
            index: ctx.expr(index),
            value: ctx.expr(value),
        },
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => StmtKind::PropertyArrayPush {
            object: Box::new(ctx.expr(object)),
            property: property.clone(),
            value: ctx.expr(value),
        },
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => StmtKind::PropertyArrayAssign {
            object: Box::new(ctx.expr(object)),
            property: property.clone(),
            index: ctx.expr(index),
            value: ctx.expr(value),
        },
        _ => return Ok(stmt.clone()),
    };

    Ok(Stmt::new(kind, span))
}
