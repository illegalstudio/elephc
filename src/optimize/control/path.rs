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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TryTailReachability {
    pub(crate) try_tail_path: TailPathKind,
    pub(crate) catch_tail_paths: Vec<TailPathKind>,
    pub(crate) finally_tail_path: Option<TailPathKind>,
    pub(crate) can_sink_into_finally: bool,
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
    let cfg = build_switch_cfg(cases, default);
    let case_tail_paths = classify_switch_cfg_paths(&cfg)
        .into_iter()
        .map(cfg_successor_tail_path)
        .collect();
    let default_tail_path = cfg
        .default_entry
        .map(|entry| cfg_successor_tail_path(classify_cfg_successor(&cfg.blocks, BasicBlockSuccessor::Block(entry))));

    SwitchTailReachability {
        case_tail_paths,
        default_tail_path,
    }
}

pub(crate) fn analyze_try_tail_paths(
    try_body: &[Stmt],
    catches: &[crate::parser::ast::CatchClause],
    finally_body: &Option<Vec<Stmt>>,
) -> TryTailReachability {
    let try_tail_path = terminal_effect_tail_path(block_terminal_effect(try_body));
    let catch_tail_paths = catches
        .iter()
        .map(|catch| terminal_effect_tail_path(block_terminal_effect(&catch.body)))
        .collect();
    let finally_tail_path = finally_body
        .as_ref()
        .map(|body| terminal_effect_tail_path(block_terminal_effect(body)));

    TryTailReachability {
        try_tail_path,
        catch_tail_paths,
        finally_tail_path,
        can_sink_into_finally: catches.is_empty()
            && !block_may_throw(try_body)
            && matches!(try_tail_path, TailPathKind::FallsThrough)
            && matches!(finally_tail_path, Some(TailPathKind::FallsThrough)),
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

fn cfg_successor_tail_path(successor: BasicBlockSuccessor) -> TailPathKind {
    match successor {
        BasicBlockSuccessor::FallsThrough => TailPathKind::FallsThrough,
        BasicBlockSuccessor::Breaks => TailPathKind::Breaks,
        BasicBlockSuccessor::Exits => TailPathKind::NoTail,
        BasicBlockSuccessor::Unknown | BasicBlockSuccessor::Block(_) => TailPathKind::Unknown,
    }
}
