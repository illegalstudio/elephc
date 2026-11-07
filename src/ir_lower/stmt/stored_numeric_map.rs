//! Purpose:
//! Marks `array_map(fn($v) => $v + C | $v - C | $v * C, $ints)` assignments whose
//! result is only observed through storage-neutral consumers, so lowering may build
//! a raw `array<int>` instead of boxing one Mixed cell per element.
//!
//! Called from:
//! - `crate::ir_lower::function::lower_body_into_function` before pre-lowering fusion.
//!
//! Key details:
//! - The marker is a synthetic call name consumed by `lower_function_call`; the checker
//!   has already typed the original call, so the rewrite changes no PHP-visible type.
//! - A candidate is marked only when every use of its destination local and value aliases
//!   is one of: indexed reads, value-`foreach`, read-only aliasing, or a whitelisted
//!   storage-neutral builtin. Any element write, direct or through an alias, keeps the
//!   generic boxed map so copy-on-write and ownership stay on the established path.
//! - The scan is deliberately conservative: unsupported expression shapes fail closed.

use std::collections::{HashMap, HashSet};

use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind, TypeExpr};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::map_read_fusion::{aliased_locals, expr_mentions, stmt_mentions};

/// Synthetic call name consumed by `crate::ir_lower::expr::function_calls`.
pub(crate) const STORED_NUMERIC_MAP: &str = "__elephc_stored_numeric_map";

/// Marks every safe stored numeric map in one function body.
pub(crate) fn mark_stored_numeric_maps(
    body: &mut [Stmt],
    env: &TypeEnv,
    by_ref_params: &HashSet<String>,
    functions: &HashMap<String, FunctionSig>,
) {
    if functions.contains_key("array_map") {
        return;
    }
    let analysis = body.to_vec();
    let aliased = aliased_locals(&analysis, by_ref_params);
    mark_block(body, &analysis, env, &aliased);
}

/// Rewrites candidates in one block and recurses into nested blocks.
fn mark_block(stmts: &mut [Stmt], analysis: &[Stmt], env: &TypeEnv, aliased: &HashSet<String>) {
    for stmt in stmts.iter_mut() {
        mark_stmt(stmt, analysis, env, aliased);
    }
}

/// Rewrites one candidate statement and recurses into its nested bodies.
fn mark_stmt(stmt: &mut Stmt, analysis: &[Stmt], env: &TypeEnv, aliased: &HashSet<String>) {
    let candidate = match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            stored_map_shape(value, env, aliased).map(|_| name.clone())
        }
        _ => None,
    };
    if let Some(mapped) = candidate {
        let tracked = aliases_of(analysis, &mapped);
        if uses_are_neutral(analysis, &tracked, &mapped, stmt.span) {
            if let StmtKind::Assign { value, .. } | StmtKind::TypedAssign { value, .. } =
                &mut stmt.kind
            {
                if let ExprKind::FunctionCall { name, .. } = &mut value.kind {
                    *name = Name::from(STORED_NUMERIC_MAP);
                }
            }
        }
    }
    match &mut stmt.kind {
        StmtKind::For { body, .. }
        | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body) => mark_block(body, analysis, env, aliased),
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            mark_block(then_body, analysis, env, aliased);
            for (_, body) in elseif_clauses {
                mark_block(body, analysis, env, aliased);
            }
            if let Some(body) = else_body {
                mark_block(body, analysis, env, aliased);
            }
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            mark_block(then_body, analysis, env, aliased);
            if let Some(body) = else_body {
                mark_block(body, analysis, env, aliased);
            }
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                mark_block(body, analysis, env, aliased);
            }
            if let Some(body) = default {
                mark_block(body, analysis, env, aliased);
            }
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            mark_block(try_body, analysis, env, aliased);
            for catch in catches {
                mark_block(&mut catch.body, analysis, env, aliased);
            }
            if let Some(body) = finally_body {
                mark_block(body, analysis, env, aliased);
            }
        }
        _ => {}
    }
}

/// Validates a candidate map call and its source local.
fn stored_map_shape(value: &Expr, env: &TypeEnv, aliased: &HashSet<String>) -> Option<()> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if name.as_str().trim_start_matches('\\') != "array_map" || args.len() != 2 {
        return None;
    }
    let ExprKind::Variable(source) = &args[1].kind else {
        return None;
    };
    if env.get(source).map(PhpType::codegen_repr)
        != Some(PhpType::Array(Box::new(PhpType::Int)))
        || aliased.contains(source)
    {
        return None;
    }
    let (param, body) = single_param_closure(&args[0])?;
    let ExprKind::BinaryOp { left, op, right } = &body.kind else {
        return None;
    };
    match op {
        BinOp::Add => {
            additive_literal(left, right, param)?;
        }
        BinOp::Sub => {
            if !matches!(
                (&left.kind, &right.kind),
                (ExprKind::Variable(name), ExprKind::IntLiteral(_)) if name == param
            ) {
                return None;
            }
        }
        BinOp::Mul => {
            if !matches!(
                (&left.kind, &right.kind),
                (ExprKind::Variable(name), ExprKind::IntLiteral(_))
                    | (ExprKind::IntLiteral(_), ExprKind::Variable(name)) if name == param
            ) {
                return None;
            }
        }
        _ => return None,
    }
    Some(())
}

/// Extracts the single parameter and return expression of a capture-free untyped closure.
fn single_param_closure(expr: &Expr) -> Option<(&str, &Expr)> {
    let ExprKind::Closure {
        params,
        variadic: None,
        return_type: None,
        body,
        captures,
        capture_refs,
        by_ref_return: false,
        ..
    } = &expr.kind
    else {
        return None;
    };
    let [(param, param_type, default, by_ref)] = params.as_slice() else {
        return None;
    };
    if !matches!(param_type, None | Some(TypeExpr::Int))
        || default.is_some()
        || *by_ref
        || !captures.is_empty()
        || !capture_refs.is_empty()
    {
        return None;
    }
    let [Stmt { kind: StmtKind::Return(Some(body)), .. }] = body.as_slice() else {
        return None;
    };
    Some((param.as_str(), body))
}

/// Returns `$param + $literal` or `$literal + $param`.
fn additive_literal(left: &Expr, right: &Expr, param: &str) -> Option<i64> {
    match (&left.kind, &right.kind) {
        (ExprKind::Variable(name), ExprKind::IntLiteral(value)) if name == param => Some(*value),
        (ExprKind::IntLiteral(value), ExprKind::Variable(name)) if name == param => Some(*value),
        _ => None,
    }
}

/// Collects the destination local and every simple value alias assigned from it.
fn aliases_of(body: &[Stmt], mapped: &str) -> HashSet<String> {
    let mut aliases = HashSet::from([mapped.to_string()]);
    loop {
        let mut changed = false;
        collect_alias_assignments(body, &mut aliases, &mut changed);
        if !changed {
            return aliases;
        }
    }
}

/// Adds one level of simple `$alias = $tracked` assignments.
fn collect_alias_assignments(stmts: &[Stmt], aliases: &mut HashSet<String>, changed: &mut bool) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
                if let ExprKind::Variable(source) = &value.kind {
                    if aliases.contains(source) && aliases.insert(name.clone()) {
                        *changed = true;
                    }
                }
            }
            StmtKind::For { body, .. }
            | StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body) => collect_alias_assignments(body, aliases, changed),
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_alias_assignments(then_body, aliases, changed);
                for (_, body) in elseif_clauses {
                    collect_alias_assignments(body, aliases, changed);
                }
                if let Some(body) = else_body {
                    collect_alias_assignments(body, aliases, changed);
                }
            }
            StmtKind::IfDef { then_body, else_body, .. } => {
                collect_alias_assignments(then_body, aliases, changed);
                if let Some(body) = else_body {
                    collect_alias_assignments(body, aliases, changed);
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_alias_assignments(body, aliases, changed);
                }
                if let Some(body) = default {
                    collect_alias_assignments(body, aliases, changed);
                }
            }
            StmtKind::Try { try_body, catches, finally_body } => {
                collect_alias_assignments(try_body, aliases, changed);
                for catch in catches {
                    collect_alias_assignments(&catch.body, aliases, changed);
                }
                if let Some(body) = finally_body {
                    collect_alias_assignments(body, aliases, changed);
                }
            }
            _ => {}
        }
    }
}

/// Returns whether every tracked use is storage-neutral.
fn uses_are_neutral(
    body: &[Stmt],
    tracked: &HashSet<String>,
    mapped: &str,
    candidate_span: Span,
) -> bool {
    body.iter()
        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
}

/// Checks one statement's tracked uses.
fn stmt_neutral(stmt: &Stmt, tracked: &HashSet<String>, mapped: &str, candidate_span: Span) -> bool {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            if name == mapped && stmt.span == candidate_span {
                return true;
            }
            if tracked.contains(name) {
                return matches!(&value.kind, ExprKind::Variable(source) if tracked.contains(source));
            }
            expr_neutral(value, tracked)
        }
        StmtKind::RefAssign { .. } => !any_tracked_stmt(stmt, tracked),
        // Any mutation of the mapped array, direct or through an alias, keeps the generic
        // boxed map. This is the ownership/COW boundary: the raw representation is read-only.
        StmtKind::ArrayAssign { array, index, value } => {
            !tracked.contains(array)
                && !expr_mentions_any(index, tracked)
                && expr_neutral(index, tracked)
                && expr_neutral(value, tracked)
        }
        StmtKind::ArrayPush { array, value } => {
            !tracked.contains(array) && !expr_mentions_any(value, tracked) && expr_neutral(value, tracked)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            root_name(target).is_none_or(|name| !tracked.contains(name))
                && !expr_mentions_any(target, tracked)
                && expr_neutral(value, tracked)
        }
        StmtKind::Foreach { array, key_var, value_var, value_by_ref, body } => {
            if *value_by_ref {
                return !expr_mentions_any(array, tracked);
            }
            if let ExprKind::Variable(name) = &array.kind {
                if tracked.contains(name) {
                    if key_var.as_ref().is_some_and(|key| tracked.contains(key))
                        || tracked.contains(value_var)
                    {
                        return false;
                    }
                    return body
                        .iter()
                        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span));
                }
            }
            expr_neutral(array, tracked)
                && body
                    .iter()
                    .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
        }
        StmtKind::For { init, condition, update, body } => {
            init.as_ref()
                .is_none_or(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                && condition.as_ref().is_none_or(|expr| expr_neutral(expr, tracked))
                && update
                    .as_ref()
                    .is_none_or(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                && body
                    .iter()
                    .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_neutral(condition, tracked)
                && body
                    .iter()
                    .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            expr_neutral(condition, tracked)
                && then_body
                    .iter()
                    .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                && elseif_clauses.iter().all(|(condition, body)| {
                    expr_neutral(condition, tracked)
                        && body
                            .iter()
                            .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
                && else_body.as_ref().is_none_or(|body| {
                    body.iter()
                        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            then_body
                .iter()
                .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                && else_body.as_ref().is_none_or(|body| {
                    body.iter()
                        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
        }
        StmtKind::Switch { subject, cases, default } => {
            expr_neutral(subject, tracked)
                && cases.iter().all(|(conditions, body)| {
                    conditions.iter().all(|condition| expr_neutral(condition, tracked))
                        && body
                            .iter()
                            .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
                && default.as_ref().is_none_or(|body| {
                    body.iter()
                        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            try_body
                .iter()
                .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                && catches.iter().all(|catch| {
                    catch
                        .body
                        .iter()
                        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
                && finally_body.as_ref().is_none_or(|body| {
                    body.iter()
                        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
        }
        StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. } => body
            .iter()
            .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span)),
        StmtKind::Echo(value) | StmtKind::Throw(value) | StmtKind::ExprStmt(value) => {
            expr_neutral(value, tracked)
        }
        StmtKind::Return(Some(value)) => expr_neutral(value, tracked),
        StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => true,
        StmtKind::Include { path, .. } => expr_neutral(path, tracked),
        StmtKind::Global { vars } => !vars.iter().any(|name| tracked.contains(name)),
        StmtKind::StaticVar { name, .. } => !tracked.contains(name),
        _ => !any_tracked_stmt(stmt, tracked),
    }
}

/// Returns whether an expression is a storage-neutral tracked use.
fn expr_neutral(expr: &Expr, tracked: &HashSet<String>) -> bool {
    if !expr_mentions_any(expr, tracked) {
        return true;
    }
    match &expr.kind {
        ExprKind::ArrayAccess { array, index } => {
            if let ExprKind::Variable(name) = &array.kind {
                if tracked.contains(name) {
                    return expr_neutral(index, tracked);
                }
            }
            false
        }
        ExprKind::FunctionCall { name, args } => {
            let canonical = name.as_str().trim_start_matches('\\');
            if !neutral_builtin(canonical) {
                return false;
            }
            args.iter().all(|arg| match &arg.kind {
                ExprKind::Variable(_) => true,
                _ => expr_neutral(arg, tracked),
            })
        }
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::NullCoalesce { value: left, default: right }
        | ExprKind::ShortTernary { value: left, default: right }
        | ExprKind::Pipe { value: left, callable: right } => {
            expr_neutral(left, tracked) && expr_neutral(right, tracked)
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            expr_neutral(condition, tracked)
                && expr_neutral(then_expr, tracked)
                && expr_neutral(else_expr, tracked)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Clone(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::InstanceOf { value: inner, .. } => expr_neutral(inner, tracked),
        ExprKind::ArrayLiteral(items) => items.iter().all(|item| expr_neutral(item, tracked)),
        ExprKind::ArrayLiteralAssoc(entries) => entries
            .iter()
            .all(|(key, value)| expr_neutral(key, tracked) && expr_neutral(value, tracked)),
        ExprKind::Match { subject, arms, default } => {
            expr_neutral(subject, tracked)
                && arms.iter().all(|(conditions, value)| {
                    conditions.iter().all(|condition| expr_neutral(condition, tracked))
                        && expr_neutral(value, tracked)
                })
                && default.as_ref().is_none_or(|value| expr_neutral(value, tracked))
        }
        _ => false,
    }
}

/// Returns whether a builtin observes the array without assuming boxed Mixed slots.
fn neutral_builtin(name: &str) -> bool {
    matches!(
        name,
        "count"
            | "sizeof"
            | "var_dump"
            | "print_r"
            | "implode"
            | "join"
            | "json_encode"
            | "array_sum"
            | "array_values"
            | "array_keys"
            | "array_key_exists"
            | "array_is_list"
    )
}

/// Returns whether any tracked variable occurs in an expression.
fn expr_mentions_any(expr: &Expr, tracked: &HashSet<String>) -> bool {
    tracked.iter().any(|name| expr_mentions(expr, name))
}

/// Returns whether any tracked variable occurs anywhere in a statement.
fn any_tracked_stmt(stmt: &Stmt, tracked: &HashSet<String>) -> bool {
    tracked.iter().any(|name| stmt_mentions(stmt, name))
}

/// Returns the root local of an array-access target, if any.
fn root_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::ArrayAccess { array, .. } => root_name(array),
        _ => None,
    }
}
