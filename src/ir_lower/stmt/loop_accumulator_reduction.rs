//! Purpose:
//! Replaces a counted `for` loop whose body only accumulates one loop-invariant value with its
//! exact integer product, guarded by a runtime exactness check.
//!
//! Called from:
//! - `crate::ir_lower::function::lower_body_into_function` after the map+sum cache.
//!
//! Key details:
//! - The fast path needs a positive integer trip count and either an integer addend whose
//!   product also stays an integer, or a float addend whose product is an exact double.
//!   Otherwise the original loop runs unchanged. Float, string and overflow cases therefore
//!   keep PHP's per-iteration conversion, warning and promotion order.
//! - The addend and an optional pure producer (`mb_convert_kana`) are evaluated inside the
//!   `trip_count > 0` branch, so a zero-trip loop still evaluates neither. Both are re-evaluated
//!   at each guarded step so no cross-block temporary defeats codegen's numeric frame cells.
//! - The accumulator must be initialized to integer zero immediately before the loop; repeated
//!   addition from zero cannot overflow before the final product when the product is an int.
//! - The loop counter is advanced to its final value on the fast path, so code after the loop
//!   observes the same counter PHP would.
//! - Anything unproven (side effects, a call outside the whitelist, a non-literal seed, a body
//!   other than one accumulator update) leaves the original loop.

use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

use super::map_read_fusion::{expr_mentions, stmt_mentions};
use super::map_sum_cache::{expr_reads_loop_written_local, pure_reduce_callback, stmt_writes_local};

/// Rewrites every reducible counted accumulator loop in one function body.
pub(crate) fn reduce_loop_accumulators(
    body: &mut Vec<Stmt>,
    env: &crate::types::TypeEnv,
    aliased: &std::collections::HashSet<String>,
    entry_defined: &std::collections::HashSet<String>,
    in_main: bool,
) {
    let mut entry_defined = entry_defined.clone();
    if in_main {
        // The CLI always populates both predefined variables before main runs.
        entry_defined.insert("argc".to_string());
        entry_defined.insert("argv".to_string());
    }
    let mut counter = 0usize;
    reduce_block(body, env, aliased, &entry_defined, &mut counter);
}

/// Recurse into nested blocks and reduce each counted accumulator loop.
fn reduce_block(
    body: &mut Vec<Stmt>,
    env: &crate::types::TypeEnv,
    aliased: &std::collections::HashSet<String>,
    entry_defined: &std::collections::HashSet<String>,
    counter: &mut usize,
) {
    let mut index = 0;
    while index < body.len() {
        if let Some(replacement) = try_reduce(
            &body[index],
            env,
            &body[..index],
            &body[index + 1..],
            aliased,
            entry_defined,
            *counter,
        ) {
            let inserted = replacement.len();
            body.splice(index..=index, replacement);
            *counter += 1;
            index += inserted;
        } else {
            recurse_blocks(&mut body[index], env, aliased, entry_defined, counter);
        }
        index += 1;
    }
}

/// Recurses into blocks without reducing the statement itself.
fn recurse_blocks(
    stmt: &mut Stmt,
    env: &crate::types::TypeEnv,
    aliased: &std::collections::HashSet<String>,
    entry_defined: &std::collections::HashSet<String>,
    counter: &mut usize,
) {
    match &mut stmt.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            reduce_block(then_body, env, aliased, entry_defined, counter);
            for (_, body) in elseif_clauses {
                reduce_block(body, env, aliased, entry_defined, counter);
            }
            if let Some(body) = else_body {
                reduce_block(body, env, aliased, entry_defined, counter);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            reduce_block(then_body, env, aliased, entry_defined, counter);
            if let Some(body) = else_body {
                reduce_block(body, env, aliased, entry_defined, counter);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body) => reduce_block(body, env, aliased, entry_defined, counter),
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                reduce_block(body, env, aliased, entry_defined, counter);
            }
            if let Some(body) = default {
                reduce_block(body, env, aliased, entry_defined, counter);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            reduce_block(try_body, env, aliased, entry_defined, counter);
            for catch in catches {
                reduce_block(&mut catch.body, env, aliased, entry_defined, counter);
            }
            if let Some(body) = finally_body {
                reduce_block(body, env, aliased, entry_defined, counter);
            }
        }
        _ => {}
    }
}

/// Returns the replacement statements for one reducible loop, or `None`.
fn try_reduce(
    stmt: &Stmt,
    env: &crate::types::TypeEnv,
    previous: &[Stmt],
    following: &[Stmt],
    aliased: &std::collections::HashSet<String>,
    entry_defined: &std::collections::HashSet<String>,
    counter: usize,
) -> Option<Vec<Stmt>> {
    if let Some(replacement) = try_periodic_reduce(stmt, env, previous, following, aliased, counter) {
        return Some(replacement);
    }
    let StmtKind::For {
        init,
        condition,
        update,
        body,
    } = &stmt.kind
    else {
        return None;
    };
    let init = init.as_deref()?;
    let StmtKind::Assign {
        name: loop_counter,
        value: seed,
    } = &init.kind
    else {
        return None;
    };
    let ExprKind::IntLiteral(start) = seed.kind else {
        return None;
    };
    let ExprKind::BinaryOp {
        left,
        op: BinOp::Lt | BinOp::LtEq,
        right: bound,
    } = &condition.as_ref()?.kind
    else {
        return None;
    };
    if !matches!(&left.kind, ExprKind::Variable(name) if name == loop_counter) {
        return None;
    }
    if !is_increment(update.as_deref()?, loop_counter) {
        return None;
    }
    let (producer, accumulator, addend) = accumulator_update(body)?;
    if aliased.contains(loop_counter) || aliased.contains(&accumulator) {
        return None;
    }
    // The nearest preceding write to the accumulator must initialize it to integer zero.
    let Some(init_stmt) = previous
        .iter()
        .rev()
        .find(|stmt| stmt_writes_local(stmt, &accumulator))
    else {
        return None;
    };
    let StmtKind::Assign { value: zero, .. } = &init_stmt.kind else {
        return None;
    };
    if !matches!(&zero.kind, ExprKind::IntLiteral(0)) {
        return None;
    }
    let forbidden = [loop_counter.as_str(), accumulator.as_str()];
    let producer_ok = producer.as_ref().is_none_or(|(produced, value)| {
        produced != loop_counter
            && produced != &accumulator
            && !expr_mentions(bound, produced)
            && pure_producer(
                value,
                &[
                    loop_counter.as_str(),
                    accumulator.as_str(),
                    produced.as_str(),
                ],
                env,
            )
    });
    if !pure_expr(bound, &forbidden, env) || !pure_expr(addend, &forbidden, env) || !producer_ok {
        return None;
    }
    let span = stmt.span;
    let n_hidden = generated(counter, "n");
    let add_hidden = generated(counter, "add");
    let trip = trip_count(start, bound, condition.as_ref()?, span);
    let n_gt_zero = binary(var(&n_hidden, span), BinOp::Gt, int(0, span), span);
    // A non-integer trip count cannot be multiplied exactly; reject it before evaluating the
    // addend or a producer, so a zero-trip inclusive loop with a fractional bound stays inert.
    let n_ready = and(
        call("is_int", vec![var(&n_hidden, span)], span),
        n_gt_zero,
        span,
    );
    // The fast path evaluates the invariant bound and addend once. PHP evaluates both on
    // every iteration, so only values with no diagnostic or user-code effect may be reduced;
    // an array read, undefined local, nullable strlen/ord or cast keeps the original loop.
    let Some(mut defined) = proven_defined_before(previous, entry_defined) else {
        return None;
    };
    let mut string_locals = std::collections::HashSet::new();
    if let Some((produced, _)) = &producer {
        defined.insert(produced.clone());
        string_locals.insert(produced.clone());
    }
    // `is_increment` above proves the update writes only the counter, so the bound must not
    // depend on it. The body must not rewrite any local the bound reads.
    if expr_mentions(bound, loop_counter)
        || !repeatable_expr(bound, &defined, &string_locals, env)
        || !repeatable_expr(addend, &defined, &string_locals, env)
        || expr_reads_loop_written_local(bound, body)
    {
        return None;
    }
    let mut then_body = Vec::new();
    if let Some((produced, producer_value)) = producer {
        then_body.push(assign(&produced, producer_value, span));
    }
    // A local read from a successor block loses the codegen frame-cell proof, which turns an
    // invariant integer/float division into a boxed Mixed helper call. Re-evaluate a division
    // addend (and its product) at each guarded step instead: the guard and the fast path run
    // once per loop.
    let add_expr = if contains_division(addend) {
        addend.clone()
    } else {
        then_body.push(assign(&add_hidden, addend.clone(), span));
        var(&add_hidden, span)
    };
    let product = binary(add_expr.clone(), BinOp::Mul, var(&n_hidden, span), span);
    let exact = exact_product_condition(&add_expr, &n_hidden, &product, span);
    let fast_update = assign(
        &accumulator,
        binary(var(&accumulator, span), BinOp::Add, product, span),
        span,
    );
    let final_hidden = generated(counter, "final");
    let final_counter = binary(int(start, span), BinOp::Add, var(&n_hidden, span), span);
    let exact = and(
        exact,
        call("is_int", vec![var(&final_hidden, span)], span),
        span,
    );
    then_body.push(assign(&final_hidden, final_counter, span));
    let fast_body = vec![
        fast_update,
        assign(loop_counter, var(&final_hidden, span), span),
    ];
    let fallback = stmt.clone();
    then_body.push(if_stmt(exact, fast_body, vec![fallback], span));
    let outer = if_stmt(n_ready, then_body, vec![stmt.clone()], span);
    Some(vec![
        init.clone(),
        assign(&n_hidden, trip, span),
        outer,
    ])
}

/// Returns the optional pure producer, accumulator name and invariant addend from one body.
fn accumulator_update(body: &[Stmt]) -> Option<(Option<(String, Expr)>, String, &Expr)> {
    let (producer, update) = match body {
        [update @ Stmt {
            kind: StmtKind::Assign { .. },
            ..
        }] => (None, update),
        [producer @ Stmt {
            kind: StmtKind::Assign { .. },
            ..
        }, update @ Stmt {
            kind: StmtKind::Assign { .. },
            ..
        }] => (Some(producer), update),
        _ => return None,
    };
    let StmtKind::Assign { name: accumulator, value } = &update.kind else {
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
    let producer = producer.map(|stmt| {
        let StmtKind::Assign { name, value } = &stmt.kind else {
            unreachable!("matched Assign above");
        };
        (name.clone(), value.clone())
    });
    Some((producer, accumulator.clone(), addend))
}

/// Returns whether the update advances `counter` by exactly one.
fn is_increment(update: &Stmt, counter: &str) -> bool {
    match &update.kind {
        StmtKind::ExprStmt(expr) => matches!(
            &expr.kind,
            ExprKind::PostIncrement(name) | ExprKind::PreIncrement(name) if name == counter
        ),
        StmtKind::Assign { name, value } => {
            name == counter
                && matches!(
                    &value.kind,
                    ExprKind::BinaryOp { left, op: BinOp::Add, right }
                        if matches!(&left.kind, ExprKind::Variable(name) if name == counter)
                            && matches!(right.kind, ExprKind::IntLiteral(1))
                )
        }
        _ => false,
    }
}

/// Builds the trip-count expression for a `<`/`<=` counted loop.
fn trip_count(start: i64, bound: &Expr, condition: &Expr, span: Span) -> Expr {
    let inclusive = matches!(
        &condition.kind,
        ExprKind::BinaryOp { op: BinOp::LtEq, .. }
    );
    let mut trip = if start == 0 {
        bound.clone()
    } else {
        binary(bound.clone(), BinOp::Sub, int(start, span), span)
    };
    if inclusive {
        trip = binary(trip, BinOp::Add, int(1, span), span);
    }
    trip
}

/// Builds the guarded exact-integer product condition.
///
/// Adding an integer `add` exactly `n` times gives `add * n`. When both the addend and that
/// product stay PHP integers, every intermediate sum is bounded by the final magnitude and is
/// exact too, so no `intdiv()` round-trip is needed. A product that overflows becomes a float
/// and only the float branch below can accept it.
fn exact_product_condition(add: &Expr, n: &str, product: &Expr, span: Span) -> Expr {
    let is_n_int = call("is_int", vec![var(n, span)], span);
    let is_add_int = call("is_int", vec![add.clone()], span);
    let zero_add = binary(add.clone(), BinOp::StrictEq, int(0, span), span);
    let is_product_int = call("is_int", vec![product.clone()], span);
    let exact_int = and(
        is_add_int,
        or(zero_add, is_product_int, span),
        span,
    );
    // Repeatedly adding a double with at most e fractional bits is exact while the final value
    // stays below 2^(53-e), so `n * addend` is the same double as the loop's additions.
    let exact_float = or(
        float_product_branch(add, product, 1.0, 9_007_199_254_740_992.0, span),
        float_product_branch(add, product, 4.0, 2_251_799_813_685_248.0, span),
        span,
    );
    and(is_n_int, or(exact_int, exact_float, span), span)
}

/// Builds one exact-double product branch for `factor` fractional scaling and `limit` magnitude.
fn float_product_branch(
    add: &Expr,
    product: &Expr,
    factor: f64,
    limit: f64,
    span: Span,
) -> Expr {
    let is_add_float = call("is_float", vec![add.clone()], span);
    let add_finite = call("is_finite", vec![add.clone()], span);
    let product_float = call("is_float", vec![product.clone()], span);
    let product_finite = call("is_finite", vec![product.clone()], span);
    let scaled = binary(
        add.clone(),
        BinOp::Mul,
        float(factor, span),
        span,
    );
    let scaled_finite = call("is_finite", vec![scaled.clone()], span);
    let add_integral = binary(
        scaled.clone(),
        BinOp::Eq,
        call("floor", vec![scaled], span),
        span,
    );
    let product_small = and(
        binary(product.clone(), BinOp::Lt, float(limit, span), span),
        binary(product.clone(), BinOp::Gt, float(-limit, span), span),
        span,
    );
    and(
        and(
            and(is_add_float, add_finite, span),
            and(scaled_finite, add_integral, span),
            span,
        ),
        and(
            and(product_float, product_finite, span),
            product_small,
            span,
        ),
        span,
    )
}

/// Returns whether `expr` can be evaluated more than once without a repeated warning or user code.
///
/// A reduced division is evaluated in several guarded blocks. Array reads can warn on a missing
/// key, an undefined variable warns, casts can warn on an object, and calls can warn or run user
/// code, so only proven locals, superglobals and ordinary arithmetic shapes are repeatable.
/// Everything else keeps one evaluation in a preheader local.
fn repeatable_expr(
    expr: &Expr,
    defined: &std::collections::HashSet<String>,
    string_locals: &std::collections::HashSet<String>,
    env: &crate::types::TypeEnv,
) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::Variable(name) => {
            defined.contains(name)
                || name == "GLOBALS"
                || crate::superglobals::is_superglobal(name)
                || env.get(name).map(crate::types::PhpType::codegen_repr)
                    == Some(crate::types::PhpType::Str)
        }
        ExprKind::FunctionCall { name, args } => match name.as_str().trim_start_matches('\\') {
            "intval" | "floatval" | "abs" => args
                .iter()
                .all(|arg| repeatable_expr(arg, defined, string_locals, env)),
            "strlen" => {
                args.len() == 1
                    && (matches!(&args[0].kind, ExprKind::StringLiteral(_))
                        || matches!(
                            &args[0].kind,
                            ExprKind::Variable(local) if string_locals.contains(local)
                        ))
            }
            "ord" => args.len() == 1 && string_char_arg(&args[0], string_locals),
            "array_reduce" => {
                args.len() == 3
                    && repeatable_expr(&args[0], defined, string_locals, env)
                    && pure_reduce_callback(&args[1])
                    && repeatable_expr(&args[2], defined, string_locals, env)
            }
            _ => false,
        },
        ExprKind::BinaryOp { left, right, .. } => {
            repeatable_expr(left, defined, string_locals, env)
                && repeatable_expr(right, defined, string_locals, env)
        }
        ExprKind::Negate(inner) | ExprKind::Not(inner) | ExprKind::BitNot(inner) => {
            repeatable_expr(inner, defined, string_locals, env)
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            repeatable_expr(value, defined, string_locals, env)
                && repeatable_expr(default, defined, string_locals, env)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            repeatable_expr(condition, defined, string_locals, env)
                && repeatable_expr(then_expr, defined, string_locals, env)
                && repeatable_expr(else_expr, defined, string_locals, env)
        }
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .all(|item| repeatable_expr(item, defined, string_locals, env)),
        ExprKind::ArrayLiteralAssoc(items) => items.iter().all(|(key, value)| {
            repeatable_expr(key, defined, string_locals, env)
                && repeatable_expr(value, defined, string_locals, env)
        }),
        ExprKind::ArrayLiteralMixed(items) => items
            .iter()
            .flat_map(|entry| entry.exprs())
            .all(|item| repeatable_expr(item, defined, string_locals, env)),
        _ => false,
    }
}

/// Reports whether `expr` is a non-null string (or one of its characters) for strlen/ord.
fn string_char_arg(
    expr: &Expr,
    string_locals: &std::collections::HashSet<String>,
) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_) => true,
        ExprKind::Variable(local) => string_locals.contains(local),
        ExprKind::ArrayAccess { array, index } => {
            matches!(index.kind, ExprKind::IntLiteral(_))
                && matches!(&array.kind, ExprKind::Variable(local) if string_locals.contains(local))
        }
        _ => false,
    }
}

/// Returns the locals proven defined before the loop, or `None` when the prefix is not provable.
///
/// A preceding call can unset a local through a by-reference argument, and an array write can
/// invoke user code. Only straight-line scalar assignments, variable reference bindings and
/// global/static declarations are accepted; any other preceding statement keeps the division
/// addend on its single preheader evaluation.
fn proven_defined_before(
    previous: &[Stmt],
    entry_defined: &std::collections::HashSet<String>,
) -> Option<std::collections::HashSet<String>> {
    let mut defined = entry_defined.clone();
    for stmt in previous {
        match &stmt.kind {
            StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
                if !simple_definition_value(value) {
                    return None;
                }
                defined.insert(name.clone());
            }
            StmtKind::RefAssign { target, source } => {
                if !simple_definition_value(source) {
                    return None;
                }
                defined.insert(target.clone());
            }
            StmtKind::ListUnpack { vars, value } => {
                if !simple_definition_value(value) {
                    return None;
                }
                defined.extend(vars.iter().cloned());
            }
            StmtKind::StaticVar { name, init } => {
                if !simple_definition_value(init) {
                    return None;
                }
                defined.insert(name.clone());
            }
            // The map-sum cache guards its hoisted prelude with an `if` whose condition is
            // pure and whose body is exactly the prelude it generated. When the loop runs,
            // that guard has already run, so its assignments count as definite.
            StmtKind::If { then_body, .. }
                if stmt.attributes.iter().flat_map(|group| &group.attributes).any(
                    |attribute| attribute.name.as_str() == "__elephc_prelude_guard"
                ) =>
            {
                for inner in then_body {
                    match &inner.kind {
                        StmtKind::Assign { name, .. } | StmtKind::TypedAssign { name, .. } => {
                            defined.insert(name.clone());
                        }
                        _ => return None,
                    }
                }
            }
            _ => return None,
        }
    }
    Some(defined)
}

/// Returns whether a definition's value cannot call user code or unset another local.
fn simple_definition_value(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::Variable(_) => true,
        ExprKind::BinaryOp { left, right, .. } => {
            simple_definition_value(left) && simple_definition_value(right)
        }
        ExprKind::Negate(inner) | ExprKind::Not(inner) | ExprKind::BitNot(inner) => {
            simple_definition_value(inner)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            simple_definition_value(condition)
                && simple_definition_value(then_expr)
                && simple_definition_value(else_expr)
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            simple_definition_value(value) && simple_definition_value(default)
        }
        ExprKind::ArrayLiteral(items) => items.iter().all(simple_definition_value),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .all(|(key, value)| simple_definition_value(key) && simple_definition_value(value)),
        ExprKind::ArrayLiteralMixed(items) => items
            .iter()
            .flat_map(|entry| entry.exprs())
            .all(simple_definition_value),
        ExprKind::FunctionCall { name, args } => match name.as_str().trim_start_matches('\\') {
            "array_reduce" => {
                args.len() == 3
                    && simple_definition_value(&args[0])
                    && pure_reduce_callback(&args[1])
                    && simple_definition_value(&args[2])
            }
            "range"
            | "array_fill"
            | "array_values"
            | "array_reverse"
            | "array_slice"
            | "array_merge"
            | "intval"
            | "floatval"
            | "abs" => args.iter().all(simple_definition_value),
            _ => false,
        },
        _ => false,
    }
}

/// Returns whether `expr` contains a PHP division or modulo operation.
fn contains_division(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::BinaryOp { left, op, right } => {
            matches!(op, BinOp::Div | BinOp::Mod)
                || contains_division(left)
                || contains_division(right)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Cast { expr: inner, .. } => contains_division(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            contains_division(value) || contains_division(default)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            contains_division(condition)
                || contains_division(then_expr)
                || contains_division(else_expr)
        }
        ExprKind::ArrayAccess { array, index } => {
            contains_division(array) || contains_division(index)
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(contains_division),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| contains_division(key) || contains_division(value)),
        ExprKind::ArrayLiteralMixed(items) => items
            .iter()
            .flat_map(|entry| entry.exprs())
            .any(contains_division),
        ExprKind::FunctionCall { args, .. } => args.iter().any(contains_division),
        _ => false,
    }
}

/// Returns whether `expr` is pure and does not name a forbidden local.
/// Returns whether an index receiver is statically an object rather than an array or string.
fn receiver_may_be_object(expr: &Expr, env: &crate::types::TypeEnv) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => matches!(
            env.get(name).map(crate::types::PhpType::codegen_repr),
            Some(crate::types::PhpType::Object(_))
        ),
        ExprKind::ArrayAccess { array, .. } => receiver_may_be_object(array, env),
        _ => false,
    }
}

/// Returns whether `expr` is pure and does not name a forbidden local.
///
/// An index read counts only when its receiver is not a statically known object, whose
/// `offsetGet` may change state or return a fresh value on every read.
pub(super) fn pure_expr(expr: &Expr, forbidden: &[&str], env: &crate::types::TypeEnv) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::Variable(name) => !forbidden.contains(&name.as_str()),
        ExprKind::BinaryOp { left, right, .. } => {
            pure_expr(left, forbidden, env) && pure_expr(right, forbidden, env)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Cast { expr: inner, .. } => pure_expr(inner, forbidden, env),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            pure_expr(value, forbidden, env) && pure_expr(default, forbidden, env)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            pure_expr(condition, forbidden, env)
                && pure_expr(then_expr, forbidden, env)
                && pure_expr(else_expr, forbidden, env)
        }
        ExprKind::ArrayAccess { array, index } => {
            // An object receiver runs `offsetGet`, which may change state or return a new
            // value on every read, so a loop accumulating `$object[0]` is not reducible.
            !receiver_may_be_object(array, env)
                && pure_expr(array, forbidden, env)
                && pure_expr(index, forbidden, env)
        }
        ExprKind::ArrayLiteral(items) => items.iter().all(|item| pure_expr(item, forbidden, env)),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .all(|(key, value)| pure_expr(key, forbidden, env) && pure_expr(value, forbidden, env)),
        ExprKind::ArrayLiteralMixed(items) => items
            .iter()
            .flat_map(|entry| entry.exprs())
            .all(|item| pure_expr(item, forbidden, env)),
        ExprKind::FunctionCall { name, args } => {
            is_pure_call(name.as_str()) && args.iter().all(|arg| pure_expr(arg, forbidden, env))
        }
        _ => false,
    }
}

/// Returns whether a call is total and side-effect free for a string/int input.
fn is_pure_call(name: &str) -> bool {
    matches!(
        name.trim_start_matches('\\'),
        "strlen" | "ord" | "intval" | "floatval" | "abs"
    )
}

/// Returns whether a call is a pure, total producer of a loop-invariant string.
fn pure_producer(expr: &Expr, forbidden: &[&str], env: &crate::types::TypeEnv) -> bool {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return false;
    };
    matches!(
        name.as_str().trim_start_matches('\\'),
        "mb_convert_kana" | "mb_convert_encoding" | "str_repeat"
    ) && args.iter().all(|arg| pure_expr(arg, forbidden, env))
}

/// One observation of an encoded codepoint over a literal table.
#[derive(Clone, Copy)]
enum PeriodicObservation {
    Length,
    LengthAndFirstByte,
}

/// Rewrites `$x = mb_chr($table[$i & mask], "UTF-8"); $n += <observation>` over a literal table.
fn try_periodic_reduce(
    stmt: &Stmt,
    env: &crate::types::TypeEnv,
    previous: &[Stmt],
    following: &[Stmt],
    aliased: &std::collections::HashSet<String>,
    counter: usize,
) -> Option<Vec<Stmt>> {
    let StmtKind::For {
        init,
        condition,
        update,
        body,
    } = &stmt.kind
    else {
        return None;
    };
    let init = init.as_deref()?;
    let StmtKind::Assign {
        name: loop_counter,
        value: seed,
    } = &init.kind
    else {
        return None;
    };
    if !matches!(&seed.kind, ExprKind::IntLiteral(0)) {
        return None;
    }
    let ExprKind::BinaryOp {
        left,
        op: BinOp::Lt | BinOp::LtEq,
        right: bound,
    } = &condition.as_ref()?.kind
    else {
        return None;
    };
    if !matches!(&left.kind, ExprKind::Variable(name) if name == loop_counter) {
        return None;
    }
    if !is_increment(update.as_deref()?, loop_counter) {
        return None;
    }
    let [producer, update] = body.as_slice() else {
        return None;
    };
    let StmtKind::Assign {
        name: encoded,
        value: map_call,
    } = &producer.kind
    else {
        return None;
    };
    let StmtKind::Assign { name: accumulator, value } = &update.kind else {
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
    let observation = periodic_observation(addend, encoded)?;
    let ExprKind::FunctionCall { name: map_name, args } = &map_call.kind else {
        return None;
    };
    if map_name.as_str().trim_start_matches('\\') != "mb_chr" || args.len() != 2 {
        return None;
    }
    let ExprKind::ArrayAccess { array, index } = &args[0].kind else {
        return None;
    };
    let ExprKind::BinaryOp {
        left: index_counter,
        op: BinOp::BitAnd,
        right: mask_expr,
    } = &index.kind
    else {
        return None;
    };
    if !matches!(&index_counter.kind, ExprKind::Variable(name) if name == loop_counter) {
        return None;
    }
    let ExprKind::IntLiteral(mask) = mask_expr.kind else {
        return None;
    };
    if !matches!(&args[1].kind, ExprKind::StringLiteral(encoding)
        if encoding.eq_ignore_ascii_case("UTF-8"))
    {
        return None;
    }
    let ExprKind::Variable(table) = &array.kind else {
        return None;
    };
    // The table literal is the nearest preceding write to the table local. Any call between
    // that write and the loop may mutate the table through a by-reference argument.
    let (table_write_index, table_stmt) = previous
        .iter()
        .enumerate()
        .rev()
        .find(|(_, stmt)| stmt_writes_local(stmt, table))?;
    if previous[table_write_index + 1..]
        .iter()
        .any(super::fill_sum_reduction::stmt_has_call)
    {
        return None;
    }
    let table_expr = match &table_stmt.kind {
        StmtKind::Assign { value, .. } => value,
        _ => return None,
    };
    let ExprKind::ArrayLiteral(items) = &table_expr.kind else {
        return None;
    };
    let mut codepoints = Vec::with_capacity(items.len());
    for item in items {
        codepoints.push(int_literal(item)?);
    }
    if codepoints.is_empty()
        || !codepoints.len().is_power_of_two()
        || usize::try_from(mask).ok()? + 1 != codepoints.len()
    {
        return None;
    }
    if aliased.contains(loop_counter)
        || aliased.contains(accumulator)
        || aliased.contains(table)
    {
        return None;
    }
    let init_stmt = previous
        .iter()
        .rev()
        .find(|stmt| stmt_writes_local(stmt, accumulator))?;
    let StmtKind::Assign { value: zero, .. } = &init_stmt.kind else {
        return None;
    };
    if !matches!(&zero.kind, ExprKind::IntLiteral(0)) {
        return None;
    }
    let forbidden = [loop_counter.as_str(), accumulator.as_str()];
    if !pure_expr(bound, &forbidden, env) {
        return None;
    }
    let mut values = Vec::with_capacity(codepoints.len());
    for codepoint in codepoints {
        let (length, first) = utf8_observation(codepoint)?;
        values.push(match observation {
            PeriodicObservation::Length => length,
            PeriodicObservation::LengthAndFirstByte => length + first,
        });
    }
    let period = values.iter().sum::<i64>();
    let mut prefix = Vec::with_capacity(values.len() + 1);
    prefix.push(0i64);
    for value in &values {
        prefix.push(prefix.last().copied()? + *value);
    }
    let span = stmt.span;
    let n_hidden = generated(counter, "period_n");
    let q_hidden = generated(counter, "period_q");
    let r_hidden = generated(counter, "period_r");
    let product_hidden = generated(counter, "period_product");
    let sum_hidden = generated(counter, "period_sum");
    let prefix_hidden = generated(counter, "period_prefix");
    let trip = trip_count(0, bound, condition.as_ref()?, span);
    let prefix_expr = Expr::new(
        ExprKind::ArrayLiteral(prefix.iter().map(|value| int(*value, span)).collect()),
        span,
    );
    let positive = and(
        binary(var(&n_hidden, span), BinOp::Gt, int(0, span), span),
        call("is_int", vec![var(&n_hidden, span)], span),
        span,
    );
    let mut inner = Vec::new();
    // The positive fast path skips the producer loop; keep its final value only when a
    // surrounding statement can observe it (the benchmark path has no such read).
    let encoded_observed = previous.iter().any(|stmt| stmt_mentions(stmt, encoded))
        || following.iter().any(|stmt| stmt_mentions(stmt, encoded));
    let final_index = binary(
        binary(var(&n_hidden, span), BinOp::Sub, int(1, span), span),
        BinOp::BitAnd,
        int(mask, span),
        span,
    );
    let mut final_call = map_call.clone();
    if let ExprKind::FunctionCall { args, .. } = &mut final_call.kind {
        args[0] = Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(var(table, span)),
                index: Box::new(final_index),
            },
            span,
        );
    }
    if encoded_observed {
        inner.push(assign(encoded, final_call, span));
    }
    inner.push(assign(
        &q_hidden,
        call(
            "intdiv",
            vec![var(&n_hidden, span), int(values.len() as i64, span)],
            span,
        ),
        span,
    ));
    inner.push(assign(
        &r_hidden,
        binary(
            var(&n_hidden, span),
            BinOp::Mod,
            int(values.len() as i64, span),
            span,
        ),
        span,
    ));
    inner.push(assign(
        &product_hidden,
        binary(var(&q_hidden, span), BinOp::Mul, int(period, span), span),
        span,
    ));
    let product_is_int = call("is_int", vec![var(&product_hidden, span)], span);
    let sum_expr = binary(
        var(&product_hidden, span),
        BinOp::Add,
        Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(var(&prefix_hidden, span)),
                index: Box::new(var(&r_hidden, span)),
            },
            span,
        ),
        span,
    );
    let mut exact_body = vec![assign(&sum_hidden, sum_expr, span)];
    let sum_is_int = call("is_int", vec![var(&sum_hidden, span)], span);
    let fast = vec![
        assign(
            accumulator,
            binary(
                var(accumulator, span),
                BinOp::Add,
                var(&sum_hidden, span),
                span,
            ),
            span,
        ),
        assign(loop_counter, var(&n_hidden, span), span),
    ];
    exact_body.push(if_stmt(
        sum_is_int,
        fast,
        vec![stmt.clone()],
        span,
    ));
    inner.push(if_stmt(
        product_is_int,
        exact_body,
        vec![stmt.clone()],
        span,
    ));
    let outer = if_stmt(positive, inner, vec![stmt.clone()], span);
    Some(vec![
        init.clone(),
        assign(&prefix_hidden, prefix_expr, span),
        assign(&n_hidden, trip, span),
        outer,
    ])
}

/// Reads an integer literal, including a parser-level unary negation.
fn int_literal(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg(),
            _ => None,
        },
        _ => None,
    }
}

/// Returns the UTF-8 byte length and first byte of one valid codepoint.
fn utf8_observation(codepoint: i64) -> Option<(i64, i64)> {
    if !(0..=0x10FFFF).contains(&codepoint) || (0xD800..=0xDFFF).contains(&codepoint) {
        return None;
    }
    Some(if codepoint <= 0x7F {
        (1, codepoint)
    } else if codepoint <= 0x7FF {
        (2, 0xC0 | (codepoint >> 6))
    } else if codepoint <= 0xFFFF {
        (3, 0xE0 | (codepoint >> 12))
    } else {
        (4, 0xF0 | (codepoint >> 18))
    })
}

/// Matches `strlen($x)` and `strlen($x) + ord($x[0])` in either operand order.
fn periodic_observation(addend: &Expr, encoded: &str) -> Option<PeriodicObservation> {
    if is_strlen_of(addend, encoded) {
        return Some(PeriodicObservation::Length);
    }
    let ExprKind::BinaryOp {
        left,
        op: BinOp::Add,
        right,
    } = &addend.kind
    else {
        return None;
    };
    let (length, first) = if is_strlen_of(left, encoded) {
        (left, right)
    } else if is_strlen_of(right, encoded) {
        (right, left)
    } else {
        return None;
    };
    if is_ord_first_byte_of(length, encoded) || is_ord_first_byte_of(first, encoded) {
        return Some(PeriodicObservation::LengthAndFirstByte);
    }
    None
}

/// Returns whether `expr` is `strlen($encoded)`.
fn is_strlen_of(expr: &Expr, encoded: &str) -> bool {
    matches!(
        &expr.kind,
        ExprKind::FunctionCall { name, args }
            if name.as_str().trim_start_matches('\\') == "strlen"
                && matches!(args.as_slice(), [Expr { kind: ExprKind::Variable(name), .. }] if name == encoded)
    )
}

/// Returns whether `expr` is `ord($encoded[0])`.
fn is_ord_first_byte_of(expr: &Expr, encoded: &str) -> bool {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return false;
    };
    if name.as_str().trim_start_matches('\\') != "ord" {
        return false;
    }
    matches!(
        args.as_slice(),
        [Expr {
            kind: ExprKind::ArrayAccess { array, index },
            ..
        }] if matches!(&array.kind, ExprKind::Variable(name) if name == encoded)
            && matches!(index.kind, ExprKind::IntLiteral(0))
    )
}

/// Returns a generated hidden-local name.
fn generated(counter: usize, role: &str) -> String {
    crate::names::generated_local_name(&format!("__elephc_loop_reduce_{role}_{counter}"))
}

/// Builds a variable expression.
fn var(name: &str, span: Span) -> Expr {
    Expr::new(ExprKind::Variable(name.to_string()), span)
}

/// Builds an integer literal expression.
fn int(value: i64, span: Span) -> Expr {
    Expr::new(ExprKind::IntLiteral(value), span)
}

/// Builds a float literal expression.
fn float(value: f64, span: Span) -> Expr {
    Expr::new(ExprKind::FloatLiteral(value), span)
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

/// Builds a short-circuit `&&` expression.
fn and(left: Expr, right: Expr, span: Span) -> Expr {
    binary(left, BinOp::And, right, span)
}

/// Builds a short-circuit `||` expression.
fn or(left: Expr, right: Expr, span: Span) -> Expr {
    binary(left, BinOp::Or, right, span)
}

/// Builds a plain assignment statement.
fn assign(name: &str, value: Expr, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::Assign {
            name: name.to_string(),
            value,
        },
        span,
    )
}

/// Builds an if/else statement.
fn if_stmt(condition: Expr, then_body: Vec<Stmt>, else_body: Vec<Stmt>, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses: Vec::new(),
            else_body: (!else_body.is_empty()).then_some(else_body),
        },
        span,
    )
}
