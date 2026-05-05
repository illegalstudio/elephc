use crate::errors::CompileError;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};
use crate::span::Span;

/// Walk a static closure body and reject any reference to `$this`. PHP forbids
/// `$this` inside `static function() {}` and `static fn() => ...` because the
/// closure isn't bound to an object instance.
pub(super) fn body_must_not_use_this(body: &[Stmt], span: Span) -> Result<(), CompileError> {
    for stmt in body {
        stmt_must_not_use_this(stmt, span)?;
    }
    Ok(())
}

fn stmt_must_not_use_this(stmt: &Stmt, span: Span) -> Result<(), CompileError> {
    match &stmt.kind {
        StmtKind::Echo(e)
        | StmtKind::Throw(e)
        | StmtKind::ExprStmt(e)
        | StmtKind::Include { path: e, .. }
        | StmtKind::ConstDecl { value: e, .. }
        | StmtKind::StaticVar { init: e, .. }
        | StmtKind::ListUnpack { value: e, .. }
        | StmtKind::Return(Some(e))
        | StmtKind::Assign { value: e, .. }
        | StmtKind::TypedAssign { value: e, .. }
        | StmtKind::ArrayPush { value: e, .. } => expr_must_not_use_this(e, span),
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_must_not_use_this(index, span)?;
            expr_must_not_use_this(value, span)
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_must_not_use_this(object, span)?;
            expr_must_not_use_this(value, span)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            expr_must_not_use_this(object, span)?;
            expr_must_not_use_this(index, span)?;
            expr_must_not_use_this(value, span)
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_must_not_use_this(value, span),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_must_not_use_this(index, span)?;
            expr_must_not_use_this(value, span)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_must_not_use_this(condition, span)?;
            body_must_not_use_this(then_body, span)?;
            for (cond, body) in elseif_clauses {
                expr_must_not_use_this(cond, span)?;
                body_must_not_use_this(body, span)?;
            }
            if let Some(body) = else_body {
                body_must_not_use_this(body, span)?;
            }
            Ok(())
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_must_not_use_this(condition, span)?;
            body_must_not_use_this(body, span)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(s) = init {
                stmt_must_not_use_this(s, span)?;
            }
            if let Some(c) = condition {
                expr_must_not_use_this(c, span)?;
            }
            if let Some(s) = update {
                stmt_must_not_use_this(s, span)?;
            }
            body_must_not_use_this(body, span)
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_must_not_use_this(array, span)?;
            body_must_not_use_this(body, span)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_must_not_use_this(subject, span)?;
            for (patterns, body) in cases {
                for pattern in patterns {
                    expr_must_not_use_this(pattern, span)?;
                }
                body_must_not_use_this(body, span)?;
            }
            if let Some(body) = default {
                body_must_not_use_this(body, span)?;
            }
            Ok(())
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_must_not_use_this(try_body, span)?;
            for catch in catches {
                body_must_not_use_this(&catch.body, span)?;
            }
            if let Some(body) = finally_body {
                body_must_not_use_this(body, span)?;
            }
            Ok(())
        }
        StmtKind::NamespaceBlock { body, .. } => body_must_not_use_this(body, span),
        StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::InterfaceDecl { .. } => Ok(()),
        _ => Ok(()),
    }
}

fn expr_must_not_use_this(expr: &Expr, span: Span) -> Result<(), CompileError> {
    match &expr.kind {
        ExprKind::This => Err(CompileError::new(
            span,
            "Cannot use $this inside a static closure",
        )),
        ExprKind::BinaryOp { left, right, .. } => {
            expr_must_not_use_this(left, span)?;
            expr_must_not_use_this(right, span)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_must_not_use_this(value, span)?;
            instanceof_target_must_not_use_this(target, span)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::Cast { expr: inner, .. } => expr_must_not_use_this(inner, span),
        ExprKind::NullCoalesce { value, default } => {
            expr_must_not_use_this(value, span)?;
            expr_must_not_use_this(default, span)
        }
        ExprKind::ShortTernary { value, default } => {
            expr_must_not_use_this(value, span)?;
            expr_must_not_use_this(default, span)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            for arg in args {
                expr_must_not_use_this(arg, span)?;
            }
            Ok(())
        }
        ExprKind::ExprCall { callee, args } => {
            expr_must_not_use_this(callee, span)?;
            for arg in args {
                expr_must_not_use_this(arg, span)?;
            }
            Ok(())
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_must_not_use_this(object, span)?;
            for arg in args {
                expr_must_not_use_this(arg, span)?;
            }
            Ok(())
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                expr_must_not_use_this(item, span)?;
            }
            Ok(())
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (k, v) in pairs {
                expr_must_not_use_this(k, span)?;
                expr_must_not_use_this(v, span)?;
            }
            Ok(())
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_must_not_use_this(array, span)?;
            expr_must_not_use_this(index, span)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_must_not_use_this(condition, span)?;
            expr_must_not_use_this(then_expr, span)?;
            expr_must_not_use_this(else_expr, span)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_must_not_use_this(subject, span)?;
            for (patterns, value) in arms {
                for p in patterns {
                    expr_must_not_use_this(p, span)?;
                }
                expr_must_not_use_this(value, span)?;
            }
            if let Some(d) = default {
                expr_must_not_use_this(d, span)?;
            }
            Ok(())
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_must_not_use_this(object, span),
        ExprKind::NamedArg { value, .. } => expr_must_not_use_this(value, span),
        ExprKind::BufferNew { len, .. } => expr_must_not_use_this(len, span),
        ExprKind::FirstClassCallable(target) => callable_target_must_not_use_this(target, span),
        ExprKind::Closure { body, .. } => body_must_not_use_this(body, span),
        _ => Ok(()),
    }
}

fn callable_target_must_not_use_this(
    target: &CallableTarget,
    span: Span,
) -> Result<(), CompileError> {
    match target {
        CallableTarget::Method { object, .. } => expr_must_not_use_this(object, span),
        CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => Ok(()),
    }
}

fn instanceof_target_must_not_use_this(
    target: &InstanceOfTarget,
    span: Span,
) -> Result<(), CompileError> {
    match target {
        InstanceOfTarget::Name(_) => Ok(()),
        InstanceOfTarget::Expr(expr) => expr_must_not_use_this(expr, span),
    }
}
