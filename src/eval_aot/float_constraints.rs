//! Purpose:
//! Collects float-predicate constraints and scope reads inside expressions.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - IEEE predicate eligibility stays separate from PHP coercion.

use super::*;

/// Collects caller-scope reads that must be int/float for IEEE float predicates.
pub(super) fn collect_float_predicate_scope_read_constraints(
    program: &[Stmt],
    scope_reads: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut constraints = BTreeSet::new();
    for stmt in program {
        collect_stmt_float_predicate_scope_read_constraints(stmt, scope_reads, &mut constraints);
    }
    constraints
}

/// Collects float-predicate constraints from one statement in the EIR AOT subset.
pub(super) fn collect_stmt_float_predicate_scope_read_constraints(
    stmt: &Stmt,
    scope_reads: &BTreeSet<String>,
    constraints: &mut BTreeSet<String>,
) {
    match &stmt.kind {
        StmtKind::Synthetic(body) => {
            for stmt in body {
                collect_stmt_float_predicate_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::Echo(expr) | StmtKind::ExprStmt(expr) | StmtKind::Return(Some(expr)) => {
            collect_expr_float_predicate_scope_read_constraints(expr, scope_reads, constraints);
        }
        StmtKind::Assign { value, .. } => {
            collect_expr_float_predicate_scope_read_constraints(value, scope_reads, constraints);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_expr_float_predicate_scope_read_constraints(
                condition,
                scope_reads,
                constraints,
            );
            for stmt in then_body {
                collect_stmt_float_predicate_scope_read_constraints(stmt, scope_reads, constraints);
            }
            for (condition, body) in elseif_clauses {
                collect_expr_float_predicate_scope_read_constraints(
                    condition,
                    scope_reads,
                    constraints,
                );
                for stmt in body {
                    collect_stmt_float_predicate_scope_read_constraints(
                        stmt,
                        scope_reads,
                        constraints,
                    );
                }
            }
            if let Some(else_body) = else_body {
                for stmt in else_body {
                    collect_stmt_float_predicate_scope_read_constraints(
                        stmt,
                        scope_reads,
                        constraints,
                    );
                }
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_expr_float_predicate_scope_read_constraints(
                condition,
                scope_reads,
                constraints,
            );
            for stmt in body {
                collect_stmt_float_predicate_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_stmt_float_predicate_scope_read_constraints(init, scope_reads, constraints);
            }
            if let Some(condition) = condition {
                collect_expr_float_predicate_scope_read_constraints(
                    condition,
                    scope_reads,
                    constraints,
                );
            }
            if let Some(update) = update {
                collect_stmt_float_predicate_scope_read_constraints(
                    update,
                    scope_reads,
                    constraints,
                );
            }
            for stmt in body {
                collect_stmt_float_predicate_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_expr_float_predicate_scope_read_constraints(array, scope_reads, constraints);
            if expr_is_static_empty_array_literal_source(array) {
                return;
            }
            for stmt in body {
                collect_stmt_float_predicate_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_expr_float_predicate_scope_read_constraints(subject, scope_reads, constraints);
            for (conditions, body) in cases {
                for condition in conditions {
                    collect_expr_float_predicate_scope_read_constraints(
                        condition,
                        scope_reads,
                        constraints,
                    );
                }
                for stmt in body {
                    collect_stmt_float_predicate_scope_read_constraints(
                        stmt,
                        scope_reads,
                        constraints,
                    );
                }
            }
            if let Some(default) = default {
                for stmt in default {
                    collect_stmt_float_predicate_scope_read_constraints(
                        stmt,
                        scope_reads,
                        constraints,
                    );
                }
            }
        }
        _ => {}
    }
}

/// Collects float-predicate constraints from one expression in the EIR AOT subset.
pub(super) fn collect_expr_float_predicate_scope_read_constraints(
    expr: &Expr,
    scope_reads: &BTreeSet<String>,
    constraints: &mut BTreeSet<String>,
) {
    match &expr.kind {
        ExprKind::UnaryPlus(inner) | ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. } => {
            collect_expr_float_predicate_scope_read_constraints(inner, scope_reads, constraints);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_expr_float_predicate_scope_read_constraints(left, scope_reads, constraints);
            collect_expr_float_predicate_scope_read_constraints(right, scope_reads, constraints);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_float_predicate_scope_read_constraints(
                condition,
                scope_reads,
                constraints,
            );
            collect_expr_float_predicate_scope_read_constraints(
                then_expr,
                scope_reads,
                constraints,
            );
            collect_expr_float_predicate_scope_read_constraints(
                else_expr,
                scope_reads,
                constraints,
            );
        }
        ExprKind::ShortTernary { value, default } | ExprKind::NullCoalesce { value, default } => {
            collect_expr_float_predicate_scope_read_constraints(value, scope_reads, constraints);
            collect_expr_float_predicate_scope_read_constraints(default, scope_reads, constraints);
        }
        ExprKind::Cast { expr, .. } => {
            collect_expr_float_predicate_scope_read_constraints(expr, scope_reads, constraints);
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_expr_float_predicate_scope_read_constraints(subject, scope_reads, constraints);
            for (conditions, result) in arms {
                for condition in conditions {
                    collect_expr_float_predicate_scope_read_constraints(
                        condition,
                        scope_reads,
                        constraints,
                    );
                }
                collect_expr_float_predicate_scope_read_constraints(
                    result,
                    scope_reads,
                    constraints,
                );
            }
            if let Some(default) = default {
                collect_expr_float_predicate_scope_read_constraints(
                    default,
                    scope_reads,
                    constraints,
                );
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_expr_float_predicate_scope_read_constraints(array, scope_reads, constraints);
            collect_expr_float_predicate_scope_read_constraints(index, scope_reads, constraints);
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_expr_float_predicate_scope_read_constraints(item, scope_reads, constraints);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                collect_expr_float_predicate_scope_read_constraints(key, scope_reads, constraints);
                collect_expr_float_predicate_scope_read_constraints(
                    value,
                    scope_reads,
                    constraints,
                );
            }
        }
        ExprKind::FunctionCall { name, args } => {
            let name = php_symbol_key(name.as_str().trim_start_matches('\\'));
            if matches!(name.as_str(), "is_finite" | "is_infinite" | "is_nan") && args.len() == 1 {
                collect_scope_read_variables_in_expr(&args[0], scope_reads, constraints);
            }
            for arg in args {
                collect_expr_float_predicate_scope_read_constraints(arg, scope_reads, constraints);
            }
        }
        _ => {}
    }
}

/// Collects scope-read variable names that occur anywhere inside an expression.
pub(super) fn collect_scope_read_variables_in_expr(
    expr: &Expr,
    scope_reads: &BTreeSet<String>,
    variables: &mut BTreeSet<String>,
) {
    match &expr.kind {
        ExprKind::Variable(name) => {
            if scope_reads.contains(name) {
                variables.insert(name.clone());
            }
        }
        ExprKind::UnaryPlus(inner) | ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::Cast { expr: inner, .. } => {
            collect_scope_read_variables_in_expr(inner, scope_reads, variables);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_scope_read_variables_in_expr(left, scope_reads, variables);
            collect_scope_read_variables_in_expr(right, scope_reads, variables);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_scope_read_variables_in_expr(condition, scope_reads, variables);
            collect_scope_read_variables_in_expr(then_expr, scope_reads, variables);
            collect_scope_read_variables_in_expr(else_expr, scope_reads, variables);
        }
        ExprKind::ShortTernary { value, default } | ExprKind::NullCoalesce { value, default } => {
            collect_scope_read_variables_in_expr(value, scope_reads, variables);
            collect_scope_read_variables_in_expr(default, scope_reads, variables);
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_scope_read_variables_in_expr(array, scope_reads, variables);
            collect_scope_read_variables_in_expr(index, scope_reads, variables);
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_scope_read_variables_in_expr(item, scope_reads, variables);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                collect_scope_read_variables_in_expr(key, scope_reads, variables);
                collect_scope_read_variables_in_expr(value, scope_reads, variables);
            }
        }
        ExprKind::FunctionCall { args, .. } => {
            for arg in args {
                collect_scope_read_variables_in_expr(arg, scope_reads, variables);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_scope_read_variables_in_expr(subject, scope_reads, variables);
            for (conditions, result) in arms {
                for condition in conditions {
                    collect_scope_read_variables_in_expr(condition, scope_reads, variables);
                }
                collect_scope_read_variables_in_expr(result, scope_reads, variables);
            }
            if let Some(default) = default {
                collect_scope_read_variables_in_expr(default, scope_reads, variables);
            }
        }
        _ => {}
    }
}
