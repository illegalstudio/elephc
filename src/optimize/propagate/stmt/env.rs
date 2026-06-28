//! Purpose:
//! Propagates constants through statement env cases.
//! Maintains scalar environments while preserving declarations and control-flow side effects.
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
    if expr_effect(value).has_side_effects {
        env.clear();
    }
    // A reference-bound local's value may change through its alias without a visible local
    // write, so never record a constant for it.
    if super::is_reference_volatile(name) {
        env.remove(name);
        return env;
    }
    if let Some(value) = assigned_scalar_value(value) {
        env.insert(name.to_string(), value);
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
    if expr_effect(value).has_side_effects {
        env.clear();
    }

    for var in vars {
        env.remove(var);
    }

    if let ExprKind::ArrayLiteral(items) = &value.kind {
        for (var, item) in vars.iter().zip(items.iter()) {
            if let Some(value) = assigned_scalar_value(item) {
                env.insert(var.clone(), value);
            }
        }
    }

    env
}
