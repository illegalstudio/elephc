//! Purpose:
//! Models optimizer side effects for aliases behavior.
//! Feeds purity, callable alias, builtin, and call-effect decisions into pruning and dead-code elimination.
//!
//! Called from:
//! - `crate::optimize::effects`
//!
//! Key details:
//! - Effect summaries must account for globals, heap/runtime state, output, throws, and by-reference mutation.

use super::*;
use super::calls::{callable_target_call_effect, closure_alias_effect, merge_callable_value_effects};

/// Extracts the callable alias effect from an expression, if the expression can resolve to a callable.
///
/// Handles first-class callables, closures, ternary/null-coalesce/match-based callable expressions,
/// and variable lookups against the active callable alias map. Returns `None` for expressions
/// that cannot produce a callable value.
pub(super) fn callable_alias_from_expr(expr: &Expr) -> Option<Effect> {
    match &expr.kind {
        ExprKind::FirstClassCallable(target) => Some(callable_target_call_effect(target)),
        ExprKind::Closure { .. } => closure_alias_effect(expr),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => merge_callable_value_effects([
            callable_alias_from_expr(then_expr),
            callable_alias_from_expr(else_expr),
        ]),
        ExprKind::ShortTernary { value, default } => merge_callable_value_effects([
            callable_alias_from_expr(value),
            callable_alias_from_expr(default),
        ]),
        ExprKind::NullCoalesce { value, default } => merge_callable_value_effects([
            callable_alias_from_expr(value),
            callable_alias_from_expr(default),
        ]),
        ExprKind::Match { arms, default, .. } => merge_callable_value_effects(
            arms.iter()
                .map(|(_, value)| callable_alias_from_expr(value))
                .chain(default.iter().map(|value| callable_alias_from_expr(value))),
        ),
        ExprKind::NamedArg { value, .. } => callable_alias_from_expr(value),
        ExprKind::Variable(name) => ACTIVE_CALLABLE_ALIAS_EFFECTS.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|effects| effects.get(name).copied())
        }),
        _ => None,
    }
}

/// Updates the callable alias map after a variable assignment or declaration.
///
/// If the right-hand side expression produces a callable effect, the alias is inserted or updated
/// in the map; otherwise the name is removed to reflect that the variable no longer holds a callable.
pub(super) fn update_callable_alias(aliases: &mut HashMap<String, Effect>, name: &str, value: &Expr) {
    if let Some(effect) = callable_alias_from_expr(value) {
        aliases.insert(name.to_string(), effect);
    } else {
        aliases.remove(name);
    }
}

/// Simulates the effect of a `catch` clause on the callable alias map.
///
/// Removes the catch variable from the alias map and then simulates the catch body,
/// returning the updated map.
pub(super) fn simulate_catch_callable_aliases(
    catch: &crate::parser::ast::CatchClause,
    mut aliases: HashMap<String, Effect>,
) -> HashMap<String, Effect> {
    if let Some(name) = &catch.variable {
        aliases.remove(name);
    }
    simulate_block_callable_aliases(&catch.body, aliases)
}

/// Merges callable alias maps across all paths of a try/catch/finally construct.
///
/// For each branch that falls through, simulates the body with the incoming aliases, then
/// applies the finally body if present. All resulting alias maps are merged to produce
/// the post-try/catch alias state.
pub(super) fn merge_try_callable_alias_paths(
    try_body: &[Stmt],
    catches: &[crate::parser::ast::CatchClause],
    finally_body: Option<&[Stmt]>,
    incoming_aliases: &HashMap<String, Effect>,
) -> HashMap<String, Effect> {
    let mut fallthrough_paths = Vec::new();

    if matches!(block_terminal_effect(try_body), TerminalEffect::FallsThrough) {
        fallthrough_paths.push(simulate_block_callable_aliases(try_body, incoming_aliases.clone()));
    }

    for catch in catches {
        if matches!(block_terminal_effect(&catch.body), TerminalEffect::FallsThrough) {
            fallthrough_paths.push(simulate_catch_callable_aliases(catch, incoming_aliases.clone()));
        }
    }

    if let Some(finally_body) = finally_body {
        fallthrough_paths = fallthrough_paths
            .into_iter()
            .map(|aliases| simulate_block_callable_aliases(finally_body, aliases))
            .collect();
    }

    merge_callable_alias_paths(fallthrough_paths)
}

/// Outcome of simulating a switch body or case block for callable aliases.
pub(super) enum SwitchAliasPathOutcome {
    /// The block falls through, yielding the updated alias map.
    FallsThrough(HashMap<String, Effect>),
    /// The block executes a `break`, yielding the updated alias map.
    Breaks(HashMap<String, Effect>),
    /// The block exits the current control flow (e.g., return, throw, exit).
    ExitsCurrentBlock,
}

/// Simulates a switch body (case block or default block) for callable aliases.
///
/// Iterates statements, updating the alias map on each assignment. Returns the appropriate
/// `SwitchAliasPathOutcome` based on the terminal effect of the last statement.
pub(super) fn simulate_switch_body_callable_aliases(
    body: &[Stmt],
    mut aliases: HashMap<String, Effect>,
) -> SwitchAliasPathOutcome {
    for stmt in body {
        apply_stmt_callable_aliases(stmt, &mut aliases);
        match stmt_terminal_effect(stmt) {
            TerminalEffect::FallsThrough => {}
            TerminalEffect::Breaks => return SwitchAliasPathOutcome::Breaks(aliases),
            TerminalEffect::ExitsCurrentBlock | TerminalEffect::TerminatesMixed => {
                return SwitchAliasPathOutcome::ExitsCurrentBlock;
            }
        }
    }

    SwitchAliasPathOutcome::FallsThrough(aliases)
}

/// Simulates switch entry from a specific case index (or from the default entry point).
///
/// If `entry_case` is `Some(i)`, simulates cases starting at index `i` and continues through
/// the default if present. If `entry_case` is `None`, simulates only the default.
/// Returns the resulting alias map from the final simulated block, or `None` if the path exits.
pub(super) fn simulate_switch_entry_callable_aliases(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    entry_case: Option<usize>,
    incoming_aliases: &HashMap<String, Effect>,
) -> Option<HashMap<String, Effect>> {
    let mut aliases = incoming_aliases.clone();

    if let Some(start_index) = entry_case {
        for (_, body) in cases.iter().skip(start_index) {
            match simulate_switch_body_callable_aliases(body, aliases) {
                SwitchAliasPathOutcome::FallsThrough(updated) => aliases = updated,
                SwitchAliasPathOutcome::Breaks(updated) => return Some(updated),
                SwitchAliasPathOutcome::ExitsCurrentBlock => return None,
            }
        }
    }

    match default {
        Some(default_body) => match simulate_switch_body_callable_aliases(default_body, aliases) {
            SwitchAliasPathOutcome::FallsThrough(updated)
            | SwitchAliasPathOutcome::Breaks(updated) => Some(updated),
            SwitchAliasPathOutcome::ExitsCurrentBlock => None,
        },
        None => Some(aliases),
    }
}

/// Merges callable alias maps across all possible switch paths.
///
/// For each case and for the default entry point, simulates the switch body with incoming aliases
/// and collects all falling-through alias maps, then merges them into a single result map.
pub(super) fn merge_switch_callable_alias_paths(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    incoming_aliases: &HashMap<String, Effect>,
) -> HashMap<String, Effect> {
    let mut fallthrough_paths = Vec::new();

    for case_index in 0..cases.len() {
        if let Some(aliases) =
            simulate_switch_entry_callable_aliases(cases, default, Some(case_index), incoming_aliases)
        {
            fallthrough_paths.push(aliases);
        }
    }

    if let Some(aliases) = simulate_switch_entry_callable_aliases(cases, default, None, incoming_aliases)
    {
        fallthrough_paths.push(aliases);
    }

    merge_callable_alias_paths(fallthrough_paths)
}

/// Applies the callable alias effect of a single statement to the alias map.
///
/// Handles assignments, static variables, globals, array operations, conditionals, loops,
/// try/catch, and switch statements. For loops and includes, clears the map unconditionally.
pub(super) fn apply_stmt_callable_aliases(stmt: &Stmt, aliases: &mut HashMap<String, Effect>) {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            let effect = with_callable_alias_effects(aliases.clone(), || callable_alias_from_expr(value));
            if let Some(effect) = effect {
                aliases.insert(name.clone(), effect);
            } else {
                aliases.remove(name);
            }
        }
        StmtKind::StaticVar { name, init } => update_callable_alias(aliases, name, init),
        StmtKind::Global { vars } => {
            for var in vars {
                aliases.remove(var);
            }
        }
        StmtKind::ArrayAssign { array, .. } | StmtKind::ArrayPush { array, .. } => {
            aliases.remove(array);
        }
        StmtKind::ListUnpack { vars, .. } => {
            for var in vars {
                aliases.remove(var);
            }
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut fallthrough_paths = Vec::new();
            if matches!(block_terminal_effect(then_body), TerminalEffect::FallsThrough) {
                fallthrough_paths.push(simulate_block_callable_aliases(then_body, aliases.clone()));
            }
            for (_, body) in elseif_clauses {
                if matches!(block_terminal_effect(body), TerminalEffect::FallsThrough) {
                    fallthrough_paths.push(simulate_block_callable_aliases(body, aliases.clone()));
                }
            }
            if let Some(body) = else_body {
                if matches!(block_terminal_effect(body), TerminalEffect::FallsThrough) {
                    fallthrough_paths.push(simulate_block_callable_aliases(body, aliases.clone()));
                }
            } else {
                fallthrough_paths.push(aliases.clone());
            }
            *aliases = merge_callable_alias_paths(fallthrough_paths);
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            let mut fallthrough_paths = Vec::new();
            if matches!(block_terminal_effect(then_body), TerminalEffect::FallsThrough) {
                fallthrough_paths.push(simulate_block_callable_aliases(then_body, aliases.clone()));
            }
            match else_body {
                Some(body) if matches!(block_terminal_effect(body), TerminalEffect::FallsThrough) => {
                    fallthrough_paths.push(simulate_block_callable_aliases(body, aliases.clone()));
                }
                None => fallthrough_paths.push(aliases.clone()),
                _ => {}
            }
            *aliases = merge_callable_alias_paths(fallthrough_paths);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            *aliases = merge_try_callable_alias_paths(
                try_body,
                catches,
                finally_body.as_deref(),
                aliases,
            );
        }
        StmtKind::Switch { cases, default, .. } => {
            *aliases = merge_switch_callable_alias_paths(cases, default.as_deref(), aliases);
        }
        StmtKind::While { .. }
        | StmtKind::DoWhile { .. }
        | StmtKind::For { .. }
        | StmtKind::Foreach { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::IncludeOnceGuard { .. }
        | StmtKind::Include { .. } => aliases.clear(),
        _ => {}
    }
}

/// Simulates a statement block for callable alias effects.
///
/// Iterates statements in order, applying each one's alias effect and stopping early
/// if the statement does not fall through. Returns the final alias map for the block.
pub(super) fn simulate_block_callable_aliases(
    body: &[Stmt],
    mut aliases: HashMap<String, Effect>,
) -> HashMap<String, Effect> {
    for stmt in body {
        apply_stmt_callable_aliases(stmt, &mut aliases);
        if !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
            break;
        }
    }
    aliases
}

/// Merges multiple callable alias paths into a single map.
///
/// Returns the intersection of all alias names that are mapped to the same effect across
/// every path. If no paths are provided, returns an empty map.
pub(super) fn merge_callable_alias_paths(
    mut paths: Vec<HashMap<String, Effect>>,
) -> HashMap<String, Effect> {
    let Some(first) = paths.pop() else {
        return HashMap::new();
    };
    first
        .into_iter()
        .filter(|(name, effect)| {
            paths.iter()
                .all(|path| path.get(name).copied() == Some(*effect))
        })
        .collect()
}
