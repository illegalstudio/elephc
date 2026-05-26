//! Purpose:
//! Implements constant propagation simulate support.
//! Tracks scalar facts through expressions, writes, simulations, and statement rewriting.
//!
//! Called from:
//! - `crate::optimize::propagate`
//!
//! Key details:
//! - Only immutable scalar facts are propagated; arrays, objects, references, and unknown calls force conservative invalidation.

use super::*;

/// Intersects multiple constant environments, retaining only variable assignments
/// that are identical across every path. Returns an empty map if no paths are provided.
///
/// - `paths`: Vector of constant environments from different control-flow paths
/// - Returns: A merged environment where each variable must have the same value in all input paths
pub(crate) fn merge_constant_env_paths(mut paths: Vec<ConstantEnv>) -> ConstantEnv {
    let Some(first) = paths.pop() else {
        return HashMap::new();
    };

    first
        .into_iter()
        .filter(|(name, value)| paths.iter().all(|path| path.get(name) == Some(value)))
        .collect()
}

/// Simulates a straight-line statement sequence, updating the constant environment
/// after each statement. Stops early if a statement does not fall through (e.g., return, break).
///
/// - `body`: Statement block to simulate
/// - `env`: Initial constant environment
/// - Returns: The updated constant environment after processing the block
pub(crate) fn simulate_block_constant_env(body: &[Stmt], mut env: ConstantEnv) -> ConstantEnv {
    for stmt in body {
        env = propagate_stmt(stmt.clone(), env).1;
        if !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
            break;
        }
    }
    env
}

#[derive(Default)]
/// Summarizes the constant environments that flow out of a loop block via different
/// control-flow paths: fallthrough (loop body completes), break, continue, and whether
/// the block exits (return, throw, or non-break/continue transfer).
pub(crate) struct ConstantLoopPathSummary {
    pub(crate) fallthrough_paths: Vec<ConstantEnv>,
    pub(crate) break_paths: Vec<ConstantEnv>,
    pub(crate) continue_paths: Vec<ConstantEnv>,
    pub(crate) exits_current_block: bool,
}

impl ConstantLoopPathSummary {
    /// Merges another summary's path vectors into this one, combining all outcome paths.
    fn append(&mut self, mut other: ConstantLoopPathSummary) {
        self.fallthrough_paths.append(&mut other.fallthrough_paths);
        self.break_paths.append(&mut other.break_paths);
        self.continue_paths.append(&mut other.continue_paths);
        self.exits_current_block |= other.exits_current_block;
    }
}

/// Entry point for loop constant-path simulation. Takes a single incoming environment
/// and delegates to `simulate_loop_block_constant_paths_from`.
pub(crate) fn simulate_loop_block_constant_paths(
    body: &[Stmt],
    env: ConstantEnv,
) -> ConstantLoopPathSummary {
    simulate_loop_block_constant_paths_from(body, vec![env])
}

/// Internal variant of `simulate_loop_block_constant_paths` that accepts multiple
/// active environments, simulating the loop body across all paths until they converge
/// or empty out.
///
/// - `body`: Loop body statements
/// - `active_paths`: Vector of constant environments representing different path contexts
/// - Returns: A summary aggregating fallthrough, break, continue, and exit paths
fn simulate_loop_block_constant_paths_from(
    body: &[Stmt],
    mut active_paths: Vec<ConstantEnv>,
) -> ConstantLoopPathSummary {
    let mut summary = ConstantLoopPathSummary::default();

    for stmt in body {
        let mut next_active_paths = Vec::new();
        for env in active_paths {
            let mut stmt_summary = simulate_loop_stmt_constant_paths(stmt, env);
            next_active_paths.append(&mut stmt_summary.fallthrough_paths);
            summary.break_paths.append(&mut stmt_summary.break_paths);
            summary.continue_paths.append(&mut stmt_summary.continue_paths);
            summary.exits_current_block |= stmt_summary.exits_current_block;
        }
        active_paths = next_active_paths;
        if active_paths.is_empty() {
            break;
        }
    }

    summary.fallthrough_paths.extend(active_paths);
    summary
}

/// Simulates a single loop-body statement, classifying its outcome as fallthrough,
/// break, continue, or block-exit based on statement kind and terminal effect.
fn simulate_loop_stmt_constant_paths(stmt: &Stmt, env: ConstantEnv) -> ConstantLoopPathSummary {
    match &stmt.kind {
        StmtKind::Break(1) => ConstantLoopPathSummary {
            break_paths: vec![env],
            ..ConstantLoopPathSummary::default()
        },
        StmtKind::Break(_) => ConstantLoopPathSummary {
            exits_current_block: true,
            ..ConstantLoopPathSummary::default()
        },
        StmtKind::Continue(1) => ConstantLoopPathSummary {
            continue_paths: vec![env],
            ..ConstantLoopPathSummary::default()
        },
        StmtKind::Continue(_) => ConstantLoopPathSummary {
            exits_current_block: true,
            ..ConstantLoopPathSummary::default()
        },
        StmtKind::Return(_) | StmtKind::Throw(_) => ConstantLoopPathSummary {
            exits_current_block: true,
            ..ConstantLoopPathSummary::default()
        },
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => simulate_loop_if_constant_paths(
            condition,
            then_body,
            elseif_clauses,
            else_body.as_deref(),
            env,
        ),
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            let mut summary = simulate_loop_block_constant_paths(then_body, env.clone());
            match else_body {
                Some(else_body) => {
                    summary.append(simulate_loop_block_constant_paths(else_body, env));
                }
                None => summary.fallthrough_paths.push(env),
            }
            summary
        }
        _ => {
            let (stmt, next_env) = propagate_stmt(stmt.clone(), env);
            match stmt_terminal_effect(&stmt) {
                TerminalEffect::FallsThrough => ConstantLoopPathSummary {
                    fallthrough_paths: vec![next_env],
                    ..ConstantLoopPathSummary::default()
                },
                TerminalEffect::Breaks => ConstantLoopPathSummary {
                    break_paths: vec![next_env],
                    ..ConstantLoopPathSummary::default()
                },
                TerminalEffect::ExitsCurrentBlock | TerminalEffect::TerminatesMixed => {
                    ConstantLoopPathSummary {
                        exits_current_block: true,
                        ..ConstantLoopPathSummary::default()
                    }
                }
            }
        }
    }
}

/// Simulates an if-statement within a loop context, evaluating the condition against
/// the incoming environment and routing to the appropriate branch path(s).
///
/// - `condition`: The if condition expression
/// - `then_body`: Statements in the then branch
/// - `elseif_clauses`: Optional elseif chain
/// - `else_body`: Optional else branch
/// - `env`: Incoming constant environment
/// - Returns: Summary of all paths flowing out of the if statement
fn simulate_loop_if_constant_paths(
    condition: &Expr,
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    env: ConstantEnv,
) -> ConstantLoopPathSummary {
    let condition = propagate_expr(condition.clone(), &env);
    let base_env = if expr_effect(&condition).has_side_effects {
        HashMap::new()
    } else {
        env
    };

    match scalar_value(&condition) {
        Some(value) if value.truthy() => simulate_loop_block_constant_paths(then_body, base_env),
        Some(_) => simulate_loop_elseif_constant_paths(elseif_clauses, else_body, base_env),
        None => {
            let mut summary = simulate_loop_block_constant_paths(then_body, base_env.clone());
            summary.append(simulate_loop_elseif_constant_paths(
                elseif_clauses,
                else_body,
                base_env,
            ));
            summary
        }
    }
}

/// Recursively simulates an elseif chain within a loop, processing each condition
/// and body pair until a matching branch is found or the else body is reached.
///
/// - `elseif_clauses`: Remaining elseif conditions and bodies
/// - `else_body`: Optional else branch
/// - `base_env`: Base constant environment for branch evaluation
/// - Returns: Summary of all paths flowing out of the elseif chain
fn simulate_loop_elseif_constant_paths(
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    base_env: ConstantEnv,
) -> ConstantLoopPathSummary {
    if let Some((condition, body)) = elseif_clauses.first() {
        let condition = propagate_expr(condition.clone(), &base_env);
        let branch_env = if expr_effect(&condition).has_side_effects {
            HashMap::new()
        } else {
            base_env.clone()
        };

        match scalar_value(&condition) {
            Some(value) if value.truthy() => simulate_loop_block_constant_paths(body, branch_env),
            Some(_) => {
                simulate_loop_elseif_constant_paths(&elseif_clauses[1..], else_body, base_env)
            }
            None => {
                let mut summary = simulate_loop_block_constant_paths(body, branch_env);
                summary.append(simulate_loop_elseif_constant_paths(
                    &elseif_clauses[1..],
                    else_body,
                    base_env,
                ));
                summary
            }
        }
    } else {
        match else_body {
            Some(else_body) => simulate_loop_block_constant_paths(else_body, base_env),
            None => ConstantLoopPathSummary {
                fallthrough_paths: vec![base_env],
                ..ConstantLoopPathSummary::default()
            },
        }
    }
}

/// Simulates a catch clause by removing the caught exception variable from the
/// environment (it is undefined in the block scope) and then simulating the catch body.
///
/// - `catch`: The catch clause containing variable name and body statements
/// - `env`: Incoming constant environment
/// - Returns: Updated environment after processing the catch body
pub(crate) fn simulate_catch_constant_env(
    catch: &crate::parser::ast::CatchClause,
    mut env: ConstantEnv,
) -> ConstantEnv {
    if let Some(name) = &catch.variable {
        env.remove(name);
    }
    simulate_block_constant_env(&catch.body, env)
}

/// Merges the constant environments from a try-catch-finally structure by
/// simulating each block and intersecting environments from all fallthrough paths.
///
/// - `try_body`: Statements in the try block
/// - `catches`: Array of catch clauses
/// - `finally_body`: Optional finally block
/// - `incoming_env`: Initial constant environment before the try block
/// - Returns: Merged constant environment after processing all reachable paths
pub(crate) fn merge_try_constant_env_paths(
    try_body: &[Stmt],
    catches: &[crate::parser::ast::CatchClause],
    finally_body: Option<&[Stmt]>,
    incoming_env: &ConstantEnv,
) -> ConstantEnv {
    let mut fallthrough_paths = Vec::new();

    if matches!(block_terminal_effect(try_body), TerminalEffect::FallsThrough) {
        fallthrough_paths.push(simulate_block_constant_env(try_body, incoming_env.clone()));
    }

    if block_may_throw(try_body) {
        for catch in catches {
            if matches!(block_terminal_effect(&catch.body), TerminalEffect::FallsThrough) {
                fallthrough_paths.push(simulate_catch_constant_env(catch, incoming_env.clone()));
            }
        }
    }

    match finally_body {
        Some(finally_body) if matches!(block_terminal_effect(finally_body), TerminalEffect::FallsThrough) => {
            merge_constant_env_paths(
                fallthrough_paths
                    .into_iter()
                    .map(|env| simulate_block_constant_env(finally_body, env))
                    .collect(),
            )
        }
        Some(_) => HashMap::new(),
        None => merge_constant_env_paths(fallthrough_paths),
    }
}

/// Represents the possible constant-environment outcomes of simulating a switch body:
/// fallthrough (no break encountered), break (break statement reached), or
/// exits-current-block (return, throw, or other terminating transfer).
pub(crate) enum SwitchConstantPathOutcome {
    FallsThrough(ConstantEnv),
    Breaks(ConstantEnv),
    ExitsCurrentBlock,
}

/// Simulates the statements within a switch body, updating the constant environment
/// until a break is encountered or the block exits.
///
/// - `body`: Switch case body statements
/// - `env`: Initial constant environment at switch entry
/// - Returns: One of three outcomes depending on terminal effect of the body
pub(crate) fn simulate_switch_body_constant_env(
    body: &[Stmt],
    mut env: ConstantEnv,
) -> SwitchConstantPathOutcome {
    for stmt in body {
        env = propagate_stmt(stmt.clone(), env).1;
        match stmt_terminal_effect(stmt) {
            TerminalEffect::FallsThrough => {}
            TerminalEffect::Breaks => return SwitchConstantPathOutcome::Breaks(env),
            TerminalEffect::ExitsCurrentBlock | TerminalEffect::TerminatesMixed => {
                return SwitchConstantPathOutcome::ExitsCurrentBlock;
            }
        }
    }

    SwitchConstantPathOutcome::FallsThrough(env)
}

/// Simulates switch entry starting from a specific case index or from the default,
/// then computes the resulting constant environment that flows out of the switch.
///
/// - `cases`: All switch cases with their expressions and bodies
/// - `default`: Optional default case body
/// - `entry_case`: Starting case index (None means start from default after all cases)
/// - `incoming_env`: Constant environment before the switch
/// - Returns: Some(updated env) if execution can reach a terminal point, None if block exits
pub(crate) fn simulate_switch_entry_constant_env(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    entry_case: Option<usize>,
    incoming_env: &ConstantEnv,
) -> Option<ConstantEnv> {
    let mut env = incoming_env.clone();

    if let Some(start_index) = entry_case {
        for (_, body) in cases.iter().skip(start_index) {
            match simulate_switch_body_constant_env(body, env) {
                SwitchConstantPathOutcome::FallsThrough(updated) => env = updated,
                SwitchConstantPathOutcome::Breaks(updated) => return Some(updated),
                SwitchConstantPathOutcome::ExitsCurrentBlock => return None,
            }
        }
    }

    match default {
        Some(default_body) => match simulate_switch_body_constant_env(default_body, env) {
            SwitchConstantPathOutcome::FallsThrough(updated)
            | SwitchConstantPathOutcome::Breaks(updated) => Some(updated),
            SwitchConstantPathOutcome::ExitsCurrentBlock => None,
        },
        None => Some(env),
    }
}

/// Merges constant environments across all paths through a switch statement by
/// simulating entry from each case index and from the default, then intersecting
/// the resulting environments.
///
/// - `subject`: The switch subject expression
/// - `cases`: All switch cases
/// - `default`: Optional default case body
/// - `incoming_env`: Constant environment before the switch
/// - Returns: Merged constant environment representing all reachable paths
pub(crate) fn merge_switch_constant_env_paths(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    incoming_env: &ConstantEnv,
) -> ConstantEnv {
    if let Some(subject_value) = scalar_value(subject) {
        return merge_known_switch_constant_env_paths(&subject_value, cases, default, incoming_env);
    }

    let mut fallthrough_paths = Vec::new();

    for case_index in 0..cases.len() {
        if let Some(env) =
            simulate_switch_entry_constant_env(cases, default, Some(case_index), incoming_env)
        {
            fallthrough_paths.push(env);
        }
    }

    if let Some(env) = simulate_switch_entry_constant_env(cases, default, None, incoming_env) {
        fallthrough_paths.push(env);
    }

    merge_constant_env_paths(fallthrough_paths)
}

/// Internal helper for when the switch subject has a known constant value.
/// Classifies each case pattern against the subject value and simulates only the
/// matching path(s), short-circuiting if a definite match is found.
///
/// - `subject`: The known constant scalar value being switched on
/// - `cases`: All switch cases with patterns and bodies
/// - `default`: Optional default case body
/// - `incoming_env`: Constant environment before the switch
/// - Returns: Merged constant environment from the matching case(s) and default
fn merge_known_switch_constant_env_paths(
    subject: &ScalarValue,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    incoming_env: &ConstantEnv,
) -> ConstantEnv {
    let mut fallthrough_paths = Vec::new();
    let mut has_unknown_pattern = false;

    for (case_index, (patterns, _)) in cases.iter().enumerate() {
        match classify_case_patterns(subject, patterns, CaseComparison::LooseSwitch) {
            CaseMatch::Matches => {
                if let Some(env) =
                    simulate_switch_entry_constant_env(cases, default, Some(case_index), incoming_env)
                {
                    fallthrough_paths.push(env);
                }
                return merge_constant_env_paths(fallthrough_paths);
            }
            CaseMatch::Unknown => {
                has_unknown_pattern = true;
                if let Some(env) =
                    simulate_switch_entry_constant_env(cases, default, Some(case_index), incoming_env)
                {
                    fallthrough_paths.push(env);
                }
            }
            CaseMatch::NoMatch => {}
        }
    }

    if has_unknown_pattern || default.is_some() {
        if let Some(env) = simulate_switch_entry_constant_env(cases, default, None, incoming_env) {
            fallthrough_paths.push(env);
        }
        return merge_constant_env_paths(fallthrough_paths);
    }

    incoming_env.clone()
}
