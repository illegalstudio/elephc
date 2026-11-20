//! Purpose:
//! Replaces two counted-loop shapes with their closed-form sums: a constant-bound
//! `$data[] = $i % D` fill followed by a full-array sum, and a running `$sum += $i` over an
//! arithmetic progression.
//!
//! Called from:
//! - `crate::ir_lower::function::lower_body_into_function` before the map+sum cache.
//!
//! Key details:
//! - The matched shape is exactly `array = []; i = 0; while (i < N) { array[] = i % D; i++; }`
//!   followed by `sum = 0; i = 0; while (i < count(array)) { sum += array[i]; i++; }`.
//! - `N` and `D` must be integer literals, `D >= 1`, and the mathematical sum must fit in
//!   `i64`; otherwise the original loops run unchanged.
//! - The array local must not be observed anywhere outside the matched statements, so removing
//!   the fill keeps every observable value identical. The array initialization is retained.
//! - Both counters end at `N` and the accumulator ends at the closed-form sum, matching the
//!   loops' final state.

use std::collections::HashSet;

use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

use super::map_read_fusion::stmt_mentions;

/// Rewrites every reducible fill+sum sequence in one function body.
pub(crate) fn reduce_fill_sums(body: &mut Vec<Stmt>, aliased: &HashSet<String>) {
    reduce_modular_fill_sums(body, aliased);
    reduce_arithmetic_sums(body, aliased);
}

/// Rewrites the modular fill+sum sequence.
fn reduce_modular_fill_sums(body: &mut Vec<Stmt>, aliased: &HashSet<String>) {
    let mut index = 0;
    while index + 5 < body.len() {
        if let Some(replacement) = try_reduce(&body[index..index + 6], body, index, aliased) {
            let inserted = replacement.len();
            body.splice(index..index + 6, replacement);
            index += inserted;
        } else {
            index += 1;
        }
    }
}

/// Rewrites `$sum = 0; $i = A; while ($i <= N) { $sum += $i; $i++; }` to its series sum.
fn reduce_arithmetic_sums(body: &mut Vec<Stmt>, aliased: &HashSet<String>) {
    let mut index = 0;
    while index + 2 < body.len() {
        if let Some(replacement) = try_reduce_series(&body[index..index + 3], aliased) {
            body.splice(index..index + 3, replacement);
            index += 2;
        } else {
            index += 1;
        }
    }
}

/// Returns the replacement statements for one matched six-statement sequence.
fn try_reduce(
    window: &[Stmt],
    body: &[Stmt],
    start: usize,
    aliased: &HashSet<String>,
) -> Option<Vec<Stmt>> {
    let [init_array, init_counter, fill_loop, init_sum, reset_counter, sum_loop] = window else {
        return None;
    };
    let StmtKind::Assign {
        name: array,
        value: array_value,
    } = &init_array.kind
    else {
        return None;
    };
    if !matches!(&array_value.kind, ExprKind::ArrayLiteral(items) if items.is_empty())
        || aliased.contains(array)
    {
        return None;
    }
    let StmtKind::Assign {
        name: fill_counter,
        value: fill_seed,
    } = &init_counter.kind
    else {
        return None;
    };
    if !matches!(fill_seed.kind, ExprKind::IntLiteral(0)) || aliased.contains(fill_counter) {
        return None;
    }
    let (bound, divisor) = match_fill_loop(fill_loop, fill_counter, array)?;
    if bound < 0 || divisor < 1 {
        return None;
    }
    let StmtKind::Assign {
        name: sum,
        value: sum_seed,
    } = &init_sum.kind
    else {
        return None;
    };
    if !matches!(sum_seed.kind, ExprKind::IntLiteral(0)) || aliased.contains(sum) {
        return None;
    }
    let StmtKind::Assign {
        name: sum_counter,
        value: sum_seed_zero,
    } = &reset_counter.kind
    else {
        return None;
    };
    if !matches!(sum_seed_zero.kind, ExprKind::IntLiteral(0)) || aliased.contains(sum_counter) {
        return None;
    }
    // A shared accumulator/counter feeds the loop condition, so the closed form cannot model it.
    if sum == sum_counter {
        return None;
    }
    if !match_sum_loop(sum_loop, sum_counter, array, sum) {
        return None;
    }
    // The array must not be observed outside the matched statements, and no call outside them
    // may inspect the local scope (`get_defined_vars()` or a dynamic scope reader).
    for (index, stmt) in body.iter().enumerate() {
        if (index < start || index >= start + 6)
            && (stmt_mentions(stmt, array) || stmt_has_call(stmt))
        {
            return None;
        }
    }
    let total = modular_sum(bound, divisor)?;
    let span = init_array.span;
    let mut replacement = vec![init_array.clone(), assign(fill_counter, bound, span)];
    if sum_counter == fill_counter {
        replacement.push(assign(sum, total, span));
    } else {
        replacement.push(assign(sum, total, span));
        replacement.push(assign(sum_counter, bound, span));
    }
    Some(replacement)
}

/// Returns whether a statement contains a function/method/closure call in its expressions.
pub(super) fn stmt_has_call(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ArrayPush { value, .. }
        | StmtKind::ExprStmt(value)
        | StmtKind::Echo(value)
        | StmtKind::Throw(value) => expr_has_call(value),
        StmtKind::ArrayAssign { index, value, .. } => expr_has_call(index) || expr_has_call(value),
        StmtKind::Return(Some(value)) => expr_has_call(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_has_call(condition)
                || then_body.iter().any(stmt_has_call)
                || elseif_clauses
                    .iter()
                    .any(|(condition, body)| expr_has_call(condition) || body.iter().any(stmt_has_call))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_has_call))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_has_call(condition) || body.iter().any(stmt_has_call)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_has_call)
                || condition.as_ref().is_some_and(expr_has_call)
                || update.as_deref().is_some_and(stmt_has_call)
                || body.iter().any(stmt_has_call)
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_has_call(array) || body.iter().any(stmt_has_call)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_has_call(subject)
                || cases.iter().any(|(patterns, body)| {
                    patterns.iter().any(expr_has_call) || body.iter().any(stmt_has_call)
                })
                || default.as_ref().is_some_and(|body| body.iter().any(stmt_has_call))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_has_call)
                || catches.iter().any(|catch| catch.body.iter().any(stmt_has_call))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_has_call))
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => body.iter().any(stmt_has_call),
        StmtKind::IfDef { then_body, else_body, .. } => {
            then_body.iter().any(stmt_has_call)
                || else_body.as_ref().is_some_and(|body| body.iter().any(stmt_has_call))
        }
        // Unknown statement shapes may hide a call; the caller fails closed.
        _ => true,
    }
}

/// Returns whether an expression contains a function/method/closure call.
fn expr_has_call(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::FunctionCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::NewGeneric { .. }
        | ExprKind::NewScopedObject { .. }
        | ExprKind::NewDynamic { .. }
        | ExprKind::NewDynamicObject { .. } => true,
        ExprKind::BinaryOp { left, right, .. } => expr_has_call(left) || expr_has_call(right),
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. } => expr_has_call(inner),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            expr_has_call(value) || expr_has_call(default)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_has_call(condition) || expr_has_call(then_expr) || expr_has_call(else_expr),
        ExprKind::ArrayAccess { array, index } => expr_has_call(array) || expr_has_call(index),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_has_call),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| expr_has_call(key) || expr_has_call(value)),
        ExprKind::ArrayLiteralMixed(items) => items
            .iter()
            .flat_map(|entry| entry.exprs())
            .any(expr_has_call),
        ExprKind::Closure { body, .. } => body.iter().any(stmt_has_call),
        _ => false,
    }
}

/// Matches a zero-seeded arithmetic accumulator loop and returns its replacement statements.
fn try_reduce_series(window: &[Stmt], aliased: &HashSet<String>) -> Option<Vec<Stmt>> {
    let [init_sum, init_counter, loop_stmt] = window else {
        return None;
    };
    let StmtKind::Assign {
        name: sum,
        value: sum_seed,
    } = &init_sum.kind
    else {
        return None;
    };
    if !matches!(sum_seed.kind, ExprKind::IntLiteral(0)) || aliased.contains(sum) {
        return None;
    }
    let StmtKind::Assign {
        name: counter,
        value: counter_seed,
    } = &init_counter.kind
    else {
        return None;
    };
    let ExprKind::IntLiteral(start) = counter_seed.kind else {
        return None;
    };
    if start < 0 || aliased.contains(counter) || counter == sum {
        return None;
    }
    let (bound, inclusive) = match_series_condition(loop_stmt, counter, sum)?;
    if bound < start {
        return None;
    }
    let count = if inclusive {
        i128::from(bound) - i128::from(start) + 1
    } else {
        i128::from(bound) - i128::from(start)
    };
    if count <= 0 {
        return None;
    }
    let total = count * (2 * i128::from(start) + count - 1) / 2;
    let total = i64::try_from(total).ok()?;
    let final_counter = i64::try_from(i128::from(start) + count).ok()?;
    let span = init_sum.span;
    Some(vec![
        assign(sum, total, span),
        assign(counter, final_counter, span),
    ])
}

/// Matches a `while ($counter <= N)` or `while ($counter < N)` series loop with one addend.
fn match_series_condition(loop_stmt: &Stmt, counter: &str, sum: &str) -> Option<(i64, bool)> {
    let StmtKind::While { condition, body } = &loop_stmt.kind else {
        return None;
    };
    let ExprKind::BinaryOp { left, op, right } = &condition.kind else {
        return None;
    };
    if !is_variable(left, counter) {
        return None;
    }
    let inclusive = match op {
        BinOp::LtEq => true,
        BinOp::Lt => false,
        _ => return None,
    };
    let ExprKind::IntLiteral(bound) = right.kind else {
        return None;
    };
    let [accumulate, update] = body.as_slice() else {
        return None;
    };
    let StmtKind::Assign {
        name: target,
        value,
    } = &accumulate.kind
    else {
        return None;
    };
    if target != sum || !is_sum_of(value, sum, counter) || !is_increment(update, counter) {
        return None;
    }
    Some((bound, inclusive))
}

/// Returns whether `value` is `$sum + $counter` or the reversed operand order.
fn is_sum_of(value: &Expr, sum: &str, counter: &str) -> bool {
    matches!(
        &value.kind,
        ExprKind::BinaryOp { left, op: BinOp::Add, right }
            if (is_variable(left, sum) && is_variable(right, counter))
                || (is_variable(right, sum) && is_variable(left, counter))
    )
}

/// Matches `while ($counter < N) { $array[] = $counter % D; $counter++; }`.
fn match_fill_loop(stmt: &Stmt, counter: &str, array: &str) -> Option<(i64, i64)> {
    let StmtKind::While { condition, body } = &stmt.kind else {
        return None;
    };
    let bound = match_count_condition(condition, counter)?;
    let [push, update] = body.as_slice() else {
        return None;
    };
    let StmtKind::ArrayPush {
        array: pushed_array,
        value,
    } = &push.kind
    else {
        return None;
    };
    if pushed_array != array {
        return None;
    }
    let ExprKind::BinaryOp {
        left,
        op: BinOp::Mod,
        right,
    } = &value.kind
    else {
        return None;
    };
    if !matches!(&left.kind, ExprKind::Variable(name) if name == counter) {
        return None;
    }
    let ExprKind::IntLiteral(divisor) = right.kind else {
        return None;
    };
    if !is_increment(update, counter) {
        return None;
    }
    Some((bound, divisor))
}

/// Matches `while ($counter < count($array)) { $sum += $array[$counter]; $counter++; }`.
fn match_sum_loop(stmt: &Stmt, counter: &str, array: &str, sum: &str) -> bool {
    let StmtKind::While { condition, body } = &stmt.kind else {
        return false;
    };
    let ExprKind::BinaryOp {
        left,
        op: BinOp::Lt,
        right,
    } = &condition.kind
    else {
        return false;
    };
    if !matches!(&left.kind, ExprKind::Variable(name) if name == counter)
        || !is_count_of(right, array)
    {
        return false;
    }
    let [accumulate, update] = body.as_slice() else {
        return false;
    };
    let StmtKind::Assign {
        name: target,
        value,
    } = &accumulate.kind
    else {
        return false;
    };
    if target != sum || !is_sum_of_element(value, sum, array, counter) {
        return false;
    }
    is_increment(update, counter)
}

/// Reads a `counter < N` condition with a literal bound.
fn match_count_condition(condition: &Expr, counter: &str) -> Option<i64> {
    let ExprKind::BinaryOp {
        left,
        op: BinOp::Lt,
        right,
    } = &condition.kind
    else {
        return None;
    };
    if !matches!(&left.kind, ExprKind::Variable(name) if name == counter) {
        return None;
    }
    let ExprKind::IntLiteral(bound) = right.kind else {
        return None;
    };
    Some(bound)
}

/// Returns whether `expr` is `count($array)`.
fn is_count_of(expr: &Expr, array: &str) -> bool {
    matches!(
        &expr.kind,
        ExprKind::FunctionCall { name, args }
            if name.as_str().trim_start_matches('\\') == "count"
                && matches!(args.as_slice(), [Expr { kind: ExprKind::Variable(name), .. }] if name == array)
    )
}

/// Returns whether `value` is `$sum + $array[$counter]` or the reversed operand order.
fn is_sum_of_element(value: &Expr, sum: &str, array: &str, counter: &str) -> bool {
    let ExprKind::BinaryOp {
        left,
        op: BinOp::Add,
        right,
    } = &value.kind
    else {
        return false;
    };
    (is_variable(left, sum) && is_element_read(right, array, counter))
        || (is_variable(right, sum) && is_element_read(left, array, counter))
}

/// Returns whether `expr` is `$name`.
fn is_variable(expr: &Expr, name: &str) -> bool {
    matches!(&expr.kind, ExprKind::Variable(found) if found == name)
}

/// Returns whether `expr` is `$array[$counter]`.
fn is_element_read(expr: &Expr, array: &str, counter: &str) -> bool {
    matches!(
        &expr.kind,
        ExprKind::ArrayAccess { array: container, index }
            if is_variable(container, array) && is_variable(index, counter)
    )
}

/// Returns whether `stmt` advances `counter` by exactly one.
fn is_increment(stmt: &Stmt, counter: &str) -> bool {
    match &stmt.kind {
        StmtKind::ExprStmt(expr) => matches!(
            &expr.kind,
            ExprKind::PostIncrement(name) | ExprKind::PreIncrement(name) if name == counter
        ),
        StmtKind::Assign { name, value } => {
            name == counter
                && matches!(
                    &value.kind,
                    ExprKind::BinaryOp { left, op: BinOp::Add, right }
                        if is_variable(left, counter)
                            && matches!(right.kind, ExprKind::IntLiteral(1))
                )
        }
        _ => false,
    }
}

/// Computes `sum(i % d for i in 0..n)` when it fits in `i64`.
fn modular_sum(n: i64, d: i64) -> Option<i64> {
    if n <= 0 || d <= 0 {
        return Some(0);
    }
    let n = i128::from(n);
    let d = i128::from(d);
    let full = n / d;
    let partial = n % d;
    let total = full * d * (d - 1) / 2 + partial * (partial - 1) / 2;
    i64::try_from(total).ok()
}

/// Builds a plain assignment statement.
fn assign(name: &str, value: i64, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::Assign {
            name: name.to_string(),
            value: Expr::new(ExprKind::IntLiteral(value), span),
        },
        span,
    )
}
