//! Purpose:
//! Top-level yield discovery for the type checker. Walks function bodies
//! looking for `yield` or `yield from`, returning `true` as soon as one is
//! found at the same generator scope (closures form a fresh scope and are
//! skipped).
//!
//! Called from:
//!  - `super::body_contains_yield` re-export consumers across the codebase.
//!
//! Key details:
//!  - The walker visits every statement and expression that can contain a
//!    yield in v1's grammar. Anything inside a `Closure` expression belongs
//!    to a different generator, so we deliberately do not peek through it.

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};

/// Scans the top-level statements of a function body for `yield` or `yield from`.
/// Returns `true` on the first yield found at the generator's own scope.
/// Closures are skipped entirely — their yields belong to a different generator
/// and are not propagated to the enclosing function's return type.
///
/// Used by the type checker to coerce a generator function's return type to
/// `Object("Generator")` and by codegen to route the function through the
/// generator pipeline.
pub(crate) fn body_contains_yield(body: &[Stmt]) -> bool {
    body.iter().any(stmt_contains_yield)
}

/// Recursively checks each statement variant for `yield` or `yield from`.
/// Skips nested `FunctionDecl`, `ClassDecl`, `TraitDecl`, and `InterfaceDecl`
/// boundaries — a yield inside any of these is its own generator.
/// Handles all statement kinds that can recursively contain other statements
/// or expressions.
fn stmt_contains_yield(stmt: &Stmt) -> bool {
    match &stmt.kind {
        // Closures form a fresh generator scope — don't peek into them.
        StmtKind::FunctionDecl { .. } | StmtKind::ClassDecl { .. } | StmtKind::TraitDecl { .. } => false,
        StmtKind::InterfaceDecl { .. } => false,
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_contains_yield)
                || catches.iter().any(|c| c.body.iter().any(stmt_contains_yield))
                || finally_body
                    .as_ref()
                    .map(|f| f.iter().any(stmt_contains_yield))
                    .unwrap_or(false)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_contains_yield(condition)
                || then_body.iter().any(stmt_contains_yield)
                || elseif_clauses
                    .iter()
                    .any(|(c, b)| expr_contains_yield(c) || b.iter().any(stmt_contains_yield))
                || else_body
                    .as_ref()
                    .map(|b| b.iter().any(stmt_contains_yield))
                    .unwrap_or(false)
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_contains_yield)
                || else_body
                    .as_ref()
                    .map(|b| b.iter().any(stmt_contains_yield))
                    .unwrap_or(false)
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_contains_yield(condition) || body.iter().any(stmt_contains_yield)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().map(stmt_contains_yield).unwrap_or(false)
                || condition.as_ref().map(expr_contains_yield).unwrap_or(false)
                || update.as_deref().map(stmt_contains_yield).unwrap_or(false)
                || body.iter().any(stmt_contains_yield)
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_contains_yield(array) || body.iter().any(stmt_contains_yield)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_contains_yield(subject)
                || cases.iter().any(|(vals, body)| {
                    vals.iter().any(expr_contains_yield) || body.iter().any(stmt_contains_yield)
                })
                || default
                    .as_ref()
                    .map(|d| d.iter().any(stmt_contains_yield))
                    .unwrap_or(false)
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            stmts.iter().any(stmt_contains_yield)
        }
        StmtKind::Echo(e) | StmtKind::ExprStmt(e) | StmtKind::Throw(e) => expr_contains_yield(e),
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ConstDecl { value, .. }
        | StmtKind::ListUnpack { value, .. }
        | StmtKind::StaticVar { init: value, .. } => expr_contains_yield(value),
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_contains_yield(index) || expr_contains_yield(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_contains_yield(target) || expr_contains_yield(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_contains_yield(value),
        StmtKind::Return(opt) => opt.as_ref().map(expr_contains_yield).unwrap_or(false),
        StmtKind::Include { path, .. } => expr_contains_yield(path),
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_contains_yield(object) || expr_contains_yield(value)
        }
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_contains_yield(object) || expr_contains_yield(value)
        }
        StmtKind::PropertyArrayAssign { object, index, value, .. } => {
            expr_contains_yield(object) || expr_contains_yield(index) || expr_contains_yield(value)
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_contains_yield(value),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_contains_yield(index) || expr_contains_yield(value)
        }
        _ => false,
    }
}

/// Recursively checks each expression variant for `yield` or `yield from`.
/// Skips `Closure` expressions — a yield inside a closure belongs to that
/// closure's generator scope. Handles all expression kinds that can contain
/// nested expressions.
/// Returns `true` on the first match.
fn expr_contains_yield(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Yield { .. } | ExprKind::YieldFrom(_) => true,
        // Don't peek into closures — their yields belong to a different generator scope.
        ExprKind::Closure { .. } => false,
        ExprKind::BinaryOp { left, right, .. } => {
            expr_contains_yield(left) || expr_contains_yield(right)
        }
        ExprKind::InstanceOf { value, .. } => expr_contains_yield(value),
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. } => expr_contains_yield(inner),
        ExprKind::NullCoalesce { value, default } => {
            expr_contains_yield(value) || expr_contains_yield(default)
        }
        ExprKind::Pipe { value, callable } => {
            expr_contains_yield(value) || expr_contains_yield(callable)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => args.iter().any(expr_contains_yield),
        ExprKind::ExprCall { callee, args } => {
            expr_contains_yield(callee) || args.iter().any(expr_contains_yield)
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_contains_yield(object) || args.iter().any(expr_contains_yield)
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_contains_yield),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(k, v)| expr_contains_yield(k) || expr_contains_yield(v)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_contains_yield(subject)
                || arms.iter().any(|(patterns, value)| {
                    patterns.iter().any(expr_contains_yield) || expr_contains_yield(value)
                })
                || default.as_ref().map(|d| expr_contains_yield(d)).unwrap_or(false)
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_contains_yield(array) || expr_contains_yield(index)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_contains_yield(condition)
                || expr_contains_yield(then_expr)
                || expr_contains_yield(else_expr)
        }
        ExprKind::ShortTernary { value, default } => {
            expr_contains_yield(value) || expr_contains_yield(default)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_contains_yield(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_contains_yield(object) || expr_contains_yield(property)
        }
        ExprKind::NamedArg { value, .. } => expr_contains_yield(value),
        ExprKind::BufferNew { len, .. } => expr_contains_yield(len),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{CallableTarget, Expr, ExprKind, Stmt, StmtKind};
    use crate::span::Span;

    /// Constructs a `Yield { key: None, value: Some(value) }` expression for testing.
    fn yield_expr(value: i64) -> Expr {
        Expr::new(
            ExprKind::Yield {
                key: None,
                value: Some(Box::new(Expr::int_lit(value))),
            },
            Span::dummy(),
        )
    }

    /// Constructs a `Pipe { value, callable }` expression wrapped in an `ExprStmt` for testing.
    fn pipe_expr(value: Expr, callable: Expr) -> Stmt {
        Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::Pipe {
                    value: Box::new(value),
                    callable: Box::new(callable),
                },
                Span::dummy(),
            )),
            Span::dummy(),
        )
    }

    /// Constructs a first-class callable expression `id` for testing.
    fn callable_expr() -> Expr {
        Expr::new(
            ExprKind::FirstClassCallable(CallableTarget::Function("id".into())),
            Span::dummy(),
        )
    }

    #[test]
    /// Verifies that a yield appearing in the value position of a Pipe expression is detected.
    fn detects_yield_in_pipe_value() {
        let stmt = pipe_expr(yield_expr(1), callable_expr());

        assert!(body_contains_yield(&[stmt]));
    }

    #[test]
    /// Verifies that a yield appearing in the callable position of a Pipe expression is detected.
    fn detects_yield_in_pipe_callable() {
        let stmt = pipe_expr(Expr::int_lit(1), yield_expr(2));

        assert!(body_contains_yield(&[stmt]));
    }
}
