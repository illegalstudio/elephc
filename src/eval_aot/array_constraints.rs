//! Purpose:
//! Collects array and associative-array constraints for direct scope reads.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - Builtin-specific requirements remain tied to accepted AOT call shapes.

use super::*;

/// Caller-scope variables that need array-specific call-site proof.
#[derive(Default)]
pub(super) struct ArrayScopeReadConstraintSets {
    pub(super) array_like: BTreeSet<String>,
    pub(super) assoc: BTreeSet<String>,
}

/// Collects caller-scope reads that must be array-like for accepted AOT calls.
pub(super) fn collect_array_scope_read_constraint_sets(
    program: &[Stmt],
    scope_reads: &BTreeSet<String>,
) -> ArrayScopeReadConstraintSets {
    let mut constraints = ArrayScopeReadConstraintSets::default();
    for stmt in program {
        collect_stmt_array_scope_read_constraints(stmt, scope_reads, &mut constraints);
    }
    constraints
}

/// Collects array constraints from one statement in the EIR AOT subset.
pub(super) fn collect_stmt_array_scope_read_constraints(
    stmt: &Stmt,
    scope_reads: &BTreeSet<String>,
    constraints: &mut ArrayScopeReadConstraintSets,
) {
    match &stmt.kind {
        StmtKind::Synthetic(body) => {
            for stmt in body {
                collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::Echo(expr) | StmtKind::ExprStmt(expr) | StmtKind::Return(Some(expr)) => {
            collect_expr_array_scope_read_constraints(expr, scope_reads, constraints);
        }
        StmtKind::Assign { value, .. } => {
            collect_expr_array_scope_read_constraints(value, scope_reads, constraints);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
            for stmt in then_body {
                collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
            }
            for (condition, body) in elseif_clauses {
                collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
                for stmt in body {
                    collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
                }
            }
            if let Some(else_body) = else_body {
                for stmt in else_body {
                    collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
                }
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
            for stmt in body {
                collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_stmt_array_scope_read_constraints(init, scope_reads, constraints);
            }
            if let Some(condition) = condition {
                collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
            }
            if let Some(update) = update {
                collect_stmt_array_scope_read_constraints(update, scope_reads, constraints);
            }
            for stmt in body {
                collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::Foreach { array, body, .. } => {
            if let ExprKind::Variable(variable) = &array.kind {
                if scope_reads.contains(variable) {
                    constraints.array_like.insert(variable.clone());
                }
            }
            collect_expr_array_scope_read_constraints(array, scope_reads, constraints);
            if expr_is_static_empty_array_literal_source(array) {
                return;
            }
            for stmt in body {
                collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_expr_array_scope_read_constraints(subject, scope_reads, constraints);
            for (conditions, body) in cases {
                for condition in conditions {
                    collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
                }
                for stmt in body {
                    collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
                }
            }
            if let Some(default) = default {
                for stmt in default {
                    collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
                }
            }
        }
        _ => {}
    }
}

/// Collects array constraints from one expression in the EIR AOT subset.
pub(super) fn collect_expr_array_scope_read_constraints(
    expr: &Expr,
    scope_reads: &BTreeSet<String>,
    constraints: &mut ArrayScopeReadConstraintSets,
) {
    match &expr.kind {
        ExprKind::UnaryPlus(inner) | ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. } => {
            collect_expr_array_scope_read_constraints(inner, scope_reads, constraints);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_expr_array_scope_read_constraints(left, scope_reads, constraints);
            collect_expr_array_scope_read_constraints(right, scope_reads, constraints);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
            collect_expr_array_scope_read_constraints(then_expr, scope_reads, constraints);
            collect_expr_array_scope_read_constraints(else_expr, scope_reads, constraints);
        }
        ExprKind::ShortTernary { value, default } | ExprKind::NullCoalesce { value, default } => {
            collect_expr_array_scope_read_constraints(value, scope_reads, constraints);
            collect_expr_array_scope_read_constraints(default, scope_reads, constraints);
        }
        ExprKind::Cast { expr, .. } => {
            collect_expr_array_scope_read_constraints(expr, scope_reads, constraints);
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_expr_array_scope_read_constraints(subject, scope_reads, constraints);
            for (conditions, result) in arms {
                for condition in conditions {
                    collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
                }
                collect_expr_array_scope_read_constraints(result, scope_reads, constraints);
            }
            if let Some(default) = default {
                collect_expr_array_scope_read_constraints(default, scope_reads, constraints);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_expr_array_scope_read_constraints(array, scope_reads, constraints);
            collect_expr_array_scope_read_constraints(index, scope_reads, constraints);
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_expr_array_scope_read_constraints(item, scope_reads, constraints);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                collect_expr_array_scope_read_constraints(key, scope_reads, constraints);
                collect_expr_array_scope_read_constraints(value, scope_reads, constraints);
            }
        }
        ExprKind::FunctionCall { name, args } => {
            collect_builtin_array_scope_read_constraints(
                name.as_str(),
                args,
                scope_reads,
                constraints,
            );
            for arg in args {
                collect_expr_array_scope_read_constraints(arg, scope_reads, constraints);
            }
        }
        _ => {}
    }
}

/// Collects array caller-type constraints from supported runtime builtin calls.
pub(super) fn collect_builtin_array_scope_read_constraints(
    name: &str,
    args: &[Expr],
    scope_reads: &BTreeSet<String>,
    constraints: &mut ArrayScopeReadConstraintSets,
) {
    let short_name = php_symbol_key(name.trim_start_matches('\\'));
    let Some(args) = normalize_eir_runtime_builtin_args(&short_name, args) else {
        return;
    };
    match short_name.as_str() {
        "count" if (1..=2).contains(&args.len()) && eir_count_mode_is_default_zero(args.get(1)) => {
            collect_scope_array_like_constraint(&args[0], scope_reads, constraints);
        }
        "array_key_exists"
            if args.len() == 2 && eir_array_key_exists_static_key_is_safe(&args[0]) =>
        {
            collect_scope_array_like_constraint(&args[1], scope_reads, constraints);
            if eir_array_key_exists_static_key_needs_assoc_array(&args[0]) {
                collect_scope_assoc_array_constraint(&args[1], scope_reads, constraints);
            }
        }
        _ => {}
    }
}

/// Records that one expression must be a caller-side array when it reads scope.
pub(super) fn collect_scope_array_like_constraint(
    expr: &Expr,
    scope_reads: &BTreeSet<String>,
    constraints: &mut ArrayScopeReadConstraintSets,
) {
    if let ExprKind::Variable(variable) = &expr.kind {
        if scope_reads.contains(variable) {
            constraints.array_like.insert(variable.clone());
        }
    }
}

/// Records that one expression must be a caller-side associative array.
pub(super) fn collect_scope_assoc_array_constraint(
    expr: &Expr,
    scope_reads: &BTreeSet<String>,
    constraints: &mut ArrayScopeReadConstraintSets,
) {
    if let ExprKind::Variable(variable) = &expr.kind {
        if scope_reads.contains(variable) {
            constraints.assoc.insert(variable.clone());
        }
    }
}
