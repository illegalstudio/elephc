//! Purpose:
//! Analyzes whether statements and blocks guarantee function exit or other terminal control flow.
//! Supports parser/type-checker reasoning about returns, throws, breaks, continues, and infinite loops.
//!
//! Called from:
//! - `crate::types` and other frontend passes that need conservative control-flow facts.
//!
//! Key details:
//! - Results are conservative because silent false positives can cause missing return or reachability bugs.

use crate::parser::ast::{CatchClause, Expr, Stmt, StmtKind};

/// Terminal effect of a statement for control flow analysis.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalEffect {
    FallsThrough,
    Breaks,
    ExitsCurrentBlock,
    TerminatesMixed,
}

/// Checks if a statement guarantees termination (return, throw, etc).
pub(crate) fn stmt_guarantees_termination(stmt: &Stmt) -> bool {
    !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough)
}

/// Checks if a block guarantees function exit.
pub(crate) fn block_guarantees_function_exit(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_guarantees_function_exit)
}

/// Computes the terminal effect of a block of statements.
pub(crate) fn block_terminal_effect(stmts: &[Stmt]) -> TerminalEffect {
    stmts
        .iter()
        .map(stmt_terminal_effect)
        .find(|effect| !matches!(effect, TerminalEffect::FallsThrough))
        .unwrap_or(TerminalEffect::FallsThrough)
}

/// Computes the terminal effect of a single statement.
pub(crate) fn stmt_terminal_effect(stmt: &Stmt) -> TerminalEffect {
    match &stmt.kind {
        StmtKind::Synthetic(stmts) => block_terminal_effect(stmts),
        StmtKind::Return(_) | StmtKind::Throw(_) | StmtKind::Continue(_) => {
            TerminalEffect::ExitsCurrentBlock
        }
        StmtKind::Break(1) => TerminalEffect::Breaks,
        StmtKind::Break(_) => TerminalEffect::ExitsCurrentBlock,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => combine_branch_effects(
            std::iter::once(block_terminal_effect(then_body))
                .chain(elseif_clauses.iter().map(|(_, body)| block_terminal_effect(body))),
            else_body.as_ref().map(|body| block_terminal_effect(body)),
        ),
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => combine_branch_effects(
            std::iter::once(block_terminal_effect(then_body)),
            else_body.as_ref().map(|body| block_terminal_effect(body)),
        ),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => try_terminal_effect(try_body, catches, finally_body),
        StmtKind::Switch { cases, default, .. } => switch_terminal_effect(cases, default),
        _ => TerminalEffect::FallsThrough,
    }
}

/// Returns true if the given statement guarantees that the current function will exit.
/// Walks into synthetic blocks, namespace blocks, and expressions (for `throw`, `exit`, `die`).
fn stmt_guarantees_function_exit(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            block_guarantees_function_exit(stmts)
        }
        StmtKind::Return(_) | StmtKind::Throw(_) => true,
        StmtKind::ExprStmt(expr) => expr_guarantees_function_exit(expr),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            else_body
                .as_ref()
                .is_some_and(|body| block_guarantees_function_exit(body))
                && block_guarantees_function_exit(then_body)
                && elseif_clauses
                    .iter()
                    .all(|(_, body)| block_guarantees_function_exit(body))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            block_guarantees_function_exit(then_body)
                && else_body
                    .as_ref()
                    .is_some_and(|body| block_guarantees_function_exit(body))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => try_guarantees_function_exit(try_body, catches, finally_body),
        StmtKind::Switch { cases, default, .. } => switch_guarantees_function_exit(cases, default),
        StmtKind::While { condition, body } => {
            expr_is_truthy_literal(condition)
                && !block_may_break_current_loop(body)
        }
        StmtKind::DoWhile { body, condition } => {
            block_guarantees_function_exit(body)
                || (expr_is_truthy_literal(condition) && !block_may_break_current_loop(body))
        }
        StmtKind::For {
            condition, body, ..
        } => {
            condition
                .as_ref()
                .is_none_or(expr_is_truthy_literal)
                && !block_may_break_current_loop(body)
        }
        _ => false,
    }
}

/// Returns true if the given expression guarantees that the current function will exit.
/// Recognizes `throw`, `exit`/`die` function calls, and expressions wrapping these.
fn expr_guarantees_function_exit(expr: &Expr) -> bool {
    match &expr.kind {
        crate::parser::ast::ExprKind::Throw(_) => true,
        crate::parser::ast::ExprKind::ErrorSuppress(inner) => expr_guarantees_function_exit(inner),
        crate::parser::ast::ExprKind::FunctionCall { name, .. } => {
            matches!(name.as_str().to_ascii_lowercase().as_str(), "exit" | "die")
        }
        _ => false,
    }
}

/// Returns true if a try/catch/finally block guarantees function exit.
/// Exit is guaranteed if the finally body exits, or if the try body and all catch bodies exit.
fn try_guarantees_function_exit(
    try_body: &[Stmt],
    catches: &[CatchClause],
    finally_body: &Option<Vec<Stmt>>,
) -> bool {
    if finally_body
        .as_ref()
        .is_some_and(|body| block_guarantees_function_exit(body))
    {
        return true;
    }

    block_guarantees_function_exit(try_body)
        && catches
            .iter()
            .all(|catch| block_guarantees_function_exit(&catch.body))
}

/// Returns true if a switch block guarantees function exit.
/// The switch exits only if all cases and the default exit, and no case body may
/// break out of the switch before reaching an exit.
fn switch_guarantees_function_exit(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
) -> bool {
    if cases
        .iter()
        .any(|(_, body)| block_may_leave_current_switch_before_function_exit(body))
        || default
            .as_ref()
            .is_some_and(|body| block_may_leave_current_switch_before_function_exit(body))
    {
        return false;
    }

    matches!(
        switch_terminal_effect(cases, default),
        TerminalEffect::ExitsCurrentBlock
    )
}

/// Returns true if the expression is a truthy literal constant.
/// Recognizes bool, int, float, and non-empty non-"0" string literals.
fn expr_is_truthy_literal(expr: &Expr) -> bool {
    match &expr.kind {
        crate::parser::ast::ExprKind::BoolLiteral(value) => *value,
        crate::parser::ast::ExprKind::IntLiteral(value) => *value != 0,
        crate::parser::ast::ExprKind::FloatLiteral(value) => *value != 0.0,
        crate::parser::ast::ExprKind::StringLiteral(value) => !value.is_empty() && value != "0",
        _ => false,
    }
}

/// Returns true if any statement in the block may break out of the current loop
/// at the given breakable depth.
fn block_may_break_current_loop(stmts: &[Stmt]) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_may_break_current_loop(stmt, 1))
}

/// Returns true if any statement in the block may leave the current switch before
/// a function exit is reached, blocking guaranteed exit analysis.
fn block_may_leave_current_switch_before_function_exit(stmts: &[Stmt]) -> bool {
    for stmt in stmts {
        if stmt_guarantees_function_exit(stmt) {
            return false;
        }
        if stmt_may_leave_current_switch(stmt, 1) {
            return true;
        }
    }
    false
}

/// Returns true if the statement may break out of `breakable_depth_to_loop` or more
/// enclosing loops. `break(1)` at depth 1 matches one loop, `break(2)` requires two.
fn stmt_may_break_current_loop(stmt: &Stmt, breakable_depth_to_loop: usize) -> bool {
    match &stmt.kind {
        StmtKind::Break(level) => *level >= breakable_depth_to_loop,
        StmtKind::Synthetic(stmts)
        | StmtKind::NamespaceBlock { body: stmts, .. }
        | StmtKind::IncludeOnceGuard { body: stmts, .. } => stmts
            .iter()
            .any(|stmt| stmt_may_break_current_loop(stmt, breakable_depth_to_loop)),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            then_body
                .iter()
                .any(|stmt| stmt_may_break_current_loop(stmt, breakable_depth_to_loop))
                || elseif_clauses.iter().any(|(_, body)| {
                    body.iter()
                        .any(|stmt| stmt_may_break_current_loop(stmt, breakable_depth_to_loop))
                })
                || else_body.as_ref().is_some_and(|body| {
                    body.iter()
                        .any(|stmt| stmt_may_break_current_loop(stmt, breakable_depth_to_loop))
                })
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body
                .iter()
                .any(|stmt| stmt_may_break_current_loop(stmt, breakable_depth_to_loop))
                || else_body.as_ref().is_some_and(|body| {
                    body.iter()
                        .any(|stmt| stmt_may_break_current_loop(stmt, breakable_depth_to_loop))
                })
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body
                .iter()
                .any(|stmt| stmt_may_break_current_loop(stmt, breakable_depth_to_loop))
                || catches.iter().any(|catch| {
                    catch
                        .body
                        .iter()
                        .any(|stmt| stmt_may_break_current_loop(stmt, breakable_depth_to_loop))
                })
                || finally_body.as_ref().is_some_and(|body| {
                    body.iter()
                        .any(|stmt| stmt_may_break_current_loop(stmt, breakable_depth_to_loop))
                })
        }
        StmtKind::Switch { cases, default, .. } => {
            cases.iter().any(|(_, body)| {
                body.iter().any(|stmt| {
                    stmt_may_break_current_loop(stmt, breakable_depth_to_loop + 1)
                })
            }) || default.as_ref().is_some_and(|body| {
                body.iter().any(|stmt| {
                    stmt_may_break_current_loop(stmt, breakable_depth_to_loop + 1)
                })
            })
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. } => body.iter().any(|stmt| {
            stmt_may_break_current_loop(stmt, breakable_depth_to_loop + 1)
        }),
        StmtKind::For {
            init, update, body, ..
        } => {
            init.as_ref()
                .is_some_and(|stmt| stmt_may_break_current_loop(stmt, breakable_depth_to_loop))
                || update
                    .as_ref()
                    .is_some_and(|stmt| stmt_may_break_current_loop(stmt, breakable_depth_to_loop))
                || body.iter().any(|stmt| {
                    stmt_may_break_current_loop(stmt, breakable_depth_to_loop + 1)
                })
        }
        _ => false,
    }
}

/// Returns true if the statement may break or continue out of `breakable_depth_to_switch`
/// or more enclosing switch levels.
fn stmt_may_leave_current_switch(stmt: &Stmt, breakable_depth_to_switch: usize) -> bool {
    match &stmt.kind {
        StmtKind::Break(level) | StmtKind::Continue(level) => {
            *level >= breakable_depth_to_switch
        }
        StmtKind::Synthetic(stmts)
        | StmtKind::NamespaceBlock { body: stmts, .. }
        | StmtKind::IncludeOnceGuard { body: stmts, .. } => stmts
            .iter()
            .any(|stmt| stmt_may_leave_current_switch(stmt, breakable_depth_to_switch)),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            then_body
                .iter()
                .any(|stmt| stmt_may_leave_current_switch(stmt, breakable_depth_to_switch))
                || elseif_clauses.iter().any(|(_, body)| {
                    body.iter()
                        .any(|stmt| stmt_may_leave_current_switch(stmt, breakable_depth_to_switch))
                })
                || else_body.as_ref().is_some_and(|body| {
                    body.iter()
                        .any(|stmt| stmt_may_leave_current_switch(stmt, breakable_depth_to_switch))
                })
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body
                .iter()
                .any(|stmt| stmt_may_leave_current_switch(stmt, breakable_depth_to_switch))
                || else_body.as_ref().is_some_and(|body| {
                    body.iter()
                        .any(|stmt| stmt_may_leave_current_switch(stmt, breakable_depth_to_switch))
                })
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body
                .iter()
                .any(|stmt| stmt_may_leave_current_switch(stmt, breakable_depth_to_switch))
                || catches.iter().any(|catch| {
                    catch
                        .body
                        .iter()
                        .any(|stmt| stmt_may_leave_current_switch(stmt, breakable_depth_to_switch))
                })
                || finally_body.as_ref().is_some_and(|body| {
                    body.iter()
                        .any(|stmt| stmt_may_leave_current_switch(stmt, breakable_depth_to_switch))
                })
        }
        StmtKind::Switch { cases, default, .. } => {
            cases.iter().any(|(_, body)| {
                body.iter().any(|stmt| {
                    stmt_may_leave_current_switch(stmt, breakable_depth_to_switch + 1)
                })
            }) || default.as_ref().is_some_and(|body| {
                body.iter().any(|stmt| {
                    stmt_may_leave_current_switch(stmt, breakable_depth_to_switch + 1)
                })
            })
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. } => body.iter().any(|stmt| {
            stmt_may_leave_current_switch(stmt, breakable_depth_to_switch + 1)
        }),
        StmtKind::For {
            init, update, body, ..
        } => {
            init.as_ref()
                .is_some_and(|stmt| stmt_may_leave_current_switch(stmt, breakable_depth_to_switch))
                || update
                    .as_ref()
                    .is_some_and(|stmt| stmt_may_leave_current_switch(stmt, breakable_depth_to_switch))
                || body.iter().any(|stmt| {
                    stmt_may_leave_current_switch(stmt, breakable_depth_to_switch + 1)
                })
        }
        _ => false,
    }
}

/// Computes the terminal effect of a try/catch/finally block.
/// If a finally body is present and does not fall through, its effect takes precedence.
/// Otherwise merges the effects of the try body and all catch bodies.
fn try_terminal_effect(
    try_body: &[Stmt],
    catches: &[CatchClause],
    finally_body: &Option<Vec<Stmt>>,
) -> TerminalEffect {
    if let Some(finally_body) = finally_body {
        let finally_effect = block_terminal_effect(finally_body);
        if !matches!(finally_effect, TerminalEffect::FallsThrough) {
            return finally_effect;
        }
    }

    merge_terminal_effects(
        std::iter::once(block_terminal_effect(try_body))
            .chain(catches.iter().map(|catch| block_terminal_effect(&catch.body))),
    )
}

/// Computes the terminal effect of an if/ifndef branch structure.
/// Requires an else branch; returns FallsThrough if absent.
/// Merges the else branch first, then then/elseif branches.
fn combine_branch_effects(
    branch_effects: impl Iterator<Item = TerminalEffect>,
    else_effect: Option<TerminalEffect>,
) -> TerminalEffect {
    let Some(else_effect) = else_effect else {
        return TerminalEffect::FallsThrough;
    };

    merge_terminal_effects(std::iter::once(else_effect).chain(branch_effects))
}

/// Merges a sequence of terminal effects into a single result.
/// Any FallsThrough short-circuits the merge. TerminatesMixed results if both Breaks
/// and ExitsCurrentBlock are seen, or if any input is TerminatesMixed.
fn merge_terminal_effects(effects: impl Iterator<Item = TerminalEffect>) -> TerminalEffect {
    let mut saw_any = false;
    let mut saw_break = false;
    let mut saw_exit = false;
    let mut saw_mixed = false;

    for effect in effects {
        saw_any = true;
        match effect {
            TerminalEffect::FallsThrough => return TerminalEffect::FallsThrough,
            TerminalEffect::Breaks => saw_break = true,
            TerminalEffect::ExitsCurrentBlock => saw_exit = true,
            TerminalEffect::TerminatesMixed => saw_mixed = true,
        }
    }

    if !saw_any {
        TerminalEffect::FallsThrough
    } else if saw_mixed || (saw_break && saw_exit) {
        TerminalEffect::TerminatesMixed
    } else if saw_exit {
        TerminalEffect::ExitsCurrentBlock
    } else if saw_break {
        TerminalEffect::Breaks
    } else {
        TerminalEffect::FallsThrough
    }
}

/// Computes the terminal effect of a switch statement.
/// Returns ExitsCurrentBlock only if a default exists and all cases (iterated in reverse)
/// also exit the current block. Breaks or TerminatesMixed in any case body invalidates exit.
fn switch_terminal_effect(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
) -> TerminalEffect {
    let Some(default_body) = default.as_ref() else {
        return TerminalEffect::FallsThrough;
    };

    let mut suffix_exits = block_terminal_effect(default_body) == TerminalEffect::ExitsCurrentBlock;
    if !suffix_exits {
        return TerminalEffect::FallsThrough;
    }

    for (_, body) in cases.iter().rev() {
        suffix_exits = match block_terminal_effect(body) {
            TerminalEffect::ExitsCurrentBlock => true,
            TerminalEffect::FallsThrough => suffix_exits,
            TerminalEffect::Breaks | TerminalEffect::TerminatesMixed => false,
        };

        if !suffix_exits {
            return TerminalEffect::FallsThrough;
        }
    }

    TerminalEffect::ExitsCurrentBlock
}
