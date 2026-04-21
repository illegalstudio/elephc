use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TailPathKind {
    NoTail,
    FallsThrough,
    Breaks,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IfTailReachability {
    pub(crate) then_sinks_tail: bool,
    pub(crate) elseif_sinks_tail: Vec<bool>,
    pub(crate) else_sinks_tail: bool,
    pub(crate) implicit_else_sinks_tail: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IfDefTailReachability {
    pub(crate) then_sinks_tail: bool,
    pub(crate) else_sinks_tail: bool,
    pub(crate) implicit_else_sinks_tail: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SwitchTailReachability {
    pub(crate) case_tail_paths: Vec<TailPathKind>,
    pub(crate) default_tail_path: Option<TailPathKind>,
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

pub(crate) fn analyze_if_tail_paths(
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: &Option<Vec<Stmt>>,
) -> IfTailReachability {
    IfTailReachability {
        then_sinks_tail: block_reaches_following_stmt(then_body),
        elseif_sinks_tail: elseif_clauses
            .iter()
            .map(|(_, body)| block_reaches_following_stmt(body))
            .collect(),
        else_sinks_tail: else_body
            .as_ref()
            .is_some_and(|body| block_reaches_following_stmt(body)),
        implicit_else_sinks_tail: else_body.is_none(),
    }
}

pub(crate) fn analyze_ifdef_tail_paths(
    then_body: &[Stmt],
    else_body: &Option<Vec<Stmt>>,
) -> IfDefTailReachability {
    IfDefTailReachability {
        then_sinks_tail: block_reaches_following_stmt(then_body),
        else_sinks_tail: else_body
            .as_ref()
            .is_some_and(|body| block_reaches_following_stmt(body)),
        implicit_else_sinks_tail: else_body.is_none(),
    }
}

pub(crate) fn analyze_switch_tail_paths(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
) -> SwitchTailReachability {
    let mut case_tail_paths = vec![TailPathKind::NoTail; cases.len()];
    let default_tail_path = default
        .as_ref()
        .map(|body| terminal_effect_tail_path(block_terminal_effect(body)));

    let mut next_tail_path = default_tail_path.unwrap_or(TailPathKind::FallsThrough);

    for (index, (_, body)) in cases.iter().enumerate().rev() {
        let case_tail_path = match block_terminal_effect(body) {
            TerminalEffect::FallsThrough => next_tail_path,
            effect => terminal_effect_tail_path(effect),
        };
        case_tail_paths[index] = case_tail_path;
        next_tail_path = case_tail_path;
    }

    SwitchTailReachability {
        case_tail_paths,
        default_tail_path,
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

fn terminal_effect_tail_path(effect: TerminalEffect) -> TailPathKind {
    match effect {
        TerminalEffect::FallsThrough => TailPathKind::FallsThrough,
        TerminalEffect::Breaks => TailPathKind::Breaks,
        TerminalEffect::ExitsCurrentBlock => TailPathKind::NoTail,
        TerminalEffect::TerminatesMixed => TailPathKind::Unknown,
    }
}
