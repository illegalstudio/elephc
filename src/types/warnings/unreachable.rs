use crate::errors::CompileWarning;
use crate::parser::ast::{CatchClause, Stmt, StmtKind};

pub(super) fn collect_unreachable_recursive(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    collect_unreachable_in_block(stmts, warnings);
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_unreachable_recursive(then_body, warnings);
                for (_, body) in elseif_clauses {
                    collect_unreachable_recursive(body, warnings);
                }
                if let Some(body) = else_body {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_unreachable_recursive(then_body, warnings);
                if let Some(body) = else_body {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. } => collect_unreachable_recursive(body, warnings),
            StmtKind::For {
                init, update, body, ..
            } => {
                if let Some(stmt) = init {
                    collect_unreachable_recursive(std::slice::from_ref(stmt), warnings);
                }
                if let Some(stmt) = update {
                    collect_unreachable_recursive(std::slice::from_ref(stmt), warnings);
                }
                collect_unreachable_recursive(body, warnings);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_unreachable_recursive(body, warnings);
                }
                if let Some(body) = default {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_unreachable_recursive(try_body, warnings);
                for catch_clause in catches {
                    collect_unreachable_recursive(&catch_clause.body, warnings);
                }
                if let Some(body) = finally_body {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::FunctionDecl { body, .. } => collect_unreachable_recursive(body, warnings),
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::InterfaceDecl { methods, .. } => {
                for method in methods {
                    collect_unreachable_recursive(&method.body, warnings);
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_unreachable_recursive(body, warnings),
            _ => {}
        }
    }
}

fn collect_unreachable_in_block(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    let mut terminated = false;
    for stmt in stmts {
        if terminated {
            warnings.push(CompileWarning::new(stmt.span, "Unreachable code"));
            continue;
        }
        if stmt_guarantees_termination(stmt) {
            terminated = true;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TerminalEffect {
    FallsThrough,
    Breaks,
    ExitsCurrentBlock,
    TerminatesMixed,
}

fn stmt_guarantees_termination(stmt: &Stmt) -> bool {
    !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough)
}

fn stmt_terminal_effect(stmt: &Stmt) -> TerminalEffect {
    match &stmt.kind {
        StmtKind::Return(_) | StmtKind::Throw(_) | StmtKind::Continue => {
            TerminalEffect::ExitsCurrentBlock
        }
        StmtKind::Break => TerminalEffect::Breaks,
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
        StmtKind::Switch { cases, default, .. } => switch_terminal_effect(cases, default),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => try_terminal_effect(try_body, catches, finally_body),
        _ => TerminalEffect::FallsThrough,
    }
}

fn block_terminal_effect(stmts: &[Stmt]) -> TerminalEffect {
    stmts
        .last()
        .map(stmt_terminal_effect)
        .unwrap_or(TerminalEffect::FallsThrough)
}

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

fn combine_branch_effects(
    branch_effects: impl Iterator<Item = TerminalEffect>,
    else_effect: Option<TerminalEffect>,
) -> TerminalEffect {
    let Some(else_effect) = else_effect else {
        return TerminalEffect::FallsThrough;
    };

    merge_terminal_effects(std::iter::once(else_effect).chain(branch_effects))
}

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

fn switch_terminal_effect(
    cases: &[(Vec<crate::parser::ast::Expr>, Vec<Stmt>)],
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
