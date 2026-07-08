//! Purpose:
//! Loop-safe environment construction and lvalue-root extraction for constant
//! propagation: filters pre-loop facts down to the names a loop cannot write,
//! using the targeted `Invalidation` analysis.
//!
//! Called from:
//! - `crate::optimize::propagate` loop statement propagation
//!   (`propagate_while_stmt`, `propagate_for_stmt`, `propagate_foreach_stmt`,
//!   `propagate_do_while_stmt`) and the invalidation/volatility helpers.
//!
//! Key details:
//! - `lvalue_root` walks nested array-access chains (`$a[0][1]` roots at `$a`)
//!   so element writes at any depth invalidate the root's fact; property
//!   lvalues have no local root (they mutate heap state, not a caller local).
//! - The write-set authority is `expr_invalidation`/`block_invalidation` —
//!   these helpers only apply its verdict to an environment.

use super::*;

/// Computes a safe constant environment for a `for` loop by filtering out every
/// variable the loop's condition, body, or update can write (targeted
/// invalidation, so calls inside the loop keep facts for unwritten variables).
/// Returns an empty map only when a write set is genuinely unknowable.
pub(crate) fn safe_loop_env(
    env: &ConstantEnv,
    conditions: &[Expr],
    body: &[Stmt],
    update: Option<&Stmt>,
) -> ConstantEnv {
    let mut inv = block_invalidation(body);
    for condition in conditions {
        inv = inv.union(expr_invalidation(condition));
    }
    if let Some(update) = update {
        inv = inv.union(stmt_invalidation(update));
    }
    filter_written(env, inv)
}

/// Computes a safe constant environment for a `foreach` loop by filtering out
/// the key/value loop variables, the by-ref array root, and everything the
/// array expression or body can write (targeted invalidation).
pub(crate) fn safe_foreach_env(
    env: &ConstantEnv,
    array: &Expr,
    key_var: Option<&str>,
    value_var: &str,
    value_by_ref: bool,
    body: &[Stmt],
) -> ConstantEnv {
    let mut inv = expr_invalidation(array).union(block_invalidation(body));
    inv.add(value_var);
    if let Some(key_var) = key_var {
        inv.add(key_var);
    }
    if value_by_ref {
        // Writes through the by-ref value var mutate the array invisibly.
        if let Some(root) = lvalue_root(array) {
            inv.add(root);
        }
    }
    filter_written(env, inv)
}

/// Returns `env` minus the names in `inv` (`All` empties it).
fn filter_written(env: &ConstantEnv, inv: Invalidation) -> ConstantEnv {
    match inv {
        Invalidation::All => HashMap::new(),
        Invalidation::Names(written) => env
            .iter()
            .filter(|(name, _)| !written.contains(*name))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
    }
}

/// Returns the local variable at the root of an lvalue expression, if any:
/// a plain variable, an array-access chain over one, or a named-argument
/// wrapper around one. Property and static-property lvalues have no local
/// root (writing through them mutates heap state, not a caller local).
pub(crate) fn lvalue_root(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::ArrayAccess { array, .. } => lvalue_root(array),
        ExprKind::NamedArg { value, .. } => lvalue_root(value),
        _ => None,
    }
}
