//! Purpose:
//! Summarizes whether a user callable can return storage borrowed from one of
//! its visible parameters for ownership-safe call-argument cleanup.
//!
//! Called from:
//! - `crate::types::checker::check_types()` after declaration checking.
//! - `crate::ir_lower::expr` when releasing owning call argument temporaries.
//!
//! Key details:
//! - `Unknown` is deliberately conservative; only proven non-aliasing paths
//!   allow cleanup that the previous type-only guard suppressed.
//! - Local provenance is merged across branches and to a fixed point in loops.

use std::collections::{BTreeSet, HashMap};

use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Expr, ExprKind, Program, Stmt, StmtKind};

/// Describes which visible parameters a callable result may reuse as storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReturnArgAlias {
    /// Every return path is proven independent of visible argument storage.
    None,
    /// A return path can transfer storage from one of these parameter indexes.
    Parameters(BTreeSet<usize>),
    /// The body contains a provenance path this lightweight analysis cannot prove.
    Unknown,
}

impl ReturnArgAlias {
    /// Creates a summary that aliases one zero-based visible parameter.
    fn parameter(index: usize) -> Self {
        Self::Parameters(BTreeSet::from([index]))
    }

    /// Conservatively combines two possible provenance paths.
    pub(crate) fn merge(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::None, rhs) => rhs.clone(),
            (lhs, Self::None) => lhs.clone(),
            (Self::Parameters(lhs), Self::Parameters(rhs)) => {
                Self::Parameters(lhs.union(rhs).copied().collect())
            }
        }
    }

    /// Returns whether this summary permits the result to reuse `parameter_index`.
    pub(crate) fn may_alias_parameter(&self, parameter_index: usize) -> bool {
        match self {
            Self::None => false,
            Self::Parameters(parameters) => parameters.contains(&parameter_index),
            Self::Unknown => true,
        }
    }
}

/// Return/argument alias summaries for source-declared functions and methods.
#[derive(Debug, Clone, Default)]
pub(crate) struct ReturnAliasSummaries {
    functions: HashMap<String, ReturnArgAlias>,
    methods: HashMap<(String, String), ReturnArgAlias>,
}

impl ReturnAliasSummaries {
    /// Looks up a source-declared function summary by its canonical name.
    pub(crate) fn function(&self, name: &str) -> Option<&ReturnArgAlias> {
        self.functions.get(name)
    }

    /// Looks up a method implementation summary by class and case-folded method name.
    pub(crate) fn method(&self, class_name: &str, method: &str) -> Option<&ReturnArgAlias> {
        self.methods.get(&(
            class_name.trim_start_matches('\\').to_string(),
            php_symbol_key(method),
        ))
    }
}

/// Collects return/argument alias summaries from all source declarations.
pub(crate) fn collect_return_alias_summaries(program: &Program) -> ReturnAliasSummaries {
    let mut summaries = ReturnAliasSummaries::default();
    collect_declaration_summaries(program, &mut summaries);
    summaries
}

/// Recursively records summaries for declarations nested in namespace blocks.
fn collect_declaration_summaries(statements: &[Stmt], summaries: &mut ReturnAliasSummaries) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                by_ref_return,
                body,
                ..
            } => {
                summaries.functions.insert(
                    name.clone(),
                    summarize_callable(
                        params.iter().map(|(name, _, _, _)| name.as_str()),
                        variadic.as_deref(),
                        *by_ref_return,
                        body,
                    ),
                );
            }
            StmtKind::ClassDecl { name, methods, .. }
            | StmtKind::EnumDecl { name, methods, .. }
            | StmtKind::TraitDecl { name, methods, .. }
            | StmtKind::InterfaceDecl { name, methods, .. } => {
                collect_method_summaries(name, methods, summaries);
            }
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                collect_declaration_summaries(body, summaries);
            }
            _ => {}
        }
    }
}

/// Records summaries for one class-like declaration's methods.
fn collect_method_summaries(
    class_name: &str,
    methods: &[ClassMethod],
    summaries: &mut ReturnAliasSummaries,
) {
    for method in methods {
        let summary = if method.has_body {
            summarize_callable(
                method.params.iter().map(|(name, _, _, _)| name.as_str()),
                method.variadic.as_deref(),
                method.by_ref_return,
                &method.body,
            )
        } else {
            ReturnArgAlias::Unknown
        };
        summaries.methods.insert(
            (
                class_name.trim_start_matches('\\').to_string(),
                php_symbol_key(&method.name),
            ),
            summary,
        );
    }
}

/// Summarizes one function-like body from its parameter names and statements.
fn summarize_callable<'a>(
    params: impl Iterator<Item = &'a str>,
    variadic: Option<&str>,
    by_ref_return: bool,
    body: &[Stmt],
) -> ReturnArgAlias {
    if by_ref_return {
        return ReturnArgAlias::Unknown;
    }
    let mut state = HashMap::new();
    for (index, name) in params.enumerate() {
        state.insert(name.to_string(), ReturnArgAlias::parameter(index));
    }
    if let Some(name) = variadic {
        let index = state.len();
        state.insert(name.to_string(), ReturnArgAlias::parameter(index));
    }
    let mut returned = ReturnArgAlias::None;
    analyze_body(body, &mut state, &mut returned);
    returned
}

/// Applies statement provenance effects and accumulates every reachable return path.
fn analyze_body(
    body: &[Stmt],
    state: &mut HashMap<String, ReturnArgAlias>,
    returned: &mut ReturnArgAlias,
) {
    for stmt in body {
        analyze_stmt(stmt, state, returned);
    }
}

/// Applies one statement to the local alias-provenance state.
fn analyze_stmt(
    stmt: &Stmt,
    state: &mut HashMap<String, ReturnArgAlias>,
    returned: &mut ReturnArgAlias,
) {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            let alias = expr_alias(value, state);
            apply_expr_effects(value, state);
            state.insert(name.clone(), alias);
        }
        StmtKind::RefAssign { target, source } => {
            apply_expr_effects(source, state);
            state.insert(target.clone(), ReturnArgAlias::Unknown);
            // The new ref cell can connect either name to storage whose later
            // writes are not represented by ordinary assignment statements.
            invalidate_all_aliases(state);
        }
        StmtKind::Return(Some(value)) => {
            *returned = returned.merge(&expr_alias(value, state));
        }
        StmtKind::Return(None) => {}
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            apply_expr_effects(condition, state);
            for (condition, _) in elseif_clauses {
                apply_expr_effects(condition, state);
            }
            let incoming = state.clone();
            let mut paths = Vec::with_capacity(elseif_clauses.len() + 2);
            paths.push(analyzed_path(then_body, &incoming, returned));
            for (_, body) in elseif_clauses {
                paths.push(analyzed_path(body, &incoming, returned));
            }
            paths.push(match else_body {
                Some(body) => analyzed_path(body, &incoming, returned),
                None => incoming,
            });
            *state = merge_states(paths);
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            let incoming = state.clone();
            let then_state = analyzed_path(then_body, &incoming, returned);
            let else_state = else_body
                .as_ref()
                .map(|body| analyzed_path(body, &incoming, returned))
                .unwrap_or(incoming);
            *state = merge_states(vec![then_state, else_state]);
        }
        StmtKind::While { condition, body } => {
            apply_expr_effects(condition, state);
            analyze_loop(body, &[], Some(condition), state, returned);
        }
        StmtKind::DoWhile { body, condition } => {
            analyze_loop(body, &[], Some(condition), state, returned);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                analyze_stmt(init, state, returned);
            }
            if let Some(condition) = condition {
                apply_expr_effects(condition, state);
            }
            let updates = update.iter().map(|stmt| stmt.as_ref()).collect::<Vec<_>>();
            analyze_loop(
                body,
                &updates,
                condition.as_ref(),
                state,
                returned,
            );
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
            ..
        } => {
            apply_expr_effects(array, state);
            let mut iteration = state.clone();
            if let Some(key) = key_var {
                iteration.insert(key.clone(), ReturnArgAlias::None);
            }
            // A by-value element can still borrow nested refcounted storage from
            // the iterated parameter, so only the container itself is known fresh.
            iteration.insert(value_var.clone(), ReturnArgAlias::Unknown);
            analyze_body(body, &mut iteration, returned);
            *state = merge_states(vec![state.clone(), iteration]);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            apply_expr_effects(subject, state);
            for (patterns, _) in cases {
                for pattern in patterns {
                    apply_expr_effects(pattern, state);
                }
            }
            let incoming = state.clone();
            let mut case_incoming = incoming.clone();
            invalidate_all_aliases(&mut case_incoming);
            let mut paths = cases
                .iter()
                .map(|(_, body)| analyzed_path(body, &case_incoming, returned))
                .collect::<Vec<_>>();
            paths.push(match default {
                Some(body) => analyzed_path(body, &case_incoming, returned),
                None => incoming,
            });
            *state = merge_states(paths);
            // Case fallthrough can compose assignments from multiple case bodies.
            // This lightweight analysis deliberately does not reproduce the full
            // switch CFG, so do not claim a fresh provenance after the join.
            invalidate_all_aliases(state);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let incoming = state.clone();
            let mut paths = vec![analyzed_path(try_body, &incoming, returned)];
            for catch in catches {
                let mut catch_state = incoming.clone();
                // A catch can run after any prefix of the try body, including
                // writes that happened immediately before the throw.
                invalidate_all_aliases(&mut catch_state);
                if let Some(variable) = &catch.variable {
                    catch_state.insert(variable.clone(), ReturnArgAlias::Unknown);
                }
                analyze_body(&catch.body, &mut catch_state, returned);
                paths.push(catch_state);
            }
            *state = merge_states(paths);
            if let Some(body) = finally_body {
                invalidate_all_aliases(state);
                analyze_body(body, state, returned);
            }
            // A catch may observe state written part-way through the try body.
            // The branch approximation above is enough to find return statements,
            // but not precise enough to prove post-try storage independence.
            invalidate_all_aliases(state);
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            apply_expr_effects(index, state);
            apply_expr_effects(value, state);
            state.entry(array.clone()).or_insert(ReturnArgAlias::Unknown);
        }
        StmtKind::ArrayPush { array, value } => {
            apply_expr_effects(value, state);
            state.entry(array.clone()).or_insert(ReturnArgAlias::Unknown);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            apply_expr_effects(target, state);
            apply_expr_effects(value, state);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            apply_expr_effects(object, state);
            apply_expr_effects(value, state);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            apply_expr_effects(object, state);
            apply_expr_effects(index, state);
            apply_expr_effects(value, state);
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => {
            apply_expr_effects(value, state);
        }
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            apply_expr_effects(index, state);
            apply_expr_effects(value, state);
        }
        StmtKind::ListUnpack { vars, value } => {
            apply_expr_effects(value, state);
            for name in vars {
                state.insert(name.clone(), ReturnArgAlias::Unknown);
            }
        }
        StmtKind::Global { vars } => {
            for name in vars {
                state.insert(name.clone(), ReturnArgAlias::Unknown);
            }
        }
        StmtKind::StaticVar { name, init } => {
            apply_expr_effects(init, state);
            state.insert(name.clone(), ReturnArgAlias::Unknown);
        }
        StmtKind::ExprStmt(expr) | StmtKind::Echo(expr) | StmtKind::Throw(expr) => {
            apply_expr_effects(expr, state);
        }
        StmtKind::IncludeOnceGuard { body, .. } => {
            let incoming = state.clone();
            let guarded = analyzed_path(body, &incoming, returned);
            *state = merge_states(vec![incoming, guarded]);
        }
        StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
            analyze_body(body, state, returned);
        }
        StmtKind::Include { path, .. } => {
            apply_expr_effects(path, state);
            invalidate_all_aliases(state);
        }
        StmtKind::ConstDecl { value, .. } => apply_expr_effects(value, state),
        _ => {}
    }
}

/// Runs one branch body from a cloned incoming state.
fn analyzed_path(
    body: &[Stmt],
    incoming: &HashMap<String, ReturnArgAlias>,
    returned: &mut ReturnArgAlias,
) -> HashMap<String, ReturnArgAlias> {
    let mut state = incoming.clone();
    analyze_body(body, &mut state, returned);
    state
}

/// Computes the monotone fixed point for a loop body and its update statement.
fn analyze_loop(
    body: &[Stmt],
    updates: &[&Stmt],
    condition: Option<&Expr>,
    state: &mut HashMap<String, ReturnArgAlias>,
    returned: &mut ReturnArgAlias,
) {
    let entry = state.clone();
    let mut current = entry.clone();
    loop {
        let mut next = current.clone();
        analyze_body(body, &mut next, returned);
        for update in updates {
            analyze_stmt(update, &mut next, returned);
        }
        if let Some(condition) = condition {
            apply_expr_effects(condition, &mut next);
        }
        next = merge_states(vec![entry.clone(), next]);
        if next == current {
            *state = next;
            return;
        }
        current = next;
    }
}

/// Merges local provenance across mutually exclusive control-flow paths.
fn merge_states(
    states: Vec<HashMap<String, ReturnArgAlias>>,
) -> HashMap<String, ReturnArgAlias> {
    let mut keys = BTreeSet::new();
    for state in &states {
        keys.extend(state.keys().cloned());
    }
    keys.into_iter()
        .map(|key| {
            let mut aliases = states.iter().filter_map(|state| state.get(&key));
            let first = aliases.next().cloned().unwrap_or(ReturnArgAlias::Unknown);
            let merged = aliases.fold(first, |current, alias| current.merge(alias));
            let missing_on_path = states.iter().any(|state| !state.contains_key(&key));
            (
                key,
                if missing_on_path {
                    ReturnArgAlias::Unknown
                } else {
                    merged
                },
            )
        })
        .collect()
}

/// Computes the argument provenance of one expression's resulting storage.
fn expr_alias(expr: &Expr, state: &HashMap<String, ReturnArgAlias>) -> ReturnArgAlias {
    match &expr.kind {
        ExprKind::Variable(name) => state
            .get(name)
            .cloned()
            .unwrap_or(ReturnArgAlias::Unknown),
        ExprKind::ErrorSuppress(inner)
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::Cast { expr: inner, .. } => expr_alias(inner, state),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_alias(value, state).merge(&expr_alias(default, state))
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => expr_alias(then_expr, state).merge(&expr_alias(else_expr, state)),
        ExprKind::Match { arms, default, .. } => {
            let mut alias = default
                .as_ref()
                .map(|expr| expr_alias(expr, state))
                .unwrap_or(ReturnArgAlias::Unknown);
            for (_, value) in arms {
                alias = alias.merge(&expr_alias(value, state));
            }
            alias
        }
        ExprKind::Assignment { value, .. } => expr_alias(value, state),
        ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::Closure { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::NewObject { .. }
        | ExprKind::NewDynamic { .. }
        | ExprKind::NewDynamicObject { .. }
        | ExprKind::NewScopedObject { .. }
        | ExprKind::Clone(_)
        | ExprKind::BufferNew { .. } => ReturnArgAlias::None,
        ExprKind::FunctionCall { name, .. }
            if builtin_result_is_proven_independent(name.as_str()) =>
        {
            ReturnArgAlias::None
        }
        ExprKind::FunctionCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::NullsafeDynamicMethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::Pipe { .. }
        | ExprKind::YieldFrom(_) => ReturnArgAlias::Unknown,
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::InstanceOf { .. }
        | ExprKind::Negate(_)
        | ExprKind::Not(_)
        | ExprKind::BitNot(_)
        | ExprKind::PtrCast { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::MagicConstant(_) => ReturnArgAlias::None,
        _ => ReturnArgAlias::Unknown,
    }
}

/// Returns whether a builtin's result storage cannot alias any caller argument.
fn builtin_result_is_proven_independent(name: &str) -> bool {
    crate::builtins::registry::returns_independent_storage(name.trim_start_matches('\\'))
}

/// Conservatively invalidates locals that an expression can rewrite by reference.
fn apply_expr_effects(expr: &Expr, state: &mut HashMap<String, ReturnArgAlias>) {
    match &expr.kind {
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            let mut ignored_return = ReturnArgAlias::None;
            analyze_body(prelude, state, &mut ignored_return);
            apply_expr_effects(target, state);
            apply_expr_effects(value, state);
            if let Some(result_target) = result_target {
                apply_expr_effects(result_target, state);
            }
            // Expression-position assignments can appear inside conditions and
            // carry compound/ref-cell semantics. Keep statement assignments
            // precise, but do not guess at these less regular storage paths.
            invalidate_all_aliases(state);
        }
        ExprKind::FunctionCall { name, args } => {
            for arg in args {
                apply_expr_effects(arg, state);
            }
            if named_call_can_rebind_unlisted_locals(name.as_str()) {
                invalidate_all_aliases(state);
            } else {
                invalidate_call_variables(args, state);
            }
        }
        ExprKind::ClosureCall { args, .. } => {
            visit_expr_effects(args, state);
            invalidate_all_aliases(state);
        }
        ExprKind::ExprCall { callee, args } => {
            apply_expr_effects(callee, state);
            visit_expr_effects(args, state);
            invalidate_all_aliases(state);
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            apply_expr_effects(object, state);
            visit_expr_effects(args, state);
            invalidate_all_aliases(state);
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            apply_expr_effects(object, state);
            apply_expr_effects(method, state);
            visit_expr_effects(args, state);
            invalidate_all_aliases(state);
        }
        ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            visit_expr_effects(args, state);
            invalidate_all_aliases(state);
        }
        ExprKind::NewDynamic { name_expr, args } => {
            apply_expr_effects(name_expr, state);
            visit_expr_effects(args, state);
            invalidate_all_aliases(state);
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            apply_expr_effects(class_name, state);
            visit_expr_effects(args, state);
            invalidate_all_aliases(state);
        }
        ExprKind::Pipe { value, callable } => {
            apply_expr_effects(value, state);
            apply_expr_effects(callable, state);
            invalidate_all_aliases(state);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            apply_expr_effects(left, state);
            apply_expr_effects(right, state);
        }
        ExprKind::InstanceOf { value, target } => {
            apply_expr_effects(value, state);
            if let crate::parser::ast::InstanceOfTarget::Expr(target) = target {
                apply_expr_effects(target, state);
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::YieldFrom(inner) => apply_expr_effects(inner, state),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            apply_expr_effects(value, state);
            apply_expr_effects(default, state);
        }
        ExprKind::ArrayLiteral(items) => visit_expr_effects(items, state),
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                apply_expr_effects(key, state);
                apply_expr_effects(value, state);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            apply_expr_effects(subject, state);
            for (patterns, value) in arms {
                visit_expr_effects(patterns, state);
                apply_expr_effects(value, state);
            }
            if let Some(default) = default {
                apply_expr_effects(default, state);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            apply_expr_effects(array, state);
            apply_expr_effects(index, state);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            apply_expr_effects(condition, state);
            apply_expr_effects(then_expr, state);
            apply_expr_effects(else_expr, state);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            apply_expr_effects(object, state);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            apply_expr_effects(object, state);
            apply_expr_effects(property, state);
        }
        ExprKind::NamedArg { value, .. } => apply_expr_effects(value, state),
        ExprKind::BufferNew { len, .. } => apply_expr_effects(len, state),
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                apply_expr_effects(key, state);
            }
            if let Some(value) = value {
                apply_expr_effects(value, state);
            }
        }
        ExprKind::Closure { capture_refs, .. } => {
            for name in capture_refs {
                state.insert(name.clone(), ReturnArgAlias::Unknown);
            }
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::Variable(_)
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::This
        | ExprKind::FirstClassCallable(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::MagicConstant(_)
        | ExprKind::IncludeValue { .. } => {}
    }
}

/// Visits a slice of expressions for nested call or assignment effects.
fn visit_expr_effects(exprs: &[Expr], state: &mut HashMap<String, ReturnArgAlias>) {
    for expr in exprs {
        apply_expr_effects(expr, state);
    }
}

/// Returns whether a named call can reach arbitrary caller locals, rather than
/// only variables explicitly passed to a by-reference parameter.
fn named_call_can_rebind_unlisted_locals(name: &str) -> bool {
    let name = php_symbol_key(name.trim_start_matches('\\'));
    if matches!(name.as_str(), "eval" | "extract") {
        return true;
    }
    if matches!(name.as_str(), "isset" | "empty" | "unset" | "exit" | "die") {
        return false;
    }
    crate::builtins::registry::lookup(&name).is_none_or(|definition| {
        definition
            .params
            .iter()
            .any(|(parameter, _)| parameter == "callback")
    })
}

/// Replaces every tracked provenance with the conservative top element.
fn invalidate_all_aliases(state: &mut HashMap<String, ReturnArgAlias>) {
    for alias in state.values_mut() {
        *alias = ReturnArgAlias::Unknown;
    }
}

/// Marks direct variable call arguments unknown because the callee may accept them by reference.
fn invalidate_call_variables(args: &[Expr], state: &mut HashMap<String, ReturnArgAlias>) {
    for arg in args {
        let value = match &arg.kind {
            ExprKind::NamedArg { value, .. } | ExprKind::Spread(value) => value.as_ref(),
            _ => arg,
        };
        if let ExprKind::Variable(name) = &value.kind {
            state.insert(name.clone(), ReturnArgAlias::Unknown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses one PHP fixture for return-alias summary unit tests.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenization failed");
        crate::parser::parse(&tokens).expect("parse failed")
    }

    /// Verifies a locally-created array remains independent through loop appends.
    #[test]
    fn fresh_local_array_does_not_alias_method_parameters() {
        let program = parse(
            "<?php class S { function scan(array $input): array { $out = []; foreach ($input as $v) { $out[] = $v; } return $out; } }",
        );
        let summaries = collect_return_alias_summaries(&program);
        assert_eq!(summaries.method("S", "scan"), Some(&ReturnArgAlias::None));
    }

    /// Verifies `implode()` returns storage independent of a method's string parameters.
    #[test]
    fn implode_return_is_independent_of_callable_parameters() {
        let program = parse(
            "<?php class H { function join(string $left, string $right): string { $parts = [$left, $right]; return implode('', $parts); } }",
        );
        let summaries = collect_return_alias_summaries(&program);
        assert_eq!(summaries.method("H", "join"), Some(&ReturnArgAlias::None));
    }

    /// Verifies a scratch-backed HTML escape result cannot alias its string parameter.
    #[test]
    fn htmlspecialchars_return_is_independent_of_callable_parameters() {
        let program = parse(
            "<?php class E { function escape(string $value): string { return htmlspecialchars($value); } }",
        );
        let summaries = collect_return_alias_summaries(&program);
        assert_eq!(summaries.method("E", "escape"), Some(&ReturnArgAlias::None));
    }

    /// Verifies direct parameter passthrough records only the returned parameter.
    #[test]
    fn passthrough_records_the_aliased_parameter() {
        let program = parse("<?php function choose(array $a, array $b): array { return $b; }");
        let summaries = collect_return_alias_summaries(&program);
        assert_eq!(
            summaries.function("choose"),
            Some(&ReturnArgAlias::Parameters(BTreeSet::from([1])))
        );
    }

    /// Verifies property and nested-element reads never claim fresh storage,
    /// because either can expose refcounted payloads stored from an argument.
    #[test]
    fn indirect_storage_reads_remain_conservative() {
        let program = parse(
            "<?php class S { public array $saved = []; function property(array $input): array { $this->saved = $input; return $this->saved; } function element(array $input): array { return $input[0]; } }",
        );
        let summaries = collect_return_alias_summaries(&program);
        assert_eq!(
            summaries.method("S", "property"),
            Some(&ReturnArgAlias::Unknown)
        );
        assert_eq!(
            summaries.method("S", "element"),
            Some(&ReturnArgAlias::Unknown)
        );
    }

    /// Verifies switch fallthrough and catch entry cannot hide a parameter alias
    /// written along a predecessor path that the lightweight CFG does not model.
    #[test]
    fn fallthrough_and_exception_joins_remain_conservative() {
        let program = parse(
            "<?php function switched(array $input): array { $out = []; switch (1) { case 1: $out = $input; case 2: return $out; } return []; } function caught(array $input): array { $out = []; try { $out = $input; throw new Exception(); } catch (Exception $e) { return $out; } }",
        );
        let summaries = collect_return_alias_summaries(&program);
        assert_eq!(
            summaries.function("switched"),
            Some(&ReturnArgAlias::Unknown)
        );
        assert_eq!(
            summaries.function("caught"),
            Some(&ReturnArgAlias::Unknown)
        );
    }
}
