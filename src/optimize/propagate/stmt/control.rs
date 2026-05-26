//! Purpose:
//! Propagates constants through statement control cases.
//! Maintains scalar environments while preserving declarations and control-flow side effects.
//!
//! Called from:
//! - `crate::optimize::propagate::stmt`
//!
//! Key details:
//! - Statement propagation must invalidate aliases and writes before substituting values across observable boundaries.

use super::*;

/// Propagates constants through an `#ifdef` statement.
/// Clones `env` for the then-branch, uses the original for the else-branch, then merges both paths.
pub(super) fn propagate_ifdef_stmt(
    symbol: String,
    then_body: Vec<Stmt>,
    else_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
    env: ConstantEnv,
) -> (Stmt, ConstantEnv) {
    let (then_body, then_env) = propagate_block(then_body, env.clone());
    let (else_body, next_env) = match else_body {
        Some(body) => {
            let (body, else_env) = propagate_block(body, env);
            (Some(body), merge_constant_env_paths(vec![then_env, else_env]))
        }
        None => (None, merge_constant_env_paths(vec![then_env, env])),
    };
    (
        Stmt::new(
            StmtKind::IfDef {
                symbol,
                then_body,
                else_body,
            },
            span,
        ),
        next_env,
    )
}

/// Propagates constants through a `while` loop.
/// Evaluates the condition, propagates into the body, then updates the environment based on loop exit.
/// If the condition is known false, returns the original env; if known true, merges break paths.
pub(super) fn propagate_while_stmt(
    condition: Expr,
    body: Vec<Stmt>,
    span: crate::span::Span,
    env: ConstantEnv,
) -> (Stmt, ConstantEnv) {
    let loop_env = safe_loop_env(&env, std::slice::from_ref(&condition), &body, None);
    let condition = propagate_expr(condition, &loop_env);
    let (body, _) = propagate_block(body, loop_env.clone());
    let next_env = match scalar_value(&condition) {
        Some(value) if !value.truthy() => env,
        Some(_) => merge_loop_exit_paths(simulate_loop_block_constant_paths(&body, loop_env.clone())),
        None => loop_env,
    };
    (
        Stmt::new(StmtKind::While { condition, body }, span),
        next_env,
    )
}

/// Propagates constants through a `do-while` loop.
/// Executes the body first, then evaluates the condition to determine the exit environment.
pub(super) fn propagate_do_while_stmt(
    body: Vec<Stmt>,
    condition: Expr,
    span: crate::span::Span,
    env: ConstantEnv,
) -> (Stmt, ConstantEnv) {
    let loop_env = safe_loop_env(&env, std::slice::from_ref(&condition), &body, None);
    let (body, _) = propagate_block(body, loop_env.clone());
    let condition = propagate_expr(condition, &loop_env);
    let next_env = match scalar_value(&condition) {
        Some(value) if value.truthy() => {
            merge_loop_exit_paths(simulate_loop_block_constant_paths(&body, loop_env.clone()))
        }
        Some(_) => merge_do_while_false_exit_paths(simulate_loop_block_constant_paths(
            &body,
            loop_env.clone(),
        )),
        None => loop_env,
    };
    (
        Stmt::new(StmtKind::DoWhile { body, condition }, span),
        next_env,
    )
}

/// Propagates constants through a `for` loop.
/// Processes init first, then builds the loop environment from the init result. Evaluates condition and update
/// in that environment, propagates into the body, and derives the exit environment from the condition.
pub(super) fn propagate_for_stmt(
    init: Option<Box<Stmt>>,
    condition: Option<Expr>,
    update: Option<Box<Stmt>>,
    body: Vec<Stmt>,
    span: crate::span::Span,
    env: ConstantEnv,
) -> (Stmt, ConstantEnv) {
    let (init, init_env) = match init {
        Some(stmt) => {
            let (stmt, next_env) = propagate_stmt(*stmt, env);
            (Some(Box::new(stmt)), next_env)
        }
        None => (None, env),
    };
    let condition_exprs = condition.iter().cloned().collect::<Vec<_>>();
    let update_stmt = update.as_deref();
    let loop_env = safe_loop_env(&init_env, &condition_exprs, &body, update_stmt);
    let condition = condition.map(|expr| propagate_expr(expr, &loop_env));
    let update = update.map(|stmt| Box::new(propagate_stmt(*stmt, loop_env.clone()).0));
    let (body, _) = propagate_block(body, loop_env.clone());
    let next_env = match condition.as_ref().and_then(scalar_value) {
        Some(value) if !value.truthy() => init_env,
        Some(_) | None if condition.is_none() => {
            merge_loop_exit_paths(simulate_loop_block_constant_paths(&body, loop_env.clone()))
        }
        _ => loop_env,
    };
    (
        Stmt::new(
            StmtKind::For {
                init,
                condition,
                update,
                body,
            },
            span,
        ),
        next_env,
    )
}

/// Propagates constants through a `foreach` loop.
/// Builds the loop environment from the source array, removes the value variable (or the array itself if by-ref),
/// propagates the array expression against the original env, then propagates the body in the loop env.
pub(super) fn propagate_foreach_stmt(
    array: Expr,
    key_var: Option<String>,
    value_var: String,
    value_by_ref: bool,
    body: Vec<Stmt>,
    span: crate::span::Span,
    env: ConstantEnv,
) -> (Stmt, ConstantEnv) {
    let mut loop_env = safe_foreach_env(&env, &array, key_var.as_deref(), &value_var, &body);
    if value_by_ref {
        if let ExprKind::Variable(array_name) = &array.kind {
            loop_env.remove(array_name);
        }
    }
    let array = propagate_expr(array, &env);
    let (body, _) = propagate_block(body, loop_env.clone());
    (
        Stmt::new(
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                value_by_ref,
                body,
            },
            span,
        ),
        loop_env,
    )
}

/// Propagates constants through a `switch` statement.
/// Propagates the subject, then propagates each case pattern and body in the base env (empty if subject has side effects).
/// The exit environment is derived by merging all case paths and the default branch.
pub(super) fn propagate_switch_stmt(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    span: crate::span::Span,
    env: ConstantEnv,
) -> (Stmt, ConstantEnv) {
    let subject = propagate_expr(subject, &env);
    let base_env = if expr_effect(&subject).has_side_effects {
        HashMap::new()
    } else {
        env
    };
    let cases: Vec<_> = cases
        .into_iter()
        .map(|(patterns, body)| {
            let patterns = patterns
                .into_iter()
                .map(|pattern| propagate_expr(pattern, &base_env))
                .collect();
            let (body, _) = propagate_block(body, base_env.clone());
            (patterns, body)
        })
        .collect();
    let default = default.map(|body| propagate_block(body, base_env.clone()).0);
    let next_env = merge_switch_constant_env_paths(&subject, &cases, default.as_deref(), &base_env);
    (
        Stmt::new(
            StmtKind::Switch {
                subject,
                cases,
                default,
            },
            span,
        ),
        next_env,
    )
}

/// Propagates constants through a `try-catch-finally` statement.
/// Propagates the try body and each catch clause against the original env, but the finally body in an empty env
/// (because finally always executes regardless of control flow). Merges all paths to produce the exit environment.
pub(super) fn propagate_try_stmt(
    try_body: Vec<Stmt>,
    catches: Vec<crate::parser::ast::CatchClause>,
    finally_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
    env: ConstantEnv,
) -> (Stmt, ConstantEnv) {
    let (try_body, _) = propagate_block(try_body, env.clone());
    let catches: Vec<_> = catches
        .into_iter()
        .map(|catch| crate::parser::ast::CatchClause {
            exception_types: catch.exception_types,
            variable: catch.variable,
            body: propagate_block(catch.body, env.clone()).0,
        })
        .collect();
    let finally_body = finally_body.map(|body| propagate_block(body, HashMap::new()).0);
    let next_env = merge_try_constant_env_paths(&try_body, &catches, finally_body.as_deref(), &env);
    (
        Stmt::new(
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            },
            span,
        ),
        next_env,
    )
}

/// Extracts the exit environment from a loop path summary by merging all break paths.
fn merge_loop_exit_paths(summary: ConstantLoopPathSummary) -> ConstantEnv {
    merge_constant_env_paths(summary.break_paths)
}

/// Extracts the exit environment for a do-while false condition by merging fallthrough, break, and continue paths.
fn merge_do_while_false_exit_paths(mut summary: ConstantLoopPathSummary) -> ConstantEnv {
    let mut paths = Vec::new();
    paths.append(&mut summary.fallthrough_paths);
    paths.append(&mut summary.break_paths);
    paths.append(&mut summary.continue_paths);
    merge_constant_env_paths(paths)
}

/// Propagates constants through an `if-elseif-else` statement.
/// Propagates the condition first; if it has side effects the base env is emptied. Processes each branch in
/// a derived env, then merges all fallthrough paths to produce the exit environment.
pub(super) fn propagate_if_stmt(
    condition: Expr,
    then_body: Vec<Stmt>,
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
    env: ConstantEnv,
) -> (Stmt, ConstantEnv) {
    let condition = propagate_expr(condition, &env);
    let base_env = if expr_effect(&condition).has_side_effects {
        HashMap::new()
    } else {
        env
    };

    let (then_body, then_env) = propagate_block(then_body, base_env.clone());
    let mut propagated_elseifs = Vec::new();
    let mut elseif_envs = Vec::new();
    for (condition, body) in elseif_clauses {
        let condition = propagate_expr(condition, &base_env);
        let branch_env = if expr_effect(&condition).has_side_effects {
            HashMap::new()
        } else {
            base_env.clone()
        };
        let (body, env_after_body) = propagate_block(body, branch_env);
        if matches!(block_terminal_effect(&body), TerminalEffect::FallsThrough) {
            elseif_envs.push(env_after_body.clone());
        }
        propagated_elseifs.push((condition, body));
    }

    let (else_body, else_env) = match else_body {
        Some(body) => {
            let (body, env_after_body) = propagate_block(body, base_env.clone());
            (Some(body), Some(env_after_body))
        }
        None => (None, Some(base_env.clone())),
    };

    let next_env = match scalar_value(&condition) {
        Some(value) if value.truthy() => then_env,
        Some(_) => else_env.unwrap_or_default(),
        None => {
            let mut paths = Vec::new();
            if matches!(block_terminal_effect(&then_body), TerminalEffect::FallsThrough) {
                paths.push(then_env);
            }
            paths.extend(elseif_envs);
            if let Some(else_env) = else_env {
                if else_body
                    .as_ref()
                    .is_none_or(|body| matches!(block_terminal_effect(body), TerminalEffect::FallsThrough))
                {
                    paths.push(else_env);
                }
            }
            merge_constant_env_paths(paths)
        }
    };

    (
        Stmt::new(
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses: propagated_elseifs,
                else_body,
            },
            span,
        ),
        next_env,
    )
}
