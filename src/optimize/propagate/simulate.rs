use super::*;

pub(crate) fn merge_constant_env_paths(mut paths: Vec<ConstantEnv>) -> ConstantEnv {
    let Some(first) = paths.pop() else {
        return HashMap::new();
    };

    first
        .into_iter()
        .filter(|(name, value)| paths.iter().all(|path| path.get(name) == Some(value)))
        .collect()
}

pub(crate) fn simulate_block_constant_env(body: &[Stmt], mut env: ConstantEnv) -> ConstantEnv {
    for stmt in body {
        env = propagate_stmt(stmt.clone(), env).1;
        if !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
            break;
        }
    }
    env
}

pub(crate) fn simulate_catch_constant_env(
    catch: &crate::parser::ast::CatchClause,
    mut env: ConstantEnv,
) -> ConstantEnv {
    if let Some(name) = &catch.variable {
        env.remove(name);
    }
    simulate_block_constant_env(&catch.body, env)
}

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

pub(crate) enum SwitchConstantPathOutcome {
    FallsThrough(ConstantEnv),
    Breaks(ConstantEnv),
    ExitsCurrentBlock,
}

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
