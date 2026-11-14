//! Purpose:
//! Rewrites foldable static builtin and callback calls through the AST.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - Statement and expression traversal preserves spans and evaluation structure.

use super::*;

/// Rewrites foldable static builtin calls in a program to integer literals.
pub(super) fn fold_static_builtin_calls_in_program(program: Program, php_only: bool) -> Program {
    program
        .into_iter()
        .map(|stmt| fold_static_builtin_calls_in_stmt(stmt, php_only))
        .collect()
}

/// Rewrites foldable static builtin calls inside one statement.
pub(super) fn fold_static_builtin_calls_in_stmt(stmt: Stmt, php_only: bool) -> Stmt {
    let fold_static_builtin_calls_in_expr =
        |expr| fold_static_builtin_calls_in_expr(expr, php_only);
    let fold_static_builtin_calls_in_program =
        |program| fold_static_builtin_calls_in_program(program, php_only);
    let kind = match stmt.kind {
        StmtKind::Echo(expr) => StmtKind::Echo(fold_static_builtin_calls_in_expr(expr)),
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name,
            value: fold_static_builtin_calls_in_expr(value),
        },
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition: fold_static_builtin_calls_in_expr(condition),
            then_body: fold_static_builtin_calls_in_program(then_body),
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(condition, body)| {
                    (
                        fold_static_builtin_calls_in_expr(condition),
                        fold_static_builtin_calls_in_program(body),
                    )
                })
                .collect(),
            else_body: else_body.map(fold_static_builtin_calls_in_program),
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition: fold_static_builtin_calls_in_expr(condition),
            body: fold_static_builtin_calls_in_program(body),
        },
        StmtKind::DoWhile { condition, body } => StmtKind::DoWhile {
            condition: fold_static_builtin_calls_in_expr(condition),
            body: fold_static_builtin_calls_in_program(body),
        },
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => StmtKind::For {
            init: init.map(|stmt| Box::new(fold_static_builtin_calls_in_stmt(*stmt, php_only))),
            condition: condition.map(fold_static_builtin_calls_in_expr),
            update: update.map(|stmt| Box::new(fold_static_builtin_calls_in_stmt(*stmt, php_only))),
            body: fold_static_builtin_calls_in_program(body),
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject: fold_static_builtin_calls_in_expr(subject),
            cases: cases
                .into_iter()
                .map(|(conditions, body)| {
                    (
                        conditions
                            .into_iter()
                            .map(fold_static_builtin_calls_in_expr)
                            .collect(),
                        fold_static_builtin_calls_in_program(body),
                    )
                })
                .collect(),
            default: default.map(fold_static_builtin_calls_in_program),
        },
        StmtKind::Return(Some(expr)) => {
            StmtKind::Return(Some(fold_static_builtin_calls_in_expr(expr)))
        }
        StmtKind::ExprStmt(expr) => StmtKind::ExprStmt(fold_static_builtin_calls_in_expr(expr)),
        other => other,
    };
    Stmt {
        kind,
        span: stmt.span,
        source_mode: stmt.source_mode,
        strict_types: stmt.strict_types,
        attributes: stmt.attributes,
    }
}

/// Rewrites foldable static builtin calls inside one expression.
pub(super) fn fold_static_builtin_calls_in_expr(expr: Expr, php_only: bool) -> Expr {
    let fold_static_builtin_calls_in_expr =
        |expr| fold_static_builtin_calls_in_expr(expr, php_only);
    let span = expr.span;
    let source_profile = expr.source_profile;
    let kind = match expr.kind {
        ExprKind::UnaryPlus(inner) => {
            ExprKind::UnaryPlus(Box::new(fold_static_builtin_calls_in_expr(*inner)))
        }
        ExprKind::Negate(inner) => {
            ExprKind::Negate(Box::new(fold_static_builtin_calls_in_expr(*inner)))
        }
        ExprKind::Not(inner) => ExprKind::Not(Box::new(fold_static_builtin_calls_in_expr(*inner))),
        ExprKind::BitNot(inner) => {
            ExprKind::BitNot(Box::new(fold_static_builtin_calls_in_expr(*inner)))
        }
        ExprKind::Print(inner) => {
            ExprKind::Print(Box::new(fold_static_builtin_calls_in_expr(*inner)))
        }
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(fold_static_builtin_calls_in_expr(*left)),
            op,
            right: Box::new(fold_static_builtin_calls_in_expr(*right)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(fold_static_builtin_calls_in_expr(*condition)),
            then_expr: Box::new(fold_static_builtin_calls_in_expr(*then_expr)),
            else_expr: Box::new(fold_static_builtin_calls_in_expr(*else_expr)),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(fold_static_builtin_calls_in_expr(*value)),
            default: Box::new(fold_static_builtin_calls_in_expr(*default)),
        },
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(fold_static_builtin_calls_in_expr(*value)),
            default: Box::new(fold_static_builtin_calls_in_expr(*default)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(fold_static_builtin_calls_in_expr(*expr)),
        },
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(fold_static_builtin_calls_in_expr(*subject)),
            arms: arms
                .into_iter()
                .map(|(conditions, result)| {
                    (
                        conditions
                            .into_iter()
                            .map(fold_static_builtin_calls_in_expr)
                            .collect(),
                        fold_static_builtin_calls_in_expr(result),
                    )
                })
                .collect(),
            default: default.map(|expr| Box::new(fold_static_builtin_calls_in_expr(*expr))),
        },
        ExprKind::ArrayLiteral(items) => ExprKind::ArrayLiteral(
            items
                .into_iter()
                .map(fold_static_builtin_calls_in_expr)
                .collect(),
        ),
        ExprKind::ArrayLiteralAssoc(pairs) => ExprKind::ArrayLiteralAssoc(
            pairs
                .into_iter()
                .map(|(key, value)| {
                    (
                        fold_static_builtin_calls_in_expr(key),
                        fold_static_builtin_calls_in_expr(value),
                    )
                })
                .collect(),
        ),
        ExprKind::FunctionCall { name, args } => {
            let folded_args = args
                .into_iter()
                .map(fold_static_builtin_calls_in_expr)
                .collect::<Vec<_>>();
            if let Some(kind) = fold_static_call_user_func_call(
                name.as_str().trim_start_matches('\\'),
                &folded_args,
                php_only,
            ) {
                kind
            } else if let Some(kind) = fold_static_builtin_call(
                name.as_str().trim_start_matches('\\'),
                &folded_args,
                php_only,
            )
            {
                kind
            } else {
                ExprKind::FunctionCall {
                    name,
                    args: folded_args,
                }
            }
        }
        other => other,
    };
    Expr { kind, span, source_profile }
}

/// Folds `call_user_func*()` when the callback is a pure foldable builtin.
pub(super) fn fold_static_call_user_func_call(
    short_name: &str,
    args: &[Expr],
    php_only: bool,
) -> Option<ExprKind> {
    match php_symbol_key(short_name).as_str() {
        "call_user_func" => {
            let (callback, callback_args) = args.split_first()?;
            fold_static_callback_call(callback, callback_args, php_only)
        }
        "call_user_func_array" => {
            let [callback, arg_array] = args else {
                return None;
            };
            let callback_args = static_call_user_func_array_args(arg_array)?;
            fold_static_callback_call(callback, &callback_args, php_only)
        }
        _ => None,
    }
}

/// Folds one static string callback when it names a pure foldable builtin.
pub(super) fn fold_static_callback_call(
    callback: &Expr,
    callback_args: &[Expr],
    php_only: bool,
) -> Option<ExprKind> {
    let ExprKind::StringLiteral(callback_name) = &callback.kind else {
        return None;
    };
    if callback_name.contains("::") {
        return None;
    }
    fold_static_builtin_call(
        callback_name.trim_start_matches('\\'),
        callback_args,
        php_only,
    )
}
