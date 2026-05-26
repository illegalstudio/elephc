//! Purpose:
//! Implements optimizer control-flow path logic.
//! Supports normalization, reachability, path analysis, and structural rewrites used by pruning and DCE.
//!
//! Called from:
//! - `crate::optimize::control`
//!
//! Key details:
//! - Control-flow helpers must treat terminal effects, switch fallthrough, and exception paths conservatively.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Describes how a control-flow path terminates: whether it falls through to the next statement,
/// breaks out of a loop/switch, exits the function entirely, or has unknown behavior.
pub(crate) enum TailPathKind {
    NoTail,
    FallsThrough,
    Breaks,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Reachability analysis for an if/elseif/else statement, describing which branches
/// may fall through to the statement following the if.
pub(crate) struct IfTailReachability {
    pub(crate) then_sinks_tail: bool,
    pub(crate) elseif_sinks_tail: Vec<bool>,
    pub(crate) else_sinks_tail: bool,
    pub(crate) implicit_else_sinks_tail: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Reachability analysis for an ifdef/else statement, describing which branches
/// may fall through to the statement following the ifdef.
pub(crate) struct IfDefTailReachability {
    pub(crate) then_sinks_tail: bool,
    pub(crate) else_sinks_tail: bool,
    pub(crate) implicit_else_sinks_tail: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Reachability analysis for a switch statement, describing the tail path kind
/// for each case and the default branch.
pub(crate) struct SwitchTailReachability {
    pub(crate) case_tail_paths: Vec<TailPathKind>,
    pub(crate) default_tail_path: Option<TailPathKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Reachability analysis for a try/catch/finally statement, describing the tail path
/// for the try body, each catch block, and the finally block. Also indicates whether
/// an exception thrown in the try body can sink into the finally block.
pub(crate) struct TryTailReachability {
    pub(crate) try_tail_path: TailPathKind,
    pub(crate) catch_tail_paths: Vec<TailPathKind>,
    pub(crate) finally_tail_path: Option<TailPathKind>,
    pub(crate) can_sink_into_finally: bool,
}

/// Returns `true` if the given statement block falls through to the following statement,
/// rather than terminating, breaking, or unconditionally throwing.
pub(crate) fn block_reaches_following_stmt(stmts: &[Stmt]) -> bool {
    matches!(block_terminal_effect(stmts), TerminalEffect::FallsThrough)
}

/// Analyzes the tail paths of an if/elseif/else statement and returns reachability
/// information for each branch. `then_body`, `elseif_clauses`, and `else_body` are the
/// statement lists for the respective branches.
pub(crate) fn analyze_if_tail_paths(
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: &Option<Vec<Stmt>>,
) -> IfTailReachability {
    let cfg = build_if_cfg(then_body, elseif_clauses, else_body);
    let body_paths = classify_if_cfg_paths(&cfg);
    IfTailReachability {
        then_sinks_tail: matches!(body_paths.first(), Some(BasicBlockSuccessor::FallsThrough)),
        elseif_sinks_tail: body_paths[1..]
            .iter()
            .map(|successor| matches!(successor, BasicBlockSuccessor::FallsThrough))
            .collect(),
        else_sinks_tail: cfg.else_entry.is_some_and(|entry| {
            matches!(
                classify_cfg_successor(&cfg.blocks, BasicBlockSuccessor::Block(entry)),
                BasicBlockSuccessor::FallsThrough
            )
        }),
        implicit_else_sinks_tail: matches!(cfg.implicit_else_successor, BasicBlockSuccessor::FallsThrough),
    }
}

/// Analyzes the tail paths of an ifdef/else statement and returns reachability
/// information for the then and else branches. `else_body` of `None` represents
/// an implicit empty else (which implicitly falls through).
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

/// Analyzes the tail paths of a switch statement and returns reachability information
/// for each case and the default branch. `cases` is a list of (match expressions, statements)
/// pairs, and `default` is the optional default block statements.
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

/// Analyzes the tail paths of a try/catch/finally statement and returns reachability
/// information for the try body, each catch block, and the finally block. Sets
/// `can_sink_into_finally` to true only when there are no catches, the try body may
/// fall through, and the finally block also falls through.
pub(crate) fn analyze_try_tail_paths(
    try_body: &[Stmt],
    catches: &[crate::parser::ast::CatchClause],
    finally_body: &Option<Vec<Stmt>>,
) -> TryTailReachability {
    let cfg = build_try_cfg(try_body, catches, finally_body);
    let mut path_iter = classify_try_cfg_paths(&cfg)
        .into_iter()
        .map(cfg_successor_tail_path);
    let try_tail_path = path_iter.next().unwrap_or(TailPathKind::Unknown);
    let catch_tail_paths: Vec<_> = path_iter.take(catches.len()).collect();
    let finally_tail_path = cfg.finally_entry.map(|entry| {
        cfg_successor_tail_path(classify_cfg_successor(&cfg.blocks, BasicBlockSuccessor::Block(entry)))
    });

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

/// Converts a `BasicBlockSuccessor` into a `TailPathKind`. Panics if the successor
/// is a `Block` variant, as this function only handles terminal successors.
fn cfg_successor_tail_path(successor: BasicBlockSuccessor) -> TailPathKind {
    match successor {
        BasicBlockSuccessor::FallsThrough => TailPathKind::FallsThrough,
        BasicBlockSuccessor::Breaks => TailPathKind::Breaks,
        BasicBlockSuccessor::Exits => TailPathKind::NoTail,
        BasicBlockSuccessor::Unknown | BasicBlockSuccessor::Block(_) => TailPathKind::Unknown,
    }
}
