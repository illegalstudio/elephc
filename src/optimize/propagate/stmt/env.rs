//! Purpose:
//! Propagates constants through statement env cases: records scalar and
//! array-literal facts after assignments and list unpacking, applying targeted
//! RHS invalidation and the volatility guard first.
//!
//! Called from:
//! - `crate::optimize::propagate::stmt`
//!
//! Key details:
//! - Statement propagation must invalidate aliases and writes before substituting values across observable boundaries.

use super::*;

/// Updates the constant environment after a scalar variable assignment `$name = $value`.
///
/// Clears the environment if `value` has side effects (preserving correctness across observable
/// boundaries). Otherwise extracts a scalar constant from `value` and inserts it; if `value` is
/// not a scalar constant, removes `name` from the environment.
///
/// - `env`: current constant environment
/// - `name`: variable being assigned
/// - `value`: RHS expression
/// Returns the updated environment.
pub(super) fn env_after_scalar_assign(mut env: ConstantEnv, name: &str, value: &Expr) -> ConstantEnv {
    expr_invalidation(value).apply(&mut env);
    // A reference-bound local's value may change through its alias without a visible local
    // write, so never record a constant for it.
    if super::is_reference_volatile(name) {
        env.remove(name);
        return env;
    }
    if let Some(value) = assigned_scalar_value(value) {
        env.insert(name.to_string(), PropagatedValue::Scalar(value));
    } else if let Some(fact) = assigned_array_fact(value) {
        env.insert(name.to_string(), PropagatedValue::ArrayLit(fact));
    } else if let ExprKind::Variable(source) = &value.kind {
        // `$b = $a` snapshots the value (PHP arrays are COW value-semantics),
        // so the source's fact — if any — is copied, not aliased. Scalars
        // never reach this arm: a scalar-fact variable was already substituted
        // by `propagate_expr`.
        match env.get(source).cloned() {
            Some(fact) => {
                env.insert(name.to_string(), fact);
            }
            None => {
                env.remove(name);
            }
        }
    } else {
        env.remove(name);
    }
    env
}

/// Updates the constant environment after a `list($vars) = $value` unpacking assignment.
///
/// Clears the environment if `value` has side effects. For each variable in `vars`, removes it
/// from the environment. If `value` is an array literal whose elements correspond to the variables,
/// extracts scalar constants from each element and inserts them into the environment.
///
/// - `env`: current constant environment
/// - `vars`: list of variables being assigned
/// - `value`: RHS expression producing the array to unpack
/// Returns the updated environment.
pub(super) fn env_after_list_unpack(mut env: ConstantEnv, vars: &[String], value: &Expr) -> ConstantEnv {
    expr_invalidation(value).apply(&mut env);

    for var in vars {
        env.remove(var);
    }

    // Unpack element facts from an inline literal, or from the value's array
    // fact when it is a variable (`list($x, $y) = $a` reads a COW snapshot).
    let literal = match &value.kind {
        ExprKind::ArrayLiteral(_) => Some(value),
        ExprKind::Variable(source) => match env.get(source) {
            Some(PropagatedValue::ArrayLit(fact)) => Some(fact),
            _ => None,
        },
        _ => None,
    };
    if let Some(ExprKind::ArrayLiteral(items)) = literal.map(|expr| &expr.kind) {
        let facts: Vec<(String, ScalarValue)> = vars
            .iter()
            .zip(items.iter())
            .filter_map(|(var, item)| {
                assigned_scalar_value(item).map(|value| (var.clone(), value))
            })
            .collect();
        for (var, value) in facts {
            env.insert(var, PropagatedValue::Scalar(value));
        }
    }

    env
}
