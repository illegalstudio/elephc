//! Purpose:
//! Fuses a value-`foreach` over a pure `array_map()` into a source iteration plus one
//! assignment to the original loop variable.
//!
//! Called from:
//! - `crate::ir_lower::stmt::map_read_fusion::fuse_direct_stmt` during pre-lowering fusion.
//!
//! Key details:
//! - The callback is a total arithmetic tree over its parameter and scalar literals, so it has
//!   no side effects, warnings, or throws; integer overflow still promotes to float.
//! - The source must be a packed `array<int>`, unaliased, and untouched by the body. The original
//!   loop variable keeps its Mixed storage, final value, and by-reference uses; only the
//!   iteration local becomes an unforgeable generated name that `lower_foreach` types as `int`.
//! - Array writes in the body must target a definitely initialized local, so the fused loop
//!   never depends on the per-iteration reinitialization of `$array[] = ...` on an unbound name.

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::map_read_fusion::{
    closure_arithmetic, mark_definite_assignments, root_local, stmt_mentions, substitute,
};

/// Proves every array write in a fused loop body targets a definitely initialized local.
///
/// The lowering of `$array[] = ...` on an uninitialized local reinitializes it on every
/// execution, which is wrong inside a loop. Requiring a prior straight-line binding keeps the
/// fused loop from depending on that behavior.
fn array_writes_are_initialized(
    body: &[Stmt],
    outer: &std::collections::HashSet<String>,
    value_var: &str,
    key_var: Option<&str>,
) -> bool {
    /// Checks one body sequence and advances its definite-initialization facts.
    fn block_ok(
        stmts: &[Stmt],
        initialized: &mut std::collections::HashSet<String>,
        value_var: &str,
        key_var: Option<&str>,
    ) -> bool {
        for stmt in stmts {
            if !stmt_array_writes_ok(stmt, initialized, value_var, key_var) {
                return false;
            }
            mark_definite_assignments(stmt, initialized);
        }
        true
    }
    let mut initialized = outer.clone();
    block_ok(body, &mut initialized, value_var, key_var)
}

/// Checks the direct array writes of one statement and recurses into nested bodies.
fn stmt_array_writes_ok(
    stmt: &Stmt,
    initialized: &std::collections::HashSet<String>,
    value_var: &str,
    key_var: Option<&str>,
) -> bool {
    let known = |name: &str| {
        initialized.contains(name) || name == value_var || key_var == Some(name)
    };
    let root_ok = |target: &Expr| {
        root_local(target).is_none_or(|name| known(name))
    };
    match &stmt.kind {
        StmtKind::ArrayPush { array, .. } | StmtKind::ArrayAssign { array, .. } => known(array),
        StmtKind::NestedArrayAssign { target, .. } => root_ok(target),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut nested = initialized.clone();
            body_ok(then_body, &mut nested, value_var, key_var)
                && elseif_clauses.iter().all(|(_, body)| {
                    let mut nested = initialized.clone();
                    body_ok(body, &mut nested, value_var, key_var)
                })
                && else_body.as_ref().is_none_or(|body| {
                    let mut nested = initialized.clone();
                    body_ok(body, &mut nested, value_var, key_var)
                })
        }
        StmtKind::For { body, .. }
        | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body) => {
            let mut nested = initialized.clone();
            body_ok(body, &mut nested, value_var, key_var)
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            let mut nested = initialized.clone();
            body_ok(then_body, &mut nested, value_var, key_var)
                && else_body.as_ref().is_none_or(|body| {
                    let mut nested = initialized.clone();
                    body_ok(body, &mut nested, value_var, key_var)
                })
        }
        StmtKind::Foreach {
            key_var: nested_key,
            value_var: nested_value,
            body,
            ..
        } => {
            let mut nested = initialized.clone();
            nested.insert(nested_value.clone());
            if let Some(key) = nested_key {
                nested.insert(key.clone());
            }
            body_ok(body, &mut nested, value_var, key_var)
        }
        StmtKind::Switch { cases, default, .. } => {
            cases.iter().all(|(_, body)| {
                let mut nested = initialized.clone();
                body_ok(body, &mut nested, value_var, key_var)
            }) && default.as_ref().is_none_or(|body| {
                let mut nested = initialized.clone();
                body_ok(body, &mut nested, value_var, key_var)
            })
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let mut nested = initialized.clone();
            body_ok(try_body, &mut nested, value_var, key_var)
                && catches.iter().all(|catch| {
                    let mut nested = initialized.clone();
                    body_ok(&catch.body, &mut nested, value_var, key_var)
                })
                && finally_body.as_ref().is_none_or(|body| {
                    let mut nested = initialized.clone();
                    body_ok(body, &mut nested, value_var, key_var)
                })
        }
        _ => true,
    }
}

/// Sequence helper shared by the nested-body checks.
fn body_ok(
    stmts: &[Stmt],
    initialized: &mut std::collections::HashSet<String>,
    value_var: &str,
    key_var: Option<&str>,
) -> bool {
    for stmt in stmts {
        if !stmt_array_writes_ok(stmt, initialized, value_var, key_var) {
            return false;
        }
        mark_definite_assignments(stmt, initialized);
    }
    true
}

/// Fuses a value-`foreach` over a pure `array_map()` into a source iteration plus one assignment.
///
/// PHP materializes the whole mapped array before the first iteration. The callback is a total
/// arithmetic tree with no side effects, warnings, or throws (integer overflow promotes to
/// float), so evaluating it at the top of each iteration is unobservable. The original loop
/// variable keeps its Mixed storage, its final value after exhaustion, and any by-reference use
/// inside the body; only the iteration variable becomes the unforgeable generated local, which
/// `lower_foreach` types as `int` from the source. The body must not mention or mutate the
/// source, and a reference alias would let it change the iterated array.
pub(super) fn try_fuse_foreach_map(
    array: &mut Expr,
    key_var: Option<&str>,
    value_var: &mut String,
    body: &mut Vec<Stmt>,
    env: &TypeEnv,
    aliased: &std::collections::HashSet<String>,
    span: Span,
    source_mode: crate::source::SourceMode,
    strict_types: bool,
    fresh_foreach: &mut usize,
    initialized: &std::collections::HashSet<String>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &array.kind else {
        return false;
    };
    if name.as_str().trim_start_matches('\\') != "array_map" || args.len() != 2 {
        return false;
    }
    let ExprKind::Variable(source) = &args[1].kind else {
        return false;
    };
    if env.get(source).map(PhpType::codegen_repr)
        != Some(PhpType::Array(Box::new(PhpType::Int)))
        || aliased.contains(source)
        || source == value_var
        || key_var == Some(source.as_str())
        || body.iter().any(|stmt| stmt_mentions(stmt, source))
        || !array_writes_are_initialized(body, initialized, value_var, key_var)
    {
        return false;
    }
    let Some((param, map_body)) = closure_arithmetic(&args[0]) else {
        return false;
    };
    let fresh = crate::names::generated_local_name(&format!(
        "__elephc_foreach_map_value_{}",
        *fresh_foreach
    ));
    *fresh_foreach += 1;
    let read = Expr {
        kind: ExprKind::Variable(fresh.clone()),
        span,
    };
    let mapped = substitute(map_body, param, &read);
    let mut assign = Stmt::new(
        StmtKind::Assign {
            name: value_var.clone(),
            value: mapped,
        },
        span,
    );
    assign.source_mode = source_mode;
    assign.strict_types = strict_types;
    body.insert(0, assign);
    let source_expr = args[1].clone();
    *array = source_expr;
    *value_var = fresh;
    true
}
