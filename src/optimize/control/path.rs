use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SwitchTailReachability {
    pub(crate) case_sinks_tail: Vec<bool>,
    pub(crate) default_sinks_tail: bool,
    pub(crate) has_break_exit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TryTailSinkPlan {
    LeaveOutside,
    IntoTryPaths,
    IntoFinally,
}

pub(crate) fn block_reaches_following_stmt(stmts: &[Stmt]) -> bool {
    matches!(block_terminal_effect(stmts), TerminalEffect::FallsThrough)
}

pub(crate) fn analyze_switch_tail_paths(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
) -> SwitchTailReachability {
    let has_break_exit = cases
        .iter()
        .any(|(_, body)| matches!(block_terminal_effect(body), TerminalEffect::Breaks))
        || default
            .as_ref()
            .is_some_and(|body| matches!(block_terminal_effect(body), TerminalEffect::Breaks));

    let mut case_sinks_tail = vec![false; cases.len()];
    let mut default_sinks_tail = false;

    if has_break_exit {
        return SwitchTailReachability {
            case_sinks_tail,
            default_sinks_tail,
            has_break_exit,
        };
    }

    if let Some(default_body) = default.as_ref() {
        if block_reaches_following_stmt(default_body) {
            default_sinks_tail = true;
        }
    } else if let Some((_, body)) = cases.last() {
        if block_reaches_following_stmt(body) {
            case_sinks_tail[cases.len() - 1] = true;
        }
    }

    SwitchTailReachability {
        case_sinks_tail,
        default_sinks_tail,
        has_break_exit,
    }
}

pub(crate) fn analyze_try_tail_plan(
    try_body: &[Stmt],
    catches: &[crate::parser::ast::CatchClause],
    finally_body: &Option<Vec<Stmt>>,
) -> TryTailSinkPlan {
    match finally_body {
        None => TryTailSinkPlan::IntoTryPaths,
        Some(finally_body)
            if catches.is_empty()
                && !block_may_throw(try_body)
                && block_reaches_following_stmt(try_body)
                && block_reaches_following_stmt(finally_body) =>
        {
            TryTailSinkPlan::IntoFinally
        }
        Some(_) => TryTailSinkPlan::LeaveOutside,
    }
}
