//! Purpose:
//! Fuses a pure `array_map()` temporary with its constant-indexed reads.
//!
//! Called from:
//! - `crate::ir_lower::function::lower_body_into_function` before lowering a function body.
//!
//! Key details:
//! - Only an untagged, capture-free closure whose body is a side-effect-free arithmetic
//!   tree over the parameter and scalar literals (`+`, `-`, `*`, unary minus, nested freely)
//!   over a packed `array<int>` qualifies: such a callback has no side effects, so reading
//!   one mapped element does not require mapping every element. Typed returns keep the map
//!   because an overflowing element must still throw.
//! - A missing source key must keep PHP's warning and null result; every fused read is
//!   guarded by `isset($source[$index])` and falls back to a normal source read.
//! - The temporary must not be observed anywhere else in the block. Unsupported statement
//!   or expression shapes fail closed instead of risking a missed use.

use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind, TypeExpr};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

/// One pure map assignment found in a straight-line block.
struct Candidate {
    mapped: String,
    source: String,
    param: String,
    body: Expr,
}

/// Rewrites every straight-line block in `body` that contains one safe map temporary.
pub(crate) fn fuse_map_reads(body: &mut Vec<Stmt>, env: &TypeEnv) {
    fuse_block(body, env);
}

/// Recurses into nested scopes and then fuses candidates in this block.
fn fuse_block(stmts: &mut Vec<Stmt>, env: &TypeEnv) {
    for stmt in stmts.iter_mut() {
        recurse_stmt(stmt, env);
    }
    let mut index = 0;
    while index < stmts.len() {
        let Some(candidate) = map_candidate(&stmts[index], env) else {
            index += 1;
            continue;
        };
        // A previous value of the temporary may survive across loop iterations.
        if stmts[..index]
            .iter()
            .any(|stmt| stmt_mentions(stmt, &candidate.mapped))
        {
            index += 1;
            continue;
        }
        let mut trial = stmts.clone();
        trial.remove(index);
        let mut supported = true;
        for stmt in trial.iter_mut().skip(index) {
            if rewrite_stmt(stmt, &candidate).is_err() {
                supported = false;
                break;
            }
        }
        if supported {
            *stmts = trial;
        } else {
            index += 1;
        }
    }
}

/// Recurses into the bodies of nested control-flow statements.
fn recurse_stmt(stmt: &mut Stmt, env: &TypeEnv) {
    match &mut stmt.kind {
        StmtKind::For { body, .. }
        | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. } => fuse_block(body, env),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            fuse_block(then_body, env);
            for (_, body) in elseif_clauses {
                fuse_block(body, env);
            }
            if let Some(body) = else_body {
                fuse_block(body, env);
            }
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                fuse_block(body, env);
            }
            if let Some(body) = default {
                fuse_block(body, env);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            fuse_block(try_body, env);
            for catch in catches {
                fuse_block(&mut catch.body, env);
            }
            if let Some(body) = finally_body {
                fuse_block(body, env);
            }
        }
        StmtKind::Synthetic(body) => fuse_block(body, env),
        _ => {}
    }
}

/// Returns the safe `array_map()` assignment represented by one statement.
fn map_candidate(stmt: &Stmt, env: &TypeEnv) -> Option<Candidate> {
    let StmtKind::Assign { name: mapped, value } = &stmt.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if name.as_str().trim_start_matches('\\') != "array_map" || args.len() != 2 {
        return None;
    }
    let ExprKind::Variable(source) = &args[1].kind else {
        return None;
    };
    if env.get(source).map(PhpType::codegen_repr)
        != Some(PhpType::Array(Box::new(PhpType::Int)))
    {
        return None;
    }
    let (param, body) = closure_arithmetic(&args[0])?;
    Some(Candidate {
        mapped: mapped.clone(),
        source: source.clone(),
        param: param.to_string(),
        body: body.clone(),
    })
}

/// Returns the parameter and return expression of a pure untyped single-parameter closure.
fn closure_arithmetic(expr: &Expr) -> Option<(&str, &Expr)> {
    let ExprKind::Closure {
        params,
        variadic: None,
        return_type,
        body,
        captures,
        capture_refs,
        by_ref_return: false,
        ..
    } = &expr.kind
    else {
        return None;
    };
    if return_type.is_some() || !captures.is_empty() || !capture_refs.is_empty() {
        return None;
    }
    let [(param, param_type, default, by_ref)] = params.as_slice() else {
        return None;
    };
    if !matches!(param_type, None | Some(TypeExpr::Int)) || default.is_some() || *by_ref {
        return None;
    }
    let [Stmt {
        kind: StmtKind::Return(Some(body)),
        ..
    }] = body.as_slice()
    else {
        return None;
    };
    pure_arithmetic(body, param).then_some((param.as_str(), body))
}

/// Returns true for a total arithmetic tree over the parameter and scalar literals.
fn pure_arithmetic(expr: &Expr, param: &str) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => name == param,
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) => true,
        ExprKind::Negate(inner) => pure_arithmetic(inner, param),
        ExprKind::BinaryOp { left, op, right } => {
            matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul)
                && pure_arithmetic(left, param)
                && pure_arithmetic(right, param)
        }
        _ => false,
    }
}

/// Copies a pure arithmetic body, replacing its parameter with the source read.
fn substitute(expr: &Expr, param: &str, replacement: &Expr) -> Expr {
    match &expr.kind {
        ExprKind::Variable(name) if name == param => replacement.clone(),
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) => expr.clone(),
        ExprKind::Negate(inner) => Expr {
            kind: ExprKind::Negate(Box::new(substitute(inner, param, replacement))),
            span: expr.span,
        },
        ExprKind::BinaryOp { left, op, right } => Expr {
            kind: ExprKind::BinaryOp {
                left: Box::new(substitute(left, param, replacement)),
                op: op.clone(),
                right: Box::new(substitute(right, param, replacement)),
            },
            span: expr.span,
        },
        _ => unreachable!("pure_arithmetic admitted a non-arithmetic node"),
    }
}

/// Builds one `$source[$index]` read at the original span.
fn source_read(source: &str, index: i64, span: Span) -> Expr {
    Expr {
        kind: ExprKind::ArrayAccess {
            array: Box::new(Expr {
                kind: ExprKind::Variable(source.to_string()),
                span,
            }),
            index: Box::new(Expr {
                kind: ExprKind::IntLiteral(index),
                span,
            }),
        },
        span,
    }
}

/// Rewrites one statement after the map assignment.
fn rewrite_stmt(stmt: &mut Stmt, candidate: &Candidate) -> Result<(), ()> {
    match &mut stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            if name == &candidate.mapped || name == &candidate.source {
                return Err(());
            }
            rewrite_expr(value, candidate)
        }
        StmtKind::ExprStmt(expr) | StmtKind::Echo(expr) => rewrite_expr(expr, candidate),
        StmtKind::Return(Some(expr)) => rewrite_expr(expr, candidate),
        StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => Ok(()),
        StmtKind::ArrayAssign { array, index, value } => {
            if array == &candidate.mapped || array == &candidate.source {
                return Err(());
            }
            rewrite_expr(index, candidate)?;
            rewrite_expr(value, candidate)
        }
        StmtKind::ArrayPush { array, value } => {
            if array == &candidate.mapped || array == &candidate.source {
                return Err(());
            }
            rewrite_expr(value, candidate)
        }
        StmtKind::Synthetic(body) => {
            for stmt in body {
                rewrite_stmt(stmt, candidate)?;
            }
            Ok(())
        }
        _ => Err(()),
    }
}

/// Rewrites every matching constant-indexed read inside one expression.
fn rewrite_expr(expr: &mut Expr, candidate: &Candidate) -> Result<(), ()> {
    if let ExprKind::ArrayAccess { array, index } = &expr.kind {
        if let ExprKind::Variable(name) = &array.kind {
            if name == &candidate.mapped {
                let ExprKind::IntLiteral(index) = &index.kind else {
                    return Err(());
                };
                let read = source_read(&candidate.source, *index, expr.span);
                let condition = Expr {
                    kind: ExprKind::FunctionCall {
                        name: Name::from("isset"),
                        args: vec![read.clone()],
                    },
                    span: expr.span,
                };
                let then_expr = substitute(&candidate.body, &candidate.param, &read);
                *expr = Expr {
                    kind: ExprKind::Ternary {
                        condition: Box::new(condition),
                        then_expr: Box::new(then_expr),
                        else_expr: Box::new(read),
                    },
                    span: expr.span,
                };
                return Ok(());
            }
        }
    }
    match &mut expr.kind {
        ExprKind::Variable(name) => {
            if name == &candidate.mapped || name == &candidate.source {
                Err(())
            } else {
                Ok(())
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            if !matches!(&array.kind, ExprKind::Variable(name) if name == &candidate.source) {
                rewrite_expr(array, candidate)?;
            }
            rewrite_expr(index, candidate)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            rewrite_expr(left, candidate)?;
            rewrite_expr(right, candidate)
        }
        ExprKind::Negate(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::InstanceOf { value: inner, .. } => rewrite_expr(inner, candidate),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            rewrite_expr(condition, candidate)?;
            rewrite_expr(then_expr, candidate)?;
            rewrite_expr(else_expr, candidate)
        }
        ExprKind::ShortTernary { value, default } => {
            rewrite_expr(value, candidate)?;
            rewrite_expr(default, candidate)
        }
        ExprKind::FunctionCall { args, .. } | ExprKind::NewObject { args, .. } => {
            for arg in args {
                rewrite_expr(arg, candidate)?;
            }
            Ok(())
        }
        ExprKind::ExprCall { callee, args } => {
            rewrite_expr(callee, candidate)?;
            for arg in args {
                rewrite_expr(arg, candidate)?;
            }
            Ok(())
        }
        ExprKind::ClosureCall { var, args } => {
            if var == &candidate.mapped || var == &candidate.source {
                return Err(());
            }
            for arg in args {
                rewrite_expr(arg, candidate)?;
            }
            Ok(())
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                rewrite_expr(item, candidate)?;
            }
            Ok(())
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                rewrite_expr(key, candidate)?;
                rewrite_expr(value, candidate)?;
            }
            Ok(())
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            rewrite_expr(subject, candidate)?;
            for (conditions, value) in arms {
                for condition in conditions {
                    rewrite_expr(condition, candidate)?;
                }
                rewrite_expr(value, candidate)?;
            }
            if let Some(default) = default {
                rewrite_expr(default, candidate)?;
            }
            Ok(())
        }
        ExprKind::Closure {
            captures,
            capture_refs,
            ..
        } => {
            if captures.iter().any(|name| name == &candidate.mapped || name == &candidate.source)
                || capture_refs
                    .iter()
                    .any(|name| name == &candidate.mapped || name == &candidate.source)
            {
                return Err(());
            }
            Ok(())
        }
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => {
            if name == &candidate.mapped || name == &candidate.source {
                Err(())
            } else {
                Ok(())
            }
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant { .. } => Ok(()),
        ExprKind::ArrayLiteralMixed(_) => Err(()),
        _ => Err(()),
    }
}

/// Reports whether a statement may mention one local, conservatively.
fn stmt_mentions(stmt: &Stmt, name: &str) -> bool {
    match &stmt.kind {
        StmtKind::Assign { name: target, value } | StmtKind::TypedAssign { name: target, value, .. } => {
            target == name || expr_mentions(value, name)
        }
        StmtKind::ExprStmt(expr) | StmtKind::Echo(expr) => expr_mentions(expr, name),
        StmtKind::Return(Some(expr)) => expr_mentions(expr, name),
        StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => false,
        StmtKind::ArrayAssign { array, index, value } => {
            array == name || expr_mentions(index, name) || expr_mentions(value, name)
        }
        StmtKind::ArrayPush { array, value } => array == name || expr_mentions(value, name),
        StmtKind::Synthetic(body) => body.iter().any(|stmt| stmt_mentions(stmt, name)),
        _ => true,
    }
}

/// Reports whether an expression may mention one local, conservatively.
fn expr_mentions(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::Variable(local) => local == name,
        ExprKind::ArrayAccess { array, index } => {
            expr_mentions(array, name) || expr_mentions(index, name)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_mentions(left, name) || expr_mentions(right, name)
        }
        ExprKind::Negate(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::InstanceOf { value: inner, .. } => expr_mentions(inner, name),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_mentions(condition, name)
                || expr_mentions(then_expr, name)
                || expr_mentions(else_expr, name)
        }
        ExprKind::ShortTernary { value, default } => {
            expr_mentions(value, name) || expr_mentions(default, name)
        }
        ExprKind::FunctionCall { args, .. } | ExprKind::NewObject { args, .. } => {
            args.iter().any(|arg| expr_mentions(arg, name))
        }
        ExprKind::ExprCall { callee, args } => {
            expr_mentions(callee, name) || args.iter().any(|arg| expr_mentions(arg, name))
        }
        ExprKind::ClosureCall { var, args } => {
            var == name || args.iter().any(|arg| expr_mentions(arg, name))
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(|item| expr_mentions(item, name)),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| expr_mentions(key, name) || expr_mentions(value, name)),
        ExprKind::Closure {
            captures,
            capture_refs,
            ..
        } => captures.iter().any(|local| local == name) || capture_refs.iter().any(|local| local == name),
        ExprKind::PreIncrement(local)
        | ExprKind::PostIncrement(local)
        | ExprKind::PreDecrement(local)
        | ExprKind::PostDecrement(local) => local == name,
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant { .. } => false,
        _ => true,
    }
}
