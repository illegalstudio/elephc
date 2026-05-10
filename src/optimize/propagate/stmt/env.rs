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

pub(super) fn env_after_scalar_assign(mut env: ConstantEnv, name: &str, value: &Expr) -> ConstantEnv {
    if expr_effect(value).has_side_effects {
        env.clear();
    }
    if let Some(value) = assigned_scalar_value(value) {
        env.insert(name.to_string(), value);
    } else {
        env.remove(name);
    }
    env
}

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
