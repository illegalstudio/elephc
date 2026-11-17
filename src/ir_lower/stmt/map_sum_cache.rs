//! Purpose:
//! Caches a loop-invariant `array_sum(array_map($callback, $source))` result in a hidden local
//! before the loop, then reuses it each iteration.
//!
//! Called from:
//! - `crate::ir_lower::function::lower_body_into_function` before stored-map marking.
//!
//! Key details:
//! - The loop must be a `for` with a provably positive bound, so hoisting the map+sum cannot
//!   add work PHP would have skipped on a zero-trip loop.
//! - The callback must be a capture-free single-parameter closure whose body is one of the
//!   supported numeric map shapes, the source must be a plain `array<int>` local that is not
//!   written or aliased in the loop, and the mapped local must have no other use.
//! - The cached value is a compiler-generated `Mixed` local. Normal lowering owns, loads and
//!   releases it, and the frame epilogue releases the final value, so no ownership path is
//!   bypassed.
//! - Anything unproven leaves the original loop untouched.

use std::collections::HashSet;

use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::fill_sum_reduction::stmt_has_call;
use super::map_read_fusion::{expr_mentions, stmt_mentions};

/// Caches every loop-invariant numeric map sum in one function body.
pub(crate) fn cache_loop_invariant_map_sums(
    body: &mut Vec<Stmt>,
    env: &TypeEnv,
    by_ref_params: &HashSet<String>,
    aliased: &HashSet<String>,
) {
    let mut counter = 0usize;
    cache_block(body, env, by_ref_params, aliased, &mut counter);
}

/// Recurse into nested statement blocks and cache each supported loop.
fn cache_block(
    body: &mut Vec<Stmt>,
    env: &TypeEnv,
    by_ref_params: &HashSet<String>,
    aliased: &HashSet<String>,
    counter: &mut usize,
) {
    let mut index = 0;
    while index < body.len() {
        let original = body[index].clone();
        let (previous, rest) = body.split_at_mut(index);
        let Some((current, following)) = rest.split_first_mut() else {
            break;
        };
        if let Some((prelude, mapped)) = try_cache_loop(
            current,
            previous,
            following,
            env,
            by_ref_params,
            aliased,
            *counter,
        ) {
            let used_elsewhere = body
                .iter()
                .enumerate()
                .any(|(other, stmt)| other != index && stmt_mentions(stmt, &mapped));
            if std::env::var_os("ELEPHC_DEBUG_MAP_CACHE").is_some() {
                eprintln!("map_sum_cache: candidate mapped={} used_elsewhere={}", mapped, used_elsewhere);
            }
            if used_elsewhere {
                body[index] = original;
            } else {
                let inserted = prelude.len();
                body.splice(index..index, prelude);
                *counter += 1;
                index += inserted;
            }
        } else {
            recurse_blocks(&mut body[index], env, by_ref_params, aliased, counter);
        }
        index += 1;
    }
}

/// Recurses into the blocks a statement owns without rewriting the statement itself.
fn recurse_blocks(
    stmt: &mut Stmt,
    env: &TypeEnv,
    by_ref_params: &HashSet<String>,
    aliased: &HashSet<String>,
    counter: &mut usize,
) {
    match &mut stmt.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            cache_block(then_body, env, by_ref_params, aliased, counter);
            for (_, body) in elseif_clauses {
                cache_block(body, env, by_ref_params, aliased, counter);
            }
            if let Some(body) = else_body {
                cache_block(body, env, by_ref_params, aliased, counter);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            cache_block(then_body, env, by_ref_params, aliased, counter);
            if let Some(body) = else_body {
                cache_block(body, env, by_ref_params, aliased, counter);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body) => {
            cache_block(body, env, by_ref_params, aliased, counter);
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                cache_block(body, env, by_ref_params, aliased, counter);
            }
            if let Some(body) = default {
                cache_block(body, env, by_ref_params, aliased, counter);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            cache_block(try_body, env, by_ref_params, aliased, counter);
            for catch in catches {
                cache_block(&mut catch.body, env, by_ref_params, aliased, counter);
            }
            if let Some(body) = finally_body {
                cache_block(body, env, by_ref_params, aliased, counter);
            }
        }
        _ => {}
    }
}

/// Rewrites one `for` loop and returns the cached assignment to insert before it.
fn try_cache_loop(
    stmt: &mut Stmt,
    previous: &[Stmt],
    following: &[Stmt],
    env: &TypeEnv,
    by_ref_params: &HashSet<String>,
    aliased: &HashSet<String>,
    counter: usize,
) -> Option<(Vec<Stmt>, String)> {
    let StmtKind::For {
        init,
        condition,
        body,
        update,
    } = &mut stmt.kind
    else {
        return None;
    };
    let Some(loop_counter) = positive_bound(init.as_deref(), condition.as_ref()?) else {
        return None;
    };
    if init
        .as_deref()
        .is_some_and(|stmt| stmt_has_foreign_write(stmt, loop_counter) || stmt_has_call(stmt))
        || update
            .as_deref()
            .is_some_and(|stmt| stmt_has_foreign_write(stmt, loop_counter) || stmt_has_call(stmt))
    {
        return None;
    }
    let foreach = try_cache_foreach_sum(
        body,
        previous,
        following,
        condition.as_ref(),
        update.as_deref(),
        env,
        by_ref_params,
        aliased,
        loop_counter,
        counter,
        stmt.span,
    );
    if let Some(prelude) = foreach {
        return Some((prelude, String::new()));
    }
    let pair = match find_map_sum_pair(body, by_ref_params, aliased) {
        Some(pair) => pair,
        None => {
            if let Some(prelude) = try_hoist_map_assignment(
                body,
                previous,
                env,
                by_ref_params,
                aliased,
                loop_counter,
                counter,
                stmt.span,
            ) {
                return Some((prelude, String::new()));
            }
            if let Some(prelude) = try_hoist_invariant_reduce(
                body,
                previous,
                env,
                by_ref_params,
                aliased,
                loop_counter,
                counter,
                stmt.span,
            ) {
                return Some((prelude, String::new()));
            }
            let prelude = try_hoist_direct_map_read(
                body,
                previous,
                env,
                by_ref_params,
                aliased,
                loop_counter,
                counter,
                stmt.span,
            )?;
            return Some((prelude, String::new()));
        }
    };
    let mapped = pair.mapped;
    let sum_expr = pair.sum_expr;
    let mut prelude = Vec::new();
    let map_call = match &pair.source.kind {
        ExprKind::Variable(source)
            if !aliased.contains(source)
                && !by_ref_params.contains(source)
                && source_is_int_array(source, env, previous, aliased) =>
        {
            pair.map_call
        }
        _ if hoistable_source(&pair.source, env, aliased, loop_counter, body) => {
            let source_hidden = crate::names::generated_local_name(&format!(
                "__elephc_cached_map_source_{counter}"
            ));
            prelude.push(Stmt::new(
                StmtKind::Assign {
                    name: source_hidden.clone(),
                    value: pair.source,
                },
                stmt.span,
            ));
            replace_map_source(pair.map_call, &source_hidden)
        }
        _ => return None,
    };
    // The mapped local may not be observed anywhere else in the function body. The caller's
    // whole block is the function body for the top-level loop, and `stmt_mentions` is
    // deliberately conservative for statement shapes it does not understand.
    let hidden = crate::names::generated_local_name(&format!("__elephc_cached_map_sum_{counter}"));
    let rewritten = replace_sum(sum_expr, &mapped, &hidden);
    if mentions_array_sum_of(&rewritten, &mapped) || expr_mentions(&rewritten, &mapped) {
        return None;
    }
    // The map assignment (index 0) is about to be removed; every remaining statement must be
    // free of the mapped local after the sum expression was rewritten. Check before mutating so
    // a rejection never leaves the loop reading an unassigned cache local.
    if body.iter().skip(2).any(|stmt| stmt_mentions(stmt, &mapped)) {
        return None;
    }
    let sum_stmt = body.get_mut(1)?;
    if let StmtKind::Assign { value, .. } = &mut sum_stmt.kind {
        *value = rewritten;
    } else {
        return None;
    }
    body.remove(0);
    prelude.push(Stmt::new(
        StmtKind::Assign {
            name: mapped.clone(),
            value: map_call,
        },
        stmt.span,
    ));
    prelude.push(Stmt::new(
        StmtKind::Assign {
            name: hidden,
            value: Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("array_sum"),
                    args: vec![Expr::new(
                        ExprKind::Variable(mapped.clone()),
                        stmt.span,
                    )],
                },
                stmt.span,
            ),
        },
        stmt.span,
    ));
    Some((prelude, mapped))
}

/// Hoists `$mapped = array_map(...)` out of a loop whose accumulator reads constant keys.
///
/// The existing loop-reduction pass can then treat those reads as invariant. The map remains
/// assigned to its original local, so a read after the loop still observes the same value.
fn try_hoist_map_assignment(
    body: &mut Vec<Stmt>,
    previous: &[Stmt],
    env: &TypeEnv,
    by_ref_params: &HashSet<String>,
    aliased: &HashSet<String>,
    loop_counter: &str,
    counter: usize,
    span: Span,
) -> Option<Vec<Stmt>> {
    let [Stmt {
        kind: StmtKind::Assign {
            name: mapped,
            value: map_call,
        },
        ..
    }, Stmt {
        kind: StmtKind::Assign { name: accumulator, value },
        ..
    }] = body.as_slice()
    else {
        return None;
    };
    let addend = match &value.kind {
        ExprKind::BinaryOp {
            left,
            op: BinOp::Add,
            right,
        } if matches!(&left.kind, ExprKind::Variable(name) if name == accumulator) => right,
        ExprKind::BinaryOp {
            left,
            op: BinOp::Add,
            right,
        } if matches!(&right.kind, ExprKind::Variable(name) if name == accumulator) => left,
        _ => return None,
    };
    if !expr_mentions(addend, mapped) || !map_reads_are_pure(addend, mapped) {
        return None;
    }
    let ExprKind::FunctionCall { name: map_name, args } = &map_call.kind else {
        return None;
    };
    if map_name.as_str().trim_start_matches('\\') != "array_map" || args.len() != 2 {
        return None;
    }
    if !pure_numeric_callback(&args[0]) {
        return None;
    }
    let source = args[1].clone();
    let mut prelude = Vec::new();
    let map_call = match &source.kind {
        ExprKind::Variable(source)
            if !aliased.contains(source)
                && !by_ref_params.contains(source)
                && source_is_int_array(source, env, previous, aliased) =>
        {
            map_call.clone()
        }
        _ if hoistable_source(&source, env, aliased, loop_counter, body) => {
            let source_hidden = crate::names::generated_local_name(&format!(
                "__elephc_hoisted_map_source_{counter}"
            ));
            prelude.push(assign_stmt(&source_hidden, source, span));
            replace_map_source(map_call.clone(), &source_hidden)
        }
        _ => return None,
    };
    prelude.push(assign_stmt(mapped, map_call, span));
    body.remove(0);
    Some(prelude)
}

/// Returns whether every use of `mapped` is a read that cannot mutate the array.
fn map_reads_are_pure(expr: &Expr, mapped: &str) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => name != mapped,
        ExprKind::ArrayAccess { array, index } => {
            !expr_mentions(index, mapped)
                && (matches!(&array.kind, ExprKind::Variable(name) if name == mapped)
                    || map_reads_are_pure(array, mapped))
        }
        ExprKind::FunctionCall { name, args } => {
            let name = name.as_str().trim_start_matches('\\');
            matches!(name, "count" | "sizeof" | "array_sum")
                && args.iter().all(|arg| {
                    matches!(&arg.kind, ExprKind::Variable(source) if source == mapped)
                        || map_reads_are_pure(arg, mapped)
                })
        }
        ExprKind::BinaryOp { left, right, .. } => {
            map_reads_are_pure(left, mapped) && map_reads_are_pure(right, mapped)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Cast { expr: inner, .. } => map_reads_are_pure(inner, mapped),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            map_reads_are_pure(value, mapped) && map_reads_are_pure(default, mapped)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            map_reads_are_pure(condition, mapped)
                && map_reads_are_pure(then_expr, mapped)
                && map_reads_are_pure(else_expr, mapped)
        }
        ExprKind::ArrayLiteral(items) => items.iter().all(|item| map_reads_are_pure(item, mapped)),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .all(|(key, value)| map_reads_are_pure(key, mapped) && map_reads_are_pure(value, mapped)),
        ExprKind::ArrayLiteralMixed(items) => items
            .iter()
            .flat_map(|entry| entry.exprs())
            .all(|item| map_reads_are_pure(item, mapped)),
        _ if !expr_mentions(expr, mapped) => true,
        _ => false,
    }
}

/// Caches `foreach (array_map(...) as $x) { $acc += $x; }` as one map+sum before the loop.
fn try_cache_foreach_sum(
    body: &mut Vec<Stmt>,
    previous: &[Stmt],
    following: &[Stmt],
    outer_condition: Option<&Expr>,
    outer_update: Option<&Stmt>,
    env: &TypeEnv,
    by_ref_params: &HashSet<String>,
    aliased: &HashSet<String>,
    loop_counter: &str,
    counter: usize,
    span: Span,
) -> Option<Vec<Stmt>> {
    if body.len() != 1 {
        return None;
    }
    let original = body.first()?.clone();
    let StmtKind::Foreach {
        array,
        key_var,
        value_var,
        value_by_ref: false,
        body: inner,
        ..
    } = &original.kind
    else {
        return None;
    };
    let [Stmt {
        kind: StmtKind::Assign { name: sum, value: update },
        ..
    }] = inner.as_slice()
    else {
        return None;
    };
    let accumulation = matches!(
        &update.kind,
        ExprKind::BinaryOp { left, op: BinOp::Add, right }
            if matches!(&left.kind, ExprKind::Variable(name) if name == sum)
                && matches!(&right.kind, ExprKind::Variable(name) if name == value_var)
    ) || matches!(
        &update.kind,
        ExprKind::BinaryOp { left, op: BinOp::Add, right }
            if matches!(&right.kind, ExprKind::Variable(name) if name == sum)
                && matches!(&left.kind, ExprKind::Variable(name) if name == value_var)
    );
    if !accumulation || aliased.contains(value_var) || value_var == sum {
        return None;
    }
    if key_var.as_ref().is_some_and(|key| {
        aliased.contains(key) || key == sum
    }) {
        return None;
    }
    let binding_mentions = |stmt: &Stmt| {
        stmt_mentions(stmt, value_var)
            || key_var
                .as_ref()
                .is_some_and(|key| stmt_mentions(stmt, key))
    };
    if outer_condition.is_some_and(|condition| {
        expr_mentions(condition, value_var)
            || key_var
                .as_ref()
                .is_some_and(|key| expr_mentions(condition, key))
    }) || outer_update.is_some_and(|update| binding_mentions(update))
        || following
            .iter()
            .any(|stmt| binding_mentions(stmt) || stmt_has_call(stmt))
    {
        return None;
    }
    let ExprKind::FunctionCall { name: map_name, args } = &array.kind else {
        return None;
    };
    if map_name.as_str().trim_start_matches('\\') != "array_map" || args.len() != 2 {
        return None;
    }
    if !pure_int_callback(&args[0]) {
        return None;
    }
    let Some((seed_index, seed_stmt)) = previous
        .iter()
        .enumerate()
        .rev()
        .find(|(_, stmt)| stmt_writes_local(stmt, sum))
    else {
        return None;
    };
    let StmtKind::Assign { value: zero, .. } = &seed_stmt.kind else {
        return None;
    };
    if !matches!(zero.kind, ExprKind::IntLiteral(0)) {
        return None;
    }
    // A by-reference call after the seed can change the accumulator; the integer guard assumes
    // it still starts at zero.
    if previous[seed_index + 1..].iter().any(stmt_has_call) {
        return None;
    }
    let source = args[1].clone();
    let mut prelude = Vec::new();
    let map_call = match &source.kind {
        ExprKind::Variable(source)
            if !aliased.contains(source)
                && !by_ref_params.contains(source)
                && source_is_int_array(source, env, previous, aliased) =>
        {
            array.clone()
        }
        _ if hoistable_source(&source, env, aliased, loop_counter, body) => {
            let source_hidden = crate::names::generated_local_name(&format!(
                "__elephc_cached_foreach_source_{counter}"
            ));
            prelude.push(Stmt::new(
                StmtKind::Assign {
                    name: source_hidden.clone(),
                    value: source,
                },
                span,
            ));
            replace_map_source(array.clone(), &source_hidden)
        }
        _ => return None,
    };
    let map_hidden = crate::names::generated_local_name(&format!(
        "__elephc_cached_foreach_map_{counter}"
    ));
    let sum_hidden = crate::names::generated_local_name(&format!(
        "__elephc_cached_foreach_sum_{counter}"
    ));
    prelude.push(assign_stmt(&map_hidden, map_call, span));
    prelude.push(assign_stmt(
        &sum_hidden,
        call("array_sum", vec![var(&map_hidden, span)], span),
        span,
    ));
    let replacement = assign_stmt(
        sum,
        binary(var(sum, span), BinOp::Add, var(&sum_hidden, span), span),
        span,
    );
    body.clear();
    body.push(replacement);
    Some(prelude)
}

/// Builds a variable expression.
fn var(name: &str, span: Span) -> Expr {
    Expr::new(ExprKind::Variable(name.to_string()), span)
}

/// Builds a function call expression.
fn call(name: &str, args: Vec<Expr>, span: Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified(name),
            args,
        },
        span,
    )
}

/// Builds a binary expression.
fn binary(left: Expr, op: BinOp, right: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        },
        span,
    )
}

/// Returns whether `stmt` writes a local other than the loop counter.
fn stmt_has_foreign_write(stmt: &Stmt, counter: &str) -> bool {
    match &stmt.kind {
        StmtKind::Assign { name, .. } | StmtKind::TypedAssign { name, .. } => name != counter,
        StmtKind::ArrayAssign { array, .. } | StmtKind::ArrayPush { array, .. } => array != counter,
        StmtKind::RefAssign { target, .. } => target != counter,
        StmtKind::ExprStmt(expr) => expr_has_foreign_write(expr, counter),
        StmtKind::Synthetic(body) => body.iter().any(|stmt| stmt_has_foreign_write(stmt, counter)),
        _ => false,
    }
}

/// Returns whether an expression assigns or increments a local other than the counter.
fn expr_has_foreign_write(expr: &Expr, counter: &str) -> bool {
    match &expr.kind {
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => name != counter,
        ExprKind::Assignment { target, prelude, .. } => {
            !matches!(&target.kind, ExprKind::Variable(name) if name == counter)
                || prelude
                    .iter()
                    .any(|stmt| stmt_has_foreign_write(stmt, counter))
        }
        _ => false,
    }
}

/// Builds a plain assignment statement for the AST cache helpers.
fn assign_stmt(name: &str, value: Expr, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::Assign {
            name: name.to_string(),
            value,
        },
        span,
    )
}

/// Hoists a direct `array_map(...)[$k]` read in a one-statement accumulator loop.
fn try_hoist_direct_map_read(
    body: &mut Vec<Stmt>,
    previous: &[Stmt],
    env: &TypeEnv,
    by_ref_params: &HashSet<String>,
    aliased: &HashSet<String>,
    loop_counter: &str,
    counter: usize,
    span: Span,
) -> Option<Vec<Stmt>> {
    // The addend is borrowed out of the body, so snapshot the statements the invariance
    // check reads before taking that borrow.
    let loop_body = body.clone();
    let [Stmt {
        kind: StmtKind::Assign { name: accumulator, value },
        ..
    }] = body.as_mut_slice()
    else {
        return None;
    };
    let addend = accumulation_addend(value, accumulator)?;
    let hidden = crate::names::generated_local_name(&format!(
        "__elephc_hoisted_direct_map_{counter}"
    ));
    let (map_call, rewritten) = replace_direct_map_array(
        addend,
        &hidden,
        previous,
        env,
        by_ref_params,
        aliased,
        loop_counter,
        &loop_body,
    )?;
    let addend_slot = accumulation_addend_mut(value, accumulator)?;
    *addend_slot = rewritten;
    Some(vec![assign_stmt(&hidden, map_call, span)])
}

/// Returns whether `expr` is an array_map call with invariant source and pure callback.
fn invariant_map_call(
    expr: &Expr,
    previous: &[Stmt],
    env: &TypeEnv,
    by_ref_params: &HashSet<String>,
    aliased: &HashSet<String>,
    loop_counter: &str,
    loop_body: &[Stmt],
) -> bool {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return false;
    };
    if name.as_str().trim_start_matches('\\') != "array_map" || args.len() != 2 {
        return false;
    }
    if !pure_numeric_callback(&args[0]) {
        return false;
    }
    match &args[1].kind {
        ExprKind::Variable(source) => {
            !aliased.contains(source)
                && !by_ref_params.contains(source)
                && source_is_int_array(source, env, previous, aliased)
        }
        _ => hoistable_source(&args[1], env, aliased, loop_counter, loop_body),
    }
}

/// Replaces the array of one direct `array_map(...)[$k]` access with a hidden local.
fn replace_direct_map_array(
    expr: &Expr,
    hidden: &str,
    previous: &[Stmt],
    env: &TypeEnv,
    by_ref_params: &HashSet<String>,
    aliased: &HashSet<String>,
    loop_counter: &str,
    loop_body: &[Stmt],
) -> Option<(Expr, Expr)> {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return None;
    };
    if !invariant_map_call(array, previous, env, by_ref_params, aliased, loop_counter, loop_body) {
        return None;
    }
    let rewritten = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(ExprKind::Variable(hidden.to_string()), array.span)),
            index: index.clone(),
        },
        expr.span,
    );
    Some((array.as_ref().clone(), rewritten))
}

/// Hoists an invariant `array_reduce()` call used as the loop accumulator addend.
fn try_hoist_invariant_reduce(
    body: &mut Vec<Stmt>,
    previous: &[Stmt],
    env: &TypeEnv,
    by_ref_params: &HashSet<String>,
    aliased: &HashSet<String>,
    loop_counter: &str,
    counter: usize,
    span: Span,
) -> Option<Vec<Stmt>> {
    let [Stmt {
        kind: StmtKind::Assign { name: accumulator, value },
        ..
    }] = body.as_mut_slice()
    else {
        return None;
    };
    let addend = accumulation_addend(value, accumulator)?.clone();
    let ExprKind::FunctionCall { name, args } = &addend.kind else {
        return None;
    };
    if name.as_str().trim_start_matches('\\') != "array_reduce" || args.len() != 3 {
        return None;
    }
    let ExprKind::Variable(source) = &args[0].kind else {
        return None;
    };
    if aliased.contains(source)
        || by_ref_params.contains(source)
        || !source_is_int_array(source, env, previous, aliased)
    {
        return None;
    }
    if !pure_reduce_callback(&args[1]) {
        return None;
    }
    let forbidden = [loop_counter, accumulator];
    if !super::loop_accumulator_reduction::pure_expr(&args[2], &forbidden, env) {
        return None;
    }
    let hidden = crate::names::generated_local_name(&format!(
        "__elephc_hoisted_reduce_{counter}"
    ));
    let rewritten = Expr::new(ExprKind::Variable(hidden.clone()), addend.span);
    let slot = accumulation_addend_mut(value, accumulator)?;
    *slot = rewritten;
    Some(vec![assign_stmt(&hidden, addend, span)])
}

/// Returns the addend of `$acc = $acc + addend` in either operand order.
fn accumulation_addend<'a>(value: &'a Expr, accumulator: &str) -> Option<&'a Expr> {
    match &value.kind {
        ExprKind::BinaryOp { left, op: BinOp::Add, right }
            if matches!(&left.kind, ExprKind::Variable(name) if name == accumulator) => Some(right),
        ExprKind::BinaryOp { left, op: BinOp::Add, right }
            if matches!(&right.kind, ExprKind::Variable(name) if name == accumulator) => Some(left),
        _ => None,
    }
}

/// Mutable counterpart of [`accumulation_addend`].
fn accumulation_addend_mut<'a>(value: &'a mut Expr, accumulator: &str) -> Option<&'a mut Expr> {
    let ExprKind::BinaryOp {
        left,
        op: BinOp::Add,
        right,
    } = &mut value.kind
    else {
        return None;
    };
    if matches!(&left.kind, ExprKind::Variable(name) if name == accumulator) {
        Some(right.as_mut())
    } else if matches!(&right.kind, ExprKind::Variable(name) if name == accumulator) {
        Some(left.as_mut())
    } else {
        None
    }
}

/// Returns whether `expr` is a capture-free two-parameter pure reduce callback.
fn pure_reduce_callback(expr: &Expr) -> bool {
    let ExprKind::Closure {
        params,
        variadic,
        body,
        captures,
        capture_refs,
        ..
    } = &expr.kind
    else {
        return false;
    };
    if params.len() != 2
        || variadic.is_some()
        || !captures.is_empty()
        || !capture_refs.is_empty()
    {
        return false;
    }
    let [Stmt {
        kind: StmtKind::Return(Some(result)),
        ..
    }] = body.as_slice()
    else {
        return false;
    };
    let allowed: Vec<&str> = params.iter().map(|(name, ..)| name.as_str()).collect();
    reduce_body_is_total(result, &allowed)
}

/// Returns whether a reduce callback body is total: it cannot throw or emit a warning.
///
/// Hoisting the reduce out of a loop evaluates it even when the loop body never runs, so the
/// callback must be safe for every integer input. Division, modulo, shifts, powers, function
/// calls, and array access can throw or warn for some integer inputs and stay in the loop.
fn reduce_body_is_total(expr: &Expr, allowed: &[&str]) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::Variable(name) => allowed.contains(&name.as_str()),
        ExprKind::BinaryOp { left, op, right } => {
            matches!(
                op,
                BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Concat
                    | BinOp::Eq
                    | BinOp::NotEq
                    | BinOp::StrictEq
                    | BinOp::StrictNotEq
                    | BinOp::Lt
                    | BinOp::Gt
                    | BinOp::LtEq
                    | BinOp::GtEq
                    | BinOp::And
                    | BinOp::Or
                    | BinOp::Xor
                    | BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::NullCoalesce
            ) && reduce_body_is_total(left, allowed)
                && reduce_body_is_total(right, allowed)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner) => reduce_body_is_total(inner, allowed),
        ExprKind::Cast { target, expr: inner } => {
            matches!(
                target,
                crate::parser::ast::CastType::Int
                    | crate::parser::ast::CastType::Float
                    | crate::parser::ast::CastType::String
                    | crate::parser::ast::CastType::Bool
            ) && reduce_body_is_total(inner, allowed)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            reduce_body_is_total(condition, allowed)
                && reduce_body_is_total(then_expr, allowed)
                && reduce_body_is_total(else_expr, allowed)
        }
        ExprKind::ShortTernary { value, default } => {
            reduce_body_is_total(value, allowed) && reduce_body_is_total(default, allowed)
        }
        _ => false,
    }
}

/// One adjacent map assignment and sum update with its original source expression.
struct Pair {
    mapped: String,
    map_call: Expr,
    sum_expr: Expr,
    source: Expr,
}

/// Finds the adjacent `$mapped = array_map(...)` and `$sum = ... array_sum($mapped)` pair.
fn find_map_sum_pair(
    body: &[Stmt],
    by_ref_params: &HashSet<String>,
    aliased: &HashSet<String>,
) -> Option<Pair> {
    if body.len() != 2 {
        return None;
    }
    let StmtKind::Assign {
        name: mapped,
        value: map_call,
    } = &body[0].kind
    else {
        return None;
    };
    let ExprKind::FunctionCall { name: map_name, args } = &map_call.kind else {
        return None;
    };
    if map_name.as_str().trim_start_matches('\\') != "array_map" || args.len() != 2 {
        return None;
    }
    let source = args[1].clone();
    if let ExprKind::Variable(source_name) = &source.kind {
        if aliased.contains(source_name) || by_ref_params.contains(source_name) {
            return None;
        }
        if body.iter().any(|stmt| stmt_writes_local(stmt, source_name)) {
            return None;
        }
    }
    if !pure_numeric_callback(&args[0]) {
        return None;
    }
    let StmtKind::Assign { value: sum_expr, .. } = &body[1].kind else {
        return None;
    };
    if !mentions_array_sum_of(sum_expr, mapped) {
        return None;
    }
    Some(Pair {
        mapped: mapped.clone(),
        map_call: map_call.clone(),
        sum_expr: sum_expr.clone(),
        source,
    })
}

/// Returns whether `expr` is a supported capture-free numeric map callback.
fn pure_numeric_callback(expr: &Expr) -> bool {
    let ExprKind::Closure {
        params,
        variadic,
        return_type,
        body,
        captures,
        capture_refs,
        ..
    } = &expr.kind
    else {
        return false;
    };
    if params.len() != 1 || variadic.is_some() {
        return false;
    }
    let (_, param_type, default, by_ref) = &params[0];
    if !captures.is_empty()
        || !capture_refs.is_empty()
        || return_type.is_some()
        || param_type.is_some()
        || default.is_some()
        || *by_ref
    {
        return false;
    }
    let [Stmt {
        kind: StmtKind::Return(Some(result)),
        ..
    }] = body.as_slice()
    else {
        return false;
    };
    super::stored_numeric_map::parse_stored_numeric_map(result, &params[0].0).is_some()
}

/// Returns whether a one-parameter callback has a deterministic integer-shaped body.
fn pure_int_callback(expr: &Expr) -> bool {
    let ExprKind::Closure {
        params,
        variadic,
        return_type,
        body,
        captures,
        capture_refs,
        ..
    } = &expr.kind
    else {
        return false;
    };
    if params.len() != 1 || variadic.is_some() {
        return false;
    }
    let (_, param_type, default, by_ref) = &params[0];
    if !captures.is_empty()
        || !capture_refs.is_empty()
        || return_type.is_some()
        || param_type.is_some()
        || default.is_some()
        || *by_ref
    {
        return false;
    }
    let [Stmt {
        kind: StmtKind::Return(Some(result)),
        ..
    }] = body.as_slice()
    else {
        return false;
    };
    matches!(
        super::stored_numeric_map::parse_stored_numeric_map(result, &params[0].0),
        Some(
            super::stored_numeric_map::StoredNumericMapShape::Single { .. }
                | super::stored_numeric_map::StoredNumericMapShape::Affine { .. }
                | super::stored_numeric_map::StoredNumericMapShape::Square
                | super::stored_numeric_map::StoredNumericMapShape::SquareAffine { .. }
                | super::stored_numeric_map::StoredNumericMapShape::IntDiv { .. }
        )
    )
}

/// Returns whether `expr` contains `array_sum($mapped)` in one of the supported positions.
fn mentions_array_sum_of(expr: &Expr, mapped: &str) -> bool {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            name.as_str().trim_start_matches('\\') == "array_sum"
                && matches!(args.as_slice(), [Expr { kind: ExprKind::Variable(source), .. }] if source == mapped)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            mentions_array_sum_of(left, mapped) || mentions_array_sum_of(right, mapped)
        }
        _ => false,
    }
}

/// Replaces every `array_sum($mapped)` call in the supported expression positions.
fn replace_sum(expr: Expr, mapped: &str, hidden: &str) -> Expr {
    match expr.kind {
        ExprKind::FunctionCall { name, args }
            if name.as_str().trim_start_matches('\\') == "array_sum"
                && matches!(args.as_slice(), [Expr { kind: ExprKind::Variable(source), .. }] if source == mapped) =>
        {
            Expr::new(ExprKind::Variable(hidden.to_string()), expr.span)
        }
        ExprKind::BinaryOp { left, op, right } => Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(replace_sum(*left, mapped, hidden)),
                op,
                right: Box::new(replace_sum(*right, mapped, hidden)),
            },
            expr.span,
        ),
        _ => expr,
    }
}

/// Returns whether a source local is proven to hold a packed `array<int>`.
///
/// The checker may type a function-local `range()` as a generic array even though lowering
/// proves it packed; fall back to the nearest preceding write's structural producer proof.
fn source_is_int_array(
    name: &str,
    env: &TypeEnv,
    previous: &[Stmt],
    aliased: &HashSet<String>,
) -> bool {
    if env.get(name).map(PhpType::codegen_repr)
        == Some(PhpType::Array(Box::new(PhpType::Int)))
    {
        return true;
    }
    previous
        .iter()
        .rev()
        .find(|stmt| stmt_writes_local(stmt, name))
        .and_then(|stmt| match &stmt.kind {
            StmtKind::Assign { value, .. } => Some(value),
            _ => None,
        })
        .is_some_and(|value| int_array_expr(value, env, aliased))
}

/// Returns whether the loop body writes a local `expr` reads.
fn expr_reads_loop_written_local(expr: &Expr, loop_body: &[Stmt]) -> bool {
    let mut locals = Vec::new();
    read_locals(expr, &mut locals);
    locals
        .iter()
        .any(|name| loop_body.iter().any(|stmt| stmt_writes_local(stmt, name)))
}

/// Collects every local an expression reads, for the shapes a hoisted producer argument takes.
fn read_locals(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Variable(name) => out.push(name.clone()),
        ExprKind::FunctionCall { args, .. } => {
            for arg in args {
                read_locals(arg, out);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                read_locals(item, out);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            read_locals(left, out);
            read_locals(right, out);
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner) => read_locals(inner, out),
        _ => {}
    }
}

/// Returns whether `expr` is a total array producer whose arguments are loop-invariant.
///
/// `loop_body` is the loop's per-iteration statements: a producer argument that the loop writes
/// changes on every iteration, so `array_fill(0, 1, $sum)` is not hoistable in a loop that does
/// `$sum += array_sum($mapped)` even though `$sum` is a plain, unaliased local.
fn hoistable_source(
    expr: &Expr,
    env: &TypeEnv,
    aliased: &HashSet<String>,
    loop_counter: &str,
    loop_body: &[Stmt],
) -> bool {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return false;
    };
    if args.iter().any(|arg| expr_mentions(arg, loop_counter)) {
        return false;
    }
    if expr_reads_loop_written_local(expr, loop_body) {
        return false;
    }
    match name.as_str().trim_start_matches('\\') {
        "range" => (1..=3).contains(&args.len())
            && args.iter().all(|arg| invariant_scalar(arg, aliased)),
        "array_fill" => {
            args.len() == 3 && args.iter().all(|arg| invariant_scalar(arg, aliased))
        }
        "array_values" => args.len() == 1 && int_array_expr(&args[0], env, aliased),
        "array_reverse" => {
            (1..=2).contains(&args.len())
                && int_array_expr(&args[0], env, aliased)
                && args[1..].iter().all(|arg| invariant_scalar(arg, aliased))
        }
        "array_merge" => {
            !args.is_empty() && args.iter().all(|arg| int_array_expr(arg, env, aliased))
        }
        "array_slice" => {
            (2..=4).contains(&args.len())
                && int_array_expr(&args[0], env, aliased)
                && args[1..].iter().all(|arg| invariant_scalar(arg, aliased))
        }
        _ => false,
    }
}

/// Returns whether `expr` is a loop-invariant scalar literal or a plain local.
fn invariant_scalar(expr: &Expr, aliased: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::Variable(name) => !aliased.contains(name),
        ExprKind::Negate(inner) => invariant_scalar(inner, aliased),
        _ => false,
    }
}

/// Returns whether `expr` produces a packed `array<int>` from loop-invariant storage.
fn int_array_expr(expr: &Expr, env: &TypeEnv, aliased: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => {
            env.get(name).map(PhpType::codegen_repr)
                == Some(PhpType::Array(Box::new(PhpType::Int)))
        }
        ExprKind::ArrayLiteral(items) => items.iter().all(|item| {
            matches!(item.kind, ExprKind::IntLiteral(_) | ExprKind::Negate(_))
        }),
        ExprKind::FunctionCall { name, args } => match name.as_str().trim_start_matches('\\') {
            "range" | "array_fill" => args.iter().all(|arg| invariant_scalar(arg, aliased)),
            "array_values" => args.len() == 1 && int_array_expr(&args[0], env, aliased),
            "array_reverse" => {
                (1..=2).contains(&args.len())
                    && int_array_expr(&args[0], env, aliased)
                    && args[1..].iter().all(|arg| invariant_scalar(arg, aliased))
            }
            "array_merge" => {
                !args.is_empty() && args.iter().all(|arg| int_array_expr(arg, env, aliased))
            }
            "array_slice" => {
                (2..=4).contains(&args.len())
                    && int_array_expr(&args[0], env, aliased)
                    && args[1..].iter().all(|arg| invariant_scalar(arg, aliased))
            }
            _ => false,
        },
        _ => false,
    }
}

/// Replaces the `array_map()` source argument with the hoisted hidden local.
fn replace_map_source(map_call: Expr, source: &str) -> Expr {
    let ExprKind::FunctionCall { name, mut args } = map_call.kind else {
        return map_call;
    };
    if args.len() == 2 {
        args[1] = Expr::new(ExprKind::Variable(source.to_string()), args[1].span);
    }
    Expr::new(ExprKind::FunctionCall { name, args }, map_call.span)
}

/// Returns whether a `for` loop starts at a non-negative counter and has a positive bound.
fn positive_bound<'a>(init: Option<&'a Stmt>, condition: &Expr) -> Option<&'a str> {
    let Some(Stmt {
        kind: StmtKind::Assign { name: counter, value },
        ..
    }) = init
    else {
        return None;
    };
    if !matches!(value.kind, ExprKind::IntLiteral(seed) if seed >= 0) {
        return None;
    }
    let ExprKind::BinaryOp {
        left,
        op: BinOp::Lt | BinOp::LtEq,
        right,
    } = &condition.kind
    else {
        return None;
    };
    if !matches!(&left.kind, ExprKind::Variable(name) if name == counter) {
        return None;
    }
    positive_expr(right).then_some(counter.as_str())
}

/// Returns whether an expression is a positive runtime bound independent of loop progress.
fn positive_expr(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(value) => *value > 0,
        ExprKind::BinaryOp {
            left,
            op: BinOp::Add,
            right,
        } => match (&left.kind, &right.kind) {
            (ExprKind::IntLiteral(value), ExprKind::Variable(_))
            | (ExprKind::Variable(_), ExprKind::IntLiteral(value)) => *value >= 0,
            _ => false,
        },
        _ => false,
    }
}

/// Writes to one local in a statement, conservatively rejecting unknown statement shapes.
pub(super) fn stmt_writes_local(stmt: &Stmt, name: &str) -> bool {
    match &stmt.kind {
        StmtKind::Assign { name: target, .. } | StmtKind::TypedAssign { name: target, .. } => {
            target == name
        }
        StmtKind::ArrayAssign { array, .. } | StmtKind::ArrayPush { array, .. } => array == name,
        StmtKind::RefAssign { target, .. } => target == name,
        StmtKind::Foreach {
            value_var,
            value_by_ref,
            ..
        } => *value_by_ref && value_var == name,
        StmtKind::Global { vars } => vars.iter().any(|var| var == name),
        StmtKind::StaticVar { name: static_name, .. } => static_name == name,
        StmtKind::Synthetic(body)
        | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(|stmt| stmt_writes_local(stmt, name)),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            then_body.iter().any(|stmt| stmt_writes_local(stmt, name))
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| body.iter().any(|stmt| stmt_writes_local(stmt, name)))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_writes_local(stmt, name)))
        }
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| body.iter().any(|stmt| stmt_writes_local(stmt, name)))
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_writes_local(stmt, name)))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(|stmt| stmt_writes_local(stmt, name))
                || catches
                    .iter()
                    .any(|catch| catch.body.iter().any(|stmt| stmt_writes_local(stmt, name)))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_writes_local(stmt, name)))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(|stmt| stmt_writes_local(stmt, name))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_writes_local(stmt, name)))
        }
        StmtKind::NestedArrayAssign { .. } => true,
        StmtKind::IncludeOnceMark { .. } => false,
        StmtKind::Echo(_)
        | StmtKind::Return(_)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::Include { .. }
        | StmtKind::Throw(_)
        | StmtKind::ExprStmt(_)
        | StmtKind::ConstDecl { .. }
        | StmtKind::ListUnpack { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::PropertyAssign { .. }
        | StmtKind::StaticPropertyAssign { .. }
        | StmtKind::StaticPropertyArrayPush { .. }
        | StmtKind::StaticPropertyArrayAssign { .. }
        | StmtKind::PropertyArrayPush { .. }
        | StmtKind::PropertyArrayAssign { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }
        | StmtKind::Goto(_)
        | StmtKind::GotoLabel(_) => true,
    }
}
