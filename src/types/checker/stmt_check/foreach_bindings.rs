//! Purpose:
//! Publishes foreach binding types at their implicit retirement exception boundaries.
//!
//! Called from:
//! - The foreach statement checker before checking the loop body.
//!
//! Key details:
//! - PHP binds the value before the key, and either displaced value can throw on release.
//! - Exceptional snapshots know the published bindings exist; zero-iteration flow does not.

use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};
use super::super::Checker;

impl Checker {
    /// Binds one iteration in PHP order without making its names present on a skipped loop.
    pub(super) fn bind_foreach_iteration(
        &mut self, array: &Expr, value_name: &str, value_type: PhpType,
        key: Option<(&str, PhpType)>, env: &mut TypeEnv,
    ) {
        let value_was_present = self.definitely_bound_locals.contains(value_name);
        let key_was_present = key.as_ref().is_some_and(|(name, _)| self.definitely_bound_locals.contains(*name));
        let can_iterate = match &array.kind {
            ExprKind::ArrayLiteral(values) => !values.is_empty(),
            ExprKind::ArrayLiteralAssoc(values) => !values.is_empty(),
            ExprKind::ArrayLiteralMixed(values) => !values.is_empty(),
            _ => true,
        };
        let retired = self.binding_retirement_type(value_name, env);
        env.insert(value_name.to_string(), value_type);
        self.definitely_bound_locals.insert(value_name.to_string());
        if can_iterate { self.record_binding_retirement_exception_flow(&retired, env); }
        if let Some((name, ty)) = &key {
            let retired = self.binding_retirement_type(name, env);
            env.insert((*name).to_string(), ty.clone());
            self.definitely_bound_locals.insert((*name).to_string());
            if can_iterate { self.record_binding_retirement_exception_flow(&retired, env); }
        }
        // The body publishes its own presence through the existing loop-flow frame. Keep
        // the entry snapshot unchanged here so an empty source cannot initialize a name.
        if !value_was_present { self.definitely_bound_locals.remove(value_name); }
        if let Some((name, _)) = key {
            if !key_was_present { self.definitely_bound_locals.remove(name); }
        }
    }
}
